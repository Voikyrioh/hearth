//! La façade : `LinkManager`. Une tâche supervisée par serveur, une API asynchrone dont aucune
//! fonction ne panique ni ne bloque indéfiniment : tout rend un `Result` typé, sous délai.
//!
//! Le gestionnaire ne contient aucune règle : il assemble les ports, lance les tâches
//! (`task.rs`), les veilleurs de réveil et de réseau (`watchers.rs`) et traduit les appels de
//! l'interface en commandes. Les règles sont dans `domain/`.

mod attempt;
mod events;
mod persist;
mod task;
mod watchers;

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use hearth_proto::api::accounts::AccountInfo;
use hearth_proto::api::hello::HelloResponse;
use hearth_proto::fingerprint::Fingerprint;
use serde_json::Value;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tokio::time::timeout;

pub use events::EventStream;
use events::Fanout;
use task::Command;

use crate::adapters::{
    FileOperationStore, FileServerStore, FileSnapshotStore, HttpTransport, HttpTransportConfig,
    OsRng, SystemClock, SystemNetWatcher,
};
use crate::domain::compat::{self, Compatibility};
use crate::domain::event::StateInfo;
use crate::domain::pending_ops::OperationId;
use crate::domain::secret::Secret;
use crate::domain::server::{LastKnown, ServerId, ServerRecord};
use crate::domain::state::{LinkState, Reason, Start, Thresholds};
use crate::domain::time::WallTime;
use crate::error::LinkError;
use crate::ports::transport::{Method, Pin, Target};
use crate::ports::vault::SecretKind;
use crate::ports::{
    Clock, EventSink, NetWatcher, OperationStore, Rng, ServerStore, SnapshotStore, Transport, Vault,
};

/// Réglages de la bibliothèque. Les valeurs par défaut sont celles de la spec ; les tests de
/// résilience les réduisent pour rester rapides.
#[derive(Debug, Clone)]
pub struct LinkConfig {
    pub thresholds: Thresholds,
    /// Période du battement (ping) sur le flux : 2 s.
    pub heartbeat_period: Duration,
    /// Durée maximale d'une tentative de connexion complète (flux, authentification, instantané).
    pub attempt_timeout: Duration,
    /// Délai d'une requête (action, relecture d'opération, déconnexion, connexion). Un seul
    /// réglage : le transport de production le reprend (`LinkManager::open`) et la tâche du
    /// serveur borne elle-même chaque action.
    pub request_timeout: Duration,
    /// Période de relecture des adresses réseau locales : 5 s.
    pub net_poll_period: Duration,
    /// Période de contrôle de l'horloge pour détecter un réveil : 1 s.
    pub wake_check_period: Duration,
    /// Au plus une sauvegarde de la dernière vue par cette période (hors changement d'état).
    pub snapshot_save_period: Duration,
    /// Attente avant de relancer une tâche tombée en panne.
    pub restart_delay: Duration,
    /// Attente avant de relire une opération que l'agent dit « en cours ».
    pub recheck_delay: Duration,
    /// S'abonner aussi au journal d'activité (compte administrateur).
    pub subscribe_audit: bool,
    /// Taille du canal d'événements.
    pub event_capacity: usize,
}

impl Default for LinkConfig {
    fn default() -> Self {
        Self {
            thresholds: Thresholds::default(),
            heartbeat_period: Duration::from_secs(2),
            attempt_timeout: Duration::from_secs(8),
            request_timeout: Duration::from_secs(10),
            net_poll_period: Duration::from_secs(5),
            wake_check_period: Duration::from_secs(1),
            snapshot_save_period: Duration::from_secs(30),
            restart_delay: Duration::from_secs(1),
            recheck_delay: Duration::from_millis(500),
            subscribe_audit: false,
            event_capacity: 1024,
        }
    }
}

/// Tout ce dont le gestionnaire a besoin du monde extérieur.
pub struct Ports {
    pub transport: Arc<dyn Transport>,
    pub vault: Arc<dyn Vault>,
    pub servers: Arc<dyn ServerStore>,
    pub snapshots: Arc<dyn SnapshotStore>,
    pub operations: Arc<dyn OperationStore>,
    pub clock: Arc<dyn Clock>,
    pub rng: Arc<dyn Rng>,
    pub net: Arc<dyn NetWatcher>,
    /// Destination supplémentaire des événements (en plus de `subscribe`).
    pub extra_sink: Option<Arc<dyn EventSink>>,
}

pub(crate) struct Deps {
    pub transport: Arc<dyn Transport>,
    pub vault: Arc<dyn Vault>,
    pub servers: Arc<dyn ServerStore>,
    pub snapshots: Arc<dyn SnapshotStore>,
    pub operations: Arc<dyn OperationStore>,
    pub clock: Arc<dyn Clock>,
    pub rng: Arc<dyn Rng>,
    pub net: Arc<dyn NetWatcher>,
    pub sink: Arc<dyn EventSink>,
    pub config: LinkConfig,
}

/// Ce que la tâche d'un serveur partage avec la façade.
pub(crate) struct Shared {
    id: ServerId,
    pub(crate) state: watch::Sender<StateInfo>,
    last_known: Mutex<Option<LastKnown>>,
    record: Mutex<ServerRecord>,
    pub(crate) restarts: AtomicU32,
}

impl Shared {
    fn new(
        record: ServerRecord,
        last_known: Option<LastKnown>,
        wall: WallTime,
        start: Start,
    ) -> Self {
        // L'état annoncé tout de suite, avant même que la tâche n'ait tourné.
        let initial = StateInfo {
            state: match start {
                Start::Connecting => LinkState::Reconnecting,
                Start::SignedOut | Start::Disconnected => LinkState::SessionExpired,
                Start::Recovered => LinkState::Offline,
            },
            blocked: None,
            reason: match start {
                Start::SignedOut => Some(Reason::NoSession),
                Start::Disconnected => Some(Reason::UserDisconnected),
                _ => None,
            },
            since: wall,
            last_contact_at: record.last_contact_at,
            next_retry_at: None,
            failed_attempts: 0,
        };
        Self {
            id: record.id.clone(),
            state: watch::channel(initial).0,
            last_known: Mutex::new(last_known),
            record: Mutex::new(record),
            restarts: AtomicU32::new(0),
        }
    }

    pub(crate) fn id(&self) -> ServerId {
        self.id.clone()
    }

    pub(crate) fn record(&self) -> ServerRecord {
        self.record
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub(crate) fn set_record(&self, record: ServerRecord) {
        *self.record.lock().unwrap_or_else(PoisonError::into_inner) = record;
    }

    /// Où et comment parler à ce serveur : toujours épinglé sur l'empreinte confirmée.
    pub(crate) fn target(&self) -> Target {
        let record = self.record();
        Target {
            host: record.host,
            port: record.port,
            pin: Pin::Pinned(record.fingerprint),
        }
    }

    pub(crate) fn last_known(&self) -> Option<LastKnown> {
        self.last_known
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub(crate) fn set_last_known(&self, view: LastKnown) {
        *self
            .last_known
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(view);
    }
}

struct Handle {
    commands: mpsc::Sender<Command>,
    shared: Arc<Shared>,
    join: JoinHandle<()>,
}

#[derive(Default)]
pub(crate) struct Registry {
    servers: Mutex<HashMap<ServerId, Handle>>,
}

impl Registry {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<ServerId, Handle>> {
        self.servers.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn get(&self, id: &ServerId) -> Option<(mpsc::Sender<Command>, Arc<Shared>)> {
        self.lock()
            .get(id)
            .map(|handle| (handle.commands.clone(), handle.shared.clone()))
    }

    /// Envoie une commande à tous les serveurs, sans jamais attendre.
    pub(crate) fn broadcast(&self, make: impl Fn() -> Command) {
        for handle in self.lock().values() {
            let _ = handle.commands.try_send(make());
        }
    }
}

struct Inner {
    deps: Arc<Deps>,
    registry: Arc<Registry>,
    fanout: Arc<Fanout>,
    watchers: Mutex<Vec<JoinHandle<()>>>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        let watchers = self
            .watchers
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner);
        for watcher in watchers.drain(..) {
            watcher.abort();
        }
    }
}

/// Un serveur à ajouter : l'empreinte est celle que l'utilisateur vient de confirmer (BR-CONN-001).
#[derive(Debug, Clone)]
pub struct NewServer {
    pub name: String,
    pub color: String,
    pub host: String,
    pub port: u16,
    pub fingerprint: Fingerprint,
    pub mac_addresses: Vec<String>,
}

/// Résultat de la première prise de contact.
#[derive(Debug, Clone, PartialEq)]
pub struct ProbeResult {
    /// Empreinte du certificat présenté : à faire confirmer par l'utilisateur.
    pub fingerprint: Fingerprint,
    pub hello: HelloResponse,
    pub compatibility: Compatibility,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginInfo {
    pub account: AccountInfo,
    pub expires_at: String,
}

/// Une action à envoyer au serveur. Le chemin est relatif à `/api/v1` (par exemple
/// `/accounts`) ; la clé d'opération est posée par la bibliothèque.
/// Le corps peut porter un mot de passe : `Debug` écrit à la main, sans le corps.
#[derive(Clone, PartialEq)]
pub struct ActionRequest {
    pub method: Method,
    pub path: String,
    pub body: Option<Value>,
}

impl std::fmt::Debug for ActionRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActionRequest")
            .field("method", &self.method)
            .field("path", &self.path)
            .field("body", &self.body.as_ref().map(|_| "***"))
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ActionOutcome {
    /// L'agent a répondu (succès ou refus : c'est `status` qui le dit).
    Completed {
        status: u16,
        body: Value,
        /// Premier résultat rendu sans ré-exécution.
        replayed: bool,
    },
    /// Le lien est tombé avant la réponse : on ne sait pas. L'issue (« fait pendant la
    /// coupure », « non exécuté », « résultat inconnu ») arrive plus tard par
    /// `Event::Operation`. L'action n'est jamais rejouée.
    ResultUnknown { id: OperationId },
}

#[derive(Clone)]
pub struct LinkManager {
    inner: Arc<Inner>,
}

impl LinkManager {
    /// Assemblage de production : transport réel, carnet et dernières vues dans `data_dir`,
    /// horloge et réseau du système. Le coffre est fourni par l'application.
    pub async fn open(
        data_dir: &Path,
        vault: Arc<dyn Vault>,
        client_name: &str,
        config: LinkConfig,
    ) -> Result<Self, LinkError> {
        let transport = HttpTransport::new(HttpTransportConfig {
            client_name: client_name.to_owned(),
            request_timeout: config.request_timeout,
            ..HttpTransportConfig::default()
        });
        Self::start(
            Ports {
                transport: Arc::new(transport),
                vault,
                servers: Arc::new(FileServerStore::new(data_dir.join("servers.json"))),
                snapshots: Arc::new(FileSnapshotStore::new(data_dir.join("snapshots"))),
                operations: Arc::new(FileOperationStore::new(data_dir.join("operations"))),
                clock: Arc::new(SystemClock::new()),
                rng: Arc::new(OsRng::default()),
                net: Arc::new(SystemNetWatcher),
                extra_sink: None,
            },
            config,
        )
        .await
    }

    /// Charge le carnet et lance la tâche de chaque serveur et les veilleurs.
    pub async fn start(ports: Ports, config: LinkConfig) -> Result<Self, LinkError> {
        let fanout = Arc::new(Fanout::new(config.event_capacity, ports.extra_sink));
        let deps = Arc::new(Deps {
            transport: ports.transport,
            vault: ports.vault,
            servers: ports.servers,
            snapshots: ports.snapshots,
            operations: ports.operations,
            clock: ports.clock,
            rng: ports.rng,
            net: ports.net,
            sink: fanout.clone(),
            config,
        });
        let registry = Arc::new(Registry::default());
        let records = deps
            .servers
            .list()
            .await
            .map_err(|e| LinkError::Store(e.0))?;
        for record in records {
            let last_known = deps.snapshots.load(&record.id).await.ok().flatten();
            let start = initial_start(&deps, &record.id, record.signed_out);
            spawn_server(&deps, &registry, record, last_known, start);
        }
        let watchers = watchers::spawn(deps.clone(), Arc::downgrade(&registry));
        Ok(Self {
            inner: Arc::new(Inner {
                deps,
                registry,
                fanout,
                watchers: Mutex::new(watchers),
            }),
        })
    }

    /// Flux des événements : états du lien, mesures, issues d'opérations, fins de session.
    pub fn subscribe(&self) -> EventStream {
        self.inner.fanout.subscribe()
    }

    fn handle(&self, id: &ServerId) -> Result<(mpsc::Sender<Command>, Arc<Shared>), LinkError> {
        self.inner.registry.get(id).ok_or(LinkError::UnknownServer)
    }

    /// Première prise de contact : l'empreinte du certificat et l'identité de l'agent, sans
    /// authentification (BR-CONN-001, 011). L'utilisateur confirme l'empreinte avant `add_server`.
    pub async fn probe(&self, host: &str, port: u16) -> Result<ProbeResult, LinkError> {
        validate_address(host, port)?;
        let target = Target {
            host: host.to_owned(),
            port,
            pin: Pin::Probe,
        };
        let deps = &self.inner.deps;
        let probed = timeout(deps.config.request_timeout, deps.transport.hello(&target))
            .await
            .map_err(|_| LinkError::Timeout)??;
        Ok(ProbeResult {
            compatibility: compat::check(probed.hello.api),
            fingerprint: probed.fingerprint,
            hello: probed.hello,
        })
    }

    /// Enregistre un serveur dont l'empreinte a été confirmée. Il reste « Session expirée »
    /// (en attente de connexion) jusqu'à `login`.
    pub async fn add_server(&self, new: NewServer) -> Result<ServerId, LinkError> {
        validate_address(&new.host, new.port)?;
        let name = new.name.trim();
        if name.is_empty() || name.chars().count() > 64 {
            return Err(LinkError::InvalidInput("nom du serveur"));
        }
        let duplicate = self.servers().iter().any(|existing| {
            existing.host.eq_ignore_ascii_case(&new.host) && existing.port == new.port
        });
        if duplicate {
            return Err(LinkError::AlreadyExists);
        }
        let id = ServerId::parse(&ulid::Ulid::generate().to_string())
            .map_err(|_| LinkError::Protocol("identifiant".into()))?;
        let record = ServerRecord {
            id: id.clone(),
            name: name.to_owned(),
            color: new.color,
            host: new.host,
            port: new.port,
            fingerprint: new.fingerprint,
            username: String::new(),
            remember: false,
            mac_addresses: new.mac_addresses,
            last_contact_at: None,
            signed_out: false,
        };
        let deps = &self.inner.deps;
        deps.servers
            .save(&record)
            .await
            .map_err(|e| LinkError::Store(e.0))?;
        spawn_server(deps, &self.inner.registry, record, None, Start::SignedOut);
        Ok(id)
    }

    /// Ouvre une session (`POST /sessions`) sur la connexion épinglée, mémorise le jeton au coffre
    /// (et le mot de passe si `remember`), puis lance le flux. Le mot de passe n'est envoyé qu'à
    /// un serveur dont l'empreinte est celle confirmée (BR-CONN-011).
    pub async fn login(
        &self,
        id: &ServerId,
        username: &str,
        password: Secret,
        remember: bool,
    ) -> Result<LoginInfo, LinkError> {
        let (commands, shared) = self.handle(id)?;
        let username = username.trim();
        if username.is_empty() || password.is_empty() {
            return Err(LinkError::InvalidInput("identifiant ou mot de passe vide"));
        }
        let deps = &self.inner.deps;
        let target = shared.target();
        let limit = deps.config.request_timeout;
        let probed = timeout(limit, deps.transport.hello(&target))
            .await
            .map_err(|_| LinkError::Timeout)??;
        let compatibility = compat::check(probed.hello.api);
        if compatibility != Compatibility::Compatible {
            return Err(LinkError::Incompatible(compatibility));
        }
        let request = attempt::login_request(username, &password);
        let outcome = timeout(limit, deps.transport.login(&target, &request)).await;
        attempt::wipe(request);
        let response = match outcome {
            Err(_) => return Err(LinkError::Timeout),
            Ok(Err(error)) => {
                let _ = commands.send(Command::LoginRefused).await;
                return Err(error.into());
            }
            Ok(Ok(response)) => response,
        };
        let mut response = response;
        let vault_error = |e: crate::ports::vault::VaultError| LinkError::Vault(e.0);
        deps.vault
            .put(
                id,
                SecretKind::Token,
                &Secret::new(std::mem::take(&mut response.token)),
            )
            .map_err(vault_error)?;
        if remember {
            deps.vault
                .put(id, SecretKind::Password, &password)
                .map_err(vault_error)?;
        } else {
            deps.vault
                .delete(id, SecretKind::Password)
                .map_err(vault_error)?;
        }
        let mut record = shared.record();
        // Un autre compte : les clés d'opération de l'ancien ne disent plus rien.
        let account_changed =
            !record.username.is_empty() && !record.username.eq_ignore_ascii_case(username);
        record.username = username.to_owned();
        record.remember = remember;
        record.signed_out = false;
        shared.set_record(record.clone());
        deps.servers
            .save(&record)
            .await
            .map_err(|e| LinkError::Store(e.0))?;
        commands
            .send(Command::LoggedIn { account_changed })
            .await
            .map_err(|_| LinkError::Stopped)?;
        Ok(LoginInfo {
            account: response.account.clone(),
            expires_at: response.expires_at.clone(),
        })
    }

    /// Déconnexion volontaire (BR-CONN-016) : ferme la session côté serveur (si possible) et
    /// efface le jeton. Le mot de passe mémorisé et la case « se souvenir » restent : seul
    /// `remove_server` efface tout. Aucune reconnexion automatique ensuite, pas même au
    /// prochain démarrage.
    pub async fn logout(&self, id: &ServerId) -> Result<(), LinkError> {
        let (commands, shared) = self.handle(id)?;
        let deps = &self.inner.deps;
        if let Ok(Some(token)) = deps.vault.get(id, SecretKind::Token) {
            // Au mieux : un serveur injoignable n'empêche pas de se déconnecter.
            let _ = timeout(
                deps.config.request_timeout,
                deps.transport.logout(&shared.target(), &token),
            )
            .await;
        }
        deps.vault
            .delete(id, SecretKind::Token)
            .map_err(|e| LinkError::Vault(e.0))?;
        let mut record = shared.record();
        record.signed_out = true;
        shared.set_record(record.clone());
        deps.servers
            .save(&record)
            .await
            .map_err(|e| LinkError::Store(e.0))?;
        commands
            .send(Command::LoggedOut)
            .await
            .map_err(|_| LinkError::Stopped)
    }

    /// Retire le serveur : tâche arrêtée, secrets, carnet et dernière vue effacés.
    pub async fn remove_server(&self, id: &ServerId) -> Result<(), LinkError> {
        let handle = self
            .inner
            .registry
            .lock()
            .remove(id)
            .ok_or(LinkError::UnknownServer)?;
        let (done, stopped) = oneshot::channel();
        if handle
            .commands
            .send(Command::Shutdown { done })
            .await
            .is_ok()
        {
            let _ = timeout(Duration::from_secs(2), stopped).await;
        }
        handle.join.abort();
        let deps = &self.inner.deps;
        let _ = deps.vault.delete(id, SecretKind::Token);
        let _ = deps.vault.delete(id, SecretKind::Password);
        let _ = deps.snapshots.remove(id).await;
        let _ = deps.operations.remove(id).await;
        deps.servers
            .remove(id)
            .await
            .map_err(|e| LinkError::Store(e.0))
    }

    /// État du lien d'un serveur, à l'instant.
    pub fn state(&self, id: &ServerId) -> Result<StateInfo, LinkError> {
        let (_, shared) = self.handle(id)?;
        let info = *shared.state.borrow();
        Ok(info)
    }

    /// Les serveurs du carnet, dans un ordre quelconque.
    pub fn servers(&self) -> Vec<ServerRecord> {
        self.inner
            .registry
            .lock()
            .values()
            .map(|handle| handle.shared.record())
            .collect()
    }

    /// État de chaque serveur : chacun a le sien, indépendant (BR-RESIL-020).
    pub fn states(&self) -> Vec<(ServerId, StateInfo)> {
        self.inner
            .registry
            .lock()
            .iter()
            .map(|(id, handle)| (id.clone(), *handle.shared.state.borrow()))
            .collect()
    }

    /// « Réessayer maintenant » : une tentative tout de suite, sans attendre le délai en cours.
    pub fn retry_now(&self, id: &ServerId) -> Result<(), LinkError> {
        let (commands, _) = self.handle(id)?;
        commands
            .try_send(Command::RetryNow)
            .map_err(|_| LinkError::TaskRestarted)
    }

    /// L'utilisateur accepte la nouvelle empreinte du serveur (BR-CONN-003) : elle remplace
    /// l'ancienne au carnet et une tentative repart.
    pub async fn accept_fingerprint(
        &self,
        id: &ServerId,
        fingerprint: Fingerprint,
    ) -> Result<(), LinkError> {
        let (commands, shared) = self.handle(id)?;
        let mut record = shared.record();
        record.fingerprint = fingerprint;
        shared.set_record(record.clone());
        self.inner
            .deps
            .servers
            .save(&record)
            .await
            .map_err(|e| LinkError::Store(e.0))?;
        commands
            .send(Command::FingerprintAccepted)
            .await
            .map_err(|_| LinkError::Stopped)
    }

    /// Envoie une action au serveur. Hors « Connecté », `NotConnected` sans rien envoyer. Si le
    /// lien tombe avant la réponse, ou si le délai de la requête passe, `ResultUnknown` avec la
    /// clé d'opération : l'action n'est jamais rejouée, l'issue arrive par `Event::Operation`.
    /// Si l'appelant abandonne l'attente, l'opération reste suivie de la même façon.
    pub async fn execute(
        &self,
        id: &ServerId,
        action: ActionRequest,
    ) -> Result<ActionOutcome, LinkError> {
        let (commands, _) = self.handle(id)?;
        let key = OperationId::parse(&ulid::Ulid::generate().to_string())
            .map_err(|_| LinkError::Protocol("clé d'opération".into()))?;
        let (reply, answer) = oneshot::channel();
        commands
            .send(Command::Execute {
                key: key.clone(),
                request: action,
                reply,
            })
            .await
            .map_err(|_| LinkError::Stopped)?;
        // Si cet appel est abandonné avant la réponse, l'opération reste suivie.
        let mut guard = AbandonGuard {
            commands: commands.clone(),
            id: Some(key),
        };
        let result = match answer.await {
            Ok(result) => result,
            Err(_) => Err(LinkError::TaskRestarted),
        };
        guard.id = None;
        result
    }

    /// Dernière vue connue du serveur (en mémoire, sinon la dernière sauvegardée).
    pub async fn last_known(&self, id: &ServerId) -> Result<Option<LastKnown>, LinkError> {
        let (_, shared) = self.handle(id)?;
        if let Some(view) = shared.last_known() {
            return Ok(Some(view));
        }
        self.inner
            .deps
            .snapshots
            .load(id)
            .await
            .map_err(|e| LinkError::Store(e.0))
    }

    /// Combien de fois la tâche de ce serveur a dû être relancée après un incident interne.
    pub fn task_restarts(&self, id: &ServerId) -> Result<u32, LinkError> {
        let (_, shared) = self.handle(id)?;
        Ok(shared.restarts.load(Ordering::SeqCst))
    }

    /// Arrête tout : tâches, veilleurs. Les appels ensuite rendent `UnknownServer`.
    pub async fn shutdown(&self) {
        let handles: Vec<Handle> = self.inner.registry.lock().drain().map(|(_, h)| h).collect();
        for handle in handles {
            let (done, stopped) = oneshot::channel();
            if handle
                .commands
                .send(Command::Shutdown { done })
                .await
                .is_ok()
            {
                let _ = timeout(Duration::from_secs(2), stopped).await;
            }
            handle.join.abort();
        }
        let mut watchers = self
            .inner
            .watchers
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        for watcher in watchers.drain(..) {
            watcher.abort();
        }
    }
}

/// Prévient la tâche du serveur quand un appel d'`execute` est abandonné avant sa réponse.
struct AbandonGuard {
    commands: mpsc::Sender<Command>,
    id: Option<OperationId>,
}

impl Drop for AbandonGuard {
    fn drop(&mut self) {
        if let Some(id) = self.id.take() {
            let _ = self.commands.try_send(Command::Abandon { id });
        }
    }
}

/// Une session mémorisée (jeton ou mot de passe) : on se connecte au démarrage ; sinon on attend.
fn initial_start(deps: &Deps, id: &ServerId, signed_out: bool) -> Start {
    if signed_out {
        return Start::Disconnected;
    }
    let has_token = matches!(deps.vault.get(id, SecretKind::Token), Ok(Some(_)));
    let has_password = matches!(deps.vault.get(id, SecretKind::Password), Ok(Some(_)));
    if has_token || has_password {
        Start::Connecting
    } else {
        Start::SignedOut
    }
}

fn spawn_server(
    deps: &Arc<Deps>,
    registry: &Registry,
    record: ServerRecord,
    last_known: Option<LastKnown>,
    start: Start,
) {
    let id = record.id.clone();
    let shared = Arc::new(Shared::new(record, last_known, deps.clock.wall(), start));
    let (commands, join) = task::spawn(deps.clone(), shared.clone(), start);
    registry.lock().insert(
        id,
        Handle {
            commands,
            shared,
            join,
        },
    );
}

fn validate_address(host: &str, port: u16) -> Result<(), LinkError> {
    let bad_char = |c: char| c.is_whitespace() || matches!(c, '/' | '\\' | '?' | '#' | '@');
    if host.is_empty() || host.len() > 253 || host.chars().any(bad_char) {
        return Err(LinkError::InvalidInput("adresse du serveur"));
    }
    if port == 0 {
        return Err(LinkError::InvalidInput("port du serveur"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::ports::transport::ApiRequest;
    use hearth_proto::api::sessions::{LoginRequest, LoginResponse};
    use hearth_proto::stream::ClientMessage;

    const PASSWORD: &str = "Tr0ub4dor&3-secret";

    /// Aucun type qui transporte un mot de passe, un corps de requête ou un jeton ne le montre
    /// dans un `Debug` : un journal ou un message d'erreur ne peut pas le fuir.
    #[test]
    fn debug_never_shows_a_password_a_token_or_a_request_body() {
        let body = json!({ "current": PASSWORD, "password": "New-Secret-12" });
        let action = ActionRequest {
            method: Method::Put,
            path: "/me/password".into(),
            body: Some(body.clone()),
        };
        let api = ApiRequest {
            method: Method::Put,
            path: "/me/password".into(),
            body: Some(body),
            idempotency_key: Some("01J9".into()),
        };
        let login = LoginRequest {
            username: "marie".into(),
            password: PASSWORD.into(),
        };
        let token = "ab".repeat(32);
        let message = ClientMessage::Auth {
            token: token.clone(),
        };
        let response = LoginResponse {
            token: token.clone(),
            expires_at: "x".into(),
            account: hearth_proto::api::accounts::AccountInfo {
                id: "A".into(),
                username: "marie".into(),
                role: hearth_proto::api::accounts::RoleName::Admin,
            },
        };
        let secret = Secret::new(PASSWORD);
        let text = format!(
            "{action:?} {api:?} {login:?} {message:?} {response:?} {secret:?} {:?}",
            Some(&action)
        );
        for hidden in [PASSWORD, "New-Secret-12", token.as_str()] {
            assert!(!text.contains(hidden), "{hidden} apparaît dans : {text}");
        }
        // Ce qui aide à diagnostiquer reste visible.
        assert!(text.contains("/me/password"));
    }
}

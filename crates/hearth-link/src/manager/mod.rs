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
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use hearth_proto::api::accounts::AccountInfo;
use hearth_proto::api::hello::HelloResponse;
use hearth_proto::api::sessions::LoginResponse;
use hearth_proto::fingerprint::Fingerprint;
use serde_json::Value;
use tokio::sync::{OwnedMutexGuard, mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tokio::time::timeout;

pub use events::EventStream;
use events::Fanout;
use task::Command;

use crate::adapters::{
    FileOperationStore, FileServerStore, FileSnapshotStore, HttpTransport, HttpTransportConfig,
    OsRng, SystemClock, SystemNetWatcher,
};
use crate::domain::book;
use crate::domain::compat::{self, Compatibility};
use crate::domain::event::StateInfo;
use crate::domain::pending_ops::OperationId;
use crate::domain::secret::Secret;
use crate::domain::server::{LastKnown, ServerId, ServerRecord};
use crate::domain::state::{LinkState, Reason, Start, Thresholds};
use crate::domain::time::WallTime;
use crate::error::{InputField, LinkError};
use crate::ports::transport::{Method, Pin, Target, TransportError};
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
    /// Délai accordé à l'écriture du suivi d'une action, avant son envoi. Passé ce délai (ou si
    /// l'écriture échoue), l'action n'est pas lancée.
    pub persist_timeout: Duration,
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
            persist_timeout: Duration::from_secs(2),
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
    /// Écrivains d'un même serveur (connexion, déconnexion, modification, oubli, suppression) :
    /// un à la fois, pour qu'une suppression gagne toujours.
    writers: Arc<tokio::sync::Mutex<()>>,
    /// Le serveur est retiré du carnet : plus aucune écriture ne le concerne.
    removed: AtomicBool,
    /// Empreinte présentée par le serveur et en attente de décision (BR-CONN-003) : posée par la
    /// tâche AVANT d'annoncer le changement, levée quand le lien repart ou à l'acceptation.
    pub(crate) presented: Mutex<Option<Fingerprint>>,
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
            writers: Arc::new(tokio::sync::Mutex::new(())),
            removed: AtomicBool::new(false),
            presented: Mutex::new(None),
        }
    }

    pub(crate) fn set_presented(&self, fingerprint: Fingerprint) {
        *self
            .presented
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(fingerprint);
    }

    pub(crate) fn clear_presented(&self) {
        *self
            .presented
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = None;
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
    /// Contrôles du carnet (nom, adresse) et enregistrement, d'un seul tenant : deux ajouts en même
    /// temps ne passent pas tous deux les contrôles.
    book: tokio::sync::Mutex<()>,
}

/// Un serveur à qui on peut écrire : son verrou d'écriture est pris et il est encore au carnet.
struct Locked {
    commands: mpsc::Sender<Command>,
    shared: Arc<Shared>,
    _guard: OwnedMutexGuard<()>,
}

/// Échec de l'ouverture d'une session : `refused` quand le serveur a refusé la connexion elle-même
/// (et non la prise de contact).
struct AuthFailure {
    error: LinkError,
    refused: bool,
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

/// Modification d'un serveur du carnet (nom, couleur, adresse). Si l'adresse change,
/// `fingerprint` est l'empreinte relue (`probe`) et confirmée de nouveau par l'utilisateur
/// (BR-CONN-009) : sans elle, la modification est refusée.
#[derive(Debug, Clone)]
pub struct ServerUpdate {
    pub name: String,
    pub color: String,
    pub host: String,
    pub port: u16,
    pub fingerprint: Option<Fingerprint>,
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

/// Réponse d'une lecture (`LinkManager::fetch`).
#[derive(Debug, Clone, PartialEq)]
pub struct FetchResponse {
    pub status: u16,
    /// `Value::Null` si la réponse n'a pas de corps.
    pub body: Value,
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
        Self::open_with_sink(data_dir, vault, client_name, config, None).await
    }

    /// Comme `open`, avec une destination d'événements branchée AVANT le lancement des tâches :
    /// elle reçoit tout, y compris ce qui est annoncé à l'ouverture (suivis perdus, empreinte
    /// changée constatée par la première tentative), qu'un `subscribe` fait après coup manquerait.
    pub async fn open_with_sink(
        data_dir: &Path,
        vault: Arc<dyn Vault>,
        client_name: &str,
        config: LinkConfig,
        extra_sink: Option<Arc<dyn EventSink>>,
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
                extra_sink,
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
        for mut record in records {
            // FIX:01M46G800Z47XQ8R64G2MDC4NP — « se souvenir » sans mot de passe au coffre (application tuée
            // pendant l'ajout) : le carnet ne le promet plus (docs/bugs/FIX-01M46G800Z47XQ8R64G2MDC4NP.md)
            if record.remember
                && matches!(deps.vault.get(&record.id, SecretKind::Password), Ok(None))
            {
                record.remember = false;
                if let Err(error) = deps.servers.save(&record).await {
                    tracing::warn!(%error, "carnet : « se souvenir » non corrigé sur disque");
                }
            }
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
                book: tokio::sync::Mutex::new(()),
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
        book::check_address(host, port)?;
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

    /// Prend le verrou d'écriture d'un serveur et vérifie qu'il est encore au carnet : une
    /// suppression qui a eu lieu entre-temps gagne, rien n'est écrit pour un serveur supprimé.
    async fn lock(&self, id: &ServerId) -> Result<Locked, LinkError> {
        let (commands, shared) = self.handle(id)?;
        let guard = shared.writers.clone().lock_owned().await;
        if shared.removed.load(Ordering::SeqCst) {
            return Err(LinkError::UnknownServer);
        }
        Ok(Locked {
            commands,
            shared,
            _guard: guard,
        })
    }

    /// Les contrôles du carnet pour un serveur à ajouter : adresse déjà enregistrée, nom unique.
    fn check_new(&self, new: &NewServer) -> Result<String, LinkError> {
        let known = self.servers();
        let duplicate = known.iter().any(|existing| {
            existing.host.eq_ignore_ascii_case(&new.host) && existing.port == new.port
        });
        if duplicate {
            return Err(LinkError::AlreadyExists);
        }
        Ok(book::check_name(&new.name, &known)?)
    }

    fn new_record(
        new: NewServer,
        name: String,
        macs: Vec<String>,
        username: String,
        remember: bool,
        role: Option<hearth_proto::api::accounts::RoleName>,
    ) -> Result<ServerRecord, LinkError> {
        let id = ServerId::parse(&ulid::Ulid::generate().to_string())
            .map_err(|_| LinkError::Protocol("identifiant".into()))?;
        Ok(ServerRecord {
            id,
            name,
            color: new.color,
            host: new.host,
            port: new.port,
            fingerprint: new.fingerprint,
            username,
            remember,
            mac_addresses: macs,
            last_contact_at: None,
            signed_out: false,
            role,
        })
    }

    /// Enregistre un serveur dont l'empreinte a été confirmée. Il reste « Session expirée »
    /// (en attente de connexion) jusqu'à `login`. L'application, elle, n'enregistre un serveur
    /// qu'à sa première connexion réussie : `add_and_login`.
    pub async fn add_server(&self, new: NewServer) -> Result<ServerId, LinkError> {
        book::check_address(&new.host, new.port)?;
        let macs = book::check_mac_addresses(&new.mac_addresses);
        let _book = self.inner.book.lock().await;
        let name = self.check_new(&new)?;
        let record = Self::new_record(new, name, macs, String::new(), false, None)?;
        let id = record.id.clone();
        let deps = &self.inner.deps;
        deps.servers
            .save(&record)
            .await
            .map_err(|e| LinkError::Store(e.0))?;
        spawn_server(deps, &self.inner.registry, record, None, Start::SignedOut);
        Ok(id)
    }

    /// Prise de contact épinglée puis ouverture d'une session : la prise de contact (version
    /// compatible), puis `POST /sessions`. Rien n'est écrit ici.
    async fn authenticate(
        &self,
        target: &Target,
        username: &str,
        password: &Secret,
    ) -> Result<LoginResponse, AuthFailure> {
        let deps = &self.inner.deps;
        let limit = deps.config.request_timeout;
        let hello = |error: LinkError| AuthFailure {
            error,
            refused: false,
        };
        let probed = timeout(limit, deps.transport.hello(target))
            .await
            .map_err(|_| hello(LinkError::Timeout))?
            .map_err(|e| hello(e.into()))?;
        let compatibility = compat::check(probed.hello.api);
        if compatibility != Compatibility::Compatible {
            return Err(hello(LinkError::Incompatible(compatibility)));
        }
        let request = attempt::login_request(username, password);
        let outcome = timeout(limit, deps.transport.login(target, &request)).await;
        attempt::wipe(request);
        match outcome {
            Err(_) => Err(hello(LinkError::Timeout)),
            Ok(Err(error)) => Err(AuthFailure {
                error: error.into(),
                refused: true,
            }),
            Ok(Ok(response)) => Ok(response),
        }
    }

    /// Ferme au mieux une session qu'on vient d'obtenir et qu'on n'utilisera pas.
    async fn close_session(&self, target: &Target, token: &str) {
        let deps = &self.inner.deps;
        let _ = timeout(
            deps.config.request_timeout,
            deps.transport.logout(target, &Secret::new(token)),
        )
        .await;
    }

    /// Première connexion d'un serveur : contacte l'adresse épinglée sur l'empreinte que
    /// l'utilisateur vient de confirmer, ouvre la session, et SEULEMENT si elle réussit enregistre le
    /// serveur (carnet, empreinte), ses secrets (coffre) et lance son lien. Un échec, un abandon ou une
    /// application tuée avant l'écriture ne laissent rien ; tuée pendant, elle laisse au pire un serveur sans session, visible et supprimable, jamais un secret orphelin (BR-CONN-002, 004).
    pub async fn add_and_login(
        &self,
        new: NewServer,
        username: &str,
        password: Secret,
        remember: bool,
    ) -> Result<(ServerId, LoginInfo), LinkError> {
        book::check_address(&new.host, new.port)?;
        let macs = book::check_mac_addresses(&new.mac_addresses);
        let username = book::check_username(username)?;
        if password.is_empty() {
            return Err(LinkError::InvalidInput(InputField::Credentials));
        }
        // Mêmes refus qu'à l'enregistrement, avant de contacter le serveur.
        self.check_new(&new)?;
        let target = Target {
            host: new.host.clone(),
            port: new.port,
            pin: Pin::Pinned(new.fingerprint),
        };
        let mut response = self
            .authenticate(&target, &username, &password)
            .await
            .map_err(|failure| failure.error)?;
        let _book = self.inner.book.lock().await;
        // Un autre ajout a pu passer pendant l'attente du réseau.
        let name = match self.check_new(&new) {
            Ok(name) => name,
            Err(error) => {
                self.close_session(&target, &response.token).await;
                return Err(error);
            }
        };
        let record = Self::new_record(
            new,
            name,
            macs,
            username,
            remember,
            Some(response.account.role),
        )?;
        let id = record.id.clone();
        let deps = &self.inner.deps;
        // Le carnet d'abord, les secrets ensuite : une application tuée entre les deux laisse au pire
        // un serveur sans session (visible, qu'on supprime), jamais un secret au coffre pour un
        // identifiant que plus rien ne connaît.
        if let Err(error) = deps.servers.save(&record).await {
            self.close_session(&target, &response.token).await;
            return Err(LinkError::Store(error.0));
        }
        let token = Secret::new(std::mem::take(&mut response.token));
        let stored = deps
            .vault
            .put(&id, SecretKind::Token, &token)
            .and_then(|()| {
                if remember {
                    deps.vault.put(&id, SecretKind::Password, &password)
                } else {
                    Ok(())
                }
            });
        if let Err(error) = stored {
            let _ = deps.vault.delete(&id, SecretKind::Token);
            let _ = deps.vault.delete(&id, SecretKind::Password);
            let _ = deps.servers.remove(&id).await;
            let _ = timeout(
                deps.config.request_timeout,
                deps.transport.logout(&target, &token),
            )
            .await;
            return Err(LinkError::Vault(error.0));
        }
        spawn_server(deps, &self.inner.registry, record, None, Start::SignedOut);
        let (commands, _) = self.handle(&id)?;
        commands
            .send(Command::LoggedIn {
                account_changed: false,
            })
            .await
            .map_err(|_| LinkError::Stopped)?;
        Ok((
            id,
            LoginInfo {
                account: response.account.clone(),
                expires_at: response.expires_at.clone(),
            },
        ))
    }

    /// Modifie un serveur du carnet. Les identifiants mémorisés sont conservés. Si l'adresse
    /// change (BR-CONN-009), la nouvelle empreinte confirmée remplace l'ancienne et le lien repart
    /// vers la nouvelle adresse ; un nom identique à celui d'un autre serveur est refusé
    /// (BR-CONN-008).
    pub async fn update_server(
        &self,
        id: &ServerId,
        update: ServerUpdate,
    ) -> Result<ServerRecord, LinkError> {
        book::check_address(&update.host, update.port)?;
        let locked = self.lock(id).await?;
        let _book = self.inner.book.lock().await;
        let known = self.servers();
        let others: Vec<&ServerRecord> = known.iter().filter(|other| &other.id != id).collect();
        if others
            .iter()
            .any(|other| other.host.eq_ignore_ascii_case(&update.host) && other.port == update.port)
        {
            return Err(LinkError::AlreadyExists);
        }
        let name = book::check_name(&update.name, others.iter().copied())?;
        let mut record = locked.shared.record();
        let moved = book::address_changed(&record, &update.host, update.port);
        if moved {
            record.fingerprint = update.fingerprint.ok_or(LinkError::VerificationRequired)?;
        }
        record.name = name;
        record.color = update.color;
        record.host = update.host;
        record.port = update.port;
        self.inner
            .deps
            .servers
            .save(&record)
            .await
            .map_err(|e| LinkError::Store(e.0))?;
        locked.shared.set_record(record.clone());
        if moved {
            // Autre adresse, autre identité possible : les clés d'opération ne disent plus rien ;
            // une tentative repart vers la nouvelle adresse, épinglée sur la nouvelle empreinte.
            locked
                .commands
                .send(Command::FingerprintAccepted)
                .await
                .map_err(|_| LinkError::Stopped)?;
        }
        Ok(record)
    }

    /// Ouvre une session (`POST /sessions`) sur la connexion épinglée, mémorise le jeton au coffre
    /// (et le mot de passe si `remember`), puis lance le flux. Le mot de passe n'est envoyé qu'à
    /// un serveur dont l'empreinte est celle confirmée (BR-CONN-011). Si le serveur est supprimé
    /// pendant l'échange avec le réseau, rien n'est écrit : la suppression gagne.
    pub async fn login(
        &self,
        id: &ServerId,
        username: &str,
        password: Secret,
        remember: bool,
    ) -> Result<LoginInfo, LinkError> {
        let (commands, shared) = self.handle(id)?;
        let username = book::check_username(username)?;
        if password.is_empty() {
            return Err(LinkError::InvalidInput(InputField::Credentials));
        }
        let deps = &self.inner.deps;
        let target = shared.target();
        let mut response = match self.authenticate(&target, &username, &password).await {
            Ok(response) => response,
            Err(failure) => {
                if failure.refused {
                    let _ = commands.send(Command::LoginRefused).await;
                }
                return Err(failure.error);
            }
        };
        // Le réseau a pu durer : le serveur est-il encore là ? Sinon, aucune écriture.
        let locked = self.lock(id).await?;
        let shared = &locked.shared;
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
            !record.username.is_empty() && !record.username.eq_ignore_ascii_case(&username);
        record.username = username;
        record.remember = remember;
        record.signed_out = false;
        record.role = Some(response.account.role);
        shared.set_record(record.clone());
        deps.servers
            .save(&record)
            .await
            .map_err(|e| LinkError::Store(e.0))?;
        locked
            .commands
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
        let deps = &self.inner.deps;
        // Sous verrou : arrêter la tâche (plus de reconnexion possible) et noter la déconnexion. Le
        // réseau, lui, se fait hors verrou : un serveur injoignable ne retient ni une suppression ni
        // une connexion pendant `request_timeout`.
        let (target, token) = {
            let locked = self.lock(id).await?;
            let mut record = locked.shared.record();
            record.signed_out = true;
            locked.shared.set_record(record);
            locked
                .commands
                .send(Command::LoggedOut)
                .await
                .map_err(|_| LinkError::Stopped)?;
            (
                locked.shared.target(),
                deps.vault.get(id, SecretKind::Token),
            )
        };
        if let Ok(Some(token)) = token {
            // Au mieux : un serveur injoignable n'empêche pas de se déconnecter.
            let _ = timeout(
                deps.config.request_timeout,
                deps.transport.logout(&target, &token),
            )
            .await;
        }
        // Le serveur a pu être supprimé pendant l'appel : alors rien à écrire.
        let locked = self.lock(id).await?;
        // FIX:01M46G7Z0ZP43T53M2F5KG4VKS — `login` remet « déconnecté » à faux sous ce même verrou en
        // rangeant son jeton : s'il est passé pendant l'appel réseau, le jeton du coffre est le sien
        // (docs/bugs/FIX-01M46G7Z0ZP43T53M2F5KG4VKS.md)
        if !locked.shared.record().signed_out {
            return Ok(());
        }
        deps.vault
            .delete(id, SecretKind::Token)
            .map_err(|e| LinkError::Vault(e.0))?;
        deps.servers
            .save(&locked.shared.record())
            .await
            .map_err(|e| LinkError::Store(e.0))
    }

    /// Oubli des identifiants : le mot de passe mémorisé est effacé du coffre et « se souvenir »
    /// repasse à faux. La session en cours n'est pas touchée ; à son expiration, l'utilisateur
    /// devra se reconnecter à la main (BR-CONN-004). Sans effet si rien n'était mémorisé.
    pub async fn forget_credentials(&self, id: &ServerId) -> Result<(), LinkError> {
        let locked = self.lock(id).await?;
        let deps = &self.inner.deps;
        deps.vault
            .delete(id, SecretKind::Password)
            .map_err(|e| LinkError::Vault(e.0))?;
        let mut record = locked.shared.record();
        if record.remember {
            record.remember = false;
            locked.shared.set_record(record.clone());
            deps.servers
                .save(&record)
                .await
                .map_err(|e| LinkError::Store(e.0))?;
        }
        Ok(())
    }

    /// Retire le serveur : tâche arrêtée, secrets, carnet et dernière vue effacés. La suppression
    /// gagne toujours : elle attend les écritures en cours de ce serveur, et plus aucune ne peut
    /// suivre (`removed`). Si le coffre refuse d'effacer un secret, rien n'est dit « supprimé » :
    /// le serveur est remis en service tel quel et l'erreur remonte (BR-CONN-010).
    pub async fn remove_server(&self, id: &ServerId) -> Result<(), LinkError> {
        let handle = self
            .inner
            .registry
            .lock()
            .remove(id)
            .ok_or(LinkError::UnknownServer)?;
        handle.shared.removed.store(true, Ordering::SeqCst);
        // Les écritures déjà commencées (connexion, déconnexion…) se terminent avant nous.
        let _writers = handle.shared.writers.clone().lock_owned().await;
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
        let token = deps.vault.delete(id, SecretKind::Token);
        let password = deps.vault.delete(id, SecretKind::Password);
        if let Err(error) = token.and(password) {
            tracing::error!(server = %id, %error, "secrets non effacés : serveur remis en service");
            let record = handle.shared.record();
            let last_known = handle.shared.last_known();
            let start = initial_start(deps, id, record.signed_out);
            spawn_server(deps, &self.inner.registry, record, last_known, start);
            return Err(LinkError::Vault(error.0));
        }
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
        let locked = self.lock(id).await?;
        // Seule l'empreinte que le serveur a présentée et que l'utilisateur a sous les yeux : refus
        // si elle diffère, ou si aucun changement d'empreinte n'attend de décision.
        {
            let mut presented = locked
                .shared
                .presented
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if *presented != Some(fingerprint) {
                return Err(LinkError::InvalidInput(InputField::Fingerprint));
            }
            *presented = None;
        }
        let mut record = locked.shared.record();
        record.fingerprint = fingerprint;
        locked.shared.set_record(record.clone());
        self.inner
            .deps
            .servers
            .save(&record)
            .await
            .map_err(|e| LinkError::Store(e.0))?;
        locked
            .commands
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

    /// Lit une ressource du serveur (`GET`, chemin relatif à `/api/v1`) : aucune clé d'opération,
    /// aucun suivi sur disque, rien à relire au retour du lien (une lecture sans effet n'a pas
    /// d'issue incertaine). Hors « Connecté », `NotConnected` sans rien envoyer. Toute réponse de
    /// l'agent, même un refus, est un `Ok` : c'est `status` qui le dit.
    pub async fn fetch(&self, id: &ServerId, path: &str) -> Result<FetchResponse, LinkError> {
        let (_, shared) = self.handle(id)?;
        if shared.state.borrow().state != LinkState::Connected {
            return Err(LinkError::NotConnected);
        }
        let token = self
            .inner
            .deps
            .vault
            .get(id, SecretKind::Token)
            .map_err(|e| LinkError::Vault(e.0))?
            .ok_or(LinkError::NotConnected)?;
        let request = crate::ports::transport::ApiRequest {
            method: Method::Get,
            path: path.to_owned(),
            body: None,
            idempotency_key: None,
        };
        let deps = &self.inner.deps;
        let target = shared.target();
        let call = timeout(
            deps.config.request_timeout,
            deps.transport.request(&target, &token, &request),
        );
        let response =
            attempt::guarded(async { call.await.unwrap_or(Err(TransportError::Timeout)) }).await?;
        Ok(FetchResponse {
            status: response.status,
            body: response.body,
        })
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

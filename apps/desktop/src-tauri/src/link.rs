//! Le pont réel entre l'interface et `hearth-link` : `LinkRuntime` enveloppe le `LinkManager`
//! (commandes) et un relais (`forward`) qui traduit ses événements en événements pour
//! l'interface. Aucune règle ici : elles sont dans `hearth-link` (`domain/`). Rien de ce module
//! ne dépend de Tauri : le relais écrit dans un [`UiSink`] (la fenêtre en production, un
//! enregistreur dans les tests).
//!
//! Un événement n'est qu'un signal de changement : ce qui doit survivre à une interface qui arrive
//! après lui (alerte d'empreinte en attente, suivis perdus, issues d'actions annoncées avant le
//! chargement de la fenêtre) est un ÉTAT tenu ici par [`PendingBook`], branché sur la liaison avant
//! le lancement de ses tâches et relu par l'interface à l'abonnement (`list_fingerprint_alerts`,
//! `take_link_notices`, `take_unread_operations`).

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use hearth_link::domain::compat::Compatibility;
use hearth_link::domain::event::Event;
use hearth_link::domain::secret::Secret;
use hearth_link::domain::server::ServerId;
use hearth_link::domain::state::{Blocked, LinkState};
use hearth_link::ports::{EventSink, Vault};
use hearth_link::{EventStream, LinkConfig, LinkError, LinkManager, NewServer, ServerUpdate};
use hearth_proto::fingerprint::Fingerprint;
use hearth_proto::product::DEFAULT_PORT;
use serde::Serialize;

use crate::dashboard::{DashBook, SnapshotEvent, events as dash_events, snapshot as dash_snapshot};
use crate::link_dto::{
    FingerprintEvent, LinkFailure, LinkStateDto, NoticeEvent, NoticeKind, OperationEventDto,
    OutcomeDto, ProbeDto, ServerDto, ServersEvent, StateBook, events, parse_fingerprint,
    servers_list,
};

/// Ce que la coquille fait des changements d'état du lien en dehors de la fenêtre : notifications
/// système et icône de la zone de notification (BR-RESIL-015, 016). Appelé à chaque événement
/// d'état, dans l'ordre ; ne doit jamais bloquer.
pub trait StateObserver: Send + Sync {
    fn on_state(&self, server: &str, name: &str, state: LinkState, failed_attempts: u32);
    fn on_removed(&self, server: &str);
}

/// Où partent les événements pour l'interface.
pub trait UiSink: Send + Sync {
    fn emit(&self, event: &str, payload: serde_json::Value);
}

fn send<T: Serialize>(sink: &dyn UiSink, event: &str, payload: &T) {
    match serde_json::to_value(payload) {
        Ok(value) => sink.emit(event, value),
        Err(error) => tracing::error!(event, %error, "événement illisible, abandonné"),
    }
}

/// Avis et issues d'actions retenus au plus, tant que l'interface ne les a pas acquittés.
const MAX_RETAINED: usize = 100;

#[derive(Default)]
struct Pending {
    /// Alerte d'empreinte en attente de décision, par serveur : un état, rejoué à chaque lecture.
    alerts: HashMap<String, FingerprintEvent>,
    /// Avis (suivis perdus) pas encore acquittés par l'interface.
    notices: Vec<NoticeEvent>,
    /// Issues d'actions pas encore acquittées par l'interface (par `opId`).
    operations: VecDeque<OperationEventDto>,
    next_notice: u32,
}

/// L'état de la liaison que l'interface doit pouvoir relire après coup. Branché comme destination
/// d'événements de la liaison dès son ouverture : aucun événement ne lui échappe, même celui d'une
/// tâche qui démarre avant que la fenêtre n'existe.
///
/// Les avis et les issues sont retenus jusqu'à un ACQUITTEMENT explicite par identifiant : une
/// lecture ne détruit rien (un abonnement qui échoue puis recommence retrouve tout), et
/// l'interface écarte les doublons par ces identifiants (un signal en direct et la lecture d'état
/// peuvent porter le même avis).
#[derive(Default)]
pub struct PendingBook {
    inner: Mutex<Pending>,
}

impl PendingBook {
    fn lock(&self) -> std::sync::MutexGuard<'_, Pending> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Alertes d'empreinte en attente (BR-CONN-003), levées quand le blocage se lève.
    pub fn alerts(&self) -> Vec<FingerprintEvent> {
        let mut alerts: Vec<FingerprintEvent> = self.lock().alerts.values().cloned().collect();
        alerts.sort_by(|a, b| a.server_id.cmp(&b.server_id));
        alerts
    }

    pub fn alert_of(&self, server: &str) -> Option<FingerprintEvent> {
        self.lock().alerts.get(server).cloned()
    }

    /// Avis retenus, jusqu'à leur acquittement.
    pub fn notices(&self) -> Vec<NoticeEvent> {
        self.lock().notices.clone()
    }

    /// Numéro de l'avis retenu pour ce serveur (0 si aucun) : le signal en direct porte le même.
    pub fn notice_id(&self, server: &str) -> u32 {
        self.lock()
            .notices
            .iter()
            .rfind(|notice| notice.server_id.as_deref() == Some(server))
            .map_or(0, |notice| notice.id)
    }

    pub fn ack_notices(&self, ids: &[u32]) {
        self.lock()
            .notices
            .retain(|notice| !ids.contains(&notice.id));
    }

    /// Issues d'actions retenues, jusqu'à leur acquittement.
    pub fn operations(&self) -> Vec<OperationEventDto> {
        self.lock().operations.iter().cloned().collect()
    }

    pub fn ack_operations(&self, op_ids: &[String]) {
        self.lock()
            .operations
            .retain(|operation| !op_ids.contains(&operation.op_id));
    }

    /// Le serveur est retiré : plus rien ne le concerne.
    pub fn forget(&self, server: &ServerId) {
        let mut pending = self.lock();
        pending.alerts.remove(server.as_str());
        pending
            .notices
            .retain(|notice| notice.server_id.as_deref() != Some(server.as_str()));
        pending
            .operations
            .retain(|operation| operation.server_id != server.as_str());
    }
}

impl EventSink for PendingBook {
    fn emit(&self, event: Event) {
        match event {
            Event::FingerprintChanged {
                server,
                expected,
                presented,
            } => {
                let alert = FingerprintEvent::new(&server, &expected, &presented);
                self.lock().alerts.insert(server.to_string(), alert);
            }
            Event::State { server, info } => {
                if info.blocked != Some(Blocked::FingerprintChanged) {
                    self.lock().alerts.remove(server.as_str());
                }
            }
            Event::OperationsLost { server } => {
                let mut pending = self.lock();
                let name = server.to_string();
                if !pending
                    .notices
                    .iter()
                    .any(|notice| notice.server_id.as_deref() == Some(name.as_str()))
                {
                    pending.next_notice = pending.next_notice.wrapping_add(1).max(1);
                    let id = pending.next_notice;
                    if pending.notices.len() >= MAX_RETAINED {
                        pending.notices.remove(0);
                    }
                    pending.notices.push(NoticeEvent {
                        id,
                        kind: NoticeKind::OperationsLost,
                        server_id: Some(name),
                    });
                }
            }
            Event::Operation {
                server,
                id,
                outcome,
            } => {
                let mut pending = self.lock();
                if pending.operations.len() >= MAX_RETAINED {
                    pending.operations.pop_front();
                }
                pending.operations.push_back(OperationEventDto {
                    op_id: id.as_str().to_owned(),
                    server_id: server.to_string(),
                    outcome: OutcomeDto::from(&outcome),
                });
            }
            Event::Lagged { .. }
            | Event::Metrics { .. }
            | Event::Snapshot { .. }
            | Event::SessionEnded { .. }
            | Event::Audit { .. } => {}
        }
    }
}

pub struct LinkRuntime {
    manager: LinkManager,
    book: Mutex<StateBook>,
    /// Série du processeur par serveur, pour le niveau « tenu 30 s » (BR-DASH-004).
    dash: Mutex<DashBook>,
    last_servers: Mutex<Option<Vec<ServerDto>>>,
    pending: Arc<PendingBook>,
    /// Dernière empreinte lue par une sonde, par adresse : seule une empreinte que cette
    /// application a vue sur la machine peut être confirmée, enregistrée ou épinglée.
    probes: Mutex<HashMap<(String, u16), Fingerprint>>,
    /// Notifications système et icône : branchés une fois, au démarrage de la coquille.
    observer: std::sync::OnceLock<Arc<dyn StateObserver>>,
}

impl LinkRuntime {
    /// Assemblage de production : transport réel, carnet, dernières vues et suivis dans
    /// `data_dir`, secrets dans le coffre fourni.
    pub async fn open(
        data_dir: &Path,
        vault: Arc<dyn Vault>,
        client_name: &str,
    ) -> Result<Self, LinkError> {
        // Le journal d'activité en direct (HRT-14) : un compte lecture seule se voit refuser ce seul
        // sujet par l'agent, le reste du flux est pris (BR-AUDIT-001).
        let config = LinkConfig {
            subscribe_audit: true,
            ..LinkConfig::default()
        };
        Self::open_with(data_dir, vault, client_name, config).await
    }

    pub async fn open_with(
        data_dir: &Path,
        vault: Arc<dyn Vault>,
        client_name: &str,
        config: LinkConfig,
    ) -> Result<Self, LinkError> {
        let pending = Arc::new(PendingBook::default());
        let manager = LinkManager::open_with_sink(
            data_dir,
            vault,
            client_name,
            config,
            Some(pending.clone()),
        )
        .await?;
        Ok(Self {
            manager,
            book: Mutex::new(StateBook::default()),
            dash: Mutex::new(DashBook::default()),
            last_servers: Mutex::new(None),
            pending,
            probes: Mutex::new(HashMap::new()),
            observer: std::sync::OnceLock::new(),
        })
    }

    /// Branche l'observateur des états (une seule fois) et lui donne tout de suite l'état courant
    /// de chaque serveur : un état annoncé avant son branchement ne lui échappe pas.
    pub fn set_observer(&self, observer: Arc<dyn StateObserver>) {
        if self.observer.set(observer).is_err() {
            return;
        }
        for (id, info) in self.manager.states() {
            self.observe(&id, info.state, info.failed_attempts);
        }
    }

    /// Un serveur absent du carnet ne s'observe pas : un état encore dans la file quand
    /// `remove_server` a déjà oublié le serveur ne le réinscrit nulle part (icône, limiteur).
    fn observe(&self, server: &ServerId, state: LinkState, failed_attempts: u32) {
        let Some(observer) = self.observer.get() else {
            return;
        };
        let Some(record) = self
            .manager
            .servers()
            .into_iter()
            .find(|record| &record.id == server)
        else {
            return;
        };
        observer.on_state(server.as_str(), &record.name, state, failed_attempts);
    }

    pub fn manager(&self) -> &LinkManager {
        &self.manager
    }

    fn book(&self) -> std::sync::MutexGuard<'_, StateBook> {
        self.book.lock().unwrap_or_else(PoisonError::into_inner)
    }

    // ── Lecture ────────────────────────────────────────────────────────────────────────────

    /// Un identifiant qui n'est pas dans le carnet vaut « aucun » : la fenêtre ne dicte pas à
    /// l'icône un serveur qui n'existe pas.
    pub fn known_server(&self, id: Option<String>) -> Option<String> {
        id.filter(|id| self.servers().iter().any(|server| &server.id == id))
    }

    fn dash(&self) -> std::sync::MutexGuard<'_, DashBook> {
        self.dash.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Dernière vue connue de la machine d'un serveur (identité, historique de 5 minutes, niveaux
    /// du dernier échantillon) : en mémoire, sinon la dernière sauvegardée sur disque, donc
    /// disponible hors ligne et avant la première connexion de la session (BR-DASH-009). `None`
    /// tant qu'aucune identité n'a été reçue.
    pub async fn dashboard(&self, server_id: &str) -> Result<Option<SnapshotEvent>, LinkFailure> {
        let id = Self::id(server_id)?;
        let Some(view) = self.manager.last_known(&id).await? else {
            return Ok(None);
        };
        let Some(machine) = view.machine else {
            return Ok(None);
        };
        let (event, _) = dash_snapshot(server_id, &machine, &view.history, now_ms());
        Ok(Some(event))
    }

    pub fn servers(&self) -> Vec<ServerDto> {
        servers_list(&self.manager.servers())
    }

    pub fn states(&self) -> Vec<LinkStateDto> {
        self.book().snapshot(&self.manager.states())
    }

    /// Alertes d'empreinte en attente : à lire après l'abonnement, comme `states`.
    pub fn fingerprint_alerts(&self) -> Vec<FingerprintEvent> {
        self.pending.alerts()
    }

    /// Avis retenus et non acquittés (lecture non destructive).
    pub fn notices(&self) -> Vec<NoticeEvent> {
        self.pending.notices()
    }

    pub fn ack_notices(&self, ids: &[u32]) {
        self.pending.ack_notices(ids);
    }

    /// Issues d'actions retenues et non acquittées (lecture non destructive).
    pub fn operations(&self) -> Vec<OperationEventDto> {
        self.pending.operations()
    }

    pub fn ack_operations(&self, op_ids: &[String]) {
        self.pending.ack_operations(op_ids);
    }

    /// Annonce la liste des serveurs si elle diffère de la dernière annoncée.
    pub fn publish_servers(&self, sink: &dyn UiSink) {
        let servers = self.servers();
        {
            let mut last = self
                .last_servers
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if last.as_ref() == Some(&servers) {
                return;
            }
            *last = Some(servers.clone());
        }
        send(sink, events::SERVERS, &ServersEvent { servers });
    }

    // ── Commandes ──────────────────────────────────────────────────────────────────────────

    fn probe_key(host: &str, port: u16) -> (String, u16) {
        (host.trim().to_lowercase(), port)
    }

    /// Une empreinte n'est acceptée que si une sonde de cette application l'a lue à cette adresse.
    fn probed(&self, host: &str, port: u16, fingerprint: &Fingerprint) -> Result<(), LinkFailure> {
        let probes = self.probes.lock().unwrap_or_else(PoisonError::into_inner);
        match probes.get(&Self::probe_key(host, port)) {
            Some(seen) if seen == fingerprint => Ok(()),
            _ => Err(LinkFailure::VerificationRequired),
        }
    }

    /// Première prise de contact : l'empreinte à faire confirmer (BR-CONN-001, 011, 012).
    pub async fn probe(&self, host: &str, port: Option<u16>) -> Result<ProbeDto, LinkFailure> {
        let port = port.unwrap_or(DEFAULT_PORT);
        let probe = self.manager.probe(host.trim(), port).await?;
        match probe.compatibility {
            Compatibility::Compatible => {}
            Compatibility::UpdateClient => return Err(LinkFailure::IncompatibleClient),
            Compatibility::UpdateAgent => return Err(LinkFailure::IncompatibleAgent),
        }
        self.probes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(Self::probe_key(host, port), probe.fingerprint);
        Ok(ProbeDto {
            fingerprint: probe.fingerprint.to_hex(),
            display: probe.fingerprint.short(),
            machine_name: probe.hello.machine_name,
            agent_version: probe.hello.agent_version,
            mac_addresses: probe.hello.mac_addresses,
        })
    }

    /// Fin de l'assistant : contacte l'adresse épinglée sur l'empreinte confirmée, ouvre la session
    /// et SEULEMENT si elle réussit enregistre le serveur, son empreinte et ses secrets
    /// (BR-CONN-002, 004). Un échec ou un abandon ne laisse rien.
    #[allow(clippy::too_many_arguments)]
    pub async fn add_and_login(
        &self,
        name: String,
        color: u8,
        host: String,
        port: Option<u16>,
        fingerprint: &str,
        mac_addresses: Vec<String>,
        username: &str,
        password: String,
        remember: bool,
        sink: &dyn UiSink,
    ) -> Result<ServerDto, LinkFailure> {
        let fingerprint = parse_fingerprint(fingerprint)?;
        let port = port.unwrap_or(DEFAULT_PORT);
        self.probed(&host, port, &fingerprint)?;
        let (id, _) = self
            .manager
            .add_and_login(
                NewServer {
                    name,
                    color: color.clamp(1, 8).to_string(),
                    host: host.trim().to_owned(),
                    port,
                    fingerprint,
                    mac_addresses,
                },
                username,
                Secret::new(password),
                remember,
            )
            .await?;
        self.publish_servers(sink);
        self.server(&id)
    }

    fn server(&self, id: &ServerId) -> Result<ServerDto, LinkFailure> {
        self.servers()
            .into_iter()
            .find(|server| server.id == id.as_str())
            .ok_or(LinkFailure::UnknownServer)
    }

    fn id(text: &str) -> Result<ServerId, LinkFailure> {
        ServerId::parse(text).map_err(|_| LinkFailure::UnknownServer)
    }

    pub async fn login(
        &self,
        server_id: &str,
        username: &str,
        password: String,
        remember: bool,
        sink: &dyn UiSink,
    ) -> Result<crate::link_dto::LoginDto, LinkFailure> {
        let id = Self::id(server_id)?;
        let info = self
            .manager
            .login(&id, username, Secret::new(password), remember)
            .await?;
        self.publish_servers(sink);
        Ok(crate::link_dto::LoginDto {
            role: crate::link_dto::RoleDto::from(info.account.role),
            username: info.account.username,
        })
    }

    pub async fn logout(&self, server_id: &str, sink: &dyn UiSink) -> Result<(), LinkFailure> {
        self.manager.logout(&Self::id(server_id)?).await?;
        self.publish_servers(sink);
        Ok(())
    }

    pub fn retry_now(&self, server_id: &str) -> Result<(), LinkFailure> {
        Ok(self.manager.retry_now(&Self::id(server_id)?)?)
    }

    /// L'utilisateur accepte la nouvelle empreinte (BR-CONN-003). Seule l'empreinte que la liaison
    /// a présentée ET que l'interface a affichée est acceptée : la commande reçoit l'empreinte
    /// affichée et la compare à celle en attente (la bibliothèque la compare aussi).
    pub async fn accept_fingerprint(
        &self,
        server_id: &str,
        fingerprint: &str,
    ) -> Result<(), LinkFailure> {
        let id = Self::id(server_id)?;
        let alert = self
            .pending
            .alert_of(id.as_str())
            .ok_or(LinkFailure::VerificationRequired)?;
        // Celle que l'utilisateur a sous les yeux doit être celle qui attend : si un signal a sauté,
        // l'interface en montre peut-être une autre.
        let shown = parse_fingerprint(fingerprint)?;
        let waiting = parse_fingerprint(&alert.presented_hex)?;
        if shown != waiting {
            return Err(LinkFailure::VerificationRequired);
        }
        self.manager.accept_fingerprint(&id, waiting).await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_server(
        &self,
        server_id: &str,
        name: String,
        color: u8,
        host: String,
        port: Option<u16>,
        fingerprint: Option<String>,
        sink: &dyn UiSink,
    ) -> Result<ServerDto, LinkFailure> {
        let id = Self::id(server_id)?;
        let port = port.unwrap_or(DEFAULT_PORT);
        let fingerprint = fingerprint.as_deref().map(parse_fingerprint).transpose()?;
        if let Some(seen) = &fingerprint {
            self.probed(&host, port, seen)?;
        }
        self.manager
            .update_server(
                &id,
                ServerUpdate {
                    name,
                    color: color.clamp(1, 8).to_string(),
                    host: host.trim().to_owned(),
                    port,
                    fingerprint,
                },
            )
            .await?;
        self.publish_servers(sink);
        self.server(&id)
    }

    pub async fn remove_server(
        &self,
        server_id: &str,
        sink: &dyn UiSink,
    ) -> Result<(), LinkFailure> {
        let id = Self::id(server_id)?;
        self.manager.remove_server(&id).await?;
        self.book().forget(&id);
        self.dash().forget(id.as_str());
        self.pending.forget(&id);
        if let Some(observer) = self.observer.get() {
            observer.on_removed(id.as_str());
        }
        self.publish_servers(sink);
        Ok(())
    }

    pub async fn forget_credentials(
        &self,
        server_id: &str,
        sink: &dyn UiSink,
    ) -> Result<(), LinkFailure> {
        self.manager
            .forget_credentials(&Self::id(server_id)?)
            .await?;
        self.publish_servers(sink);
        Ok(())
    }

    // ── Relais des événements ──────────────────────────────────────────────────────────────

    /// Traduit les événements de la liaison en signaux pour l'interface, jusqu'à l'arrêt de la
    /// bibliothèque. Les mesures et le journal d'activité ne sont pas relayés ici (tickets
    /// suivants).
    pub async fn forward(&self, mut stream: EventStream, sink: &dyn UiSink) {
        while let Some(event) = stream.recv().await {
            let lagged = matches!(event, Event::Lagged { .. });
            self.relay(event, sink);
            if lagged {
                self.resync_dashboards(sink).await;
            }
        }
    }

    /// Après un retard d'écoute, des instantanés ont pu être perdus : la dernière vue connue de
    /// chaque serveur est réannoncée et la série du processeur repart d'elle.
    pub async fn resync_dashboards(&self, sink: &dyn UiSink) {
        for record in self.manager.servers() {
            let Ok(id) = ServerId::parse(record.id.as_str()) else {
                continue;
            };
            let Ok(Some(view)) = self.manager.last_known(&id).await else {
                continue;
            };
            let Some(machine) = view.machine else {
                continue;
            };
            let snapshot = self
                .dash()
                .on_snapshot(id.as_str(), &machine, &view.history, now_ms());
            send(sink, dash_events::SNAPSHOT, &snapshot);
        }
    }

    pub fn relay(&self, event: Event, sink: &dyn UiSink) {
        match event {
            Event::State { server, info } => {
                let state = self.book().apply(&server, &info);
                self.observe(&server, info.state, info.failed_attempts);
                if let Some(state) = state {
                    send(sink, events::STATE, &state);
                }
                // Le rôle ou l'identifiant peuvent avoir changé avec cet état.
                self.publish_servers(sink);
            }
            Event::Operation {
                server,
                id,
                outcome,
            } => send(
                sink,
                events::OPERATION,
                &OperationEventDto {
                    op_id: id.as_str().to_owned(),
                    server_id: server.to_string(),
                    outcome: OutcomeDto::from(&outcome),
                },
            ),
            // Un signal : l'alerte elle-même est un état (`PendingBook`), relu à l'abonnement.
            Event::FingerprintChanged {
                server,
                expected,
                presented,
            } => send(
                sink,
                events::FINGERPRINT,
                &FingerprintEvent::new(&server, &expected, &presented),
            ),
            Event::OperationsLost { server } => send(
                sink,
                events::NOTICE,
                &NoticeEvent {
                    id: self.pending.notice_id(server.as_str()),
                    kind: NoticeKind::OperationsLost,
                    server_id: Some(server.to_string()),
                },
            ),
            Event::Lagged { .. } => {
                send(
                    sink,
                    events::NOTICE,
                    &NoticeEvent {
                        id: 0,
                        kind: NoticeKind::Lagged,
                        server_id: None,
                    },
                );
                // Des signaux ont pu être perdus : on réannonce tout l'état courant, états des
                // liens ET alertes d'empreinte en attente (jamais d'alerte introuvable).
                for state in self.states() {
                    send(sink, events::STATE, &state);
                }
                for alert in self.pending.alerts() {
                    send(sink, events::FINGERPRINT, &alert);
                }
            }
            // Mesures : chaque seconde, avec les niveaux d'alerte déjà décidés (hearth-proto).
            Event::Metrics { server, sample } => {
                let metrics = self.dash().on_metrics(server.as_str(), &sample, now_ms());
                send(sink, dash_events::METRICS, &metrics);
            }
            Event::Snapshot {
                server,
                machine,
                history,
            } => {
                let snapshot =
                    self.dash()
                        .on_snapshot(server.as_str(), &machine, &history, now_ms());
                send(sink, dash_events::SNAPSHOT, &snapshot);
            }
            // Une entrée du journal, en direct (HRT-14) : la page la fusionne à sa liste.
            Event::Audit { server, event } => {
                send(
                    sink,
                    crate::audit::EVENT,
                    &crate::audit::live(&server, &event),
                );
            }
            Event::SessionEnded { .. } => {}
        }
    }
}

/// Maintenant, en millisecondes depuis l'époque (repli d'un échantillon à la date illisible).
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| i64::try_from(elapsed.as_millis()).ok())
        .unwrap_or(0)
}

/// Nom du poste annoncé à l'agent (`X-Hearth-Client`) : `{ordinateur}/{version}`.
pub fn client_name(version: &str) -> String {
    let computer = std::env::var("COMPUTERNAME")
        .ok()
        .map(|name| {
            name.chars()
                .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
                .take(32)
                .collect::<String>()
        })
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "poste".to_owned());
    format!("{computer}/{version}")
}

//! Le pont réel entre l'interface et `hearth-link` : `LinkRuntime` enveloppe le `LinkManager`
//! (commandes) et un relais (`forward`) qui traduit ses événements en événements pour
//! l'interface. Aucune règle ici : elles sont dans `hearth-link` (`domain/`). Rien de ce module
//! ne dépend de Tauri : le relais écrit dans un [`UiSink`] (la fenêtre en production, un
//! enregistreur dans les tests).

use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use hearth_link::domain::compat::Compatibility;
use hearth_link::domain::event::Event;
use hearth_link::domain::secret::Secret;
use hearth_link::domain::server::ServerId;
use hearth_link::ports::Vault;
use hearth_link::{EventStream, LinkConfig, LinkError, LinkManager, NewServer, ServerUpdate};
use hearth_proto::product::DEFAULT_PORT;
use serde::Serialize;

use crate::link_dto::{
    FingerprintEvent, LinkFailure, LinkStateDto, LoginDto, NoticeEvent, NoticeKind,
    OperationEventDto, OutcomeDto, ProbeDto, RoleDto, ServerDto, ServersEvent, SessionEndedEvent,
    StateBook, events, parse_fingerprint, servers_list,
};

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

pub struct LinkRuntime {
    manager: LinkManager,
    book: Mutex<StateBook>,
    last_servers: Mutex<Option<Vec<ServerDto>>>,
}

impl LinkRuntime {
    /// Assemblage de production : transport réel, carnet, dernières vues et suivis dans
    /// `data_dir`, secrets dans le coffre fourni.
    pub async fn open(
        data_dir: &Path,
        vault: Arc<dyn Vault>,
        client_name: &str,
    ) -> Result<Self, LinkError> {
        let manager =
            LinkManager::open(data_dir, vault, client_name, LinkConfig::default()).await?;
        Ok(Self::new(manager))
    }

    pub fn new(manager: LinkManager) -> Self {
        Self {
            manager,
            book: Mutex::new(StateBook::default()),
            last_servers: Mutex::new(None),
        }
    }

    pub fn manager(&self) -> &LinkManager {
        &self.manager
    }

    fn book(&self) -> std::sync::MutexGuard<'_, StateBook> {
        self.book.lock().unwrap_or_else(PoisonError::into_inner)
    }

    // ── Lecture ────────────────────────────────────────────────────────────────────────────

    pub fn servers(&self) -> Vec<ServerDto> {
        servers_list(&self.manager.servers())
    }

    pub fn states(&self) -> Vec<LinkStateDto> {
        self.book().snapshot(&self.manager.states())
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

    /// Première prise de contact : l'empreinte à faire confirmer (BR-CONN-001, 011, 012).
    pub async fn probe(&self, host: &str, port: Option<u16>) -> Result<ProbeDto, LinkFailure> {
        let probe = self
            .manager
            .probe(host.trim(), port.unwrap_or(DEFAULT_PORT))
            .await?;
        match probe.compatibility {
            Compatibility::Compatible => {}
            Compatibility::UpdateClient => return Err(LinkFailure::IncompatibleClient),
            Compatibility::UpdateAgent => return Err(LinkFailure::IncompatibleAgent),
        }
        Ok(ProbeDto {
            fingerprint: probe.fingerprint.to_hex(),
            display: probe.fingerprint.short(),
            machine_name: probe.hello.machine_name,
            agent_version: probe.hello.agent_version,
            mac_addresses: probe.hello.mac_addresses,
        })
    }

    /// Enregistre le serveur dont l'empreinte vient d'être confirmée (BR-CONN-002).
    #[allow(clippy::too_many_arguments)]
    pub async fn add_server(
        &self,
        name: String,
        color: u8,
        host: String,
        port: Option<u16>,
        fingerprint: &str,
        mac_addresses: Vec<String>,
        sink: &dyn UiSink,
    ) -> Result<ServerDto, LinkFailure> {
        let fingerprint = parse_fingerprint(fingerprint)?;
        let id = self
            .manager
            .add_server(NewServer {
                name,
                color: color.clamp(1, 8).to_string(),
                host: host.trim().to_owned(),
                port: port.unwrap_or(DEFAULT_PORT),
                fingerprint,
                mac_addresses,
            })
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
    ) -> Result<LoginDto, LinkFailure> {
        let id = Self::id(server_id)?;
        let info = self
            .manager
            .login(&id, username, Secret::new(password), remember)
            .await?;
        self.publish_servers(sink);
        Ok(LoginDto {
            role: RoleDto::from(info.account.role),
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

    pub async fn accept_fingerprint(
        &self,
        server_id: &str,
        fingerprint: &str,
    ) -> Result<(), LinkFailure> {
        let fingerprint = parse_fingerprint(fingerprint)?;
        Ok(self
            .manager
            .accept_fingerprint(&Self::id(server_id)?, fingerprint)
            .await?)
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
        let fingerprint = fingerprint.as_deref().map(parse_fingerprint).transpose()?;
        self.manager
            .update_server(
                &id,
                ServerUpdate {
                    name,
                    color: color.clamp(1, 8).to_string(),
                    host: host.trim().to_owned(),
                    port: port.unwrap_or(DEFAULT_PORT),
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

    /// Traduit les événements de la liaison en événements pour l'interface, jusqu'à l'arrêt de la
    /// bibliothèque. Les mesures et le journal d'activité ne sont pas relayés ici (tickets
    /// suivants).
    pub async fn forward(&self, mut stream: EventStream, sink: &dyn UiSink) {
        while let Some(event) = stream.recv().await {
            self.relay(event, sink);
        }
    }

    pub fn relay(&self, event: Event, sink: &dyn UiSink) {
        match event {
            Event::State { server, info } => {
                let state = self.book().apply(&server, &info);
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
            Event::FingerprintChanged {
                server,
                expected,
                presented,
            } => send(
                sink,
                events::FINGERPRINT,
                &FingerprintEvent::new(&server, &expected, &presented),
            ),
            Event::SessionEnded { server, kind } => send(
                sink,
                events::SESSION_ENDED,
                &SessionEndedEvent {
                    server_id: server.to_string(),
                    kind: kind.into(),
                },
            ),
            Event::OperationsLost { server } => send(
                sink,
                events::NOTICE,
                &NoticeEvent {
                    kind: NoticeKind::OperationsLost,
                    server_id: Some(server.to_string()),
                },
            ),
            Event::Lagged { .. } => {
                send(
                    sink,
                    events::NOTICE,
                    &NoticeEvent {
                        kind: NoticeKind::Lagged,
                        server_id: None,
                    },
                );
                // Des changements d'état ont pu être perdus : on réannonce l'état courant.
                for state in self.states() {
                    send(sink, events::STATE, &state);
                }
            }
            Event::Metrics { .. } | Event::Snapshot { .. } | Event::Audit { .. } => {}
        }
    }
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

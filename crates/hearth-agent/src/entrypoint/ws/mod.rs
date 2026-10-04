//! Flux temps réel `GET /api/v1/stream` (WebSocket sur la même adresse HTTPS, ADR-0004).
//!
//! Le protocole (messages, déroulement) est décrit dans `hearth_proto::stream`. Ici :
//! - `StreamContext` : ce que le flux emprunte à l'agent en dehors des cas d'usage (le journal à
//!   diffuser, les délais, le signal d'arrêt) ;
//! - `stream` : le handler de mise à niveau, déclaré dans `ENDPOINTS` (`Access::FirstMessage` :
//!   l'authentification se fait par le premier message du flux, pas par l'en-tête de la requête) ;
//! - `connection` : une connexion, de l'authentification à la fermeture.

mod connection;

use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::extract::ws::{WebSocketUpgrade, rejection::WebSocketUpgradeRejection};
use axum::response::Response;
use hearth_proto::stream::{AUTH_TIMEOUT_S, MAX_CLIENT_MESSAGE_BYTES};
use tokio::sync::watch;

use super::http::{ApiError, AppState};
use crate::application::ports::AuditFeed;
use crate::domain::stream::{IDLE_TIMEOUT, SEND_TIMEOUT, SESSION_CHECK_PERIOD};

/// Délais du flux. Les valeurs par défaut sont celles du protocole ; les tests les raccourcissent.
#[derive(Debug, Clone, Copy)]
pub struct StreamSettings {
    /// Délai laissé au client pour envoyer `auth` après l'ouverture.
    pub auth_timeout: Duration,
    /// Silence du client au bout duquel le flux est fermé.
    pub idle_timeout: Duration,
    /// Période de revérification de la session ouverte.
    pub session_check_period: Duration,
    /// Temps accordé à l'envoi d'un message.
    pub send_timeout: Duration,
}

impl Default for StreamSettings {
    fn default() -> Self {
        Self {
            auth_timeout: Duration::from_secs(AUTH_TIMEOUT_S),
            idle_timeout: IDLE_TIMEOUT,
            session_check_period: SESSION_CHECK_PERIOD,
            send_timeout: SEND_TIMEOUT,
        }
    }
}

/// Ce que partagent toutes les connexions du flux.
#[derive(Clone)]
pub struct StreamContext {
    pub(crate) audit: Arc<dyn AuditFeed>,
    pub(crate) settings: StreamSettings,
    shutdown: Arc<watch::Sender<bool>>,
}

impl StreamContext {
    pub fn new(audit: Arc<dyn AuditFeed>, settings: StreamSettings) -> Self {
        Self {
            audit,
            settings,
            shutdown: Arc::new(watch::channel(false).0),
        }
    }

    /// Demande à toutes les connexions de se fermer proprement (code 1001, « parti »). Appelé à
    /// l'arrêt de l'agent : sans cela, l'arrêt attendrait le délai de grâce des connexions
    /// longues.
    pub fn begin_shutdown(&self) {
        self.shutdown.send_replace(true);
    }

    pub(crate) fn shutdown_signal(&self) -> watch::Receiver<bool> {
        self.shutdown.subscribe()
    }
}

/// `GET /api/v1/stream` : mise à niveau en WebSocket. Le contrôle de version d'interface s'est
/// déjà appliqué (la table `ENDPOINTS` le demande) ; le jeton viendra du premier message.
pub async fn stream(
    upgrade: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
    State(state): State<AppState>,
) -> Result<Response, ApiError> {
    let upgrade = upgrade.map_err(|_| {
        ApiError::invalid(
            "upgrade",
            "Cette route est un flux WebSocket : ouvre-la par une mise à niveau",
        )
    })?;
    Ok(upgrade
        .max_message_size(MAX_CLIENT_MESSAGE_BYTES)
        .max_frame_size(MAX_CLIENT_MESSAGE_BYTES)
        .on_upgrade(move |socket| connection::run(socket, state)))
}

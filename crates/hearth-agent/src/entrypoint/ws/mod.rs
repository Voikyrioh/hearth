//! Flux temps réel `GET /api/v1/stream` (WebSocket sur la même adresse HTTPS, ADR-0004).
//!
//! Le protocole (messages, déroulement) est décrit dans `hearth_proto::stream`. Ici :
//! - `StreamContext` : ce que le flux emprunte à l'agent en dehors des cas d'usage (le journal à
//!   diffuser, les délais, le signal d'arrêt) ;
//! - `stream` : le handler de mise à niveau, déclaré dans `ENDPOINTS` (`Access::FirstMessage` :
//!   l'authentification se fait par le premier message du flux, pas par l'en-tête de la requête) ;
//! - `connection` : une connexion, de l'authentification à la fermeture.

mod connection;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use axum::extract::State;
use axum::extract::ws::{WebSocketUpgrade, rejection::WebSocketUpgradeRejection};
use axum::response::Response;
use hearth_proto::error::ErrorCode;
use hearth_proto::stream::{AUTH_TIMEOUT_S, MAX_CLIENT_MESSAGE_BYTES};
use tokio::sync::watch;

use super::http::{ApiError, AppState};
use crate::application::ports::AuditFeed;
use crate::domain::stream::{
    Admission, IDLE_TIMEOUT, MAX_STREAMS_PER_ACCOUNT, MAX_STREAMS_TOTAL, MIN_SUBSCRIBE_INTERVAL,
    SEND_TIMEOUT, SESSION_CHECK_PERIOD, admission,
};

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
    /// Flux ouverts en même temps, au total.
    pub max_total: usize,
    /// Flux ouverts en même temps par un même compte.
    pub max_per_account: usize,
    /// Délai minimal entre deux `subscribe` d'une connexion.
    pub min_subscribe_interval: Duration,
}

impl Default for StreamSettings {
    fn default() -> Self {
        Self {
            auth_timeout: Duration::from_secs(AUTH_TIMEOUT_S),
            idle_timeout: IDLE_TIMEOUT,
            session_check_period: SESSION_CHECK_PERIOD,
            send_timeout: SEND_TIMEOUT,
            max_total: MAX_STREAMS_TOTAL,
            max_per_account: MAX_STREAMS_PER_ACCOUNT,
            min_subscribe_interval: MIN_SUBSCRIBE_INTERVAL,
        }
    }
}

/// Ce que partagent toutes les connexions du flux.
#[derive(Clone)]
pub struct StreamContext {
    pub(crate) audit: Arc<dyn AuditFeed>,
    pub(crate) settings: StreamSettings,
    shutdown: Arc<watch::Sender<bool>>,
    open: Arc<Mutex<Open>>,
}

/// Flux ouverts : au total et par compte.
#[derive(Default)]
struct Open {
    total: usize,
    per_account: HashMap<String, usize>,
}

/// Une place de flux ouverte ; la rendre (la laisser tomber) libère la place.
pub(crate) struct Permit {
    open: Arc<Mutex<Open>>,
    account: Option<String>,
}

impl Permit {
    /// Rattache la place à un compte, une fois authentifié : refusé si ce compte a déjà trop de
    /// flux ouverts.
    pub(crate) fn bind_account(&mut self, account: &str, max_per_account: usize) -> bool {
        let mut open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        let count = open.per_account.get(account).copied().unwrap_or(0);
        if admission(0, count, usize::MAX, max_per_account) != Admission::Admitted {
            return false;
        }
        open.per_account.insert(account.to_owned(), count + 1);
        self.account = Some(account.to_owned());
        true
    }
}

impl Drop for Permit {
    fn drop(&mut self) {
        let mut open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        open.total = open.total.saturating_sub(1);
        if let Some(account) = self.account.take()
            && let Some(count) = open.per_account.get_mut(&account)
        {
            *count = count.saturating_sub(1);
            if *count == 0 {
                open.per_account.remove(&account);
            }
        }
    }
}

impl StreamContext {
    pub fn new(audit: Arc<dyn AuditFeed>, settings: StreamSettings) -> Self {
        Self {
            audit,
            settings,
            shutdown: Arc::new(watch::channel(false).0),
            open: Arc::default(),
        }
    }

    /// Réserve une place de flux : refusée si l'agent a déjà trop de flux ouverts.
    pub(crate) fn try_open(&self) -> Option<Permit> {
        let mut open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        if admission(open.total, 0, self.settings.max_total, usize::MAX) != Admission::Admitted {
            return None;
        }
        open.total += 1;
        Some(Permit {
            open: self.open.clone(),
            account: None,
        })
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
    let permit = state.stream.try_open().ok_or_else(|| {
        ApiError::new(
            ErrorCode::Busy,
            "Trop de flux ouverts sur cet agent, réessaie dans un instant",
        )
    })?;
    Ok(upgrade
        .max_message_size(MAX_CLIENT_MESSAGE_BYTES)
        .max_frame_size(MAX_CLIENT_MESSAGE_BYTES)
        .on_upgrade(move |socket| connection::run(socket, state, permit)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use tokio::sync::broadcast;

    struct NoFeed;

    impl AuditFeed for NoFeed {
        fn subscribe(&self) -> Option<broadcast::Receiver<Arc<Value>>> {
            None
        }
    }

    fn context(max_total: usize) -> StreamContext {
        StreamContext::new(
            Arc::new(NoFeed),
            StreamSettings {
                max_total,
                ..StreamSettings::default()
            },
        )
    }

    #[test]
    fn places_are_limited_in_total_and_given_back() {
        let context = context(2);
        let first = context.try_open().expect("première place");
        let _second = context.try_open().expect("deuxième place");
        assert!(context.try_open().is_none(), "plafond atteint");
        drop(first);
        assert!(
            context.try_open().is_some(),
            "une place rendue est reprenable"
        );
    }

    #[test]
    fn places_are_limited_per_account_and_given_back() {
        let context = context(10);
        let mut permits: Vec<_> = (0..4).filter_map(|_| context.try_open()).collect();
        for permit in &mut permits {
            assert!(permit.bind_account("marie", 4));
        }
        let mut fifth = context.try_open().expect("place du total");
        assert!(!fifth.bind_account("marie", 4), "5e flux de marie refusé");
        assert!(fifth.bind_account("lucas", 4), "un autre compte passe");
        drop(permits.pop());
        let mut sixth = context.try_open().expect("place du total");
        assert!(sixth.bind_account("marie", 4), "une place de marie rendue");
    }

    #[tokio::test]
    async fn a_receiver_created_after_the_shutdown_still_sees_it() {
        let context = context(1);
        context.begin_shutdown();
        let mut late = context.shutdown_signal();
        tokio::time::timeout(Duration::from_secs(30), late.wait_for(|stopped| *stopped))
            .await
            .expect("le signal d'arrêt est déjà levé")
            .expect("canal ouvert");
    }
}

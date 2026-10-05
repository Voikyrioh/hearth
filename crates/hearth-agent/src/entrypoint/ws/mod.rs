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
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use axum::extract::ws::{WebSocketUpgrade, rejection::WebSocketUpgradeRejection};
use axum::extract::{ConnectInfo, State};
use axum::response::Response;
use hearth_proto::error::ErrorCode;
use hearth_proto::stream::{AUTH_TIMEOUT_S, MAX_CLIENT_MESSAGE_BYTES};
use tokio::sync::watch;

use super::http::{ApiError, AppState};
use crate::application::ports::AuditFeed;
use crate::domain::stream::{
    IDLE_TIMEOUT, MAX_PENDING_PER_ADDRESS, MAX_PENDING_TOTAL, MAX_STREAMS_PER_ACCOUNT,
    MAX_STREAMS_TOTAL, MIN_SUBSCRIBE_INTERVAL, SEND_TIMEOUT, SESSION_CHECK_PERIOD, within_caps,
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
    /// Connexions en attente d'authentification, au total.
    pub max_pending_total: usize,
    /// Connexions en attente d'authentification depuis une même adresse.
    pub max_pending_per_address: usize,
    /// Flux authentifiés ouverts en même temps, au total.
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
            max_pending_total: MAX_PENDING_TOTAL,
            max_pending_per_address: MAX_PENDING_PER_ADDRESS,
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

/// Places prises : connexions en attente d'authentification (par adresse) et flux authentifiés
/// (par compte). Deux quotas distincts.
#[derive(Default)]
struct Open {
    pending_total: usize,
    pending_per_address: HashMap<IpAddr, usize>,
    streams_total: usize,
    streams_per_account: HashMap<String, usize>,
}

/// Une place de connexion. D'abord en attente d'authentification (prise à l'ouverture, par
/// adresse), puis, après un `auth` réussi, place de flux authentifié (par compte) : la place
/// d'attente est rendue à ce moment. La laisser tomber rend celle qu'elle tient, quelle que soit
/// l'issue (délai, coupure, erreur, fermeture).
pub(crate) struct Permit {
    open: Arc<Mutex<Open>>,
    address: Option<IpAddr>,
    account: Option<String>,
}

impl Permit {
    /// Passe de « en attente » à « flux authentifié » pour ce compte : rend la place d'attente et
    /// prend une place de flux ; refusé (la place d'attente est alors gardée jusqu'à la
    /// fermeture) si l'agent ou le compte a déjà trop de flux.
    pub(crate) fn authenticate(
        &mut self,
        account: &str,
        max_total: usize,
        max_per_account: usize,
    ) -> bool {
        let mut open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        let own = open.streams_per_account.get(account).copied().unwrap_or(0);
        if !within_caps(open.streams_total, own, max_total, max_per_account) {
            return false;
        }
        if let Some(address) = self.address.take() {
            open.pending_total = open.pending_total.saturating_sub(1);
            drop_one(&mut open.pending_per_address, &address);
        }
        open.streams_total += 1;
        *open
            .streams_per_account
            .entry(account.to_owned())
            .or_insert(0) += 1;
        self.account = Some(account.to_owned());
        true
    }
}

fn drop_one<K: std::hash::Hash + Eq>(map: &mut HashMap<K, usize>, key: &K) {
    if let Some(count) = map.get_mut(key) {
        *count = count.saturating_sub(1);
        if *count == 0 {
            map.remove(key);
        }
    }
}

impl Drop for Permit {
    fn drop(&mut self) {
        let mut open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(address) = self.address.take() {
            open.pending_total = open.pending_total.saturating_sub(1);
            drop_one(&mut open.pending_per_address, &address);
        }
        if let Some(account) = self.account.take() {
            open.streams_total = open.streams_total.saturating_sub(1);
            drop_one(&mut open.streams_per_account, &account);
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

    /// Réserve une place d'attente d'authentification pour cette adresse : refusée si l'adresse
    /// ou l'agent a déjà trop de connexions en attente.
    pub(crate) fn try_wait(&self, address: IpAddr) -> Option<Permit> {
        let mut open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        let own = open.pending_per_address.get(&address).copied().unwrap_or(0);
        if !within_caps(
            open.pending_total,
            own,
            self.settings.max_pending_total,
            self.settings.max_pending_per_address,
        ) {
            return None;
        }
        open.pending_total += 1;
        *open.pending_per_address.entry(address).or_insert(0) += 1;
        Some(Permit {
            open: self.open.clone(),
            address: Some(address),
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
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
) -> Result<Response, ApiError> {
    let upgrade = upgrade.map_err(|_| {
        ApiError::invalid(
            "upgrade",
            "Cette route est un flux WebSocket : ouvre-la par une mise à niveau",
        )
    })?;
    // Quota des connexions pas encore authentifiées : par adresse et au total. Refus au format
    // d'erreur de l'API, `503 BUSY` (avec `Retry-After`).
    let permit = state.stream.try_wait(peer.ip()).ok_or_else(|| {
        ApiError::new(
            ErrorCode::Busy,
            "Trop de connexions en attente d'authentification : envoie auth plus vite ou réessaie dans un instant",
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

    fn context(settings: StreamSettings) -> StreamContext {
        StreamContext::new(Arc::new(NoFeed), settings)
    }

    fn ip(last: u8) -> IpAddr {
        IpAddr::from([10, 0, 0, last])
    }

    #[test]
    fn an_address_holds_at_most_two_waiting_places_and_gives_them_back() {
        let context = context(StreamSettings::default());
        let first = context.try_wait(ip(1)).expect("première attente");
        let _second = context.try_wait(ip(1)).expect("deuxième attente");
        assert!(
            context.try_wait(ip(1)).is_none(),
            "3e attente de la même adresse"
        );
        assert!(context.try_wait(ip(2)).is_some(), "une autre adresse passe");
        drop(first);
        assert!(context.try_wait(ip(1)).is_some(), "place d'attente rendue");
    }

    #[test]
    fn sixteen_anonymous_waiters_never_take_a_stream_place() {
        let context = context(StreamSettings::default());
        // 16 anonymes muets depuis 16 adresses : le quota d'attente est plein...
        let mut anonymous: Vec<_> = (1..=16).filter_map(|n| context.try_wait(ip(n))).collect();
        assert_eq!(anonymous.len(), 16);
        assert!(context.try_wait(ip(100)).is_none(), "attente pleine");
        // ... mais aucune place de flux n'est prise : un client déjà connecté (en attente) qui
        // s'authentifie obtient la sienne.
        let mut legit = anonymous.pop().expect("un client");
        assert!(legit.authenticate("marie", 32, 4));
        // Sa place d'attente est rendue : une adresse de plus peut attendre.
        assert!(context.try_wait(ip(101)).is_some());
    }

    #[test]
    fn stream_places_are_capped_in_total_and_per_account() {
        let context = context(StreamSettings::default());
        let mut permits: Vec<_> = (1..=5).filter_map(|n| context.try_wait(ip(n))).collect();
        for permit in permits.iter_mut().take(4) {
            assert!(permit.authenticate("marie", 6, 4));
        }
        let fifth = &mut permits[4];
        assert!(
            !fifth.authenticate("marie", 6, 4),
            "5e flux de marie refusé"
        );
        assert!(fifth.authenticate("lucas", 6, 4), "un autre compte passe");
        // Un flux fermé rend sa place.
        permits.remove(0);
        let mut again = context.try_wait(ip(9)).expect("attente");
        assert!(again.authenticate("marie", 6, 4));
    }

    #[test]
    fn the_total_of_streams_is_capped() {
        let context = context(StreamSettings::default());
        let mut a = context.try_wait(ip(1)).unwrap();
        let mut b = context.try_wait(ip(2)).unwrap();
        assert!(a.authenticate("marie", 1, 4));
        assert!(!b.authenticate("lucas", 1, 4), "agent plein");
    }

    #[tokio::test]
    async fn a_receiver_created_after_the_shutdown_still_sees_it() {
        let context = context(StreamSettings::default());
        context.begin_shutdown();
        let mut late = context.shutdown_signal();
        tokio::time::timeout(Duration::from_secs(30), late.wait_for(|stopped| *stopped))
            .await
            .expect("le signal d'arrêt est déjà levé")
            .expect("canal ouvert");
    }
}

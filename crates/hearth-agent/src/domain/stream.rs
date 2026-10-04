//! Règles du flux temps réel (BR-DASH-001, BR-DASH-002, BR-DASH-011) : délais et plafonds par
//! défaut, et les décisions pures du flux (jamais deux fois le même échantillon, combien de flux,
//! à quelle cadence s'abonner).

use std::time::Duration;

use time::Duration as MonoDuration;

use super::metrics::HistoryWindow;

/// Historique joint au `snapshot` : les 5 dernières minutes.
pub const SNAPSHOT_WINDOW: HistoryWindow = HistoryWindow::FiveMinutes;

/// Sans le moindre message du client pendant ce délai, le flux est fermé : le client envoie un
/// `ping` toutes les 2 s, un silence aussi long est un lien mort.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

/// Période de revérification de la session pendant le flux (révocation, expiration, rôle).
pub const SESSION_CHECK_PERIOD: Duration = Duration::from_secs(5);

/// Temps accordé à l'envoi d'un message : un client qui ne lit plus est abandonné, il ne retient
/// ni l'agent ni les autres abonnés.
pub const SEND_TIMEOUT: Duration = Duration::from_secs(10);

/// Flux ouverts en même temps, au total.
pub const MAX_STREAMS_TOTAL: usize = 32;

/// Flux ouverts en même temps par un même compte.
pub const MAX_STREAMS_PER_ACCOUNT: usize = 4;

/// Délai minimal entre deux `subscribe` d'une même connexion : chacun coûte un `snapshot`.
pub const MIN_SUBSCRIBE_INTERVAL: Duration = Duration::from_secs(1);

/// Un échantillon doit-il être envoyé, sachant le dernier déjà envoyé ? Se décide sur l'instant
/// **monotone** : l'horloge murale peut reculer, pas celle-ci. Le `snapshot` couvre l'historique
/// jusqu'à son dernier échantillon ; le flux reprend après, sans trou ni doublon.
pub fn is_new(last_sent: Option<MonoDuration>, mono: MonoDuration) -> bool {
    last_sent.is_none_or(|last| mono > last)
}

/// Un nouveau flux est-il admis ?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    Admitted,
    /// L'agent a déjà trop de flux ouverts.
    TotalFull,
    /// Ce compte a déjà trop de flux ouverts.
    AccountFull,
}

/// Décide d'après le nombre de flux déjà ouverts (au total, et pour ce compte).
pub fn admission(
    open_total: usize,
    open_for_account: usize,
    max_total: usize,
    max_per_account: usize,
) -> Admission {
    if open_total >= max_total {
        Admission::TotalFull
    } else if open_for_account >= max_per_account {
        Admission::AccountFull
    } else {
        Admission::Admitted
    }
}

/// Un `subscribe` est-il admis, sachant le temps écoulé depuis le précédent de la connexion ?
pub fn may_subscribe(since_last: Option<Duration>, min_interval: Duration) -> bool {
    since_last.is_none_or(|elapsed| elapsed >= min_interval)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_strictly_newer_samples_are_sent() {
        let t = MonoDuration::seconds(100);
        assert!(is_new(None, t));
        assert!(is_new(Some(t), t + MonoDuration::milliseconds(1)));
        assert!(!is_new(Some(t), t));
        assert!(!is_new(Some(t), t - MonoDuration::seconds(1)));
    }

    #[test]
    fn the_client_heartbeat_fits_in_the_idle_timeout() {
        // Le client envoie un ping toutes les 2 s : au moins dix battements avant la coupure.
        assert!(IDLE_TIMEOUT >= Duration::from_secs(20));
    }

    #[test]
    fn streams_are_admitted_up_to_the_caps() {
        assert_eq!(admission(0, 0, 32, 4), Admission::Admitted);
        assert_eq!(admission(31, 3, 32, 4), Admission::Admitted);
        assert_eq!(admission(31, 4, 32, 4), Admission::AccountFull);
        assert_eq!(admission(32, 0, 32, 4), Admission::TotalFull);
        // Le plafond total prime : l'agent plein refuse tout le monde.
        assert_eq!(admission(32, 4, 32, 4), Admission::TotalFull);
    }

    #[test]
    fn a_subscription_per_second_at_most() {
        let second = Duration::from_secs(1);
        assert!(may_subscribe(None, second));
        assert!(!may_subscribe(Some(Duration::from_millis(999)), second));
        assert!(may_subscribe(Some(second), second));
        assert!(may_subscribe(Some(Duration::from_secs(60)), second));
    }
}

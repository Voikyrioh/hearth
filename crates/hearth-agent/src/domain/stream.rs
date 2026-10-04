//! Règles du flux temps réel (BR-DASH-001, BR-DASH-002, BR-DASH-011) : délais et réglages par
//! défaut, et la décision de ne jamais envoyer deux fois le même échantillon.

use std::time::Duration;

use time::OffsetDateTime;

use super::metrics::HistoryWindow;

/// Historique joint au `snapshot` : les 5 dernières minutes.
pub const SNAPSHOT_WINDOW: HistoryWindow = HistoryWindow::FiveMinutes;

/// Sans le moindre message du client pendant ce délai, le flux est fermé : le client envoie un
/// `ping` toutes les 2 s, un silence aussi long est un lien mort.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

/// Période de revérification de la session pendant le flux (révocation, expiration).
pub const SESSION_CHECK_PERIOD: Duration = Duration::from_secs(5);

/// Temps accordé à l'envoi d'un message : un client qui ne lit plus est abandonné, il ne retient
/// ni l'agent ni les autres abonnés.
pub const SEND_TIMEOUT: Duration = Duration::from_secs(10);

/// Un échantillon doit-il être envoyé, sachant le dernier déjà envoyé ? Le `snapshot` couvre
/// l'historique jusqu'à son dernier échantillon ; le flux reprend après, sans trou ni doublon.
pub fn is_new(last_sent: Option<OffsetDateTime>, at: OffsetDateTime) -> bool {
    last_sent.is_none_or(|last| at > last)
}

#[cfg(test)]
mod tests {
    use time::Duration as TimeDuration;

    use super::*;

    #[test]
    fn only_strictly_newer_samples_are_sent() {
        let t = OffsetDateTime::UNIX_EPOCH + TimeDuration::seconds(100);
        assert!(is_new(None, t));
        assert!(is_new(Some(t), t + TimeDuration::milliseconds(1)));
        assert!(!is_new(Some(t), t));
        assert!(!is_new(Some(t), t - TimeDuration::seconds(1)));
    }

    #[test]
    fn the_client_heartbeat_fits_in_the_idle_timeout() {
        // Le client envoie un ping toutes les 2 s : au moins dix battements avant la coupure.
        assert!(IDLE_TIMEOUT >= Duration::from_secs(20));
    }
}

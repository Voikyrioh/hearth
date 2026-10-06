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

/// Flux **authentifiés** ouverts en même temps, au total.
pub const MAX_STREAMS_TOTAL: usize = 32;

/// Flux authentifiés ouverts en même temps par un même compte.
pub const MAX_STREAMS_PER_ACCOUNT: usize = 4;

/// Connexions **pas encore authentifiées** en attente de leur `auth`, au total. Quota distinct de
/// celui des flux : un anonyme qui ouvre des connexions muettes ne prend jamais la place d'un
/// flux authentifié.
pub const MAX_PENDING_TOTAL: usize = 16;

/// Connexions en attente d'authentification depuis une même adresse.
pub const MAX_PENDING_PER_ADDRESS: usize = 2;

/// Places d'attente d'authentification réservées aux adresses déjà connues (qui ont une session
/// valide ou une connexion réussie récente, ADR-0022) : un inconnu n'en prend jamais plus de
/// `MAX_PENDING_TOTAL - RESERVED_PENDING_FOR_KNOWN`, les autres restent pour un client légitime.
pub const RESERVED_PENDING_FOR_KNOWN: usize = 4;

/// Délai minimal entre deux `subscribe` d'une même connexion : chacun coûte un `snapshot`.
pub const MIN_SUBSCRIBE_INTERVAL: Duration = Duration::from_secs(1);

/// Un échantillon doit-il être envoyé, sachant le dernier déjà envoyé ? Se décide sur l'instant
/// **monotone** : l'horloge murale peut reculer, pas celle-ci. Le `snapshot` couvre l'historique
/// jusqu'à son dernier échantillon ; le flux reprend après, sans trou ni doublon.
pub fn is_new(last_sent: Option<MonoDuration>, mono: MonoDuration) -> bool {
    last_sent.is_none_or(|last| mono > last)
}

/// Reste-t-il de la place : sous le plafond total **et** sous le plafond de celui qui demande
/// (une adresse pour les connexions en attente, un compte pour les flux) ?
pub fn within_caps(open_total: usize, open_own: usize, max_total: usize, max_own: usize) -> bool {
    open_total < max_total && open_own < max_own
}

/// Places d'attente réservées aux adresses connues pour un plafond total donné : au plus
/// `RESERVED_PENDING_FOR_KNOWN`, et jamais plus du quart du total (un plafond réduit, en test, ne
/// laisse pas les inconnus sans place).
pub fn reserved_for_known(max_total: usize) -> usize {
    RESERVED_PENDING_FOR_KNOWN.min(max_total / 4)
}

/// L'adresse d'une connexion en attente est-elle déjà connue de l'agent ?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    Unknown,
    Known,
}

/// Pourquoi une place d'attente est refusée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingRefusal {
    /// Cette adresse a déjà tout son quota : une adresse connue n'y échappe pas.
    AddressFull,
    /// L'agent n'a plus de place pour ce type d'adresse (les inconnues s'arrêtent avant les
    /// places réservées).
    Saturated,
}

/// Une connexion de plus peut-elle attendre son `auth` ? `total` : connexions déjà en attente,
/// `own` : celles de la même adresse. Une adresse connue peut prendre les places réservées.
pub fn admit_pending(
    total: usize,
    own: usize,
    standing: Standing,
    max_total: usize,
    reserved_for_known: usize,
    max_own: usize,
) -> Result<(), PendingRefusal> {
    if own >= max_own {
        return Err(PendingRefusal::AddressFull);
    }
    let ceiling = match standing {
        Standing::Known => max_total,
        Standing::Unknown => max_total.saturating_sub(reserved_for_known),
    };
    if total >= ceiling {
        return Err(PendingRefusal::Saturated);
    }
    Ok(())
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
    fn a_request_is_admitted_below_both_caps_only() {
        assert!(within_caps(0, 0, 32, 4));
        assert!(within_caps(31, 3, 32, 4));
        assert!(!within_caps(31, 4, 32, 4), "plafond du demandeur");
        assert!(!within_caps(32, 0, 32, 4), "plafond total");
        assert!(!within_caps(32, 4, 32, 4));
    }

    #[test]
    fn the_pre_authentication_quota_is_small_and_distinct() {
        assert_eq!(MAX_PENDING_PER_ADDRESS, 2);
        const { assert!(MAX_PENDING_TOTAL < MAX_STREAMS_TOTAL) };
    }

    #[test]
    fn unknown_addresses_stop_before_the_places_reserved_for_known_ones() {
        let (total, reserved, own) = (MAX_PENDING_TOTAL, RESERVED_PENDING_FOR_KNOWN, 2);
        let ceiling = total - reserved;
        let admit = |count, standing, own_count| {
            admit_pending(count, own_count, standing, total, reserved, own)
        };
        assert_eq!(admit(ceiling - 1, Standing::Unknown, 0), Ok(()));
        assert_eq!(
            admit(ceiling, Standing::Unknown, 0),
            Err(PendingRefusal::Saturated)
        );
        assert_eq!(admit(ceiling, Standing::Known, 0), Ok(()), "place réservée");
        assert_eq!(admit(total - 1, Standing::Known, 0), Ok(()));
        assert_eq!(
            admit(total, Standing::Known, 0),
            Err(PendingRefusal::Saturated)
        );
    }

    #[test]
    fn a_known_address_keeps_its_own_per_address_quota() {
        assert_eq!(
            admit_pending(0, 2, Standing::Known, 16, 4, 2),
            Err(PendingRefusal::AddressFull)
        );
        assert_eq!(
            admit_pending(0, 2, Standing::Unknown, 16, 4, 2),
            Err(PendingRefusal::AddressFull)
        );
    }

    #[test]
    fn the_reservation_is_four_places_of_sixteen_and_never_more_than_a_quarter() {
        assert_eq!(
            reserved_for_known(MAX_PENDING_TOTAL),
            RESERVED_PENDING_FOR_KNOWN
        );
        assert_eq!(reserved_for_known(3), 0);
        assert_eq!(reserved_for_known(8), 2);
        assert_eq!(reserved_for_known(0), 0);
    }

    #[test]
    fn the_reservation_leaves_room_to_the_unknown() {
        const { assert!(RESERVED_PENDING_FOR_KNOWN > 0) };
        const { assert!(MAX_PENDING_TOTAL - RESERVED_PENDING_FOR_KNOWN >= MAX_PENDING_PER_ADDRESS) };
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

//! Ordre d'éviction des tables de suivi bornées (compteurs de connexion et ralentissements par
//! identifiant, ADR-0022) : une règle de sécurité, donc écrite ici, **une seule fois**, pour les
//! deux tables. Les adaptateurs la traduisent en tri (SQL) et leurs tests la comparent à `rank`.
//!
//! On oublie d'abord les lignes **sans attente en cours**, puis celles de **moins d'échecs**, puis
//! les **plus anciennes** : un flot d'identifiants ou d'adresses inventés (un échec chacun) évince
//! ses propres lignes, jamais celle de l'identifiant réellement attaqué ni une attente en cours.

use time::OffsetDateTime;

/// Rang d'une ligne : les plus petits rangs sont oubliés les premiers.
pub fn rank(
    waiting: bool,
    failures: u32,
    last_activity: OffsetDateTime,
) -> (bool, u32, OffsetDateTime) {
    (waiting, failures, last_activity)
}

#[cfg(test)]
mod tests {
    use time::Duration;

    use super::*;

    fn t0() -> OffsetDateTime {
        OffsetDateTime::UNIX_EPOCH + Duration::days(20_000)
    }

    #[test]
    fn a_wait_in_progress_is_evicted_last() {
        assert!(rank(false, 50, t0()) < rank(true, 1, t0()));
    }

    #[test]
    fn fewer_failures_go_first_then_the_oldest() {
        assert!(rank(false, 1, t0()) < rank(false, 2, t0() - Duration::days(1)));
        assert!(rank(false, 1, t0() - Duration::days(1)) < rank(false, 1, t0()));
    }
}

//! Règles de portée du journal : qui le lit (BR-AUDIT-001), ce qui y entre (BR-AUDIT-003 et
//! BR-AUDIT-004), combien de temps on le garde (BR-AUDIT-008).

use time::{Duration, OffsetDateTime};

use super::event::OutcomeKind;
use crate::domain::accounts::Role;

/// Durée de conservation d'une entrée.
pub const RETENTION: Duration = Duration::days(90);
/// Nombre maximal d'entrées conservées : les plus anciennes partent d'abord.
pub const MAX_ENTRIES: u64 = 50_000;

/// Lignes supprimées par transaction : la purge procède par lots, chacun dans sa propre
/// transaction, pour ne pas tenir longtemps le verrou d'écriture (chaque ligne supprimée met aussi
/// à jour la table de recherche).
pub const PURGE_BATCH: u64 = 1_000;
/// Le plafond est aussi contrôlé par l'écriture : toutes les 500 entrées écrites.
pub const CAP_CHECK_EVERY: u64 = 500;

/// Avant cette date, une entrée est trop ancienne.
pub fn retention_cutoff(now: OffsetDateTime) -> OffsetDateTime {
    now - RETENTION
}

/// Combien d'entrées en trop, quand le journal en compte `count` : à supprimer, les plus
/// anciennes d'abord. La première limite atteinte (âge ou nombre) joue : la purge applique l'âge,
/// puis cette règle sur ce qui reste.
pub fn excess_entries(count: u64) -> u64 {
    count.saturating_sub(MAX_ENTRIES)
}

/// BR-AUDIT-001 : seul un administrateur lit le journal. **Une seule source de vérité** :
/// `Role::can_read_audit`, que le flux temps réel (sujet `audit`, rôle relu toutes les 5 s)
/// interroge directement ; cette fonction est le nom du journal pour la même règle.
pub fn can_read_journal(role: Role) -> bool {
    role.can_read_audit()
}

/// Nature d'une requête : elle consulte, ou elle modifie quelque chose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestKind {
    Consultation,
    Modification,
}

/// BR-AUDIT-003 et BR-AUDIT-004 : une requête de cette nature, avec ce résultat, entre-t-elle au
/// journal ?
///
/// - Un refus faute de droits est toujours consigné, consultation comprise (BR-AUDIT-021).
/// - Une requête qui modifie est consignée, réussie ou échouée.
/// - Une consultation qui aboutit ou échoue ne l'est pas (tableau de bord, journal, état).
pub fn is_journaled(kind: RequestKind, outcome: OutcomeKind) -> bool {
    match (kind, outcome) {
        (_, OutcomeKind::Denied) => true,
        (RequestKind::Modification, _) => true,
        (RequestKind::Consultation, OutcomeKind::Ok | OutcomeKind::Failed) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retention_is_ninety_days() {
        let now = OffsetDateTime::UNIX_EPOCH + Duration::days(1000);
        assert_eq!(retention_cutoff(now), now - Duration::days(90));
    }

    #[test]
    fn the_journal_keeps_fifty_thousand_entries_and_trims_the_surplus() {
        assert_eq!(excess_entries(0), 0);
        assert_eq!(excess_entries(MAX_ENTRIES), 0);
        assert_eq!(excess_entries(MAX_ENTRIES + 1), 1);
        assert_eq!(excess_entries(MAX_ENTRIES + 1234), 1234);
        assert_eq!(excess_entries(u64::MAX), u64::MAX - MAX_ENTRIES);
    }

    #[test]
    fn only_an_administrator_reads_the_journal() {
        assert!(can_read_journal(Role::Admin));
        assert!(!can_read_journal(Role::ReadOnly));
    }

    #[test]
    fn what_is_journaled_follows_the_rules_of_the_spec() {
        use OutcomeKind::{Denied, Failed, Ok};
        use RequestKind::{Consultation, Modification};
        // Modifications : toujours, quel que soit le résultat.
        for outcome in [Ok, Denied, Failed] {
            assert!(is_journaled(Modification, outcome), "{outcome:?}");
        }
        // Consultations : jamais, sauf un refus faute de droits.
        assert!(!is_journaled(Consultation, Ok));
        assert!(!is_journaled(Consultation, Failed));
        assert!(is_journaled(Consultation, Denied));
    }
}

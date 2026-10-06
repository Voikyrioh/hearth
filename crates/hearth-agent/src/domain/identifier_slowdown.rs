//! Ralentissement par identifiant (ADR-0022, BR-CONN-018).
//!
//! Un compteur par identifiant saisi (qu'il existe ou non) compte les échecs venus d'adresses
//! **non connues** du compte. Après `FREE_FAILURES` échecs, chaque échec impose une attente avant
//! la tentative suivante : `FIRST_DELAY` doublée, **plafonnée à `MAX_DELAY`**. Jamais un blocage :
//! même sous attaque continue, une tentative reste possible à chaque fin d'attente.
//!
//! Fonctions pures `(état, maintenant) → (état, décision)`. Les dates sont des dates murales
//! persistées (l'attente doit survivre à un redémarrage) ; les garde-fous contre une horloge qui
//! recule sont dans `remaining` et `record_failure`.

use time::{Duration, OffsetDateTime};

/// Échecs sans attente : le compteur du couple (identifiant, adresse) verrouille déjà au 5e
/// depuis une même adresse, une personne qui se trompe ne voit donc jamais ce compteur.
pub const FREE_FAILURES: u32 = 10;
/// Attente imposée par le premier échec au-delà des échecs gratuits.
pub const FIRST_DELAY: Duration = Duration::seconds(2);
/// Plafond explicite de l'attente.
pub const MAX_DELAY: Duration = Duration::seconds(120);
/// Le compteur repart à zéro après ce délai sans échec.
pub const RESET_AFTER: Duration = Duration::minutes(30);
/// Identifiants suivis au plus ; au-delà, on oublie d'abord les moins attaqués, les plus anciens.
pub const MAX_TRACKED: usize = 10_000;

/// Ce que l'on retient d'un identifiant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Slowdown {
    pub failures: u32,
    /// Fin de l'attente en cours.
    pub wait_until: Option<OffsetDateTime>,
    pub last_failure_at: Option<OffsetDateTime>,
}

/// L'état observé à `now`, **ramené à ce qu'une attente légitime peut être** : une attente qui se
/// termine à plus de `MAX_DELAY` de `now` ne peut venir que d'une horloge qui a reculé (une
/// attente légitime n'est jamais plus loin devant `now` que son plafond). Elle est alors ramenée
/// à `now + MAX_DELAY` : un recul d'horloge ne prolonge jamais une attente au-delà du plafond, ni
/// ne la raccourcit à zéro. Rend l'état normalisé (à réécrire s'il a changé) et l'attente
/// restante, `None` s'il n'y en a pas. Une horloge avancée met fin aux attentes (aucun blocage).
pub fn observe(state: &Slowdown, now: OffsetDateTime) -> (Slowdown, Option<Duration>) {
    let ceiling = now + MAX_DELAY;
    let normalized = Slowdown {
        wait_until: state.wait_until.map(|until| until.min(ceiling)),
        ..*state
    };
    let wait = normalized
        .wait_until
        .map(|until| until - now)
        .filter(|wait| *wait > Duration::ZERO);
    (normalized, wait)
}

/// Attente restante à `now` (voir `observe`).
pub fn remaining(state: &Slowdown, now: OffsetDateTime) -> Option<Duration> {
    observe(state, now).1
}

/// Attente imposée par l'échec numéro `failures`.
fn delay_for(failures: u32) -> Option<Duration> {
    let beyond = failures.checked_sub(FREE_FAILURES + 1)?;
    let seconds = FIRST_DELAY
        .whole_seconds()
        .saturating_mul(1_i64 << beyond.min(16));
    Some(Duration::seconds(seconds).min(MAX_DELAY))
}

/// Un échec de plus (venu d'une adresse non connue). Rend le nouvel état et l'attente qu'il
/// impose, s'il y en a une. Un échec compté pendant une attente ne change rien : la tentative
/// n'aurait pas dû être admise.
pub fn record_failure(state: Slowdown, now: OffsetDateTime) -> (Slowdown, Option<Duration>) {
    if let Some(wait) = remaining(&state, now) {
        return (state, Some(wait));
    }
    // Sans échec depuis RESET_AFTER (dans un sens ou dans l'autre : une horloge très reculée ne
    // fige pas le compteur) : on repart de zéro.
    let stale = state
        .last_failure_at
        .is_none_or(|last| (now - last).abs() >= RESET_AFTER);
    let failures = if stale { 0 } else { state.failures }.saturating_add(1);
    let wait = delay_for(failures);
    (
        Slowdown {
            failures,
            wait_until: wait.map(|delay| now + delay),
            last_failure_at: Some(now),
        },
        wait,
    )
}

/// Lignes à oublier pour revenir sous `MAX_TRACKED`.
pub fn excess(count: usize) -> usize {
    count.saturating_sub(MAX_TRACKED)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> OffsetDateTime {
        OffsetDateTime::UNIX_EPOCH + Duration::days(20_000)
    }

    /// `count` échecs, chacun fait quand l'attente précédente est finie ; rend l'état, la dernière
    /// décision et l'instant suivant.
    fn failures(count: u32) -> (Slowdown, Option<Duration>, OffsetDateTime) {
        let mut state = Slowdown::default();
        let mut wait = None;
        let mut now = t0();
        for _ in 0..count {
            (state, wait) = record_failure(state, now);
            now = state.wait_until.unwrap_or(now) + Duration::seconds(1);
        }
        (state, wait, now)
    }

    #[test]
    fn the_first_ten_failures_cost_nothing() {
        for count in 1..=FREE_FAILURES {
            let (state, wait, _) = failures(count);
            assert_eq!(wait, None, "{count} échecs");
            assert_eq!(state.wait_until, None);
            assert_eq!(state.failures, count);
        }
    }

    #[test]
    fn the_delay_doubles_from_two_seconds_up_to_the_two_minute_cap() {
        let expected = [2, 4, 8, 16, 32, 64, 120, 120, 120];
        for (index, seconds) in expected.into_iter().enumerate() {
            let count = FREE_FAILURES + 1 + u32::try_from(index).unwrap_or(0);
            let (_, wait, _) = failures(count);
            assert_eq!(wait, Some(Duration::seconds(seconds)), "échec {count}");
        }
    }

    #[test]
    fn the_delay_never_exceeds_the_cap_whatever_the_count() {
        let state = Slowdown {
            failures: u32::MAX,
            wait_until: None,
            last_failure_at: Some(t0()),
        };
        let (next, wait) = record_failure(state, t0() + Duration::seconds(1));
        assert_eq!(wait, Some(MAX_DELAY));
        assert_eq!(next.failures, u32::MAX);
    }

    #[test]
    fn during_the_wait_the_remaining_time_is_announced_and_nothing_changes() {
        let (state, _, _) = failures(FREE_FAILURES + 3);
        let until = state.wait_until.expect("attente");
        let now = until - Duration::seconds(3);
        assert_eq!(remaining(&state, now), Some(Duration::seconds(3)));
        assert_eq!(
            record_failure(state, now),
            (state, Some(Duration::seconds(3)))
        );
    }

    #[test]
    fn the_attempt_exactly_at_the_end_of_the_wait_is_admitted() {
        let (state, _, _) = failures(FREE_FAILURES + 1);
        let until = state.wait_until.expect("attente");
        assert_eq!(remaining(&state, until), None);
    }

    #[test]
    fn thirty_minutes_without_failure_start_over() {
        let (state, _, _) = failures(FREE_FAILURES + 5);
        let later = state.last_failure_at.expect("dernier échec") + RESET_AFTER;
        let (next, wait) = record_failure(state, later);
        assert_eq!(next.failures, 1);
        assert_eq!(wait, None);
    }

    #[test]
    fn a_failure_just_inside_the_reset_delay_keeps_counting() {
        let (state, _, _) = failures(FREE_FAILURES + 5);
        let last = state.last_failure_at.expect("dernier échec");
        let until = state.wait_until.expect("attente");
        let at = (last + RESET_AFTER - Duration::seconds(1)).max(until);
        let (next, _) = record_failure(state, at);
        assert_eq!(next.failures, state.failures + 1);
    }

    #[test]
    fn a_clock_set_back_never_extends_the_wait_beyond_the_cap_nor_cuts_it_to_zero() {
        // Une attente posée à t0 (2 s) ; l'horloge recule ensuite d'une heure, puis d'un an.
        for set_back in [Duration::hours(1), Duration::days(365)] {
            let state = Slowdown {
                failures: 11,
                wait_until: Some(t0() + Duration::seconds(2)),
                last_failure_at: Some(t0()),
            };
            let now = t0() - set_back;
            let (normalized, wait) = observe(&state, now);
            assert_eq!(wait, Some(MAX_DELAY), "ni zéro, ni plus que le plafond");
            assert_eq!(normalized.wait_until, Some(now + MAX_DELAY));
            // L'attente ainsi ramenée se termine bien au plafond, même si l'horloge reste reculée.
            let later = now + MAX_DELAY;
            assert_eq!(remaining(&normalized, later), None);
        }
    }

    #[test]
    fn a_last_failure_far_in_the_future_does_not_freeze_the_counter() {
        let state = Slowdown {
            failures: 50,
            wait_until: None,
            last_failure_at: Some(t0() + Duration::days(3)),
        };
        let (next, wait) = record_failure(state, t0());
        assert_eq!(next.failures, 1);
        assert_eq!(wait, None);
    }

    #[test]
    fn a_clock_set_forward_ends_the_wait() {
        let (state, _, _) = failures(FREE_FAILURES + 4);
        let until = state.wait_until.expect("attente");
        assert_eq!(remaining(&state, until + Duration::days(1)), None);
    }

    #[test]
    fn the_cap_is_two_minutes_and_the_table_is_bounded() {
        assert_eq!(MAX_DELAY, Duration::minutes(2));
        assert_eq!(excess(MAX_TRACKED), 0);
        assert_eq!(excess(MAX_TRACKED + 7), 7);
        assert_eq!(excess(0), 0);
    }
}

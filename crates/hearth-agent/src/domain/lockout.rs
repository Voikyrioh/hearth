//! Verrouillage progressif des tentatives de connexion (BR-CONN-006, BR-CONN-007).
//!
//! Fonction pure `(état, événement, maintenant) → (état, décision)` : le stockage lit l'état, appelle
//! `step`, écrit le nouvel état. Les tentatives sont comptées par couple identifiant + adresse du
//! client, que l'identifiant existe ou non : un identifiant inconnu se verrouille comme un autre,
//! sinon le verrouillage révélerait quels comptes existent (BR-CONN-013).
//!
//! Palier : le 5e échec impose 1 minute d'attente, chaque échec suivant (fait après l'attente)
//! la double, plafonnée à 15 minutes. Un succès remet tout à zéro.

use time::{Duration, OffsetDateTime};

/// Nombre d'échecs qui déclenche la première attente.
pub const FAILURES_BEFORE_LOCK: u32 = 5;
/// Première attente.
pub const FIRST_LOCK: Duration = Duration::seconds(60);
/// Plafond de l'attente.
pub const MAX_LOCK: Duration = Duration::seconds(15 * 60);

/// Un compteur sans activité depuis ce délai (et sans attente en cours) est oublié (BR-CONN-006).
pub const ATTEMPT_RETENTION: Duration = Duration::hours(24);

/// Longueur maximale (en caractères) de l'identifiant retenu dans la clé : borne la taille du
/// stockage face à un client qui enverrait des identifiants démesurés.
const MAX_KEY_PART: usize = 64;

/// Clé du compteur : identifiant saisi (normalisé) + adresse du client.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AttemptKey(String);

impl AttemptKey {
    pub fn new(username: &str, addr: &str) -> Self {
        let username: String = username
            .trim()
            .to_lowercase()
            .chars()
            .take(MAX_KEY_PART)
            .collect();
        let addr: String = addr.chars().take(MAX_KEY_PART).collect();
        Self(format!("{username}|{addr}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Ce que l'on retient d'un couple identifiant + adresse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LockoutState {
    pub failures: u32,
    pub locked_until: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockoutEvent {
    /// Une tentative arrive : est-elle admise avant même de vérifier le mot de passe ?
    Attempt,
    /// Le mot de passe (ou l'identifiant) était faux.
    Failed,
    /// La connexion a réussi.
    Succeeded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockoutDecision {
    Allowed,
    /// Refus : attendre `retry_after` avant la prochaine tentative.
    Blocked {
        retry_after: Duration,
    },
}

/// Durée d'attente imposée par l'échec numéro `failures` (à partir du 5e).
fn lock_duration(failures: u32) -> Duration {
    let doublings = failures.saturating_sub(FAILURES_BEFORE_LOCK).min(16);
    let seconds = FIRST_LOCK
        .whole_seconds()
        .saturating_mul(1_i64 << doublings);
    Duration::seconds(seconds).min(MAX_LOCK)
}

pub fn step(
    state: LockoutState,
    event: LockoutEvent,
    now: OffsetDateTime,
) -> (LockoutState, LockoutDecision) {
    let still_locked = state.locked_until.filter(|&until| until > now);
    match event {
        LockoutEvent::Attempt => match still_locked {
            Some(until) => (
                state,
                LockoutDecision::Blocked {
                    retry_after: until - now,
                },
            ),
            None => (state, LockoutDecision::Allowed),
        },
        // Un échec compté pendant une attente ne rallonge rien : la tentative n'aurait pas dû
        // être admise ; on garde l'état.
        LockoutEvent::Failed if still_locked.is_some() => {
            let retry_after = still_locked.map_or(Duration::ZERO, |until| until - now);
            (state, LockoutDecision::Blocked { retry_after })
        }
        LockoutEvent::Failed => {
            let failures = state.failures.saturating_add(1);
            if failures >= FAILURES_BEFORE_LOCK {
                let wait = lock_duration(failures);
                (
                    LockoutState {
                        failures,
                        locked_until: Some(now + wait),
                    },
                    LockoutDecision::Blocked { retry_after: wait },
                )
            } else {
                (
                    LockoutState {
                        failures,
                        locked_until: None,
                    },
                    LockoutDecision::Allowed,
                )
            }
        }
        LockoutEvent::Succeeded => (LockoutState::default(), LockoutDecision::Allowed),
    }
}

/// Attente à annoncer au client, en secondes entières, arrondie au-dessus (jamais 0 tant que
/// l'attente n'est pas finie).
pub fn retry_after_seconds(wait: Duration) -> u64 {
    let whole = wait.whole_seconds().max(0);
    let rounded = if wait.subsec_nanoseconds() > 0 {
        whole + 1
    } else {
        whole
    };
    u64::try_from(rounded).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> OffsetDateTime {
        OffsetDateTime::UNIX_EPOCH + Duration::days(20_000)
    }

    /// Enchaîne `count` échecs espacés d'une seconde ; rend l'état et la dernière décision.
    fn fail_times(count: u32) -> (LockoutState, LockoutDecision, OffsetDateTime) {
        let mut state = LockoutState::default();
        let mut decision = LockoutDecision::Allowed;
        let mut now = t0();
        for _ in 0..count {
            (state, decision) = step(state, LockoutEvent::Failed, now);
            now += Duration::seconds(1);
        }
        (state, decision, now)
    }

    #[test]
    fn a_fresh_state_admits_attempts() {
        let (state, decision) = step(LockoutState::default(), LockoutEvent::Attempt, t0());
        assert_eq!(decision, LockoutDecision::Allowed);
        assert_eq!(state, LockoutState::default());
    }

    #[test]
    fn the_first_four_failures_do_not_lock() {
        for count in 1..FAILURES_BEFORE_LOCK {
            let (state, decision, _) = fail_times(count);
            assert_eq!(decision, LockoutDecision::Allowed, "{count} échecs");
            assert_eq!(state.failures, count);
            assert_eq!(state.locked_until, None);
        }
    }

    #[test]
    fn the_fifth_failure_locks_for_one_minute() {
        let (state, decision, _) = fail_times(5);
        assert_eq!(
            decision,
            LockoutDecision::Blocked {
                retry_after: Duration::seconds(60)
            }
        );
        assert_eq!(state.failures, 5);
        assert_eq!(
            state.locked_until,
            Some(t0() + Duration::seconds(4) + Duration::seconds(60))
        );
    }

    /// Chaque palier : échec fait juste après la fin de l'attente précédente.
    #[test]
    fn each_new_failure_doubles_the_wait_up_to_the_cap() {
        let expected = [60, 120, 240, 480, 900, 900, 900];
        let mut state = LockoutState::default();
        let mut now = t0();
        for _ in 0..4 {
            (state, _) = step(state, LockoutEvent::Failed, now);
        }
        for (index, seconds) in expected.into_iter().enumerate() {
            (state, _) = step(state, LockoutEvent::Attempt, now);
            let (next, decision) = step(state, LockoutEvent::Failed, now);
            state = next;
            assert_eq!(
                decision,
                LockoutDecision::Blocked {
                    retry_after: Duration::seconds(seconds)
                },
                "échec {}",
                5 + index
            );
            now = state.locked_until.expect("verrou posé");
        }
    }

    #[test]
    fn an_attempt_during_the_wait_is_blocked_with_the_remaining_time() {
        let (state, _, _) = fail_times(5);
        let locked_until = state.locked_until.expect("verrou posé");
        let now = locked_until - Duration::seconds(10);
        let (after, decision) = step(state, LockoutEvent::Attempt, now);
        assert_eq!(
            decision,
            LockoutDecision::Blocked {
                retry_after: Duration::seconds(10)
            }
        );
        assert_eq!(after, state, "refuser ne change rien");
    }

    #[test]
    fn the_attempt_exactly_at_the_end_of_the_wait_is_admitted() {
        let (state, _, _) = fail_times(5);
        let locked_until = state.locked_until.expect("verrou posé");
        let (_, decision) = step(state, LockoutEvent::Attempt, locked_until);
        assert_eq!(decision, LockoutDecision::Allowed);
    }

    #[test]
    fn a_failure_counted_during_the_wait_changes_nothing() {
        let (state, _, _) = fail_times(5);
        let now = t0() + Duration::seconds(10);
        let (after, decision) = step(state, LockoutEvent::Failed, now);
        assert_eq!(after, state);
        assert!(matches!(decision, LockoutDecision::Blocked { .. }));
    }

    #[test]
    fn a_success_resets_the_counter_and_the_wait() {
        let (state, _, now) = fail_times(4);
        let (after, decision) = step(state, LockoutEvent::Succeeded, now);
        assert_eq!(decision, LockoutDecision::Allowed);
        assert_eq!(after, LockoutState::default());
        let (state, _, _) = fail_times(5);
        let after_wait = state.locked_until.expect("verrou") + Duration::seconds(1);
        let (after, _) = step(state, LockoutEvent::Succeeded, after_wait);
        assert_eq!(after, LockoutState::default());
    }

    #[test]
    fn after_a_reset_the_count_starts_over() {
        let (state, _, now) = fail_times(4);
        let (state, _) = step(state, LockoutEvent::Succeeded, now);
        let (state, decision) = step(state, LockoutEvent::Failed, now);
        assert_eq!(decision, LockoutDecision::Allowed);
        assert_eq!(state.failures, 1);
    }

    #[test]
    fn a_huge_failure_count_stays_at_the_cap_without_overflow() {
        let state = LockoutState {
            failures: u32::MAX,
            locked_until: None,
        };
        let (_, decision) = step(state, LockoutEvent::Failed, t0());
        assert_eq!(
            decision,
            LockoutDecision::Blocked {
                retry_after: MAX_LOCK
            }
        );
    }

    #[test]
    fn the_retry_delay_is_announced_rounded_up() {
        assert_eq!(retry_after_seconds(Duration::seconds(60)), 60);
        assert_eq!(retry_after_seconds(Duration::milliseconds(59_001)), 60);
        assert_eq!(retry_after_seconds(Duration::milliseconds(1)), 1);
        assert_eq!(retry_after_seconds(Duration::ZERO), 0);
        assert_eq!(retry_after_seconds(Duration::seconds(-5)), 0);
    }

    #[test]
    fn the_key_is_per_username_and_address_and_ignores_case() {
        let a = AttemptKey::new(" Marie ", "10.0.0.1");
        assert_eq!(a, AttemptKey::new("marie", "10.0.0.1"));
        assert_ne!(a, AttemptKey::new("marie", "10.0.0.2"));
        assert_ne!(a, AttemptKey::new("paul", "10.0.0.1"));
    }

    #[test]
    fn the_key_stays_bounded_whatever_the_username() {
        let key = AttemptKey::new(&"x".repeat(10_000), "10.0.0.1");
        assert!(key.as_str().len() <= 2 * MAX_KEY_PART + 1);
    }
}

//! Verrouillage progressif des tentatives de connexion (BR-CONN-006, BR-CONN-007).
//!
//! Fonction pure `(état, événement, maintenant) → (état, décision)` : le stockage lit l'état, appelle
//! `step`, écrit le nouvel état. Les tentatives sont comptées par couple identifiant + adresse du
//! client, que l'identifiant existe ou non : un identifiant inconnu se verrouille comme un autre,
//! sinon le verrouillage révélerait quels comptes existent (BR-CONN-013).
//!
//! Palier : le 5e échec impose 1 minute d'attente, chaque échec suivant (fait après l'attente)
//! la double, plafonnée à 15 minutes. Un succès remet tout à zéro.
//!
//! Second compteur, par adresse seule et tous identifiants confondus (`step_address`) : 20
//! échecs en 10 minutes bloquent l'adresse, mêmes paliers doublés, même plafond. Il ferme le
//! balayage d'identifiants (un identifiant différent par requête échappe au compteur par couple).
//! Un succès ne le remet pas à zéro : un attaquant intercalerait sinon une connexion valide.

use time::{Duration, OffsetDateTime};

use super::text::is_unsafe_char;

/// Nombre d'échecs qui déclenche la première attente.
pub const FAILURES_BEFORE_LOCK: u32 = 5;
/// Première attente.
pub const FIRST_LOCK: Duration = Duration::seconds(60);
/// Plafond de l'attente.
pub const MAX_LOCK: Duration = Duration::seconds(15 * 60);

/// Échecs, toutes identités confondues, qui bloquent une adresse.
pub const ADDRESS_FAILURES_BEFORE_LOCK: u32 = 20;
/// Fenêtre dans laquelle ces échecs sont comptés (elle s'ouvre au premier échec).
pub const ADDRESS_WINDOW: Duration = Duration::minutes(10);

/// Connexions qu'une même adresse peut laisser en attente de leur tour (BR-CONN-007) : une est
/// traitée, huit attendent au plus ; la suivante est refusée tout de suite, sans mot de passe
/// gardé en mémoire.
pub const MAX_WAITING_PER_ADDRESS: usize = 8;

/// Un compteur sans activité depuis ce délai (et sans attente en cours) est oublié (BR-CONN-006).
pub const ATTEMPT_RETENTION: Duration = Duration::hours(24);

/// Séparateur identifiant / adresse dans la clé d'un couple : un caractère de contrôle, retiré
/// de l'identifiant, donc sans ambiguïté.
const SEPARATOR: char = '\u{1f}';

/// Longueur maximale (en caractères) de l'identifiant retenu dans la clé : borne la taille du
/// stockage face à un client qui enverrait des identifiants démesurés.
const MAX_KEY_PART: usize = 64;

/// Clé du compteur : identifiant saisi (normalisé) + adresse du client.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AttemptKey(String);

impl AttemptKey {
    pub fn new(username: &str, addr: &str) -> Self {
        // Sans caractère de contrôle, séparateur de ligne Unicode ni caractère de format
        // (bidirectionnel…) : la clé et les traces (une ligne de journal) ne peuvent pas être
        // forgées par l'identifiant, et le séparateur ne peut pas y apparaître.
        let username: String = username
            .trim()
            .to_lowercase()
            .chars()
            .filter(|&c| !is_unsafe_char(c))
            .take(MAX_KEY_PART)
            .collect();
        let addr: String = addr.chars().take(MAX_KEY_PART).collect();
        Self(format!("{username}{SEPARATOR}{addr}"))
    }

    /// L'identifiant normalisé de la clé (pour les traces ; vide pour une clé d'adresse).
    pub fn username(&self) -> &str {
        self.0
            .split_once(SEPARATOR)
            .map_or("", |(username, _)| username)
    }

    /// Clé du compteur par adresse seule. Ne peut pas coïncider avec une clé de couple (celles-ci
    /// contiennent toujours le séparateur, jamais une adresse).
    pub fn address(addr: &str) -> Self {
        let addr: String = addr.chars().take(MAX_KEY_PART).collect();
        Self(format!("addr:{addr}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Une connexion de plus peut-elle faire la queue pour son adresse ? `in_flight` = connexions
/// déjà admises pour cette adresse, celle en cours de traitement comprise.
pub fn admits_in_queue(in_flight: usize) -> bool {
    in_flight <= MAX_WAITING_PER_ADDRESS
}

/// Ce que l'on retient d'un couple identifiant + adresse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LockoutState {
    pub failures: u32,
    pub locked_until: Option<OffsetDateTime>,
    /// Ouverture de la fenêtre de comptage (compteur par adresse seulement).
    pub window_started_at: Option<OffsetDateTime>,
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

/// Durée d'attente imposée par l'échec numéro `failures`, une fois `threshold` atteint.
fn lock_duration(failures: u32, threshold: u32) -> Duration {
    let doublings = failures.saturating_sub(threshold).min(16);
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
                let wait = lock_duration(failures, FAILURES_BEFORE_LOCK);
                (
                    LockoutState {
                        failures,
                        locked_until: Some(now + wait),
                        window_started_at: None,
                    },
                    LockoutDecision::Blocked { retry_after: wait },
                )
            } else {
                (
                    LockoutState {
                        failures,
                        locked_until: None,
                        window_started_at: None,
                    },
                    LockoutDecision::Allowed,
                )
            }
        }
        LockoutEvent::Succeeded => (LockoutState::default(), LockoutDecision::Allowed),
    }
}

/// Compteur par adresse seule (tous identifiants confondus) : 20 échecs en 10 minutes bloquent
/// l'adresse, attente 1 minute doublée à chaque échec suivant, plafond 15 minutes. La fenêtre
/// s'ouvre au premier échec et se rouvre quand elle est écoulée. Un succès ne change rien.
pub fn step_address(
    state: LockoutState,
    event: LockoutEvent,
    now: OffsetDateTime,
) -> (LockoutState, LockoutDecision) {
    let still_locked = state.locked_until.filter(|&until| until > now);
    let blocked = |until: OffsetDateTime| LockoutDecision::Blocked {
        retry_after: until - now,
    };
    match event {
        LockoutEvent::Succeeded => (state, LockoutDecision::Allowed),
        LockoutEvent::Attempt => match still_locked {
            Some(until) => (state, blocked(until)),
            None => (state, LockoutDecision::Allowed),
        },
        LockoutEvent::Failed => {
            if let Some(until) = still_locked {
                return (state, blocked(until));
            }
            let in_window = state
                .window_started_at
                .is_some_and(|start| now - start < ADDRESS_WINDOW);
            let (failures, window_started_at) = if in_window {
                (state.failures.saturating_add(1), state.window_started_at)
            } else {
                (1, Some(now))
            };
            if failures >= ADDRESS_FAILURES_BEFORE_LOCK {
                let wait = lock_duration(failures, ADDRESS_FAILURES_BEFORE_LOCK);
                (
                    LockoutState {
                        failures,
                        locked_until: Some(now + wait),
                        window_started_at,
                    },
                    LockoutDecision::Blocked { retry_after: wait },
                )
            } else {
                (
                    LockoutState {
                        failures,
                        locked_until: None,
                        window_started_at,
                    },
                    LockoutDecision::Allowed,
                )
            }
        }
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
            window_started_at: None,
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

    // ---- compteur par adresse seule

    /// `count` échecs espacés de `gap` à partir de `t0` ; rend l'état, la décision, l'instant suivant.
    fn address_failures(
        count: u32,
        gap: Duration,
    ) -> (LockoutState, LockoutDecision, OffsetDateTime) {
        let mut state = LockoutState::default();
        let mut decision = LockoutDecision::Allowed;
        let mut now = t0();
        for _ in 0..count {
            (state, decision) = step_address(state, LockoutEvent::Failed, now);
            now += gap;
        }
        (state, decision, now)
    }

    #[test]
    fn nineteen_failures_in_the_window_do_not_block_the_address() {
        let (state, decision, _) = address_failures(19, Duration::seconds(10));
        assert_eq!(decision, LockoutDecision::Allowed);
        assert_eq!(state.failures, 19);
        assert_eq!(state.locked_until, None);
    }

    #[test]
    fn the_twentieth_failure_in_ten_minutes_blocks_for_one_minute() {
        let (state, decision, _) = address_failures(20, Duration::seconds(10));
        assert_eq!(
            decision,
            LockoutDecision::Blocked {
                retry_after: Duration::seconds(60)
            }
        );
        assert!(state.locked_until.is_some());
    }

    #[test]
    fn failures_spread_over_more_than_the_window_never_block() {
        // Un échec toutes les 31 secondes : jamais 20 dans la même fenêtre de 10 minutes.
        let (state, decision, _) = address_failures(60, Duration::seconds(31));
        assert_eq!(decision, LockoutDecision::Allowed);
        assert!(state.failures < ADDRESS_FAILURES_BEFORE_LOCK);
    }

    #[test]
    fn the_window_reopens_when_it_has_elapsed() {
        let (state, _, now) = address_failures(10, Duration::seconds(1));
        let later = now + ADDRESS_WINDOW;
        let (state, _) = step_address(state, LockoutEvent::Failed, later);
        assert_eq!(state.failures, 1);
        assert_eq!(state.window_started_at, Some(later));
    }

    #[test]
    fn address_waits_double_up_to_the_cap() {
        let mut state = LockoutState::default();
        let mut now = t0();
        for _ in 0..19 {
            (state, _) = step_address(state, LockoutEvent::Failed, now);
        }
        for expected in [60, 120, 240, 480, 900, 900] {
            let (next, decision) = step_address(state, LockoutEvent::Failed, now);
            assert_eq!(
                decision,
                LockoutDecision::Blocked {
                    retry_after: Duration::seconds(expected)
                }
            );
            state = next;
            // Attente écoulée, dans la même fenêtre : on repart juste après.
            now = state.locked_until.expect("verrou");
            state.window_started_at = Some(now - Duration::seconds(1));
        }
    }

    #[test]
    fn an_attempt_during_the_address_wait_is_blocked_and_changes_nothing() {
        let (state, _, _) = address_failures(20, Duration::seconds(1));
        let now = t0() + Duration::seconds(30);
        let (after, decision) = step_address(state, LockoutEvent::Attempt, now);
        assert_eq!(after, state);
        assert!(matches!(decision, LockoutDecision::Blocked { .. }));
        let (after, _) = step_address(state, LockoutEvent::Failed, now);
        assert_eq!(after, state);
    }

    #[test]
    fn a_success_does_not_reset_the_address_counter() {
        let (state, _, now) = address_failures(15, Duration::seconds(1));
        let (after, decision) = step_address(state, LockoutEvent::Succeeded, now);
        assert_eq!(after, state);
        assert_eq!(decision, LockoutDecision::Allowed);
    }

    #[test]
    fn the_address_key_cannot_collide_with_a_pair_key() {
        let address = AttemptKey::address("10.0.0.1");
        assert_ne!(address, AttemptKey::new("addr:10.0.0.1", ""));
        assert!(!address.as_str().contains(SEPARATOR));
        assert!(
            AttemptKey::new("marie", "10.0.0.1")
                .as_str()
                .contains(SEPARATOR)
        );
    }

    #[test]
    fn control_characters_never_reach_the_key_or_the_traces() {
        let key = AttemptKey::new("marie\nWARN forged line\r\t\u{1f}x", "10.0.0.1");
        assert_eq!(key.username(), "marieWARN forged linex".to_lowercase());
        assert!(!key.username().chars().any(char::is_control));
        assert!(!key.as_str().contains('\n'));
    }

    #[test]
    fn unicode_line_separators_and_format_characters_never_reach_the_key_or_the_traces() {
        let key = AttemptKey::new(
            "ma\u{2028}rie\u{2029}\u{202E}evil\u{200F}\u{2066}x\u{FEFF}",
            "10.0.0.1",
        );
        assert_eq!(key.username(), "marieevilx");
        assert!(
            !key.as_str()
                .contains(['\u{2028}', '\u{2029}', '\u{202E}', '\u{200F}'])
        );
    }

    #[test]
    fn one_connection_runs_and_eight_wait_the_ninth_waiter_is_refused() {
        assert!(admits_in_queue(0), "la première s'exécute");
        assert!(admits_in_queue(1), "première en attente");
        assert!(
            admits_in_queue(MAX_WAITING_PER_ADDRESS),
            "huitième en attente"
        );
        assert!(
            !admits_in_queue(MAX_WAITING_PER_ADDRESS + 1),
            "neuvième : refusée"
        );
        assert!(!admits_in_queue(usize::MAX));
    }

    #[test]
    fn a_pipe_in_the_username_is_kept_and_the_key_stays_unambiguous() {
        let key = AttemptKey::new("a|b", "10.0.0.1");
        assert_eq!(key.username(), "a|b");
        // « a|b » + « 10.0.0.1 » et « a » + « b|10.0.0.1 » ne coïncident pas.
        assert_ne!(key, AttemptKey::new("a", "b|10.0.0.1"));
        assert_eq!(AttemptKey::address("10.0.0.1").username(), "");
    }
}

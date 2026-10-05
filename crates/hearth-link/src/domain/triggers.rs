//! Déclencheurs de reconnexion immédiate (BR-RESIL-006) : réveil du PC, changement de réseau.

use std::collections::BTreeSet;
use std::net::IpAddr;
use std::time::Duration;

use super::time::{Mono, WallTime};

/// Saut d'horloge au-delà duquel on considère que le PC s'est réveillé.
pub const WAKE_JUMP: Duration = Duration::from_secs(5);

/// Le PC vient-il de se réveiller ? On compare deux relevés pris à `expected_period` d'écart :
/// - l'horloge monotone ou murale a avancé de plus que prévu de `jump` (veille, processus gelé) ;
/// - ou l'horloge murale et l'horloge monotone se sont écartées de plus de `jump` (l'une a
///   continué pendant la veille, l'autre non).
pub fn detect_wake(
    previous: (Mono, WallTime),
    now: (Mono, WallTime),
    expected_period: Duration,
    jump: Duration,
) -> bool {
    let mono = now.0.since(previous.0);
    let wall = now.1.since(previous.1);
    let limit = expected_period.saturating_add(jump);
    let drift = mono.abs_diff(wall);
    mono > limit || wall > limit || drift > jump
}

/// La liste des adresses locales a-t-elle changé ? (câble vers Wi-Fi, VPN, nouvelle adresse.)
pub fn network_changed(previous: &BTreeSet<IpAddr>, now: &BTreeSet<IpAddr>) -> bool {
    previous != now
}

#[cfg(test)]
mod tests {
    use super::*;

    const PERIOD: Duration = Duration::from_secs(1);

    fn at(mono_ms: u64, wall_ms: i64) -> (Mono, WallTime) {
        (Mono::from_millis(mono_ms), WallTime::from_millis(wall_ms))
    }

    #[test]
    fn a_regular_tick_is_not_a_wake() {
        assert!(!detect_wake(
            at(0, 1_000_000),
            at(1_000, 1_001_000),
            PERIOD,
            WAKE_JUMP
        ));
        // Un peu de retard n'est pas un réveil.
        assert!(!detect_wake(
            at(0, 1_000_000),
            at(1_900, 1_001_900),
            PERIOD,
            WAKE_JUMP
        ));
    }

    #[test]
    fn a_jump_above_five_seconds_is_a_wake() {
        assert!(detect_wake(at(0, 0), at(60_000, 60_000), PERIOD, WAKE_JUMP));
        // À la limite : période + 5 s tolérés, au-delà c'est un réveil.
        assert!(!detect_wake(at(0, 0), at(6_000, 6_000), PERIOD, WAKE_JUMP));
        assert!(detect_wake(at(0, 0), at(6_001, 6_001), PERIOD, WAKE_JUMP));
    }

    #[test]
    fn a_wall_clock_that_jumps_alone_is_a_wake() {
        // L'horloge monotone a stoppé pendant la veille, la murale a avancé d'une heure.
        assert!(detect_wake(
            at(0, 0),
            at(1_000, 3_601_000),
            PERIOD,
            WAKE_JUMP
        ));
    }

    #[test]
    fn a_wall_clock_set_backwards_is_not_a_wake() {
        assert!(!detect_wake(
            at(0, 100_000),
            at(1_000, 50_000),
            PERIOD,
            WAKE_JUMP
        ));
        assert!(!detect_wake(
            at(0, 100_000),
            at(1_000, 100_500),
            PERIOD,
            WAKE_JUMP
        ));
    }

    #[test]
    fn address_lists_are_compared_as_sets() {
        let a: BTreeSet<IpAddr> = ["192.168.1.5".parse().unwrap()].into();
        let b: BTreeSet<IpAddr> = ["192.168.1.5".parse().unwrap()].into();
        let c: BTreeSet<IpAddr> = ["10.0.0.9".parse().unwrap()].into();
        assert!(!network_changed(&a, &b));
        assert!(network_changed(&a, &c));
        assert!(network_changed(&a, &BTreeSet::new()));
    }
}

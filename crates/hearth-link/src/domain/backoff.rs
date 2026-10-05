//! Délais entre deux tentatives de reconnexion (BR-RESIL-005).
//!
//! Suite : 0,5 s, 1 s, 2 s, 4 s, 8 s, 15 s, 30 s, 30 s… sans fin, avec ± 20 % d'aléa pour que
//! plusieurs clients ne frappent pas ensemble. Le plafond de 30 s est une borne dure : l'aléa ne
//! la dépasse jamais. La source d'aléa est fournie par l'appelant (déterministe en test).

use std::time::Duration;

/// Délais de base, en millisecondes. Le dernier se répète indéfiniment.
pub const BASE_DELAYS_MS: [u64; 7] = [500, 1_000, 2_000, 4_000, 8_000, 15_000, 30_000];

/// Plafond absolu d'un délai (BR-RESIL-005).
pub const MAX_DELAY: Duration = Duration::from_secs(30);

/// Amplitude de l'aléa, en pour mille (± 20 %).
const JITTER_PERMILLE: u64 = 200;

#[derive(Debug, Clone)]
pub struct Backoff {
    failures: u32,
    /// Diviseur de tous les délais : 1 en production. Les tests de résilience le montent pour
    /// que 30 s ne durent pas 30 s.
    divisor: u32,
}

impl Default for Backoff {
    fn default() -> Self {
        Self::new()
    }
}

impl Backoff {
    pub fn new() -> Self {
        Self::scaled(1)
    }

    /// Mêmes délais divisés par `divisor` (au moins 1) : pour les tests.
    pub fn scaled(divisor: u32) -> Self {
        Self {
            failures: 0,
            divisor: divisor.max(1),
        }
    }

    /// Plafond d'un délai, aléa compris.
    pub fn cap(&self) -> Duration {
        MAX_DELAY / self.divisor
    }

    /// Nombre d'échecs depuis le dernier succès.
    pub fn failures(&self) -> u32 {
        self.failures
    }

    /// Délai de base du prochain échec, sans aléa.
    pub fn base_delay(&self) -> Duration {
        let index = usize::try_from(self.failures).unwrap_or(usize::MAX);
        let millis = BASE_DELAYS_MS
            .get(index)
            .or(BASE_DELAYS_MS.last())
            .copied()
            .unwrap_or(30_000);
        Duration::from_millis(millis) / self.divisor
    }

    /// Délai à attendre avant la prochaine tentative, puis compte un échec de plus.
    /// `random` : un entier tiré au hasard par l'appelant.
    pub fn next_delay(&mut self, random: u32) -> Duration {
        let delay = jittered_up_to(self.base_delay(), random, self.cap());
        self.failures = self.failures.saturating_add(1);
        delay
    }

    /// Un succès : la suite repart de 0,5 s.
    pub fn reset(&mut self) {
        self.failures = 0;
    }
}

/// `base` ± 20 % selon `random`, plafonné à 30 s.
pub fn jittered(base: Duration, random: u32) -> Duration {
    jittered_up_to(base, random, MAX_DELAY)
}

fn jittered_up_to(base: Duration, random: u32, cap: Duration) -> Duration {
    let base_ms = u64::try_from(base.as_millis()).unwrap_or(u64::MAX);
    let span = JITTER_PERMILLE * 2 + 1;
    // Facteur entre 800 et 1 200 pour mille : 800 + (0 ..= 400).
    let factor = 1_000 - JITTER_PERMILLE + u64::from(random) % span;
    let millis = base_ms.saturating_mul(factor) / 1_000;
    Duration::from_millis(millis).min(cap)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn delays(backoff: &mut Backoff) -> Vec<u128> {
        // random = 200 : facteur 1000 pour mille, c'est-à-dire sans aléa.
        (0..10)
            .map(|_| backoff.next_delay(200).as_millis())
            .collect()
    }

    #[test]
    fn the_sequence_doubles_then_stays_at_thirty_seconds() {
        let mut backoff = Backoff::new();
        assert_eq!(
            delays(&mut backoff),
            [
                500, 1_000, 2_000, 4_000, 8_000, 15_000, 30_000, 30_000, 30_000, 30_000
            ]
        );
    }

    #[test]
    fn a_success_restarts_the_sequence() {
        let mut backoff = Backoff::new();
        for _ in 0..5 {
            backoff.next_delay(200);
        }
        assert_eq!(backoff.failures(), 5);
        backoff.reset();
        assert_eq!(backoff.failures(), 0);
        assert_eq!(backoff.next_delay(200), Duration::from_millis(500));
    }

    #[test]
    fn jitter_stays_within_twenty_percent_and_never_above_the_cap() {
        for base_ms in BASE_DELAYS_MS {
            let base = Duration::from_millis(base_ms);
            for random in (0..=u32::MAX)
                .step_by(9_973)
                .chain([0, 1, 399, 400, 401, u32::MAX])
            {
                let delay = jittered(base, random);
                let low = u128::from(base_ms * 800 / 1_000);
                let high = u128::from((base_ms * 1_200 / 1_000).min(30_000));
                let millis = delay.as_millis();
                assert!(
                    (low..=high).contains(&millis),
                    "{base_ms} -> {millis} (random {random})"
                );
            }
        }
    }

    #[test]
    fn jitter_reaches_both_ends_of_the_range() {
        let base = Duration::from_millis(2_000);
        assert_eq!(jittered(base, 0).as_millis(), 1_600);
        assert_eq!(jittered(base, 400).as_millis(), 2_400);
        assert_eq!(jittered(base, 200).as_millis(), 2_000);
    }

    #[test]
    fn the_cap_holds_even_with_the_largest_jitter() {
        assert_eq!(jittered(MAX_DELAY, 400), MAX_DELAY);
        assert_eq!(jittered(MAX_DELAY, 0).as_millis(), 24_000);
    }

    #[test]
    fn a_scaled_backoff_keeps_the_shape_and_the_cap() {
        let mut backoff = Backoff::scaled(10);
        let delays: Vec<u128> = (0..9)
            .map(|_| backoff.next_delay(200).as_millis())
            .collect();
        assert_eq!(delays, [50, 100, 200, 400, 800, 1_500, 3_000, 3_000, 3_000]);
        // Même avec le plus grand aléa, le plafond réduit tient.
        let mut backoff = Backoff::scaled(10);
        for _ in 0..12 {
            assert!(backoff.next_delay(400) <= Duration::from_secs(3));
        }
        assert_eq!(Backoff::scaled(0).base_delay(), Duration::from_millis(500));
    }

    #[test]
    fn the_failure_counter_never_overflows() {
        let mut backoff = Backoff {
            failures: u32::MAX,
            divisor: 1,
        };
        assert!(backoff.next_delay(7).as_millis() > 0);
        assert_eq!(backoff.failures(), u32::MAX);
    }
}

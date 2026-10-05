//! Horloges et hasard réels.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::domain::time::{Mono, WallTime};
use crate::ports::{Clock, Rng};

/// Horloge du système : monotone depuis le démarrage, murale depuis l'époque Unix.
pub struct SystemClock {
    origin: Instant,
}

impl SystemClock {
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn mono(&self) -> Mono {
        Mono::from_millis(u64::try_from(self.origin.elapsed().as_millis()).unwrap_or(u64::MAX))
    }

    fn wall(&self) -> WallTime {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
            .unwrap_or(0);
        WallTime::from_millis(millis)
    }
}

/// Horloge qui suit celle de Tokio : sous `tokio::time::pause`, le temps virtuel avance seul.
/// Pour les tests de longue durée ; la date murale part d'une base donnée.
pub struct TokioClock {
    origin: tokio::time::Instant,
    wall_base: WallTime,
}

impl TokioClock {
    pub fn new(wall_base: WallTime) -> Self {
        Self {
            origin: tokio::time::Instant::now(),
            wall_base,
        }
    }
}

impl Clock for TokioClock {
    fn mono(&self) -> Mono {
        let elapsed = self.origin.elapsed();
        Mono::from_millis(u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
    }

    fn wall(&self) -> WallTime {
        self.wall_base.plus(self.origin.elapsed())
    }
}

/// Aléa du système. Si le système n'en donne pas, un mélange de l'horloge et d'un compteur
/// (suffisant pour décaler des tentatives).
#[derive(Default)]
pub struct OsRng {
    counter: std::sync::atomic::AtomicU64,
}

impl Rng for OsRng {
    fn next_u32(&self) -> u32 {
        let mut bytes = [0u8; 4];
        if getrandom::fill(&mut bytes).is_ok() {
            return u32::from_le_bytes(bytes);
        }
        let tick = self
            .counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let mixed = u64::from(nanos)
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(tick.wrapping_mul(0xBF58_476D_1CE4_E5B9));
        u32::try_from(mixed >> 32).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_clock_moves_forward() {
        let clock = SystemClock::new();
        let first = clock.mono();
        std::thread::sleep(std::time::Duration::from_millis(5));
        assert!(clock.mono() > first);
        assert!(clock.wall().as_millis() > 1_700_000_000_000);
    }

    #[test]
    fn the_os_rng_gives_different_numbers() {
        let rng = OsRng::default();
        let numbers: std::collections::HashSet<u32> = (0..16).map(|_| rng.next_u32()).collect();
        assert!(numbers.len() > 8);
    }

    #[tokio::test(start_paused = true)]
    async fn the_tokio_clock_follows_virtual_time() {
        let clock = TokioClock::new(WallTime::from_millis(1_000));
        tokio::time::sleep(std::time::Duration::from_secs(90)).await;
        assert_eq!(clock.mono().as_millis(), 90_000);
        assert_eq!(clock.wall().as_millis(), 91_000);
    }
}

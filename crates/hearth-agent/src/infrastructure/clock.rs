//! Horloge du système.
//!
//! La précision de l'horloge est celle du stockage : la milliseconde. La troncature se fait ici,
//! une seule fois, pour que la date que rend un cas d'usage après une écriture soit exactement
//! celle qu'on relira en base.

use std::time::Instant;

use time::OffsetDateTime;

use crate::application::ports::{Clock, MonotonicClock};

const NANOS_PER_MILLI: u32 = 1_000_000;

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> OffsetDateTime {
        truncate_to_millis(OffsetDateTime::now_utc())
    }
}

/// Horloge monotone du système : le temps écoulé depuis sa création (au démarrage de l'agent).
#[derive(Debug, Clone, Copy)]
pub struct SystemMonotonic(Instant);

impl SystemMonotonic {
    pub fn new() -> Self {
        Self(Instant::now())
    }
}

impl Default for SystemMonotonic {
    fn default() -> Self {
        Self::new()
    }
}

impl MonotonicClock for SystemMonotonic {
    fn elapsed(&self) -> time::Duration {
        time::Duration::try_from(self.0.elapsed()).unwrap_or(time::Duration::MAX)
    }
}

fn truncate_to_millis(date: OffsetDateTime) -> OffsetDateTime {
    let nanos = date.nanosecond() / NANOS_PER_MILLI * NANOS_PER_MILLI;
    date.replace_nanosecond(nanos).unwrap_or(date)
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Duration;

    #[test]
    fn truncation_drops_everything_below_the_millisecond() {
        let date = OffsetDateTime::UNIX_EPOCH + Duration::new(100, 123_456_789);
        assert_eq!(
            truncate_to_millis(date),
            OffsetDateTime::UNIX_EPOCH + Duration::new(100, 123_000_000)
        );
    }

    #[test]
    fn truncation_keeps_a_whole_millisecond() {
        let date = OffsetDateTime::UNIX_EPOCH + Duration::new(100, 5_000_000);
        assert_eq!(truncate_to_millis(date), date);
    }

    #[test]
    fn the_monotonic_clock_never_goes_back() {
        let clock = SystemMonotonic::new();
        let first = clock.elapsed();
        let second = clock.elapsed();
        assert!(first >= time::Duration::ZERO);
        assert!(second >= first);
    }

    #[test]
    fn the_system_clock_has_no_sub_millisecond_digits() {
        let now = SystemClock.now();
        assert_eq!(now.nanosecond() % NANOS_PER_MILLI, 0);
    }
}

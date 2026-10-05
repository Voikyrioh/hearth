//! Instants du domaine. Le domaine ne lit jamais d'horloge : on lui donne le temps.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Instant monotone, en millisecondes depuis une origine arbitraire (démarrage du processus).
/// Sert à mesurer des durées (seuils, silence, attente) : il ne recule jamais.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Mono(u64);

impl Mono {
    pub const ZERO: Self = Self(0);

    pub const fn from_millis(millis: u64) -> Self {
        Self(millis)
    }

    pub const fn as_millis(self) -> u64 {
        self.0
    }

    /// Durée écoulée depuis `earlier` (zéro si `earlier` est postérieur).
    pub fn since(self, earlier: Self) -> Duration {
        Duration::from_millis(self.0.saturating_sub(earlier.0))
    }

    /// Cet instant, `delay` plus tard (sature au lieu de déborder).
    pub fn after(self, delay: Duration) -> Self {
        let millis = u64::try_from(delay.as_millis()).unwrap_or(u64::MAX);
        Self(self.0.saturating_add(millis))
    }

    /// Cet instant, `delay` plus tôt (sature à zéro).
    pub fn before(self, delay: Duration) -> Self {
        let millis = u64::try_from(delay.as_millis()).unwrap_or(u64::MAX);
        Self(self.0.saturating_sub(millis))
    }
}

/// Instant mural, en millisecondes depuis l'époque Unix. Sert à dater pour l'affichage
/// (dernier contact, âge d'une donnée) : il peut sauter (réglage de l'heure, veille).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct WallTime(i64);

impl WallTime {
    pub const fn from_millis(millis: i64) -> Self {
        Self(millis)
    }

    pub const fn as_millis(self) -> i64 {
        self.0
    }

    pub fn plus(self, delay: Duration) -> Self {
        let millis = i64::try_from(delay.as_millis()).unwrap_or(i64::MAX);
        Self(self.0.saturating_add(millis))
    }

    pub fn minus(self, delay: Duration) -> Self {
        let millis = i64::try_from(delay.as_millis()).unwrap_or(i64::MAX);
        Self(self.0.saturating_sub(millis))
    }

    /// Durée écoulée depuis `earlier` (zéro si `earlier` est postérieur).
    pub fn since(self, earlier: Self) -> Duration {
        let millis = self.0.saturating_sub(earlier.0).max(0);
        Duration::from_millis(u64::try_from(millis).unwrap_or(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arithmetic_saturates_instead_of_overflowing() {
        let late = Mono::from_millis(u64::MAX - 1);
        assert_eq!(late.after(Duration::from_secs(5)).as_millis(), u64::MAX);
        assert_eq!(Mono::ZERO.before(Duration::from_secs(5)), Mono::ZERO);
        assert_eq!(Mono::ZERO.since(late), Duration::ZERO);
        let wall = WallTime::from_millis(i64::MAX);
        assert_eq!(wall.plus(Duration::from_secs(1)).as_millis(), i64::MAX);
        assert_eq!(
            WallTime::from_millis(i64::MIN)
                .minus(Duration::from_secs(1))
                .as_millis(),
            i64::MIN
        );
        assert_eq!(
            WallTime::from_millis(5).since(WallTime::from_millis(9)),
            Duration::ZERO
        );
    }

    #[test]
    fn durations_are_exact_to_the_millisecond() {
        let start = Mono::from_millis(1_000);
        assert_eq!(
            start.after(Duration::from_millis(2_999)).since(start),
            Duration::from_millis(2_999)
        );
    }
}

//! Pourcentage du téléchargement (BR-UPDATE-013) : un message par pourcentage entier, jamais un
//! par morceau reçu (le flux n'est pas inondé), jamais de recul.

/// Suit le téléchargement et dit quand le pourcentage affiché change.
#[derive(Debug, Default)]
pub struct PercentTracker {
    last: Option<u8>,
}

impl PercentTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Le nouveau pourcentage entier (0 à 100) s'il a changé, `None` sinon. Sans taille totale
    /// connue (ou nulle), le pourcentage est indéterminé : jamais de message.
    pub fn update(&mut self, received: u64, total: Option<u64>) -> Option<u8> {
        let total = total.filter(|total| *total > 0)?;
        let received = received.min(total);
        // Calcul en u128 : aucun dépassement, même pour des tailles énormes.
        let percent = u8::try_from(u128::from(received) * 100 / u128::from(total)).unwrap_or(100);
        if self.last.is_some_and(|last| percent <= last) {
            return None;
        }
        self.last = Some(percent);
        Some(percent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_known_size_gives_zero_then_only_changes() {
        let mut tracker = PercentTracker::new();
        assert_eq!(tracker.update(0, Some(1000)), Some(0));
        assert_eq!(tracker.update(5, Some(1000)), None, "0,5 % reste à 0 %");
        assert_eq!(tracker.update(10, Some(1000)), Some(1));
        assert_eq!(tracker.update(19, Some(1000)), None);
        assert_eq!(tracker.update(350, Some(1000)), Some(35));
        assert_eq!(tracker.update(1000, Some(1000)), Some(100));
        assert_eq!(
            tracker.update(1000, Some(1000)),
            None,
            "100 % une seule fois"
        );
    }

    #[test]
    fn the_percentage_never_goes_back_and_never_exceeds_100() {
        let mut tracker = PercentTracker::new();
        assert_eq!(tracker.update(500, Some(1000)), Some(50));
        assert_eq!(tracker.update(400, Some(1000)), None);
        assert_eq!(tracker.update(5000, Some(1000)), Some(100));
    }

    #[test]
    fn an_unknown_or_empty_total_never_gives_a_percentage() {
        let mut tracker = PercentTracker::new();
        assert_eq!(tracker.update(10, None), None);
        assert_eq!(tracker.update(10, Some(0)), None);
    }

    #[test]
    fn huge_sizes_do_not_overflow() {
        let mut tracker = PercentTracker::new();
        assert_eq!(tracker.update(u64::MAX / 2, Some(u64::MAX)), Some(49));
    }
}

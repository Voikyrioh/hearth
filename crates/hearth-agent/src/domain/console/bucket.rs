//! Le débit d'une console (HRT-268, ADR-0067 D7, BR-CONSOLE-008) : un seau à jetons, 20 commandes par minute et par
//! service, une rafale de 5. Du pur : le temps est donné en millisecondes par l'appelant, jamais lu ici.

/// Combien de commandes par minute.
pub const PER_MINUTE: u32 = 20;
/// La rafale permise.
pub const BURST: u32 = 5;

/// Un jeton revient toutes les `60 000 / PER_MINUTE` millisecondes.
const REFILL_MS: u64 = 60_000 / PER_MINUTE as u64;

#[derive(Debug, Clone)]
pub struct TokenBucket {
    /// Millièmes de jeton : un jeton entier vaut `REFILL_MS` unités (une unité par milliseconde écoulée).
    level: u64,
    last_ms: u64,
}

impl TokenBucket {
    /// Un seau plein à l'instant `now_ms`.
    pub fn full(now_ms: u64) -> Self {
        Self {
            level: u64::from(BURST) * REFILL_MS,
            last_ms: now_ms,
        }
    }

    /// Prend un jeton. Sinon : le nombre de secondes (au moins 1) avant le prochain.
    pub fn take(&mut self, now_ms: u64) -> Result<(), u64> {
        let elapsed = now_ms.saturating_sub(self.last_ms);
        self.last_ms = self.last_ms.max(now_ms);
        self.level = (self.level + elapsed).min(u64::from(BURST) * REFILL_MS);
        if self.level >= REFILL_MS {
            self.level -= REFILL_MS;
            Ok(())
        } else {
            Err((REFILL_MS - self.level).div_ceil(1000).max(1))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_burst_of_five_then_a_refusal_with_the_wait() {
        let mut bucket = TokenBucket::full(0);
        for _ in 0..BURST {
            assert_eq!(bucket.take(0), Ok(()));
        }
        assert_eq!(bucket.take(0), Err(3));
        // Un jeton revient au bout de 3 secondes.
        assert_eq!(bucket.take(2_999), Err(1));
        assert_eq!(bucket.take(3_000), Ok(()));
        assert!(bucket.take(3_000).is_err());
    }

    #[test]
    fn the_steady_rate_is_twenty_a_minute_and_a_long_pause_refills_only_the_burst() {
        let mut bucket = TokenBucket::full(0);
        let mut sent = 0;
        // Une commande toutes les 3 secondes pendant une minute : toutes passent.
        for step in 0..20u64 {
            if bucket.take(step * 3_000).is_ok() {
                sent += 1;
            }
        }
        assert_eq!(sent, 20);
        // Une longue pause ne donne pas plus que la rafale.
        let mut bucket = TokenBucket::full(0);
        for _ in 0..BURST {
            assert!(bucket.take(0).is_ok());
        }
        let after = 10 * 60_000;
        let passed = (0..10).filter(|_| bucket.take(after).is_ok()).count();
        assert_eq!(passed, BURST as usize);
    }

    #[test]
    fn a_clock_going_backwards_never_gives_tokens() {
        let mut bucket = TokenBucket::full(10_000);
        for _ in 0..BURST {
            assert!(bucket.take(10_000).is_ok());
        }
        assert!(bucket.take(0).is_err());
    }
}

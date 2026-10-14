//! La politique de reprise d'une lecture de journaux (HRT-67, `BR-SLOG-014`) : des nombres, aucun accès au
//! réseau ni à l'horloge. Les durées sont PASSÉES à la politique (la lecture mesure, la politique décide) : les
//! tests rejouent n'importe quelle suite de coupures sans attendre.
//!
//! Pourquoi une borne côté client : chaque ouverture de socket consomme une ouverture du frein de l'agent
//! (douze en rafale, une rendue toutes les cinq secondes, `BR-SLOG-027`). Sans borne ici, un socket qui tombe en
//! boucle viderait ce frein et l'utilisateur lirait « tu as ouvert beaucoup de journaux » sans rien avoir fait.

use std::time::Duration;

/// Coupures de suite avant d'arrêter (« injoignable ») : au plus ce nombre de réouvertures automatiques.
pub const MAX_FAILURES: u32 = 6;
/// Plafond du délai croissant entre deux réouvertures (avant l'aléa).
pub const MAX_WAIT: Duration = Duration::from_secs(30);
/// Une connexion qui a tenu au moins ce temps remet le compte des coupures à zéro : une coupure rare après des
/// heures de lecture n'use pas les essais, une boucle de coupures rapides les use tous.
pub const STABLE_AFTER: Duration = Duration::from_secs(30);
/// Aléa ajouté au délai, en pour cent (jamais retiré : une reprise n'est jamais plus tôt que prévu).
pub const JITTER_PERCENT: u64 = 25;
/// Réponses `503` ou `429` de suite avant d'arrêter : moins d'essais que pour une coupure, l'agent a dit
/// « pas maintenant ».
pub const MAX_BUSY: u32 = 3;
/// Au-delà de ce délai demandé par l'agent, la reprise automatique n'attend pas : la lecture s'arrête (le bouton
/// reste, l'utilisateur décide). Garde contre une valeur absurde ; jamais un délai raccourci.
pub const MAX_ASKED_WAIT: Duration = Duration::from_secs(3_600);
/// Aléa ajouté à un délai demandé par l'agent : au plus cette durée (jamais retiré, jamais de rafale en phase).
const BUSY_JITTER_MAX: Duration = Duration::from_millis(1_000);

/// Ce que la lecture fait après un échec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    /// Attendre cette durée puis rouvrir.
    Wait(Duration),
    /// Ne plus rouvrir seule : fin « injoignable » avec un bouton.
    GiveUp,
}

#[derive(Debug, Default)]
pub struct Retry {
    failures: u32,
    busy: u32,
}

impl Retry {
    /// Le socket est tombé (ou n'a pas pu s'ouvrir). `lasted` : combien de temps il a tenu ouvert (zéro s'il ne
    /// s'est pas ouvert) ; `base` : le premier délai ; `jitter` : un nombre au hasard.
    pub fn cut(&mut self, lasted: Duration, base: Duration, jitter: u32) -> Next {
        if lasted >= STABLE_AFTER {
            self.failures = 0;
            self.busy = 0;
        }
        self.failures += 1;
        if self.failures > MAX_FAILURES {
            return Next::GiveUp;
        }
        Next::Wait(growing_wait(base, self.failures, jitter))
    }

    /// L'agent a répondu `503` ou `429` et demande d'attendre `asked`. L'attente est AU MOINS ce délai.
    pub fn busy(&mut self, asked: Duration, jitter: u32) -> Next {
        self.busy += 1;
        if self.busy > MAX_BUSY || asked > MAX_ASKED_WAIT {
            return Next::GiveUp;
        }
        let extra = BUSY_JITTER_MAX
            .mul_f64(f64::from(jitter) / f64::from(u32::MAX))
            .min(BUSY_JITTER_MAX);
        Next::Wait(asked + extra)
    }

    /// Échecs de suite comptés (tests).
    #[cfg(test)]
    pub fn failures(&self) -> u32 {
        self.failures
    }
}

/// Délai de la `failures`-ième réouverture : `base`, `2 base`, `4 base`... plafonné à [`MAX_WAIT`], plus un
/// aléa de 0 à [`JITTER_PERCENT`] pour cent.
pub fn growing_wait(base: Duration, failures: u32, jitter: u32) -> Duration {
    let doubled = base.saturating_mul(1 << failures.saturating_sub(1).min(5));
    let wait = doubled.min(MAX_WAIT);
    let extra_percent = u64::from(jitter) * JITTER_PERCENT / u64::from(u32::MAX);
    wait + wait.mul_f64(extra_percent as f64 / 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: Duration = Duration::from_secs(1);
    const NONE: u32 = 0;
    const MAX_JITTER: u32 = u32::MAX;

    fn wait_of(next: Next) -> Duration {
        match next {
            Next::Wait(wait) => wait,
            Next::GiveUp => panic!("attendu un délai"),
        }
    }

    #[test]
    fn the_wait_doubles_up_to_the_cap() {
        let waits: Vec<_> = (1..=6).map(|n| growing_wait(BASE, n, NONE)).collect();
        assert_eq!(
            waits,
            [1, 2, 4, 8, 16, 30].map(Duration::from_secs),
            "1, 2, 4, 8, 16 puis le plafond de 30 s"
        );
    }

    #[test]
    fn the_jitter_only_adds_and_stays_within_a_quarter() {
        for failures in 1..=6 {
            let plain = growing_wait(BASE, failures, NONE);
            let full = growing_wait(BASE, failures, MAX_JITTER);
            assert!(full >= plain);
            assert!(
                full <= plain + plain / 4 + Duration::from_millis(1),
                "{full:?}"
            );
        }
        assert!(growing_wait(BASE, 3, MAX_JITTER / 2) > growing_wait(BASE, 3, NONE));
    }

    #[test]
    fn a_socket_that_falls_in_a_loop_gives_up_after_six_reopenings() {
        let mut retry = Retry::default();
        let mut reopenings = 0;
        let mut total = Duration::ZERO;
        // Chaque socket tient une seconde puis tombe : jamais « stable ».
        while let Next::Wait(wait) = retry.cut(Duration::from_secs(1), BASE, NONE) {
            reopenings += 1;
            total += wait;
        }
        assert_eq!(reopenings, MAX_FAILURES);
        // 1 + 2 + 4 + 8 + 16 + 30 = 61 s : bien moins de douze ouvertures en 61 s pour le frein de l'agent.
        assert_eq!(total, Duration::from_secs(61));
        assert!(
            reopenings + 1 < 12,
            "le frein de l'agent (12) n'est pas vidé"
        );
    }

    #[test]
    fn a_connection_that_held_long_enough_clears_the_count() {
        let mut retry = Retry::default();
        for _ in 0..MAX_FAILURES {
            retry.cut(Duration::from_secs(1), BASE, NONE);
        }
        assert_eq!(retry.failures(), MAX_FAILURES);
        let next = retry.cut(STABLE_AFTER, BASE, NONE);
        assert_eq!(wait_of(next), BASE, "repart du premier délai");
        assert_eq!(retry.failures(), 1);
    }

    #[test]
    fn a_connection_just_under_the_stable_time_does_not_clear_the_count() {
        let mut retry = Retry::default();
        retry.cut(Duration::from_secs(1), BASE, NONE);
        retry.cut(STABLE_AFTER - Duration::from_millis(1), BASE, NONE);
        assert_eq!(retry.failures(), 2);
    }

    #[test]
    fn a_busy_answer_is_waited_at_least_as_asked_even_beyond_thirty_seconds() {
        let mut retry = Retry::default();
        let asked = Duration::from_secs(90);
        for jitter in [NONE, MAX_JITTER / 3, MAX_JITTER] {
            let mut fresh = Retry::default();
            let wait = wait_of(fresh.busy(asked, jitter));
            assert!(wait >= asked, "{wait:?}");
            assert!(wait <= asked + Duration::from_secs(1));
        }
        assert_eq!(wait_of(retry.busy(asked, NONE)), asked);
    }

    #[test]
    fn busy_answers_are_retried_fewer_times_than_cuts_and_never_in_a_burst() {
        let mut retry = Retry::default();
        let one = Duration::from_secs(1);
        let mut tries = 0;
        while let Next::Wait(wait) = retry.busy(one, NONE) {
            assert!(wait >= one, "jamais avant le délai demandé");
            tries += 1;
        }
        assert_eq!(tries, MAX_BUSY);
        const { assert!(MAX_BUSY < MAX_FAILURES) };
    }

    #[test]
    fn an_absurd_delay_asked_by_the_agent_stops_the_automatic_retry() {
        let mut retry = Retry::default();
        assert_eq!(
            retry.busy(MAX_ASKED_WAIT + Duration::from_secs(1), NONE),
            Next::GiveUp
        );
        assert_eq!(
            wait_of(Retry::default().busy(MAX_ASKED_WAIT, NONE)),
            MAX_ASKED_WAIT
        );
    }
}

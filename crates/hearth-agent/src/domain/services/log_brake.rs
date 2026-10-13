//! Le frein sur les lectures de journaux d'un compte (HRT-107, BR-SLOG-026, BR-SLOG-027).
//!
//! Un compte en lecture seule pouvait tenir les dix places du serveur et rouvrir sans fin des lectures
//! de mille lignes. Deux limites par compte, décidées par le propriétaire le 2026-10-13, valeurs de
//! départ à ajuster après mesure sur forge :
//! - **3 lectures en direct** en même temps, tous postes confondus (BR-SLOG-026) ;
//! - **12 ouvertures par minute** : un seau de 12 jetons, un jeton rendu toutes les 5 secondes
//!   (BR-SLOG-027). La reprise après une coupure compte comme une ouverture.
//!
//! Quand un compte est refusé plus de 30 fois en 10 minutes par le frein, UNE entrée est écrite au journal
//! d'activité, puis plus rien pendant 10 minutes : le frein ne remplit pas le journal par le geste même
//! qu'il arrête.
//!
//! Règle pure : le temps arrive en millisecondes depuis une origine monotone choisie par l'appelant, rien
//! n'est lu de l'horloge ici.

use std::collections::VecDeque;

use hearth_proto::api::service_logs::{MAX_READS_PER_ACCOUNT, OPEN_BURST, OPEN_REFILL_S};

/// Fenêtre où l'on compte les refus du frein.
pub const REFUSAL_WINDOW_MS: u64 = 10 * 60 * 1000;
/// Au-delà de ce nombre de refus dans la fenêtre, une entrée est écrite.
pub const REFUSALS_BEFORE_ENTRY: u32 = 30;
/// Les refus retenus au plus par compte (le compte se borne : un compte acharné n'agrandit pas la mémoire).
const MAX_REMEMBERED_REFUSALS: usize = 4_096;

/// Les trois nombres du frein.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrakeSettings {
    /// Lectures en direct simultanées au plus pour un compte.
    pub max_reads: usize,
    /// Réserve d'ouvertures.
    pub burst: u32,
    /// Une ouverture rendue toutes les `refill_ms` millisecondes.
    pub refill_ms: u64,
}

impl BrakeSettings {
    /// Les valeurs de départ du propriétaire.
    pub const fn production() -> Self {
        Self {
            max_reads: MAX_READS_PER_ACCOUNT,
            burst: OPEN_BURST,
            refill_ms: OPEN_REFILL_S as u64 * 1000,
        }
    }

    /// Pour les tests qui ne portent pas sur le frein : rien n'arrête.
    pub const fn relaxed() -> Self {
        Self {
            max_reads: usize::MAX,
            burst: u32::MAX,
            refill_ms: 1,
        }
    }
}

impl Default for BrakeSettings {
    fn default() -> Self {
        Self::production()
    }
}

/// Pourquoi une ouverture est refusée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenRefusal {
    /// Le compte lit déjà autant de services qu'il peut (BR-SLOG-026).
    AccountLimit,
    /// Trop d'ouvertures en peu de temps (BR-SLOG-027). `announce` : le nombre de refus à inscrire au
    /// journal d'activité, présent UNE fois par fenêtre de 10 minutes.
    RateLimited {
        retry_after_s: u32,
        announce: Option<u32>,
    },
}

/// L'état du frein d'UN compte.
#[derive(Debug, Clone)]
pub struct AccountBrake {
    settings: BrakeSettings,
    reads: usize,
    tokens: u32,
    /// Le moment où le dernier jeton rendu a été compté (le prochain arrive `refill_ms` plus tard).
    refilled_at: u64,
    refusals: VecDeque<u64>,
    silent_until: u64,
}

impl AccountBrake {
    pub fn new(settings: BrakeSettings, now_ms: u64) -> Self {
        Self {
            settings,
            reads: 0,
            tokens: settings.burst,
            refilled_at: now_ms,
            refusals: VecDeque::new(),
            silent_until: 0,
        }
    }

    /// Lectures en direct de ce compte en ce moment.
    pub fn reads(&self) -> usize {
        self.reads
    }

    /// Ce compte a-t-il encore quelque chose à retenir ? Faux : l'état peut être oublié (aucune lecture,
    /// seau plein, aucun refus récent).
    pub fn is_idle(&mut self, now_ms: u64) -> bool {
        self.refill(now_ms);
        self.reads == 0
            && self.tokens >= self.settings.burst
            && self.refusals.is_empty()
            && now_ms >= self.silent_until
    }

    /// Demande une ouverture. Le frein d'abord (chaque tentative coûte un jeton, réussie ou non plus
    /// loin : refuser sans compter laisserait rouvrir sans fin), puis le plafond de lectures. En cas de
    /// réussite la lecture est comptée : [`Self::release`] la rend.
    pub fn try_open(&mut self, now_ms: u64) -> Result<(), OpenRefusal> {
        self.refill(now_ms);
        if self.tokens == 0 {
            return Err(self.refuse_rate(now_ms));
        }
        self.tokens -= 1;
        if self.reads >= self.settings.max_reads {
            return Err(OpenRefusal::AccountLimit);
        }
        self.reads += 1;
        Ok(())
    }

    /// Une lecture de ce compte est finie, quelle qu'en soit la raison.
    pub fn release(&mut self) {
        self.reads = self.reads.saturating_sub(1);
    }

    fn refill(&mut self, now_ms: u64) {
        if self.tokens >= self.settings.burst {
            // Seau plein : le temps ne s'accumule pas. Le premier jeton pris recommence à compter d'ici.
            self.refilled_at = now_ms;
            return;
        }
        let elapsed = now_ms.saturating_sub(self.refilled_at);
        let gained = elapsed / self.settings.refill_ms.max(1);
        if gained > 0 {
            let gained = u32::try_from(gained).unwrap_or(u32::MAX);
            self.tokens = self.tokens.saturating_add(gained).min(self.settings.burst);
            self.refilled_at += u64::from(gained) * self.settings.refill_ms;
            if self.tokens >= self.settings.burst {
                self.refilled_at = now_ms;
            }
        }
    }

    fn refuse_rate(&mut self, now_ms: u64) -> OpenRefusal {
        let next = self.refilled_at + self.settings.refill_ms;
        let wait_ms = next.saturating_sub(now_ms);
        let retry_after_s = u32::try_from(wait_ms.div_ceil(1000))
            .unwrap_or(u32::MAX)
            .max(1);
        // Les refus de la fenêtre de 10 minutes.
        while self
            .refusals
            .front()
            .is_some_and(|at| now_ms.saturating_sub(*at) >= REFUSAL_WINDOW_MS)
        {
            self.refusals.pop_front();
        }
        if self.refusals.len() >= MAX_REMEMBERED_REFUSALS {
            self.refusals.pop_front();
        }
        self.refusals.push_back(now_ms);
        let count = u32::try_from(self.refusals.len()).unwrap_or(u32::MAX);
        let announce = if count > REFUSALS_BEFORE_ENTRY && now_ms >= self.silent_until {
            self.silent_until = now_ms + REFUSAL_WINDOW_MS;
            Some(count)
        } else {
            None
        };
        OpenRefusal::RateLimited {
            retry_after_s,
            announce,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STEP: u64 = 5_000;

    fn brake() -> AccountBrake {
        AccountBrake::new(BrakeSettings::production(), 0)
    }

    #[test]
    fn the_starting_values_are_the_owner_s() {
        let settings = BrakeSettings::production();
        assert_eq!(settings.max_reads, 3);
        assert_eq!(settings.burst, 12);
        assert_eq!(settings.refill_ms, 5_000);
    }

    #[test]
    fn a_fourth_live_reading_is_refused_and_closing_one_gives_the_place_back() {
        let mut brake = brake();
        for _ in 0..3 {
            assert_eq!(brake.try_open(0), Ok(()));
        }
        assert_eq!(brake.try_open(0), Err(OpenRefusal::AccountLimit));
        assert_eq!(brake.reads(), 3);
        brake.release();
        assert_eq!(brake.reads(), 2);
        assert_eq!(brake.try_open(0), Ok(()));
    }

    #[test]
    fn a_refused_open_does_not_count_as_a_reading() {
        let mut brake = brake();
        for _ in 0..3 {
            brake.try_open(0).unwrap();
        }
        for _ in 0..4 {
            assert_eq!(brake.try_open(0), Err(OpenRefusal::AccountLimit));
        }
        assert_eq!(brake.reads(), 3);
        // trois fermetures rendent trois places, pas plus
        for _ in 0..5 {
            brake.release();
        }
        assert_eq!(brake.reads(), 0);
    }

    #[test]
    fn twelve_opens_in_a_burst_are_served_and_the_thirteenth_is_braked_with_its_wait() {
        let mut brake = brake();
        for round in 0..12 {
            assert_eq!(brake.try_open(round), Ok(()), "ouverture {round}");
            brake.release();
        }
        match brake.try_open(12) {
            Err(OpenRefusal::RateLimited {
                retry_after_s,
                announce: None,
            }) => assert_eq!(retry_after_s, 5),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn one_open_comes_back_every_five_seconds() {
        let mut brake = brake();
        for _ in 0..12 {
            brake.try_open(0).unwrap();
            brake.release();
        }
        assert!(matches!(
            brake.try_open(4_999),
            Err(OpenRefusal::RateLimited {
                retry_after_s: 1,
                ..
            })
        ));
        assert_eq!(brake.try_open(STEP), Ok(()));
        brake.release();
        // un seul jeton est revenu
        assert!(matches!(
            brake.try_open(STEP + 1),
            Err(OpenRefusal::RateLimited { .. })
        ));
        // une minute entière : 12 jetons, pas un de plus
        let later = STEP + 60_000;
        for round in 0..12 {
            assert_eq!(brake.try_open(later), Ok(()), "ouverture {round}");
            brake.release();
        }
        assert!(matches!(
            brake.try_open(later),
            Err(OpenRefusal::RateLimited { .. })
        ));
    }

    #[test]
    fn the_wait_counts_down_to_the_next_token() {
        let mut brake = brake();
        for _ in 0..12 {
            brake.try_open(0).unwrap();
            brake.release();
        }
        for (now, expected) in [(0u64, 5u32), (1_000, 4), (3_500, 2), (4_999, 1)] {
            match brake.try_open(now) {
                Err(OpenRefusal::RateLimited { retry_after_s, .. }) => {
                    assert_eq!(retry_after_s, expected, "à {now} ms");
                }
                other => panic!("{other:?}"),
            }
        }
    }

    #[test]
    fn an_idle_account_does_not_bank_more_than_its_burst() {
        let mut brake = brake();
        // une heure sans rien faire : toujours 12 ouvertures, pas 720
        let hour = 3_600_000;
        for round in 0..12 {
            assert_eq!(brake.try_open(hour), Ok(()), "ouverture {round}");
            brake.release();
        }
        assert!(matches!(
            brake.try_open(hour),
            Err(OpenRefusal::RateLimited { .. })
        ));
    }

    #[test]
    fn a_refusal_by_the_cap_still_costs_a_token() {
        let mut brake = brake();
        for _ in 0..3 {
            brake.try_open(0).unwrap();
        }
        // 9 jetons restent ; 9 refus par le plafond les vident
        for _ in 0..9 {
            assert_eq!(brake.try_open(0), Err(OpenRefusal::AccountLimit));
        }
        assert!(matches!(
            brake.try_open(0),
            Err(OpenRefusal::RateLimited { .. })
        ));
    }

    #[test]
    fn one_entry_after_thirty_refusals_then_silence_for_ten_minutes() {
        let mut brake = brake();
        for _ in 0..12 {
            brake.try_open(0).unwrap();
            brake.release();
        }
        let mut announced = Vec::new();
        // 40 refus en quelques secondes (aucun jeton ne revient : < 5 s)
        for round in 0..40u64 {
            if let Err(OpenRefusal::RateLimited {
                announce: Some(count),
                ..
            }) = brake.try_open(round)
            {
                announced.push((round, count));
            }
        }
        // le 31e refus (indice 30) dépasse 30 : une seule annonce, avec le nombre
        assert_eq!(announced, vec![(30, 31)]);
        // dix minutes de silence : plus rien, même après des centaines de refus
        let mut later_announce = 0;
        for round in 0..200u64 {
            let now = 1_000 + round * 2;
            if let Err(OpenRefusal::RateLimited { announce, .. }) = brake.try_open(now)
                && announce.is_some()
            {
                later_announce += 1;
            }
        }
        assert_eq!(later_announce, 0);
    }

    #[test]
    fn thirty_refusals_or_fewer_write_nothing() {
        let mut brake = brake();
        for _ in 0..12 {
            brake.try_open(0).unwrap();
            brake.release();
        }
        for round in 0..30u64 {
            assert!(matches!(
                brake.try_open(round),
                Err(OpenRefusal::RateLimited { announce: None, .. })
            ));
        }
    }

    #[test]
    fn a_new_entry_is_possible_after_the_ten_minutes_of_silence() {
        let mut brake = brake();
        for _ in 0..12 {
            brake.try_open(0).unwrap();
            brake.release();
        }
        let mut entries = 0;
        for round in 0..31u64 {
            if let Err(OpenRefusal::RateLimited {
                announce: Some(_), ..
            }) = brake.try_open(round)
            {
                entries += 1;
            }
        }
        assert_eq!(entries, 1);
        // dix minutes plus tard, le compte recommence : drainer le seau puis 31 refus
        let base = 10 * 60 * 1000 + 100;
        for _ in 0..12 {
            // les jetons sont revenus : on les reprend
            let _ = brake.try_open(base);
            brake.release();
        }
        let mut again = 0;
        for round in 0..40u64 {
            if let Err(OpenRefusal::RateLimited {
                announce: Some(count),
                ..
            }) = brake.try_open(base + round)
            {
                assert!(count > REFUSALS_BEFORE_ENTRY);
                again += 1;
            }
        }
        assert_eq!(again, 1);
    }

    #[test]
    fn an_account_with_nothing_to_remember_can_be_forgotten() {
        let mut brake = brake();
        assert!(brake.is_idle(0));
        brake.try_open(0).unwrap();
        assert!(!brake.is_idle(1), "une lecture est ouverte");
        brake.release();
        assert!(!brake.is_idle(1), "le seau n'est pas plein");
        assert!(brake.is_idle(STEP + 1), "le jeton est revenu");
    }

    #[test]
    fn the_remembered_refusals_are_bounded() {
        let mut brake = brake();
        for _ in 0..12 {
            brake.try_open(0).unwrap();
            brake.release();
        }
        for round in 0..10_000u64 {
            let _ = brake.try_open(round % 4_000);
        }
        assert!(brake.refusals.len() <= MAX_REMEMBERED_REFUSALS);
    }

    #[test]
    fn relaxed_settings_never_stop_anything() {
        let mut brake = AccountBrake::new(BrakeSettings::relaxed(), 0);
        for _ in 0..1_000 {
            assert_eq!(brake.try_open(0), Ok(()));
        }
    }
}

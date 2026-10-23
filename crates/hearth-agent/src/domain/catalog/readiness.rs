//! Le signe « le jeu est prêt » et les phases d'un serveur de jeu (HRT-101, BR-FICHE-028, BR-FICHE-056,
//! BR-JEU-038). Du domaine pur : aucune lecture de journal, aucune horloge, aucune E/S.
//!
//! Un service de jeu a quatre phases, que le détail dit en mots :
//! - `FirstStart` : le jeu fait ce que la fiche annonce comme long (téléchargement, grand monde) ;
//! - `Starting` : lancé, le signe « prêt » n'est pas encore passé ;
//! - `Ready` : le signe est passé dans une ligne POSTÉRIEURE au lancement ;
//! - `Unknown` : « Hearth ne peut pas savoir si le jeu est prêt » (aucun signe essayé, ou le délai de la fiche est
//!   passé sans le signe). Rien n'est arrêté ni relancé pour autant.
//!
//! Le signe est une sous-chaîne LITTÉRALE, jamais une expression régulière. Un signe `untested` n'est jamais cru
//! (`stop_plan::ready_sign`) : la phase ne devient jamais `Ready` par lui.

use time::OffsetDateTime;

use super::loading::model::Fiche;
use super::stop_plan::ready_sign;

/// La phase d'un serveur de jeu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GamePhase {
    FirstStart,
    Starting,
    Ready { at: OffsetDateTime },
    Unknown,
}

/// Ce qu'une fiche permet de savoir de l'état d'un jeu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadyRule {
    /// Le signe littéral, seulement s'il a été essayé sur la vraie image.
    pub sign: Option<String>,
    /// Le premier démarrage est long (téléchargements déclarés, ou génération du monde).
    pub long_first_start: bool,
    /// Combien de secondes la fiche laisse au jeu pour être prêt.
    pub timeout_s: u32,
}

impl ReadyRule {
    pub fn of(fiche: &Fiche) -> Self {
        let first_start = &fiche.file.first_start;
        Self {
            sign: ready_sign(fiche).map(str::to_owned),
            long_first_start: first_start.world_generation || !first_start.download.is_empty(),
            timeout_s: first_start.ready_timeout_s,
        }
    }
}

/// Le signe est-il dans l'une de ces lignes, comme sous-chaîne littérale ? Un signe vide n'est jamais trouvé.
pub fn ready_in<'a>(sign: &str, mut lines: impl Iterator<Item = &'a str>) -> bool {
    !sign.is_empty() && lines.any(|line| line.contains(sign))
}

/// L'heure (millisecondes depuis l'époque) de la première ligne qui porte le signe, parmi les lignes écrites à
/// partir du lancement (`launch_ms` compris) : une ligne d'avant le lancement (un lancement précédent du même
/// conteneur) ne compte jamais.
pub fn ready_since<'a>(
    sign: &str,
    launch_ms: i64,
    lines: impl Iterator<Item = (i64, &'a str)>,
) -> Option<i64> {
    if sign.is_empty() {
        return None;
    }
    lines
        .filter(|(at_ms, _)| *at_ms >= launch_ms)
        .find(|(_, line)| line.contains(sign))
        .map(|(at_ms, _)| at_ms)
}

/// La phase, d'après la règle de la fiche, le temps écoulé depuis le lancement et l'heure du signe s'il a été vu.
pub fn phase_of(
    rule: &ReadyRule,
    first_start: bool,
    elapsed_s: u64,
    ready_at: Option<OffsetDateTime>,
) -> GamePhase {
    if rule.sign.is_some()
        && let Some(at) = ready_at
    {
        return GamePhase::Ready { at };
    }
    // Le délai de la fiche passé sans le signe : Hearth ne peut pas savoir, rien n'est arrêté.
    if elapsed_s > u64::from(rule.timeout_s) {
        return GamePhase::Unknown;
    }
    match (rule.sign.is_some(), first_start && rule.long_first_start) {
        // Premier démarrage long : jusqu'au signe, ou jusqu'au délai.
        (_, true) => GamePhase::FirstStart,
        (true, false) => GamePhase::Starting,
        // Aucun signe essayé : Hearth ne sera jamais en mesure de dire « prêt ».
        (false, false) => GamePhase::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use time::Duration;

    use super::*;
    use crate::domain::catalog::test_support::{base, rich};

    fn at() -> OffsetDateTime {
        OffsetDateTime::UNIX_EPOCH + Duration::seconds(1_790_000_000)
    }

    fn rule(sign: Option<&str>, long: bool, timeout_s: u32) -> ReadyRule {
        ReadyRule {
            sign: sign.map(str::to_owned),
            long_first_start: long,
            timeout_s,
        }
    }

    #[test]
    fn the_sign_is_searched_only_in_lines_after_the_launch_and_as_a_literal() {
        let lines = [
            (1_000, "Server started"),
            (2_000, "Loading world"),
            (3_000, "Server started"),
        ];
        // Une ligne d'AVANT le lancement (un lancement précédent) ne compte pas.
        assert_eq!(
            ready_since("Server started", 2_500, lines.into_iter()),
            Some(3_000)
        );
        assert_eq!(
            ready_since("Server started", 1_000, lines.into_iter()),
            Some(1_000),
            "le lancement lui-même compte"
        );
        assert_eq!(
            ready_since("Server started", 3_001, lines.into_iter()),
            None
        );
        // Un littéral : ni expression régulière, ni joker.
        let wild = [(5_000, "Server is started now")];
        assert_eq!(ready_since("Server .* started", 0, wild.into_iter()), None);
        assert_eq!(ready_since("Server is*", 0, wild.into_iter()), None);
        assert_eq!(
            ready_since("Server is started", 0, wild.into_iter()),
            Some(5_000)
        );
        // Un signe vide ne trouve rien.
        assert_eq!(ready_since("", 0, wild.into_iter()), None);
        assert!(!ready_in("", ["x"].into_iter()));
        assert!(ready_in("started", ["Server started"].into_iter()));
        assert!(!ready_in("started", ["Server stopped"].into_iter()));
    }

    #[test]
    fn an_untested_sign_is_never_believed_and_the_phase_is_unknown() {
        // Les fiches de fabrication : `rich` a un signe `tested`, `base` n'en a pas.
        let tested = ReadyRule::of(&rich());
        assert_eq!(tested.sign.as_deref(), Some("Done"));
        assert!(tested.long_first_start && tested.timeout_s == 3600);
        let none = ReadyRule::of(&base());
        assert_eq!(none.sign, None);
        // Même une ligne qui porte « le signe » ne rend pas le jeu prêt quand le signe n'est pas cru.
        assert_eq!(phase_of(&none, false, 5, Some(at())), GamePhase::Unknown);
        assert_eq!(
            phase_of(&tested, false, 5, Some(at())),
            GamePhase::Ready { at: at() }
        );
    }

    #[test]
    fn the_phases_follow_the_first_start_the_sign_and_the_delay_of_the_fiche() {
        let long = rule(Some("Done"), true, 600);
        assert_eq!(phase_of(&long, true, 10, None), GamePhase::FirstStart);
        assert_eq!(phase_of(&long, true, 601, None), GamePhase::Unknown);
        assert_eq!(
            phase_of(&long, true, 10, Some(at())),
            GamePhase::Ready { at: at() }
        );
        let short = rule(Some("Done"), false, 600);
        assert_eq!(phase_of(&short, true, 10, None), GamePhase::Starting);
        assert_eq!(phase_of(&long, false, 10, None), GamePhase::Starting);
        // Aucun signe essayé : premier démarrage long annoncé, puis « Hearth ne peut pas savoir ».
        let blind = rule(None, true, 600);
        assert_eq!(phase_of(&blind, true, 10, None), GamePhase::FirstStart);
        assert_eq!(phase_of(&blind, true, 601, None), GamePhase::Unknown);
        assert_eq!(phase_of(&blind, false, 10, None), GamePhase::Unknown);
    }
}

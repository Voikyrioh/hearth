//! Arrêter un serveur de jeu en enregistrant son monde : les règles pures (HRT-103, ADR-0052, BR-JEU-002 à
//! BR-JEU-007, BR-JEU-034).
//!
//! Rien ici ne parle au moteur ni à l'horloge : le plan de l'opération, le signe cherché dans les journaux du
//! jeu, ce qui est cru et ce qui ne l'est pas. Le déroulé (envoi des ordres, attente, décision) est
//! `application/game_stop.rs`.
//!
//! **Ce que le plan de la fiche devient** ([`GameStopKind`]) : une fiche qui dit « l'image enregistre sur le
//! signal » est un arrêt ordinaire (aucun ordre, `NotNeeded`) ; une suite `tested` est déroulée ; tout le reste
//! (suite non essayée, jeu qui n'enregistre pas, fiche introuvable dans cette construction) est une DÉCISION : rien
//! n'est arrêté sans le choix du compte (BR-JEU-006, BR-JEU-007, BR-JEU-038).

use super::lifecycle::Gesture;
use super::plan::{OpPlan, PlanError, PlannedStep, StepName};
use crate::domain::audit::SaveMention;
use crate::domain::catalog::stop_plan::{FicheLiteral, PlannedStep as FicheStep, StopPlan};
use crate::domain::services::logs::LogLine;

/// Les étapes du plan d'une opération d'arrêt de jeu (codes stables du journal).
pub mod step {
    /// Les ordres d'enregistrement envoyés au jeu, et l'attente de leur signe.
    pub const SAVE_WORLD: &str = "save_world";
    /// L'ordre de sortie, puis l'arrêt demandé au moteur.
    pub const STOP: &str = "stop";
    /// Le démarrage qui suit un redémarrage.
    pub const START: &str = "start";
}

/// Comment un arrêt ou un redémarrage de jeu se déroule, d'après la fiche.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameStopKind {
    /// L'image enregistre le monde quand elle reçoit le signal : l'arrêt est celui du moteur avec le délai du
    /// service, sans aucun ordre (BR-JEU-008, BR-JEU-034 pour 7 Days to Die).
    SelfSaving,
    /// Une suite d'étapes essayée : déroulée telle que la fiche la déclare.
    Steps(StopPlan),
}

impl GameStopKind {
    /// Le genre d'un plan : `ByImageOnSignal` n'a rien à dérouler, tout autre plan passe par le déroulé.
    pub fn of(plan: StopPlan) -> Self {
        match plan {
            StopPlan::ByImageOnSignal { .. } => Self::SelfSaving,
            other => Self::Steps(other),
        }
    }
}

/// Le plan figé de l'opération (`OpPlan`) d'un arrêt ou d'un redémarrage de jeu. Aucune étape n'est destructive :
/// un arrêt ne détruit rien que l'agent garde, et une opération interrompue par un redémarrage de l'agent est
/// classée « interrompue », jamais reprise ni rejouée (BR-JEU-011, BR-LIFE-020).
///
/// - un arrêt déroulé : `save_world` (s'il y a des ordres ou des attentes avant la sortie), `stop` ;
/// - un redémarrage déroulé : les mêmes, puis `start` ;
/// - un arrêt ordinaire (`SelfSaving`) : le plan du geste, une seule étape.
pub fn op_plan(gesture: Gesture, kind: &GameStopKind) -> Result<OpPlan, PlanError> {
    let GameStopKind::Steps(plan) = kind else {
        return gesture.plan();
    };
    let mut names: Vec<&str> = Vec::new();
    if has_save_steps(plan) {
        names.push(step::SAVE_WORLD);
    }
    names.push(step::STOP);
    if gesture == Gesture::Restart {
        names.push(step::START);
    }
    let steps = names
        .into_iter()
        .map(|name| {
            Ok(PlannedStep {
                name: StepName::parse(name).ok_or(PlanError::StepName)?,
                destructive: false,
            })
        })
        .collect::<Result<Vec<_>, PlanError>>()?;
    OpPlan::new(steps, None)
}

/// Le plan a-t-il des étapes AVANT l'ordre de sortie (un ordre d'enregistrement, une attente) ?
pub fn has_save_steps(plan: &StopPlan) -> bool {
    match plan {
        StopPlan::Sequence { steps, .. } => before_quit(steps).next().is_some(),
        _ => false,
    }
}

/// Les étapes d'une suite avant l'ordre de sortie (sans lui).
pub fn before_quit(steps: &[FicheStep]) -> impl Iterator<Item = &FicheStep> {
    steps
        .iter()
        .take_while(|step| !matches!(step, FicheStep::StdinLine { quits: true, .. }))
}

/// L'ordre de sortie d'une suite, s'il y en a un (la fiche le place en dernier, vérifié au chargement).
pub fn quit_line(steps: &[FicheStep]) -> Option<&FicheLiteral> {
    steps.iter().find_map(|step| match step {
        FicheStep::StdinLine { line, quits: true } => Some(line),
        _ => None,
    })
}

/// Cette ligne du journal du jeu est-elle le signe attendu (BR-JEU-005) ? Sous-chaîne LITTÉRALE, jamais une
/// expression régulière, dans les seules lignes écrites À PARTIR de l'envoi de l'ordre. Le moteur ne filtre
/// qu'à la seconde : l'appelant écarte ici le surplus, à la milliseconde.
pub fn confirms(line: &LogLine, contains: &str, since_ms: i64) -> bool {
    line.at_ms >= since_ms && !contains.is_empty() && line.text.contains(contains)
}

/// Pourquoi une décision est demandée avant d'arrêter (BR-JEU-006, BR-JEU-007, BR-JEU-038).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AskReason {
    /// La fiche ne sait pas enregistrer le monde, ou sa suite n'a pas été essayée (jamais envoyée) : l'ordre est
    /// « impossible » avant même d'être tenté.
    NoUsableSequence,
    /// Un ordre n'a pas pu être envoyé (entrée non ouverte, service qui ne répond pas) : BR-JEU-007.
    OrderRefused,
    /// Le signe obligatoire n'a pas été vu dans le délai : BR-JEU-006.
    SignNotSeen,
}

impl AskReason {
    /// La mention de l'entrée du journal si le compte choisit « Arrêter quand même ».
    pub const fn mention_if_stopped_anyway(self) -> SaveMention {
        match self {
            Self::NoUsableSequence | Self::OrderRefused => SaveMention::Impossible,
            Self::SignNotSeen => SaveMention::StoppedAnyway,
        }
    }

    /// Après « Arrêter quand même », l'ordre de sortie de la fiche peut-il encore être tenté ? Seulement quand le
    /// canal de l'entrée a fonctionné : c'est le signe qui manquait. Un ordre refusé, ou une suite qui n'existe
    /// pas, ne le retente pas : le compte a choisi l'arrêt du moteur, rien de plus.
    pub const fn may_still_quit(self) -> bool {
        matches!(self, Self::SignNotSeen)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::catalog::stop_plan::{DecisionReason, Signal};
    use crate::domain::services::logs::LogSource;

    fn line(at_ms: i64, text: &str) -> LogLine {
        LogLine {
            at_ms,
            source: LogSource::Out,
            text: text.to_owned(),
            cut: None,
        }
    }

    fn terraria() -> StopPlan {
        StopPlan::Sequence {
            steps: vec![
                FicheStep::StdinLine {
                    line: "save".into(),
                    quits: false,
                },
                FicheStep::Wait { seconds: 15 },
                FicheStep::StdinLine {
                    line: "exit".into(),
                    quits: true,
                },
            ],
            grace_s: 10,
            signal: Signal::ImageDefault,
        }
    }

    fn names(plan: &OpPlan) -> Vec<&str> {
        plan.steps().iter().map(|s| s.name.as_str()).collect()
    }

    #[test]
    fn the_plan_of_the_operation_follows_the_sequence_and_destroys_nothing() {
        let steps = GameStopKind::Steps(terraria());
        let stop = op_plan(Gesture::Stop, &steps).unwrap();
        assert_eq!(names(&stop), ["save_world", "stop"]);
        let restart = op_plan(Gesture::Restart, &steps).unwrap();
        assert_eq!(names(&restart), ["save_world", "stop", "start"]);
        assert!(stop.destructive_steps().next().is_none() && restart.no_return().is_none());
        // Un arrêt ordinaire garde le plan du geste (7 Days to Die : aucun ordre).
        assert_eq!(
            names(&op_plan(Gesture::Stop, &GameStopKind::SelfSaving).unwrap()),
            ["stop"]
        );
        assert_eq!(
            names(&op_plan(Gesture::Restart, &GameStopKind::SelfSaving).unwrap()),
            ["restart"]
        );
        // Une décision seule (rien à dérouler avant l'arrêt) n'a pas d'étape d'enregistrement.
        let decision = GameStopKind::Steps(StopPlan::NeedsDecision {
            reason: DecisionReason::UntestedSequence,
            grace_s: 10,
            signal: Signal::ImageDefault,
        });
        assert_eq!(names(&op_plan(Gesture::Stop, &decision).unwrap()), ["stop"]);
        assert_eq!(
            names(&op_plan(Gesture::Restart, &decision).unwrap()),
            ["stop", "start"]
        );
    }

    #[test]
    fn a_plan_that_saves_by_the_image_is_self_saving_and_everything_else_is_unrolled() {
        let by_image = StopPlan::ByImageOnSignal {
            grace_s: 120,
            signal: Signal::ImageDefault,
        };
        assert_eq!(GameStopKind::of(by_image), GameStopKind::SelfSaving);
        assert!(matches!(
            GameStopKind::of(terraria()),
            GameStopKind::Steps(_)
        ));
    }

    #[test]
    fn the_sequence_is_cut_at_the_exit_order_which_comes_last() {
        let StopPlan::Sequence { steps, .. } = terraria() else {
            unreachable!()
        };
        assert_eq!(before_quit(&steps).count(), 2);
        assert_eq!(quit_line(&steps).map(FicheLiteral::as_str), Some("exit"));
        assert!(has_save_steps(&terraria()));
        // Une suite réduite à l'ordre de sortie n'a rien à faire avant lui.
        let only_exit = StopPlan::Sequence {
            steps: vec![FicheStep::StdinLine {
                line: "exit".into(),
                quits: true,
            }],
            grace_s: 10,
            signal: Signal::ImageDefault,
        };
        assert!(!has_save_steps(&only_exit));
    }

    #[test]
    fn the_sign_is_a_literal_in_a_line_written_since_the_order_never_an_expression() {
        let seen = line(10_000, "World saved: 100%");
        assert!(confirms(&seen, "World saved", 10_000));
        assert!(confirms(&seen, "World saved", 9_000));
        // Écrite AVANT l'ordre : ne compte pas (le moteur ne filtre qu'à la seconde).
        assert!(!confirms(&seen, "World saved", 10_001));
        // Une sous-chaîne littérale, pas une expression régulière.
        assert!(!confirms(&seen, "World .*saved", 0));
        assert!(!confirms(&seen, "world saved", 0), "la casse compte");
        assert!(!confirms(&seen, "", 0), "un signe vide ne confirme rien");
    }

    #[test]
    fn a_game_stop_cut_by_an_agent_restart_is_interrupted_and_never_replayed() {
        // BR-JEU-011 : redémarrage de l'agent entre l'ordre et l'arrêt : le service tourne toujours, rien n'est
        // renvoyé. Les étapes ne sont pas destructives : le tableau de reprise dit « interrompue », verrou rendu.
        use crate::domain::ops::{
            Cause, Observed, OpKind, OpStatus, StepRecord, StepState, UndoInfo, recover,
        };
        let plan = op_plan(Gesture::Stop, &GameStopKind::Steps(terraria())).unwrap();
        let row = StepRecord {
            seq: 1,
            step: StepName::parse(step::SAVE_WORLD).unwrap(),
            state: StepState::Intent,
            undo: UndoInfo::default(),
            at: time::OffsetDateTime::UNIX_EPOCH,
        };
        for observed in [Observed::Unreadable, Observed::Ambiguous] {
            let recovery = recover(OpKind::Stop, &plan, std::slice::from_ref(&row), observed);
            assert_eq!(recovery.status, OpStatus::Interrupted, "{observed:?}");
            assert_eq!(recovery.cause, Cause::Interrupted);
            assert!(
                !recovery.keeps_lock && recovery.choices.is_empty() && recovery.cleanup.is_empty()
            );
        }
    }

    #[test]
    fn after_stopping_anyway_the_exit_order_is_tried_again_only_when_the_input_worked() {
        assert!(AskReason::SignNotSeen.may_still_quit());
        assert!(!AskReason::OrderRefused.may_still_quit());
        assert!(!AskReason::NoUsableSequence.may_still_quit());
        assert_eq!(
            AskReason::OrderRefused.mention_if_stopped_anyway(),
            SaveMention::Impossible
        );
        assert_eq!(
            AskReason::SignNotSeen.mention_if_stopped_anyway(),
            SaveMention::StoppedAnyway
        );
    }
}

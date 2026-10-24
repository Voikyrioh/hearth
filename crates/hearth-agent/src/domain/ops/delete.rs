//! Supprimer un service : le plan de l'opération (HRT-85, BR-DEL-013, BR-DEL-014, BR-DEL-015, BR-DEL-025).
//!
//! L'ordre est celui de la spec et ne change jamais : verrou du service (pris à l'acceptation), arrêt s'il
//! tournait, **suppression du conteneur (point de non-retour)**, oubli de la description et de ses valeurs
//! scellées, **puis seulement** l'effacement des dossiers gérés (si la case est cochée). L'effacement vient
//! APRÈS le conteneur, jamais avant : un échec laisse du superflu sur le disque, jamais un service à moitié
//! vidé (BR-DEL-014).
//!
//! Trois étapes détruisent (`remove_container`, `forget`, `erase`) : chacune écrit son `intent` avant le
//! geste. Un agent qui redémarre au milieu laisse donc une opération « à vérifier », verrou et nom repris,
//! jamais rejouée (`recover`, BR-DEL-025). L'arrêt ne détruit rien : une coupure avant le point de
//! non-retour laisse le service entier (arrêté ou non), l'opération est « interrompue ».

use super::kind::OpKind;
use super::plan::{OpPlan, PlanError, PlannedStep, StepName};

/// Les noms d'étapes (codes stables du journal et du suivi).
pub mod step {
    pub const STOP: &str = "stop";
    pub const REMOVE_CONTAINER: &str = "remove_container";
    pub const FORGET: &str = "forget";
    pub const ERASE: &str = "erase";
}

/// Le plan figé à l'acceptation. `erase_data` : la case « Supprimer aussi les données » est cochée, l'étape
/// d'effacement fait partie du plan (sinon elle n'existe pas : rien ne peut effacer sans l'avoir annoncé).
pub fn op_plan(erase_data: bool) -> Result<OpPlan, PlanError> {
    let name = |text: &str| StepName::parse(text).ok_or(PlanError::StepName);
    let planned = |text: &str, destructive| {
        Ok::<_, PlanError>(PlannedStep {
            name: name(text)?,
            destructive,
        })
    };
    let mut steps = vec![
        planned(step::STOP, false)?,
        planned(step::REMOVE_CONTAINER, true)?,
        planned(step::FORGET, true)?,
    ];
    if erase_data {
        steps.push(planned(step::ERASE, true)?);
    }
    OpPlan::new(steps, Some(name(step::REMOVE_CONTAINER)?))
}

/// Une suppression réserve le nom du service et prend le verrou exclusif (BR-DEL-019, BR-DEL-031).
pub const KIND: OpKind = OpKind::Delete;

#[cfg(test)]
mod tests {
    use super::super::decision::{CancelVerdict, cancel_verdict};
    use super::super::kind::OpStatus;
    use super::super::plan::{StepRecord, StepState, UndoInfo};
    use super::*;
    use time::OffsetDateTime;

    fn row(seq: u32, text: &str, state: StepState) -> StepRecord {
        StepRecord {
            seq,
            step: StepName::parse(text).expect("nom"),
            state,
            undo: UndoInfo::default(),
            at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn the_plan_stops_then_removes_the_container_then_forgets_then_erases() {
        let plan = op_plan(true).expect("plan");
        let names: Vec<_> = plan.steps().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["stop", "remove_container", "forget", "erase"]);
        let destructive: Vec<_> = plan.destructive_steps().map(|s| s.name.as_str()).collect();
        assert_eq!(destructive, ["remove_container", "forget", "erase"]);
        assert_eq!(
            plan.no_return().map(StepName::as_str),
            Some("remove_container")
        );
    }

    #[test]
    fn without_the_data_box_nothing_in_the_plan_can_erase() {
        let plan = op_plan(false).expect("plan");
        assert!(
            plan.steps().iter().all(|s| s.name.as_str() != "erase"),
            "l'effacement n'existe pas dans le plan"
        );
    }

    #[test]
    fn the_data_is_erased_after_the_container_never_before() {
        let plan = op_plan(true).expect("plan");
        let position = |text: &str| {
            plan.steps()
                .iter()
                .position(|s| s.name.as_str() == text)
                .expect("étape")
        };
        assert!(position(step::REMOVE_CONTAINER) < position(step::ERASE));
        assert!(position(step::FORGET) < position(step::ERASE));
    }

    #[test]
    fn cancelling_is_possible_until_the_container_removal_starts_then_too_late() {
        let plan = op_plan(true).expect("plan");
        assert_eq!(
            cancel_verdict(OpStatus::Running, &plan, &[]),
            CancelVerdict::Cancel
        );
        let stopped = [
            row(1, "stop", StepState::Intent),
            row(2, "stop", StepState::Done),
        ];
        assert_eq!(
            cancel_verdict(OpStatus::Running, &plan, &stopped),
            CancelVerdict::Cancel
        );
        let removing = [
            row(1, "stop", StepState::Done),
            row(2, "remove_container", StepState::Intent),
        ];
        assert_eq!(
            cancel_verdict(OpStatus::Running, &plan, &removing),
            CancelVerdict::TooLate
        );
    }

    #[test]
    fn a_delete_cut_by_a_restart_is_never_resumed_and_offers_only_the_check() {
        use super::super::kind::Choice;
        use super::super::recover::{Observed, recover};
        let plan = op_plan(true).expect("plan");
        let mid_erase = [
            row(1, "stop", StepState::Done),
            row(2, "remove_container", StepState::Intent),
            row(3, "remove_container", StepState::Done),
            row(4, "forget", StepState::Intent),
            row(5, "forget", StepState::Done),
            row(6, "erase", StepState::Intent),
        ];
        let recovery = recover(KIND, &plan, &mid_erase, Observed::Unreadable);
        assert_eq!(recovery.status, OpStatus::NeedsAttention);
        assert!(
            recovery.keeps_lock,
            "le nom reste réservé tant que rien n'est vérifié"
        );
        assert_eq!(recovery.choices, [Choice::Verified]);
        assert!(recovery.cleanup.is_empty(), "rien n'est retiré ni rejoué");
    }
}

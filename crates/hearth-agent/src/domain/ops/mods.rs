//! Appliquer un lot de mods : le plan de l'opération (HRT-258, ADR-0062, BR-MOD-025 à 027).
//!
//! Les étapes (codes stables du journal) : `stop` (seulement si le service tournait), `copy` et `verify` (la
//! sauvegarde « Avant mods », cœur commun de `Backups`), `place` (la mise en place : l'UNIQUE étape destructive,
//! point de non-retour de l'annulation), `start` et `watch` (seulement si le service tournait).
//!
//! `place` ne supprime rien : elle pose et renomme sans remplacer. Chaque mouvement est écrit au journal AVANT
//! d'être fait (`UndoInfo::moves`), ce qui permet le retour (HRT-259).

use super::kind::OpKind;
use super::plan::{OpPlan, PlanError, PlannedStep, StepName};

pub mod step {
    pub const STOP: &str = "stop";
    pub const COPY: &str = "copy";
    pub const VERIFY: &str = "verify";
    pub const PLACE: &str = "place";
    pub const START: &str = "start";
    pub const WATCH: &str = "watch";
}

/// Un lot de mods prend le verrou exclusif du service ET la place des opérations lourdes du serveur.
pub const KIND: OpKind = OpKind::Mods;

/// Le plan figé à l'acceptation : `running` dit si le service tourne à ce moment.
pub fn op_plan(running: bool) -> Result<OpPlan, PlanError> {
    let step_of = |text: &str, destructive: bool| -> Result<PlannedStep, PlanError> {
        Ok(PlannedStep {
            name: StepName::parse(text).ok_or(PlanError::StepName)?,
            destructive,
        })
    };
    let mut steps = Vec::new();
    if running {
        steps.push(step_of(step::STOP, false)?);
    }
    steps.push(step_of(step::COPY, false)?);
    steps.push(step_of(step::VERIFY, false)?);
    steps.push(step_of(step::PLACE, true)?);
    if running {
        steps.push(step_of(step::START, false)?);
        steps.push(step_of(step::WATCH, false)?);
    }
    OpPlan::new(
        steps,
        Some(StepName::parse(step::PLACE).ok_or(PlanError::StepName)?),
    )
}

#[cfg(test)]
mod tests {
    use super::super::kind::OpStatus;
    use super::*;

    fn names(plan: &OpPlan) -> Vec<&str> {
        plan.steps().iter().map(|s| s.name.as_str()).collect()
    }

    #[test]
    fn a_running_service_is_stopped_saved_changed_started_and_watched() {
        let plan = op_plan(true).expect("plan");
        assert_eq!(
            names(&plan),
            ["stop", "copy", "verify", "place", "start", "watch"]
        );
    }

    #[test]
    fn a_stopped_service_stays_stopped() {
        let plan = op_plan(false).expect("plan");
        assert_eq!(names(&plan), ["copy", "verify", "place"]);
    }

    #[test]
    fn only_place_destroys_and_it_is_the_point_of_no_return() {
        for running in [true, false] {
            let plan = op_plan(running).expect("plan");
            let destructive: Vec<&str> =
                plan.destructive_steps().map(|s| s.name.as_str()).collect();
            assert_eq!(destructive, ["place"]);
            assert_eq!(plan.no_return().map(StepName::as_str), Some("place"));
        }
    }

    #[test]
    fn cancelling_is_allowed_until_the_placement_begins_and_too_late_after() {
        use super::super::decision::{CancelVerdict, cancel_verdict};
        use super::super::plan::{StepRecord, StepState, UndoInfo};
        let plan = op_plan(true).expect("plan");
        let row = |seq, name: &str, state| StepRecord {
            seq,
            step: StepName::parse(name).unwrap(),
            state,
            undo: UndoInfo::default(),
            at: time::OffsetDateTime::UNIX_EPOCH,
        };
        let early = [
            row(1, "stop", StepState::Done),
            row(2, "copy", StepState::Intent),
        ];
        assert_eq!(
            cancel_verdict(OpStatus::Running, &plan, &early),
            CancelVerdict::Cancel
        );
        let placing = [
            row(1, "stop", StepState::Done),
            row(2, "copy", StepState::Done),
            row(3, "verify", StepState::Done),
            row(4, "place", StepState::Intent),
        ];
        assert_eq!(
            cancel_verdict(OpStatus::Running, &plan, &placing),
            CancelVerdict::TooLate
        );
    }

    #[test]
    fn the_kind_is_exclusive_heavy_io_and_audited_as_service_mods() {
        assert!(KIND.takes_heavy_io());
        assert_eq!(KIND.as_str(), "mods");
        assert_eq!(
            KIND.attention_choices(),
            &[super::super::Choice::Revert, super::super::Choice::Verified]
        );
    }
}

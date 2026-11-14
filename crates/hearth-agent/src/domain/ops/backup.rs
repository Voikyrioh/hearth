//! Sauvegarder un service : le plan de l'opération et les modes (HRT-97, BR-BACKUP-006 à 008, 010, 011, 025,
//! 029).
//!
//! Aucune étape ne détruit ce que l'agent garde : la copie écrit une archive qui n'existait pas (un objet
//! CRÉÉ par l'opération, retiré après vérification de son inode si l'agent s'arrête, `BR-BACKUP-025`), et un
//! arrêt n'enlève rien. Une opération interrompue par un redémarrage de l'agent est donc classée
//! « interrompue », jamais reprise ni rejouée.
//!
//! Les étapes (codes stables du journal) : `stop` (seulement « Arrêter, sauvegarder, relancer »), `copy`,
//! `verify`, `start` (seulement ce mode, et seulement si le service tournait).

use hearth_proto::api::backups::BackupMode;

use super::kind::OpKind;
use super::plan::{OpPlan, PlanError, PlannedStep, StepName};

pub mod step {
    pub const STOP: &str = "stop";
    pub const COPY: &str = "copy";
    pub const VERIFY: &str = "verify";
    pub const START: &str = "start";
}

/// Une sauvegarde prend le verrou exclusif du service ET la place des opérations lourdes du serveur.
pub const KIND: OpKind = OpKind::Backup;

/// Le plan figé à l'acceptation.
pub fn op_plan(mode: BackupMode) -> Result<OpPlan, PlanError> {
    let planned = |text: &str| {
        Ok::<_, PlanError>(PlannedStep {
            name: StepName::parse(text).ok_or(PlanError::StepName)?,
            destructive: false,
        })
    };
    let mut steps = Vec::new();
    if mode == BackupMode::StopCopyStart {
        steps.push(planned(step::STOP)?);
    }
    steps.push(planned(step::COPY)?);
    steps.push(planned(step::VERIFY)?);
    if mode == BackupMode::StopCopyStart {
        steps.push(planned(step::START)?);
    }
    OpPlan::new(steps, None)
}

/// Le mode effectif d'après l'état du service (BR-BACKUP-006) : un service qui n'est pas en marche se copie
/// directement (`Stopped`), quel que soit le mode demandé ; un service en marche exige un choix explicite entre
/// la copie à chaud et l'arrêt (Hearth n'arrête jamais un service sans que le compte ait choisi ce mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeVerdict {
    Mode(BackupMode),
    /// Le mode demandé n'est pas permis : `stopped` pour un service en marche, ou un mode inconnu.
    Refused,
}

pub fn effective_mode(requested: BackupMode, running: bool) -> ModeVerdict {
    match (requested, running) {
        (BackupMode::Unknown, _) => ModeVerdict::Refused,
        (BackupMode::Stopped, true) => ModeVerdict::Refused,
        (_, false) => ModeVerdict::Mode(BackupMode::Stopped),
        (mode, true) => ModeVerdict::Mode(mode),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(plan: &OpPlan) -> Vec<&str> {
        plan.steps().iter().map(|s| s.name.as_str()).collect()
    }

    #[test]
    fn a_stopped_or_hot_backup_copies_then_verifies_and_never_stops_the_service() {
        for mode in [BackupMode::Stopped, BackupMode::Hot] {
            let plan = op_plan(mode).expect("plan");
            assert_eq!(names(&plan), ["copy", "verify"]);
            assert_eq!(plan.destructive_steps().count(), 0);
            assert!(plan.no_return().is_none());
        }
    }

    #[test]
    fn stopping_to_copy_brackets_the_copy_with_a_stop_and_a_start() {
        let plan = op_plan(BackupMode::StopCopyStart).expect("plan");
        assert_eq!(names(&plan), ["stop", "copy", "verify", "start"]);
        assert_eq!(plan.destructive_steps().count(), 0);
    }

    #[test]
    fn a_service_that_is_not_running_is_copied_directly_whatever_was_asked() {
        for requested in [
            BackupMode::Hot,
            BackupMode::StopCopyStart,
            BackupMode::Stopped,
        ] {
            assert_eq!(
                effective_mode(requested, false),
                ModeVerdict::Mode(BackupMode::Stopped)
            );
        }
    }

    #[test]
    fn a_running_service_needs_an_explicit_choice_and_is_never_stopped_by_default() {
        assert_eq!(
            effective_mode(BackupMode::Hot, true),
            ModeVerdict::Mode(BackupMode::Hot)
        );
        assert_eq!(
            effective_mode(BackupMode::StopCopyStart, true),
            ModeVerdict::Mode(BackupMode::StopCopyStart)
        );
        assert_eq!(
            effective_mode(BackupMode::Stopped, true),
            ModeVerdict::Refused
        );
        assert_eq!(
            effective_mode(BackupMode::Unknown, true),
            ModeVerdict::Refused
        );
        assert_eq!(
            effective_mode(BackupMode::Unknown, false),
            ModeVerdict::Refused
        );
    }

    #[test]
    fn a_backup_is_exclusive_and_takes_the_heavy_io_place() {
        assert!(KIND.takes_heavy_io());
    }
}

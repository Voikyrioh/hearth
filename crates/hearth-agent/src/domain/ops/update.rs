//! Modifier un service par recréation : le plan de l'opération (HRT-87, BR-EDIT-018 à BR-EDIT-022, BR-EDIT-031).
//!
//! L'ordre est celui de la spec et ne change jamais : vérification, récupération de l'image (le service continue
//! de tourner, annulable), puis l'ancien conteneur est arrêté s'il tournait, **mis de côté** (renommé), le nouveau
//! est créé depuis la description, lancé s'il tournait (puis surveillé 10 secondes), et seulement alors l'ancien
//! est supprimé. En cas d'échec du nouveau, il est retiré et l'ancien remis (retour arrière).
//!
//! Les étapes qui détruisent ou remplacent écrivent leur `intent` avant le geste (`stop_old`, `rename_old`,
//! `create_new`, `start_new`, `remove_old`) : un agent qui redémarre au milieu laisse une opération
//! `interrupted` si l'état est cohérent, `needs_attention` s'il est ambigu (deux conteneurs), jamais reprise
//! (BR-EDIT-031, `recover`).
//!
//! Le point de non-retour de l'annulation est la première étape destructive du plan : tant que la récupération
//! de l'image n'est pas finie et l'arrêt pas commencé, rien n'a été touché (BR-EDIT-021).

use super::kind::OpKind;
use super::plan::{OpPlan, PlanError, PlannedStep, StepName};

/// Les noms d'étapes (codes stables du journal et du suivi).
pub mod step {
    pub const CHECK: &str = "check";
    pub const PULL: &str = "pull";
    pub const STOP_OLD: &str = "stop_old";
    pub const RENAME_OLD: &str = "rename_old";
    pub const CREATE_NEW: &str = "create_new";
    pub const START_NEW: &str = "start_new";
    pub const WATCH: &str = "watch";
    pub const REMOVE_OLD: &str = "remove_old";
}

/// Une modification est une opération `update`, qui prend le verrou exclusif du service (BR-EDIT-025).
pub const KIND: OpKind = OpKind::Update;

/// Ce que l'acceptation a constaté du service, et qui décide des étapes du plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shape {
    /// Un conteneur existe (sinon : « Recréer le service » d'un conteneur disparu, BR-EDIT-036).
    pub had_container: bool,
    /// Le service tournait : il est arrêté avant d'être recréé, puis relancé (BR-EDIT-015).
    pub was_running: bool,
}

/// Le plan figé à l'acceptation.
///
/// - sans conteneur : `check`, `pull`, `create_new` ; le service reste arrêté (jamais démarré seul) ;
/// - arrêté : `check`, `pull`, `rename_old`, `create_new`, `remove_old` ; il reste arrêté (BR-EDIT-015) ;
/// - en marche : tout le chemin, arrêt et démarrage compris.
pub fn op_plan(shape: Shape) -> Result<OpPlan, PlanError> {
    let name = |text: &str| StepName::parse(text).ok_or(PlanError::StepName);
    let planned = |text: &str, destructive| {
        Ok::<_, PlanError>(PlannedStep {
            name: name(text)?,
            destructive,
        })
    };
    let mut steps = vec![planned(step::CHECK, false)?, planned(step::PULL, false)?];
    if shape.had_container {
        if shape.was_running {
            steps.push(planned(step::STOP_OLD, true)?);
        }
        steps.push(planned(step::RENAME_OLD, true)?);
    }
    steps.push(planned(step::CREATE_NEW, true)?);
    if shape.had_container && shape.was_running {
        steps.push(planned(step::START_NEW, true)?);
        steps.push(planned(step::WATCH, false)?);
    }
    if shape.had_container {
        steps.push(planned(step::REMOVE_OLD, true)?);
    }
    // Le point de non-retour : la première étape destructive du plan.
    let first = steps
        .iter()
        .find(|planned| planned.destructive)
        .map(|planned| planned.name.clone());
    OpPlan::new(steps, first)
}

/// Le plan d'un changement qui ne recrée rien (l'adresse à donner aux joueurs) : une seule étape, non
/// destructive. La description change, le conteneur n'est pas touché ; l'opération existe pour que le changement
/// ait son issue au journal d'activité (BR-EDIT-033).
pub fn settings_plan() -> Result<OpPlan, PlanError> {
    let name = StepName::parse(step::CHECK).ok_or(PlanError::StepName)?;
    OpPlan::new(
        vec![PlannedStep {
            name,
            destructive: false,
        }],
        None,
    )
}

/// Le nom provisoire sous lequel l'ancien conteneur est mis de côté pendant la modification
/// (BR-EDIT-018) : le nom du service, un point, la marque et la révision de départ.
pub fn parked_name(service_name: &str, rev: u32) -> String {
    format!("{service_name}.hearth-prev-{rev}")
}

/// L'issue du nouveau service jugée après sa surveillance (BR-EDIT-019).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Watched {
    /// Il est resté en marche pendant toute la fenêtre.
    Held,
    /// Il s'est arrêté (code zéro compris), a été relancé par le moteur ou a disparu dans la fenêtre.
    Fell,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(plan: &OpPlan) -> Vec<&str> {
        plan.steps().iter().map(|s| s.name.as_str()).collect()
    }

    #[test]
    fn a_running_service_goes_all_the_way_and_the_old_one_goes_last() {
        let plan = op_plan(Shape {
            had_container: true,
            was_running: true,
        })
        .expect("plan");
        assert_eq!(
            names(&plan),
            [
                "check",
                "pull",
                "stop_old",
                "rename_old",
                "create_new",
                "start_new",
                "watch",
                "remove_old"
            ]
        );
        assert_eq!(plan.no_return().map(StepName::as_str), Some("stop_old"));
        let destructive: Vec<_> = plan.destructive_steps().map(|s| s.name.as_str()).collect();
        assert_eq!(
            destructive,
            [
                "stop_old",
                "rename_old",
                "create_new",
                "start_new",
                "remove_old"
            ]
        );
    }

    #[test]
    fn a_stopped_service_is_recreated_and_stays_stopped() {
        let plan = op_plan(Shape {
            had_container: true,
            was_running: false,
        })
        .expect("plan");
        assert_eq!(
            names(&plan),
            ["check", "pull", "rename_old", "create_new", "remove_old"]
        );
        assert_eq!(plan.no_return().map(StepName::as_str), Some("rename_old"));
        assert!(
            plan.steps()
                .iter()
                .all(|s| s.name.as_str() != "start_new" && s.name.as_str() != "stop_old"),
            "Hearth ne démarre jamais un service que personne n'a demandé de démarrer (BR-EDIT-015)"
        );
    }

    #[test]
    fn a_missing_container_is_recreated_without_an_old_one_and_never_started() {
        let plan = op_plan(Shape {
            had_container: false,
            was_running: false,
        })
        .expect("plan");
        assert_eq!(names(&plan), ["check", "pull", "create_new"]);
        assert_eq!(plan.no_return().map(StepName::as_str), Some("create_new"));
    }

    #[test]
    fn the_pull_comes_before_anything_that_touches_the_service() {
        for (had_container, was_running) in [(true, true), (true, false), (false, false)] {
            let plan = op_plan(Shape {
                had_container,
                was_running,
            })
            .expect("plan");
            let at = |text: &str| plan.steps().iter().position(|s| s.name.as_str() == text);
            assert!(
                at("pull")
                    < plan
                        .destructive_steps()
                        .next()
                        .and_then(|d| at(d.name.as_str()))
            );
            assert!(
                !plan
                    .step(&StepName::parse("pull").unwrap())
                    .unwrap()
                    .destructive
            );
        }
    }

    #[test]
    fn a_settings_change_recreates_nothing() {
        let plan = settings_plan().expect("plan");
        assert_eq!(names(&plan), ["check"]);
        assert_eq!(plan.destructive_steps().count(), 0);
        assert!(plan.no_return().is_none());
    }

    #[test]
    fn the_old_container_is_parked_under_a_provisional_name() {
        assert_eq!(parked_name("terraria", 4), "terraria.hearth-prev-4");
    }
}

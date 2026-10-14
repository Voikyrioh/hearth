//! Ce que « le moteur dit du service » veut dire pour la reprise (HRT-144, ADR-0045) : de ce que l'adaptateur
//! a COMPTÉ (aucun conteneur, un seul, plusieurs, illisible) à l'[`Observed`] que `recover` classe. Fonction
//! pure : le domaine ne parle pas au moteur, il juge un nombre.
//!
//! La règle est prudente. Un état n'est dit **cohérent** que si le plan de l'opération est allé jusqu'au
//! bout de ses gestes destructifs (chacun a son `done`) ET que ce que le moteur montre est exactement ce
//! que la fin du plan laisse : un conteneur, ou aucun pour une suppression. Tout le reste est « à vérifier »,
//! jamais supposé bon :
//!
//! - plusieurs conteneurs pour la même clé : [`Observed::Ambiguous`] (l'ancien et le nouveau coexistent) ;
//! - le moteur n'a pas répondu : [`Observed::Unreadable`] ;
//! - un plan à moitié exécuté, ou un état qui ne correspond pas à la fin du plan : [`Observed::Unreadable`]
//!   (le compte regarde, l'agent ne devine pas).

use super::kind::OpKind;
use super::plan::{OpPlan, StepRecord, StepState, latest_state};
use super::recover::{ConfigSide, Observed};

/// Combien de conteneurs le moteur montre pour la clé du service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    Absent,
    One,
    Several,
    /// Le moteur n'a pas répondu, ou n'ouvre pas ce service (clé qu'il ne sait pas résoudre).
    Unreadable,
}

/// Le service existe-t-il encore une fois le plan terminé ? Une suppression le retire, tout autre genre le
/// laisse.
const fn survives(kind: OpKind) -> bool {
    !matches!(kind, OpKind::Delete)
}

/// Tous les gestes destructifs du plan ont leur `done`, et il y en a au moins un.
fn destructive_part_is_complete(plan: &OpPlan, records: &[StepRecord]) -> bool {
    let mut any = false;
    for step in plan.destructive_steps() {
        any = true;
        if latest_state(records, &step.name) != Some(StepState::Done) {
            return false;
        }
    }
    any
}

/// L'état observé d'après le décompte des conteneurs.
pub fn observed_from_presence(
    kind: OpKind,
    plan: &OpPlan,
    records: &[StepRecord],
    presence: Presence,
) -> Observed {
    match presence {
        Presence::Unreadable => Observed::Unreadable,
        Presence::Several => Observed::Ambiguous,
        Presence::Absent | Presence::One => {
            if !destructive_part_is_complete(plan, records) {
                return Observed::Unreadable;
            }
            let expected = if survives(kind) {
                Presence::One
            } else {
                Presence::Absent
            };
            if presence == expected {
                Observed::Coherent(ConfigSide::After)
            } else {
                Observed::Unreadable
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use time::OffsetDateTime;

    use super::super::plan::{PlannedStep, StepName, UndoInfo};
    use super::*;

    fn name(text: &str) -> StepName {
        StepName::parse(text).unwrap()
    }

    fn plan() -> OpPlan {
        let step = |text: &str, destructive| PlannedStep {
            name: name(text),
            destructive,
        };
        OpPlan::new(
            vec![
                step("check", false),
                step("stop_old", true),
                step("create_new", true),
            ],
            Some(name("create_new")),
        )
        .unwrap()
    }

    fn row(seq: u32, step: &str, state: StepState) -> StepRecord {
        StepRecord {
            seq,
            step: name(step),
            state,
            undo: UndoInfo::default(),
            at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    fn complete() -> Vec<StepRecord> {
        vec![
            row(1, "stop_old", StepState::Intent),
            row(2, "stop_old", StepState::Done),
            row(3, "create_new", StepState::Intent),
            row(4, "create_new", StepState::Done),
        ]
    }

    #[test]
    fn the_table_present_absent_duplicated_unreadable() {
        let half = vec![
            row(1, "stop_old", StepState::Intent),
            row(2, "stop_old", StepState::Done),
        ];
        for (why, kind, rows, presence, expected) in [
            (
                "plan complet, un conteneur : cohérent",
                OpKind::Update,
                complete(),
                Presence::One,
                Observed::Coherent(ConfigSide::After),
            ),
            (
                "plan complet, deux conteneurs : ambigu",
                OpKind::Update,
                complete(),
                Presence::Several,
                Observed::Ambiguous,
            ),
            (
                "deux conteneurs même sans rien de fini : ambigu, jamais supposé bon",
                OpKind::Update,
                vec![],
                Presence::Several,
                Observed::Ambiguous,
            ),
            (
                "le moteur ne répond pas : illisible",
                OpKind::Update,
                complete(),
                Presence::Unreadable,
                Observed::Unreadable,
            ),
            (
                "plan complet mais plus aucun conteneur : ne correspond pas à la fin du plan",
                OpKind::Update,
                complete(),
                Presence::Absent,
                Observed::Unreadable,
            ),
            (
                "plan à moitié exécuté : jamais présumé cohérent",
                OpKind::Update,
                half,
                Presence::One,
                Observed::Unreadable,
            ),
            (
                "aucun geste destructif fait : rien à dire",
                OpKind::Update,
                vec![],
                Presence::One,
                Observed::Unreadable,
            ),
            (
                "suppression terminée, plus de conteneur : cohérent",
                OpKind::Delete,
                complete(),
                Presence::Absent,
                Observed::Coherent(ConfigSide::After),
            ),
            (
                "suppression terminée mais un conteneur reste : à vérifier",
                OpKind::Delete,
                complete(),
                Presence::One,
                Observed::Unreadable,
            ),
        ] {
            assert_eq!(
                observed_from_presence(kind, &plan(), &rows, presence),
                expected,
                "{why}"
            );
        }
    }

    #[test]
    fn an_undone_or_failed_step_is_not_a_complete_plan() {
        let rows = vec![
            row(1, "stop_old", StepState::Intent),
            row(2, "stop_old", StepState::Done),
            row(3, "create_new", StepState::Intent),
            row(4, "create_new", StepState::Failed),
        ];
        assert_eq!(
            observed_from_presence(OpKind::Update, &plan(), &rows, Presence::One),
            Observed::Unreadable
        );
    }
}

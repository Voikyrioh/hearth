//! Que faire d'une opération non terminée quand l'agent redémarre (HRT-83, ADR-0045, BR-EDIT-031,
//! BR-DEL-025, BR-BACKUP-025, BR-JEU-011, BR-MONDE-026, BR-LIFE-013).
//!
//! **L'agent ne reprend, ne rejoue et ne défait jamais seul un geste sur un service ou sur ses
//! données.** Il constate, classe, dit. Le tableau (fonction pure, testée en table, sans base ni
//! moteur) :
//!
//! | Ce que le journal et l'observation disent | Résultat |
//! |---|---|
//! | Aucune étape destructive n'a de ligne | `interrupted`, « non exécuté » |
//! | Une étape destructive a un `intent` sans `done` | `needs_attention`, verrou REPRIS |
//! | Une étape destructive a échoué | `needs_attention`, verrou repris |
//! | Tout est `done`, un seul état complet (ancien ou nouveau) | `interrupted`, « tourne avec {l'ancienne, la nouvelle} » |
//! | Tout est `done`, deux états coexistent | `needs_attention` |
//! | Tout est `done`, l'état n'a pas pu être lu | `needs_attention` |
//!
//! La seule chose retirée sans qu'on le demande : les objets que l'opération avait créés POUR ELLE-MÊME
//! (archive partielle, partie reçue, dossier d'import, dossier d'attente d'une restauration non
//! commencée), nommés au journal. Le retrait revérifie l'objet par son numéro d'inode (infrastructure).

use std::collections::HashSet;

use super::kind::{Cause, Choice, OpKind, OpStatus};
use super::plan::{CreatedObject, ObjectKind, OpPlan, StepRecord, StepState, latest_state};

/// De quel côté de la modification se trouve le service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSide {
    /// La configuration d'avant l'opération.
    Before,
    /// La configuration voulue par l'opération.
    After,
}

/// Ce que le moteur dit du service, jugé par l'appelant (l'adaptateur du moteur, jamais le domaine).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Observed {
    /// Un seul état COMPLET : un conteneur, entier, de ce côté.
    Coherent(ConfigSide),
    /// Deux états coexistent (deux conteneurs, un dossier à moitié échangé).
    Ambiguous,
    /// Le moteur n'a pas répondu, ou ce qu'il a dit ne se range nulle part. Traité comme ambigu.
    Unreadable,
}

/// La décision de l'agent pour une opération non terminée.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recovery {
    /// `Interrupted` ou `NeedsAttention`, jamais autre chose.
    pub status: OpStatus,
    pub cause: Cause,
    /// Le verrou du service reste tenu (opération `needs_attention`).
    pub keeps_lock: bool,
    /// Les choix offerts au compte (vide si rien n'attend).
    pub choices: Vec<Choice>,
    /// Les objets que l'opération avait créés pour elle-même et qui doivent partir.
    pub cleanup: Vec<CreatedObject>,
}

/// Classe une opération non terminée au démarrage de l'agent.
pub fn recover(
    kind: OpKind,
    plan: &OpPlan,
    records: &[StepRecord],
    observed: Observed,
) -> Recovery {
    let destructive: Vec<_> = plan.destructive_steps().collect();
    let any_destructive_row = destructive
        .iter()
        .any(|step| latest_state(records, &step.name).is_some());
    let cleanup = removable_objects(plan, records, any_destructive_row);

    let attention = |cause| Recovery {
        status: OpStatus::NeedsAttention,
        cause,
        keeps_lock: true,
        choices: kind.attention_choices().to_vec(),
        cleanup: cleanup.clone(),
    };
    let interrupted = |cause| Recovery {
        status: OpStatus::Interrupted,
        cause,
        keeps_lock: false,
        choices: Vec::new(),
        cleanup: cleanup.clone(),
    };

    if !any_destructive_row {
        // Démarrer, arrêter, redémarrer (HRT-64, BR-LIFE-020) : ni l'ordre donné au moteur ni son issue ne se
        // lisent dans le journal (aucune de leurs étapes n'est destructive), donc « non exécuté » serait
        // faux : l'issue est inconnue, et le compte relit l'état réel.
        let cause = if kind.is_lifecycle() {
            Cause::Interrupted
        } else {
            Cause::NotExecuted
        };
        return interrupted(cause);
    }
    let states: Vec<_> = destructive
        .iter()
        .filter_map(|step| latest_state(records, &step.name))
        .collect();
    if states.contains(&StepState::Intent) {
        return attention(Cause::IntentWithoutDone);
    }
    if states.contains(&StepState::Failed) {
        return attention(Cause::StepFailed);
    }
    match observed {
        Observed::Coherent(ConfigSide::Before) => interrupted(Cause::RunsWithPrevious),
        Observed::Coherent(ConfigSide::After) => interrupted(Cause::RunsWithNew),
        Observed::Ambiguous => attention(Cause::TwoStates),
        Observed::Unreadable => attention(Cause::StateUnknown),
    }
}

/// Les objets qu'une opération ANNULÉE (par le compte, ou par l'expiration d'une décision sans réponse) avait
/// créés pour elle-même et qui doivent partir : les mêmes que pour une reprise, et rien d'autre (HRT-144,
/// ADR-0045). L'annulation n'existe qu'avant le point de non-retour, mais un geste destructif a pu précéder
/// ce point : tant qu'un geste destructif a une ligne, un dossier d'import ou d'attente reste en place (il
/// pourrait être celui d'un retour arrière). Aucun geste sur le service ou ses données.
pub fn cleanup_on_cancel(plan: &OpPlan, records: &[StepRecord]) -> Vec<CreatedObject> {
    let any_destructive_row = plan
        .destructive_steps()
        .any(|step| latest_state(records, &step.name).is_some());
    removable_objects(plan, records, any_destructive_row)
}

/// Une opération dont le journal des étapes ne se relit pas (ligne illisible, état inconnu, version plus
/// récente du binaire) : on ne sait pas ce qui a eu lieu, donc `needs_attention`, verrou repris, et RIEN
/// n'est retiré (le journal qui nommerait les objets créés est précisément ce qui ne se lit pas).
pub fn recover_unreadable_journal(kind: OpKind) -> Recovery {
    Recovery {
        status: OpStatus::NeedsAttention,
        cause: Cause::StateUnknown,
        keeps_lock: true,
        choices: kind.attention_choices().to_vec(),
        cleanup: Vec::new(),
    }
}

/// Les objets créés par l'opération qui sont à retirer, sans doublon, dans l'ordre du journal. Une ligne
/// du journal qui nomme un objet hors règle a déjà été écartée à la lecture (`UndoInfo::from_json`).
fn removable_objects(
    plan: &OpPlan,
    records: &[StepRecord],
    any_destructive_row: bool,
) -> Vec<CreatedObject> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let mut ordered: Vec<&StepRecord> = records.iter().collect();
    ordered.sort_by_key(|record| record.seq);
    for record in ordered {
        // Une ligne d'un nom d'étape absent du plan ne dit rien de fiable : rien n'est retiré pour elle.
        if plan.step(&record.step).is_none() {
            continue;
        }
        for object in &record.undo.created {
            let removable = match object.kind {
                ObjectKind::PartialArchive | ObjectKind::ReceivedPart => {
                    latest_state(records, &record.step) != Some(StepState::Done)
                }
                ObjectKind::ImportDir | ObjectKind::StagingDir => !any_destructive_row,
            };
            if removable && object.is_well_formed() && seen.insert(object.clone()) {
                out.push(object.clone());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use time::OffsetDateTime;

    use super::super::plan::{ObjectRoot, PlannedStep, StepName, UndoInfo};
    use super::*;

    fn name(text: &str) -> StepName {
        StepName::parse(text).unwrap()
    }

    /// Mise à jour par recréation : vérification (non destructive), puis cinq gestes destructifs.
    fn update_plan() -> OpPlan {
        let step = |text: &str, destructive| PlannedStep {
            name: name(text),
            destructive,
        };
        OpPlan::new(
            vec![
                step("check", false),
                step("stop_old", true),
                step("rename_old", true),
                step("create_new", true),
                step("start_new", true),
                step("remove_old", true),
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

    fn object(rel: &str, kind: ObjectKind) -> CreatedObject {
        CreatedObject {
            root: ObjectRoot::Staging,
            rel: rel.to_owned(),
            dev: 7,
            ino: 42,
            kind,
        }
    }

    fn with_object(mut record: StepRecord, object: CreatedObject) -> StepRecord {
        record.undo.created.push(object);
        record
    }

    use StepState::{Done, Failed, Intent, Undone};

    #[test]
    fn the_table_of_the_design() {
        struct Case {
            why: &'static str,
            rows: Vec<StepRecord>,
            observed: Observed,
            status: OpStatus,
            cause: Cause,
            lock: bool,
        }
        let cases = [
            Case {
                why: "aucune ligne du tout",
                rows: vec![],
                observed: Observed::Coherent(ConfigSide::Before),
                status: OpStatus::Interrupted,
                cause: Cause::NotExecuted,
                lock: false,
            },
            Case {
                why: "seule la vérification non destructive est faite",
                rows: vec![row(1, "check", Done)],
                observed: Observed::Unreadable,
                status: OpStatus::Interrupted,
                cause: Cause::NotExecuted,
                lock: false,
            },
            Case {
                why: "le moteur ne répond pas mais rien de destructif n'a commencé : rien à craindre",
                rows: vec![row(1, "check", Intent)],
                observed: Observed::Ambiguous,
                status: OpStatus::Interrupted,
                cause: Cause::NotExecuted,
                lock: false,
            },
            Case {
                why: "intent sans done sur la première étape destructive",
                rows: vec![row(1, "check", Done), row(2, "stop_old", Intent)],
                observed: Observed::Coherent(ConfigSide::Before),
                status: OpStatus::NeedsAttention,
                cause: Cause::IntentWithoutDone,
                lock: true,
            },
            Case {
                why: "intent sans done après des étapes faites : le moteur ne change rien",
                rows: vec![
                    row(1, "stop_old", Intent),
                    row(2, "stop_old", Done),
                    row(3, "rename_old", Intent),
                    row(4, "rename_old", Done),
                    row(5, "create_new", Intent),
                ],
                observed: Observed::Coherent(ConfigSide::After),
                status: OpStatus::NeedsAttention,
                cause: Cause::IntentWithoutDone,
                lock: true,
            },
            Case {
                why: "dernière étape done, ancien état complet et unique",
                rows: vec![row(1, "stop_old", Intent), row(2, "stop_old", Done)],
                observed: Observed::Coherent(ConfigSide::Before),
                status: OpStatus::Interrupted,
                cause: Cause::RunsWithPrevious,
                lock: false,
            },
            Case {
                why: "dernière étape done, nouvel état complet et unique",
                rows: vec![
                    row(1, "create_new", Intent),
                    row(2, "create_new", Done),
                    row(3, "start_new", Intent),
                    row(4, "start_new", Done),
                    row(5, "remove_old", Intent),
                    row(6, "remove_old", Done),
                ],
                observed: Observed::Coherent(ConfigSide::After),
                status: OpStatus::Interrupted,
                cause: Cause::RunsWithNew,
                lock: false,
            },
            Case {
                why: "tout est done mais deux états coexistent",
                rows: vec![row(1, "create_new", Intent), row(2, "create_new", Done)],
                observed: Observed::Ambiguous,
                status: OpStatus::NeedsAttention,
                cause: Cause::TwoStates,
                lock: true,
            },
            Case {
                why: "tout est done mais le moteur ne répond pas",
                rows: vec![row(1, "stop_old", Intent), row(2, "stop_old", Done)],
                observed: Observed::Unreadable,
                status: OpStatus::NeedsAttention,
                cause: Cause::StateUnknown,
                lock: true,
            },
            Case {
                why: "un geste destructif a échoué",
                rows: vec![row(1, "stop_old", Intent), row(2, "stop_old", Failed)],
                observed: Observed::Coherent(ConfigSide::Before),
                status: OpStatus::NeedsAttention,
                cause: Cause::StepFailed,
                lock: true,
            },
            Case {
                why: "un geste défait par un retour arrière, l'ancien état est revenu",
                rows: vec![
                    row(1, "stop_old", Intent),
                    row(2, "stop_old", Done),
                    row(3, "stop_old", Undone),
                ],
                observed: Observed::Coherent(ConfigSide::Before),
                status: OpStatus::Interrupted,
                cause: Cause::RunsWithPrevious,
                lock: false,
            },
        ];
        for case in cases {
            let recovery = recover(OpKind::Update, &update_plan(), &case.rows, case.observed);
            assert_eq!(recovery.status, case.status, "{}", case.why);
            assert_eq!(recovery.cause, case.cause, "{}", case.why);
            assert_eq!(recovery.keeps_lock, case.lock, "{}", case.why);
            assert_eq!(
                recovery.choices.is_empty(),
                case.status == OpStatus::Interrupted,
                "{}",
                case.why
            );
        }
    }

    #[test]
    fn needs_attention_offers_the_choices_of_the_kind_and_never_decides_for_the_account() {
        let rows = vec![row(1, "stop_old", Intent)];
        let update = recover(OpKind::Update, &update_plan(), &rows, Observed::Ambiguous);
        assert_eq!(
            update.choices,
            [Choice::Revert, Choice::Keep, Choice::Verified]
        );
        let delete = recover(OpKind::Delete, &update_plan(), &rows, Observed::Ambiguous);
        assert_eq!(delete.choices, [Choice::Verified]);
        // Quel que soit le cas, la décision de l'agent n'est jamais « terminé avec succès », « défait »
        // ou « relancé » : seulement interrompu ou à vérifier.
        for observed in [
            Observed::Coherent(ConfigSide::Before),
            Observed::Coherent(ConfigSide::After),
            Observed::Ambiguous,
            Observed::Unreadable,
        ] {
            for rows in [vec![], rows.clone(), vec![row(1, "stop_old", Done)]] {
                let status = recover(OpKind::Update, &update_plan(), &rows, observed).status;
                assert!(matches!(
                    status,
                    OpStatus::Interrupted | OpStatus::NeedsAttention
                ));
            }
        }
    }

    #[test]
    fn a_partial_archive_goes_unless_its_step_is_done() {
        let archive = object("backup/01J.tar.gz.part", ObjectKind::PartialArchive);
        let rows = vec![with_object(row(1, "create_new", Intent), archive.clone())];
        let recovery = recover(OpKind::Update, &update_plan(), &rows, Observed::Ambiguous);
        assert_eq!(recovery.cleanup, std::slice::from_ref(&archive));
        // L'étape qui l'a produite est finie : l'archive est complète, elle reste.
        let done = vec![
            with_object(row(1, "create_new", Intent), archive.clone()),
            with_object(row(2, "create_new", Done), archive),
        ];
        let recovery = recover(OpKind::Update, &update_plan(), &done, Observed::Ambiguous);
        assert!(recovery.cleanup.is_empty());
    }

    #[test]
    fn an_import_or_staging_directory_goes_only_while_nothing_destructive_has_begun() {
        let import = object("import-01J", ObjectKind::ImportDir);
        let staging = object("restore-01J", ObjectKind::StagingDir);
        // La vérification (non destructive) a créé le dossier ; rien de destructif n'a de ligne.
        let rows = vec![
            with_object(row(1, "check", Done), import.clone()),
            with_object(row(2, "check", Done), staging.clone()),
        ];
        let recovery = recover(OpKind::Restore, &update_plan(), &rows, Observed::Unreadable);
        assert_eq!(recovery.status, OpStatus::Interrupted);
        assert_eq!(recovery.cleanup, [import.clone(), staging.clone()]);
        // Dès qu'un geste destructif a une ligne, le dossier d'attente peut être tout ce qui reste de la
        // donnée : il n'est plus retiré.
        let begun = vec![
            with_object(row(1, "check", Done), import),
            with_object(row(2, "check", Done), staging),
            row(3, "stop_old", Intent),
        ];
        let recovery = recover(
            OpKind::Restore,
            &update_plan(),
            &begun,
            Observed::Unreadable,
        );
        assert_eq!(recovery.status, OpStatus::NeedsAttention);
        assert!(recovery.cleanup.is_empty());
    }

    #[test]
    fn only_objects_named_in_the_journal_are_ever_removed_and_each_only_once() {
        let archive = object("a.part", ObjectKind::PartialArchive);
        let rows = vec![
            with_object(row(1, "create_new", Intent), archive.clone()),
            // Le même objet nommé deux fois (relecture, deuxième ligne) : un seul retrait.
            with_object(row(2, "create_new", Failed), archive.clone()),
        ];
        let recovery = recover(OpKind::Update, &update_plan(), &rows, Observed::Ambiguous);
        assert_eq!(recovery.cleanup, std::slice::from_ref(&archive));
        // Pas de ligne, pas de nettoyage.
        let none = recover(OpKind::Update, &update_plan(), &[], Observed::Ambiguous);
        assert!(none.cleanup.is_empty());
    }

    #[test]
    fn a_journal_line_for_a_step_the_plan_does_not_know_removes_nothing() {
        let archive = object("a.part", ObjectKind::PartialArchive);
        let rows = vec![with_object(row(1, "mystery", Intent), archive)];
        let recovery = recover(OpKind::Update, &update_plan(), &rows, Observed::Ambiguous);
        assert!(recovery.cleanup.is_empty());
    }

    #[test]
    fn an_object_with_a_path_outside_its_root_is_never_listed() {
        let escaping = object("../outside", ObjectKind::PartialArchive);
        let rows = vec![with_object(row(1, "create_new", Intent), escaping)];
        let recovery = recover(OpKind::Update, &update_plan(), &rows, Observed::Ambiguous);
        assert!(recovery.cleanup.is_empty());
    }

    /// Démarrer, arrêter, redémarrer (HRT-64, BR-LIFE-020) : aucun geste n'est destructif au sens du journal
    /// (un arrêt ne perd aucune donnée de l'agent, et il se rejoue par le compte), donc l'agent qui redémarre
    /// classe l'action `interrupted` avec sa cause propre, verrou rendu, jamais « à vérifier ».
    #[test]
    fn a_lifecycle_action_cut_by_a_restart_is_interrupted_and_frees_the_service() {
        let plan = OpPlan::new(
            vec![PlannedStep {
                name: name("stop"),
                destructive: false,
            }],
            None,
        )
        .unwrap();
        for kind in [OpKind::Start, OpKind::Stop, OpKind::Restart] {
            for rows in [
                vec![],
                vec![row(1, "stop", Intent)],
                vec![row(1, "stop", Done)],
            ] {
                let recovery = recover(kind, &plan, &rows, Observed::Unreadable);
                assert_eq!(recovery.status, OpStatus::Interrupted, "{kind:?}");
                assert_eq!(recovery.cause, Cause::Interrupted, "{kind:?}");
                assert!(!recovery.keeps_lock);
                assert!(recovery.choices.is_empty());
            }
        }
        // Les autres genres gardent leur cause : « non exécuté ».
        let delete = recover(OpKind::Delete, &plan, &[], Observed::Unreadable);
        assert_eq!(delete.cause, Cause::NotExecuted);
    }
}

//! BR-UPDATE-028 : un travail laissé en cours. Une mise à jour laisse des traces dès sa demande
//! (`update/state.json`), puis `job.json` au lancement du superviseur, puis la sauvegarde de
//! l'ancien binaire à l'échange. Au démarrage de l'agent, ces traces, sans superviseur vivant et
//! sans résultat à annoncer, **croisées avec la version qui tourne**, disent jusqu'où la mise à
//! jour est allée : elle est conclue de façon déterministe selon l'étape atteinte, jamais laissée
//! « en cours » ni ignorée. Un fichier de trace illisible n'est pas un fichier absent.

use super::record::{Job, Requester, SupervisorState};
use super::resume::MAX_RESUMES;
use crate::domain::install::Version;

/// Ce que le dossier `update/` contient au démarrage.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Leftovers {
    pub state: Option<SupervisorState>,
    pub job: Option<Job>,
    /// L'ancien binaire est gardé à côté du binaire installé : l'échange a eu lieu et n'a pas été
    /// conclu (ou a été défait à la main).
    pub backup_present: bool,
    /// `state.json` ou `job.json` existe mais ne se lit pas.
    pub unreadable: bool,
    /// Combien de reprises ont déjà été tentées pour cet échange : les reprises du superviseur plus les
    /// lancements ratés de l'agent, comptés dans le marqueur d'étape (`Marker::attempts`). Sans marqueur
    /// (agent d'avant HRT-27), un travail marqué `recover` compte pour toutes. À `MAX_RESUMES`, la reprise
    /// n'est plus relancée (BR-UPDATE-028, BR-UPDATE-033).
    pub recovery_attempts: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Orphan {
    /// Rien d'inachevé.
    None,
    /// L'agent est mort avant de lancer le superviseur (téléchargement, vérification, dépôt) :
    /// rien n'a changé sur le serveur. Abandon propre, résultat `failed` / `interrupted`.
    BeforeLaunch {
        version: String,
        previous: String,
        requester: Requester,
    },
    /// Le superviseur a été lancé mais n'a jamais échangé les binaires : l'agent d'avant tourne
    /// toujours. Même conclusion, avec le demandeur du travail.
    LaunchedNoSwap(Job),
    /// La nouvelle version tourne déjà, sans sauvegarde de l'ancien binaire : l'échange est fait et
    /// la réussite n'a pas été écrite (superviseur tué entre le nettoyage et son résultat).
    Completed {
        version: String,
        previous: String,
        requester: Requester,
    },
    /// La version d'avant tourne déjà alors que la sauvegarde est restée (retour arrière dont le
    /// résultat n'a pas été écrit, ou reprise à la main) : conclu `rolled_back`, traces retirées,
    /// **sans jamais toucher à la base**, qui a vécu depuis.
    AlreadyRolledBack(Job),
    /// Une TROISIÈME version tourne (ni la visée, ni celle d'avant) alors que la sauvegarde est
    /// restée : quelqu'un a remplacé le binaire à la main. Ni reprise (elle remettrait l'ancien
    /// binaire et la copie PÉRIMÉE de la base sur son travail), ni retour arrière : conclu
    /// `failed` / `interrupted`, traces retirées, **sans jamais toucher à la base**.
    // FIX:01M47N6Z485TWN2H770KQ5H80R : docs/bugs/FIX-01M47N6Z485TWN2H770KQ5H80R.md
    ForeignVersion(Job),
    /// Les reprises de cet échange sont épuisées (`Leftovers::recovery_attempts` à la borne) et n'ont
    /// rien conclu : elle n'est pas relancée (BR-UPDATE-028). Conclu `failed` / `rollback_failed`, copies gardées pour la
    /// reprise à la main, traces de travail retirées.
    RecoveryAlreadyTried(Job),
    /// La nouvelle version tourne et les binaires ont été échangés sans que personne conclue :
    /// reprise par un superviseur (contrôle du binaire en place, sinon retour exact).
    AfterSwap(Job),
    /// Une trace ne se lit pas : conclu sans deviner. Si l'ancien binaire est gardé, la reprise est
    /// à faire à la main (les copies sont conservées) ; sinon, tentative interrompue.
    Unreadable { backup_present: bool },
}

/// Classe ce qui reste. `supervising` : un superviseur tient le verrou (il conclura lui-même).
/// `current` : la version de l'agent qui démarre.
pub fn classify_orphan(leftovers: &Leftovers, supervising: bool, current: Version) -> Orphan {
    if supervising {
        return Orphan::None;
    }
    if leftovers.unreadable {
        return Orphan::Unreadable {
            backup_present: leftovers.backup_present,
        };
    }
    match (&leftovers.job, &leftovers.state) {
        (Some(job), _) => {
            let version = Version::parse(&job.version).ok();
            let previous = Version::parse(&job.previous).ok();
            if leftovers.backup_present {
                match (version, previous) {
                    // Le binaire en place est celui qui a été visé : le contrôle tranche, UNE fois.
                    // Des reprises épuisées (comptées sur le disque, dans le marqueur) qui n'ont rien
                    // conclu ne sont jamais relancées : sans cela, une reprise muette bouclerait.
                    (Some(target), Some(_)) if target == current => {
                        if leftovers.recovery_attempts >= MAX_RESUMES {
                            Orphan::RecoveryAlreadyTried(job.clone())
                        } else {
                            Orphan::AfterSwap(job.clone())
                        }
                    }
                    // L'ancienne version tourne déjà.
                    (Some(_), Some(before)) if before == current => {
                        Orphan::AlreadyRolledBack(job.clone())
                    }
                    // Ni l'une ni l'autre : le binaire a été remplacé à la main.
                    (Some(_), Some(_)) => Orphan::ForeignVersion(job.clone()),
                    // Une version qui ne se lit pas : on ne devine pas. Rien n'est restauré ni
                    // effacé, la reprise est à faire à la main.
                    _ => Orphan::Unreadable {
                        backup_present: true,
                    },
                }
            } else {
                match version {
                    Some(target) if target == current => Orphan::Completed {
                        version: job.version.clone(),
                        previous: job.previous.clone(),
                        requester: job.requester(),
                    },
                    Some(_) => Orphan::LaunchedNoSwap(job.clone()),
                    None => Orphan::Unreadable {
                        backup_present: false,
                    },
                }
            }
        }
        (None, Some(state)) => {
            if Version::parse(&state.version).ok() == Some(current) {
                Orphan::Completed {
                    version: state.version.clone(),
                    previous: state.previous.clone(),
                    requester: state.requester.clone(),
                }
            } else {
                Orphan::BeforeLaunch {
                    version: state.version.clone(),
                    previous: state.previous.clone(),
                    requester: state.requester.clone(),
                }
            }
        }
        (None, None) => Orphan::None,
    }
}

#[cfg(test)]
mod tests {
    use hearth_proto::api::update::UpdateStep;

    use super::*;

    const OLD: Version = Version::new(0, 1, 0);
    const NEW: Version = Version::new(0, 2, 0);

    fn state(step: UpdateStep) -> SupervisorState {
        SupervisorState {
            version: "0.2.0".into(),
            step,
            previous: "0.1.0".into(),
            requester: Requester {
                by: Some("marie".into()),
                name: None,
                addr: None,
            },
        }
    }

    fn job() -> Job {
        Job {
            version: "0.2.0".into(),
            previous: "0.1.0".into(),
            binary: "/usr/local/bin/hearth-agent".into(),
            staged: "/d/update/hearth-agent.new".into(),
            backup: "/usr/local/bin/.hearth-agent.previous".into(),
            probe_addr: "127.0.0.1:7341".into(),
            fingerprint: "ab".repeat(32),
            grace_ms: 0,
            check_window_ms: 60_000,
            poll_ms: 1000,
            requested_by: Some("marie".into()),
            client_name: None,
            client_addr: None,
            recover: false,
        }
    }

    #[test]
    fn nothing_left_is_nothing() {
        assert_eq!(
            classify_orphan(&Leftovers::default(), false, OLD),
            Orphan::None
        );
    }

    #[test]
    fn a_living_supervisor_concludes_by_itself_whatever_is_left() {
        let left = Leftovers {
            state: Some(state(UpdateStep::Check)),
            job: Some(job()),
            backup_present: true,
            unreadable: false,
            recovery_attempts: 0,
        };
        assert_eq!(classify_orphan(&left, true, NEW), Orphan::None);
    }

    #[test]
    fn an_agent_that_died_while_downloading_leaves_only_its_intent() {
        for step in [
            UpdateStep::Download,
            UpdateStep::Verify,
            UpdateStep::Install,
        ] {
            let left = Leftovers {
                state: Some(state(step)),
                ..Leftovers::default()
            };
            assert!(
                matches!(classify_orphan(&left, false, OLD), Orphan::BeforeLaunch { ref version, .. } if version == "0.2.0"),
                "{step:?}"
            );
        }
    }

    #[test]
    fn a_supervisor_that_never_swapped_is_an_abandoned_launch() {
        let left = Leftovers {
            state: Some(state(UpdateStep::Restart)),
            job: Some(job()),
            ..Leftovers::default()
        };
        assert_eq!(
            classify_orphan(&left, false, OLD),
            Orphan::LaunchedNoSwap(job())
        );
    }

    #[test]
    fn a_recovery_is_tried_again_under_its_bound_and_never_at_it_whatever_the_job_says() {
        let mut work = job();
        work.recover = true; // ne dit rien de plus : seuls les reprises comptées font foi
        for attempts in 0..MAX_RESUMES {
            let left = Leftovers {
                state: Some(state(UpdateStep::Check)),
                job: Some(work.clone()),
                backup_present: true,
                unreadable: false,
                recovery_attempts: attempts,
            };
            assert_eq!(
                classify_orphan(&left, false, NEW),
                Orphan::AfterSwap(work.clone())
            );
        }
        let left = Leftovers {
            state: Some(state(UpdateStep::Check)),
            job: Some(job()),
            backup_present: true,
            unreadable: false,
            recovery_attempts: MAX_RESUMES,
        };
        assert!(matches!(
            classify_orphan(&left, false, NEW),
            Orphan::RecoveryAlreadyTried(_)
        ));
    }

    #[test]
    fn a_swap_nobody_concluded_is_recovered_whatever_step_was_written() {
        for step in [UpdateStep::Restart, UpdateStep::Check] {
            let left = Leftovers {
                state: Some(state(step)),
                job: Some(job()),
                backup_present: true,
                unreadable: false,
                recovery_attempts: 0,
            };
            // La nouvelle version tourne : son contrôle tranche.
            assert_eq!(
                classify_orphan(&left, false, NEW),
                Orphan::AfterSwap(job()),
                "{step:?}"
            );
        }
        let left = Leftovers {
            job: Some(job()),
            backup_present: true,
            ..Leftovers::default()
        };
        assert_eq!(classify_orphan(&left, false, NEW), Orphan::AfterSwap(job()));
    }

    #[test]
    fn the_new_version_already_running_without_a_backup_is_a_success_nobody_wrote() {
        let left = Leftovers {
            job: Some(job()),
            ..Leftovers::default()
        };
        assert!(matches!(
            classify_orphan(&left, false, NEW),
            Orphan::Completed { ref version, .. } if version == "0.2.0"
        ));
        // Même sans `job.json` (supprimé), l'intention le dit.
        let left = Leftovers {
            state: Some(state(UpdateStep::Check)),
            ..Leftovers::default()
        };
        assert!(matches!(
            classify_orphan(&left, false, NEW),
            Orphan::Completed { .. }
        ));
    }

    #[test]
    fn the_old_version_already_running_with_the_backup_left_is_a_rollback_to_conclude_untouched() {
        // Reprise à la main après un `rollback_failed` : l'ancien agent tourne, les traces
        // restent. Jamais une reprise qui recopierait une base périmée.
        let left = Leftovers {
            state: Some(state(UpdateStep::Check)),
            job: Some(job()),
            backup_present: true,
            unreadable: false,
            recovery_attempts: 0,
        };
        assert_eq!(
            classify_orphan(&left, false, OLD),
            Orphan::AlreadyRolledBack(job())
        );
    }

    #[test]
    fn a_third_version_with_the_backup_left_is_never_recovered_nor_rolled_back() {
        // FIX:01M47N6Z485TWN2H770KQ5H80R : 0.1.0 vers 0.2.0 laissée en cours, puis 0.9.9
        // installée à la main : la reprise remettrait 0.1.0 et la copie périmée de la base.
        let left = Leftovers {
            state: Some(state(UpdateStep::Check)),
            job: Some(job()),
            backup_present: true,
            unreadable: false,
            recovery_attempts: 0,
        };
        assert_eq!(
            classify_orphan(&left, false, Version::new(0, 9, 9)),
            Orphan::ForeignVersion(job())
        );
    }

    #[test]
    fn an_unreadable_trace_is_never_read_as_nothing() {
        for backup_present in [false, true] {
            let left = Leftovers {
                unreadable: true,
                backup_present,
                ..Leftovers::default()
            };
            assert_eq!(
                classify_orphan(&left, false, OLD),
                Orphan::Unreadable { backup_present }
            );
        }
    }

    // ---- Table de vérité (T27) : une ligne = un test. Entrées : `job.json` (absent / lisible /
    // versions illisibles), `state.json`, sauvegarde du binaire, version en place, `recover`.
    // Aucune ligne ne boucle (`AfterSwap` seulement une fois, jamais si `recover`), ne restaure
    // une copie périmée (seul `AfterSwap` peut remettre la base, par le retour arrière de CE
    // travail) ni ne laisse un ancien binaire devant une base migrée.

    fn left(job: Option<Job>, state: bool, backup_present: bool) -> Leftovers {
        // Un travail `recover` sans marqueur (agent d'avant) compte pour toutes les reprises.
        let recovery_attempts = if job.as_ref().is_some_and(|job| job.recover) {
            MAX_RESUMES
        } else {
            0
        };
        Leftovers {
            recovery_attempts,
            state: state.then(|| state_of(UpdateStep::Check)),
            job,
            backup_present,
            unreadable: false,
        }
    }

    fn state_of(step: UpdateStep) -> SupervisorState {
        state(step)
    }

    fn job_with(version: &str, previous: &str, recover: bool) -> Job {
        let mut job = job();
        job.version = version.into();
        job.previous = previous.into();
        job.recover = recover;
        job
    }

    macro_rules! row {
        ($name:ident, $left:expr, $current:expr, $expected:pat) => {
            #[test]
            fn $name() {
                let got = classify_orphan(&$left, false, $current);
                assert!(matches!(got, $expected), "{got:?}");
            }
        };
    }

    const THIRD: Version = Version::new(0, 9, 9);

    row!(
        row_01_nothing_at_all,
        left(None, false, false),
        OLD,
        Orphan::None
    );
    row!(
        row_02_nothing_but_a_backup_left,
        left(None, false, true),
        OLD,
        Orphan::None
    );
    row!(
        row_03_intent_only_new_running,
        left(None, true, false),
        NEW,
        Orphan::Completed { .. }
    );
    row!(
        row_04_intent_only_other_running,
        left(None, true, false),
        OLD,
        Orphan::BeforeLaunch { .. }
    );
    row!(
        row_05_job_no_backup_new_running,
        left(Some(job()), true, false),
        NEW,
        Orphan::Completed { .. }
    );
    row!(
        row_06_job_no_backup_other_running,
        left(Some(job()), true, false),
        OLD,
        Orphan::LaunchedNoSwap(_)
    );
    row!(
        row_07_job_no_backup_third_running,
        left(Some(job()), true, false),
        THIRD,
        Orphan::LaunchedNoSwap(_)
    );
    row!(
        row_08_job_backup_new_running_first_time,
        left(Some(job()), true, true),
        NEW,
        Orphan::AfterSwap(_)
    );
    row!(
        row_09_job_backup_new_running_recovery_already_tried,
        left(Some(job_with("0.2.0", "0.1.0", true)), true, true),
        NEW,
        Orphan::RecoveryAlreadyTried(_)
    );
    row!(
        row_10_job_backup_old_running,
        left(Some(job()), true, true),
        OLD,
        Orphan::AlreadyRolledBack(_)
    );
    row!(
        row_11_job_backup_old_running_after_a_recovery,
        left(Some(job_with("0.2.0", "0.1.0", true)), true, true),
        OLD,
        Orphan::AlreadyRolledBack(_)
    );
    row!(
        row_12_job_backup_third_running,
        left(Some(job()), true, true),
        THIRD,
        Orphan::ForeignVersion(_)
    );
    row!(
        row_13_target_unreadable_backup_new_looking,
        left(Some(job_with("x", "0.1.0", false)), true, true),
        NEW,
        Orphan::Unreadable {
            backup_present: true
        }
    );
    row!(
        row_14_target_unreadable_backup_any_version,
        left(Some(job_with("x", "0.1.0", true)), true, true),
        THIRD,
        Orphan::Unreadable {
            backup_present: true
        }
    );
    row!(
        row_15_previous_unreadable_backup_other_than_target,
        left(Some(job_with("0.2.0", "?", false)), true, true),
        THIRD,
        Orphan::Unreadable {
            backup_present: true
        }
    );
    row!(
        row_16_previous_unreadable_backup_target_running,
        left(Some(job_with("0.2.0", "?", false)), true, true),
        NEW,
        Orphan::Unreadable {
            backup_present: true
        }
    );
    row!(
        row_17_target_unreadable_no_backup,
        left(Some(job_with("x", "0.1.0", false)), true, false),
        OLD,
        Orphan::Unreadable {
            backup_present: false
        }
    );
    row!(
        row_18_previous_unreadable_no_backup_other_running,
        left(Some(job_with("0.2.0", "?", false)), true, false),
        THIRD,
        Orphan::LaunchedNoSwap(_)
    );

    #[test]
    fn no_combination_loops_or_restores_a_stale_copy() {
        // Exhaustif : tout ce que `AfterSwap` (seule porte vers la remise de la base) exige.
        let versions = ["0.2.0", "0.1.0", "x"];
        for version in versions {
            for previous in versions {
                for recover in [false, true] {
                    for backup in [false, true] {
                        for current in [OLD, NEW, THIRD] {
                            let job = job_with(version, previous, recover);
                            let got =
                                classify_orphan(&left(Some(job), true, backup), false, current);
                            if matches!(got, Orphan::AfterSwap(_)) {
                                assert!(
                                    backup
                                        && !recover
                                        && version == current.to_string()
                                        && previous != "x",
                                    "AfterSwap à tort : {version} {previous} recover={recover} backup={backup} {current:?}"
                                );
                            }
                            if backup && (version == "x" || previous == "x") {
                                assert_eq!(
                                    got,
                                    Orphan::Unreadable {
                                        backup_present: true
                                    },
                                    "{version} {previous}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

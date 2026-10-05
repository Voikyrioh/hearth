//! BR-UPDATE-028 : un travail laissé en cours. Une mise à jour laisse des traces dès sa demande
//! (`update/state.json`), puis `job.json` au lancement du superviseur, puis la sauvegarde de
//! l'ancien binaire à l'échange. Au démarrage de l'agent, ces traces, sans superviseur vivant et
//! sans résultat à annoncer, **croisées avec la version qui tourne**, disent jusqu'où la mise à
//! jour est allée : elle est conclue de façon déterministe selon l'étape atteinte, jamais laissée
//! « en cours » ni ignorée. Un fichier de trace illisible n'est pas un fichier absent.

use super::record::{Job, Requester, SupervisorState};
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
    /// Les binaires ont été échangés, ni la nouvelle ni l'ancienne version ne tourne comme prévu :
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
                if previous == Some(current) && version != Some(current) {
                    Orphan::AlreadyRolledBack(job.clone())
                } else {
                    Orphan::AfterSwap(job.clone())
                }
            } else if version == Some(current) {
                Orphan::Completed {
                    version: job.version.clone(),
                    previous: job.previous.clone(),
                    requester: job.requester(),
                }
            } else {
                Orphan::LaunchedNoSwap(job.clone())
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
    fn a_swap_nobody_concluded_is_recovered_whatever_step_was_written() {
        for step in [UpdateStep::Restart, UpdateStep::Check] {
            let left = Leftovers {
                state: Some(state(step)),
                job: Some(job()),
                backup_present: true,
                unreadable: false,
            };
            // La nouvelle version tourne, ou une autre : le contrôle tranche.
            for current in [NEW, Version::new(0, 9, 9)] {
                assert_eq!(
                    classify_orphan(&left, false, current),
                    Orphan::AfterSwap(job()),
                    "{step:?}"
                );
            }
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
        };
        assert_eq!(
            classify_orphan(&left, false, OLD),
            Orphan::AlreadyRolledBack(job())
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
}

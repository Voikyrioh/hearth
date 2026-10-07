//! BR-UPDATE-030 à 034 : un superviseur de mise à jour **rejouable** (HRT-27). Le superviseur écrit,
//! après chaque étape durable, un marqueur (`update/phase.json`) ; relancé après une interruption
//! (processus tué, redémarrage de son unité par systemd), il lit ce marqueur et reprend à l'étape
//! où il s'est arrêté, sans refaire une étape déjà faite : jamais un binaire cassé, jamais une
//! base remise deux fois, jamais un retour arrière rejoué.
//!
//! Fonctions pures : le marqueur lu (ou son absence, ou son illisibilité) en entrée, la décision en
//! sortie. Les faits du disque (copie de la base, sauvegarde du binaire) arrivent en booléens.
//!
//! **Pas de boucle** : le marqueur compte les reprises (`MAX_RESUMES`) ; passé ce nombre, le
//! superviseur abandonne (copies gardées, journalisé une fois) et plus rien ne le relance. Le
//! compteur de systemd ne suffit pas (il se perd quand l'unité transitoire disparaît).

use hearth_proto::api::update::{UpdateOutcome, UpdateReason};
use serde::{Deserialize, Serialize};

use super::record::Job;

/// Combien de fois un superviseur interrompu est repris, au plus, pour une même mise à jour.
pub const MAX_RESUMES: u32 = 3;

/// Où en est le superviseur. L'ordre est celui de l'exécution : d'abord le chemin « avant », puis
/// le retour arrière, puis la fin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Le travail est pris, rien n'est fait.
    Started,
    /// Le service est arrêté.
    Stopped,
    /// La copie de la base est faite (ou il n'y avait pas de base).
    DatabaseSaved,
    /// L'échange des binaires est décidé ; a-t-il eu lieu ? Les faits du disque le disent
    /// (`swap_done`).
    Swapping,
    /// Les binaires sont échangés ; le nouveau reste à démarrer.
    Swapped,
    /// Le nouvel agent est démarré, son contrôle est en cours.
    Checking,
    /// Le retour arrière est décidé (`reason`).
    RollingBack,
    /// Retour arrière : le service est arrêté.
    RollbackStopped,
    /// Retour arrière : la base d'avant est remise (ou n'avait pas à l'être).
    RollbackDatabase,
    /// Retour arrière : l'ancien binaire est remis.
    RollbackBinary,
    /// Le résultat est décidé (`outcome`) ; reste à l'écrire et à nettoyer.
    Concluded,
    /// Reprise abandonnée, copies gardées : plus rien ne s'exécute, le résultat est écrit.
    Abandoned,
}

impl Phase {
    /// Le service a pu être arrêté par le superviseur lui-même (jamais « arrêté par un
    /// administrateur »). Un abandon à ce stade relance le service : binaire et base y forment
    /// toujours un couple cohérent (la base est remise AVANT le binaire, BR-UPDATE-029).
    pub fn service_may_be_stopped(self) -> bool {
        !matches!(
            self,
            Self::Started | Self::Checking | Self::Concluded | Self::Abandoned
        )
    }
}

/// Le marqueur d'étape. Du JSON, relu par un autre processus (le même binaire, copié) : les champs
/// s'ajoutent avec `#[serde(default)]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Marker {
    /// La version visée : un marqueur d'une autre version est périmé.
    pub version: String,
    pub phase: Phase,
    /// Pourquoi le retour arrière.
    #[serde(default)]
    pub reason: Option<UpdateReason>,
    /// Une copie de la base a été prise et doit exister jusqu'à la fin du travail.
    #[serde(default)]
    pub database_copy: bool,
    /// L'ancien binaire est gardé à côté. Tombe juste AVANT le renommage qui le remet en place :
    /// « tombé et absent » veut dire « remis » (`binary_restored`).
    #[serde(default)]
    pub backup_kept: bool,
    /// Combien de fois ce superviseur a été repris.
    #[serde(default)]
    pub resumes: u32,
    /// Combien de fois l'agent a tenté de lancer une reprise sans que le superviseur démarre (le
    /// lancement lui-même a échoué). Écrit AVANT chaque tentative, remis à zéro quand un superviseur
    /// démarre (BR-UPDATE-033).
    #[serde(default)]
    pub launches: u32,
    /// Le résultat décidé (phase `Concluded`).
    #[serde(default)]
    pub outcome: Option<UpdateOutcome>,
    /// L'abandon est réglé (résultat écrit, traces de travail retirées). Faux : un superviseur tué
    /// pendant l'abandon le règle à la reprise, sans rien journaliser de plus (BR-UPDATE-033).
    #[serde(default = "settled_by_default")]
    pub settled: bool,
    /// Le service a pu être arrêté par le superviseur : l'abandon le relance.
    #[serde(default)]
    pub revive: bool,
    /// RFC 3339 : l'instant du résultat, fixé une fois (une reprise réécrit le même résultat).
    #[serde(default)]
    pub at: Option<String>,
}

fn settled_by_default() -> bool {
    true
}

impl Marker {
    /// Le travail n'est ni conclu ni abandonné : un superviseur peut encore avoir à le reprendre.
    pub fn is_live(&self) -> bool {
        !matches!(self.phase, Phase::Concluded | Phase::Abandoned)
    }

    /// Un superviseur relancé reprendrait ce travail (vivant, sous la borne de reprises).
    pub fn can_resume(&self) -> bool {
        self.is_live() && self.attempts() < MAX_RESUMES
    }

    /// Les tentatives de reprise déjà faites : celles du superviseur et les lancements ratés de l'agent.
    pub fn attempts(&self) -> u32 {
        self.resumes + self.launches
    }

    /// Le marqueur d'un travail qui commence. Une reprise d'orphelin (`job.recover`) part du
    /// contrôle : les binaires sont déjà échangés (le superviseur relève alors les copies gardées
    /// sur le disque, pas ici).
    pub fn begin(job: &Job) -> Self {
        Self {
            version: job.version.clone(),
            phase: if job.recover {
                Phase::Checking
            } else {
                Phase::Started
            },
            reason: None,
            database_copy: false,
            backup_kept: false,
            resumes: 0,
            launches: 0,
            settled: true,
            revive: false,
            outcome: None,
            at: None,
        }
    }
}

/// Ce que le disque contient comme marqueur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stored {
    Absent,
    Found(Marker),
    /// Le fichier existe et ne se lit pas (écriture à moitié faite par autre chose que nous,
    /// disque abîmé, édition à la main).
    Unreadable,
}

/// Ce que le superviseur fait en démarrant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// Un travail neuf (ou un marqueur d'une autre version).
    Begin(Marker),
    /// Une reprise : le marqueur, avec une reprise de plus.
    Resume(Marker),
    /// Le résultat est décidé : il reste à l'écrire et à nettoyer.
    Finish(Marker),
    /// Trop de reprises : abandon, copies gardées.
    Exhausted(Marker),
    /// Abandon décidé et journalisé, mais pas réglé (superviseur tué entre les deux) : le régler, sans
    /// rien journaliser de plus.
    Settle(Marker),
    /// Déjà abandonné (et journalisé) : rien à faire, et plus rien ne relance.
    Nothing,
    /// Marqueur illisible : aucune étape ne se devine ; abandon sans rien toucher.
    Unreadable,
}

/// L'entrée du superviseur : le marqueur lu, le travail confié.
pub fn enter(stored: Stored, job: &Job) -> Entry {
    match stored {
        Stored::Absent => Entry::Begin(Marker::begin(job)),
        Stored::Unreadable => Entry::Unreadable,
        // Un marqueur d'une autre version n'est pas celui de ce travail (l'agent retire le marqueur
        // avant d'écrire un nouveau travail) : il ne se devine pas, rien n'est touché.
        Stored::Found(marker) if marker.version != job.version => Entry::Unreadable,
        Stored::Found(marker) => match marker.phase {
            Phase::Abandoned if !marker.settled => Entry::Settle(marker),
            Phase::Abandoned => Entry::Nothing,
            Phase::Concluded => Entry::Finish(marker),
            _ if marker.attempts() >= MAX_RESUMES => Entry::Exhausted(marker),
            phase => Entry::Resume(Marker {
                resumes: marker.resumes + 1,
                launches: 0,
                phase: resume_phase(phase),
                ..marker
            }),
        },
    }
}

/// Où reprend un superviseur interrompu dans `phase`. Le retour arrière reprend à son arrêt du
/// service : entre-temps la machine a pu redémarrer, et le NOUVEAU binaire (encore en place) avoir
/// démarré sur la base remise et l'avoir migrée de nouveau. La base d'avant se remet donc une
/// seconde fois, tant que l'ancien binaire n'est pas revenu (la copie est gardée jusqu'à la fin) ;
/// une fois l'ancien binaire remis, plus jamais (`binary_restored`).
fn resume_phase(phase: Phase) -> Phase {
    match phase {
        Phase::RollbackStopped | Phase::RollbackDatabase => Phase::RollingBack,
        other => other,
    }
}

/// L'ancien binaire a-t-il déjà été remis ? Le drapeau `backup_kept` tombe AVANT le renommage qui le
/// remet : « drapeau tombé et sauvegarde absente » veut dire que le renommage a eu lieu.
pub fn binary_restored(backup_kept_flag: bool, backup_present: bool) -> bool {
    !backup_kept_flag && !backup_present
}

/// L'échange des binaires a-t-il eu lieu ? Il ne se rejoue jamais s'il a eu lieu : le rejouer
/// remplacerait la sauvegarde de l'ancien binaire par le nouveau.
///
/// Le binaire en place est copié depuis le binaire déposé (`staged`), puis renommé en place : il
/// n'a le contenu du binaire déposé qu'une fois le renommage fait. Le critère est ce contenu, pas la
/// sauvegarde (une copie de sauvegarde interrompue peut exister sans que l'échange ait eu lieu).
pub fn swap_done(installed_is_staged: bool) -> bool {
    installed_is_staged
}

/// Que faire de la copie de la base au retour arrière. Elle est gardée jusqu'à la fin du travail :
/// « copie absente alors qu'il devait y en avoir une » veut toujours dire « perdue ».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseCopy {
    /// La remettre (service arrêté).
    Restore,
    /// Rien à remettre (pas de base à l'époque).
    Nothing,
    /// Elle devrait être là et n'y est plus : ne remettre ni la base ni le binaire (un ancien binaire
    /// devant une base peut-être migrée est la panne que BR-UPDATE-029 interdit).
    Lost,
}

pub fn database_copy(expected: bool, copy_present: bool) -> DatabaseCopy {
    match (copy_present, expected) {
        (true, _) => DatabaseCopy::Restore,
        (false, false) => DatabaseCopy::Nothing,
        (false, true) => DatabaseCopy::Lost,
    }
}

/// Que faire de la sauvegarde de l'ancien binaire au retour arrière (même logique).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryBackup {
    Restore,
    /// Déjà remis.
    Nothing,
    /// Perdue : l'ancien binaire n'existe plus nulle part.
    Lost,
}

pub fn binary_backup(flag_raised: bool, backup_present: bool) -> BinaryBackup {
    match (backup_present, flag_raised) {
        (true, _) => BinaryBackup::Restore,
        (false, false) => BinaryBackup::Nothing,
        (false, true) => BinaryBackup::Lost,
    }
}

/// Ce que le disque dit au retour arrière.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RollbackFacts {
    /// Une copie de la base a été prise (`Marker::database_copy`).
    pub database_expected: bool,
    pub database_present: bool,
    /// L'ancien binaire est encore gardé (`Marker::backup_kept`).
    pub backup_expected: bool,
    pub backup_present: bool,
}

/// Ce que fait une étape du retour arrière.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RollbackAct {
    StopService,
    /// Remettre la base d'avant (service arrêté).
    RestoreDatabase,
    /// Remettre l'ancien binaire.
    RestoreBinary,
    RestartService,
    /// Rien à faire à cette étape (pas de base à l'époque, ou déjà remise).
    Nothing,
    /// Abandon : une copie qui devait exister a disparu. Remettre l'une sans l'autre est la panne que
    /// BR-UPDATE-029 interdit.
    Lost(&'static str),
}

/// **L'ordre du retour arrière** (BR-UPDATE-029, invariant) : arrêt du service, **base d'avant, puis
/// ancien binaire**, redémarrage. Un ancien binaire ne se retrouve jamais devant une base déjà migrée ;
/// une fois l'ancien binaire revenu, la base ne se remet plus jamais (l'ancien agent a pu y écrire).
/// Rend l'acte de l'étape et la phase qui suit son succès ; `None` hors du retour arrière.
/// `Phase::Concluded` suit le redémarrage : le résultat est alors à contrôler puis à décider.
pub fn rollback_step(phase: Phase, facts: RollbackFacts) -> Option<(RollbackAct, Phase)> {
    match phase {
        Phase::RollingBack => Some((RollbackAct::StopService, Phase::RollbackStopped)),
        Phase::RollbackStopped => {
            if binary_restored(facts.backup_expected, facts.backup_present) {
                return Some((RollbackAct::Nothing, Phase::RollbackBinary));
            }
            let act = match database_copy(facts.database_expected, facts.database_present) {
                DatabaseCopy::Restore => RollbackAct::RestoreDatabase,
                DatabaseCopy::Nothing => RollbackAct::Nothing,
                DatabaseCopy::Lost => RollbackAct::Lost(
                    "la copie de la base a disparu : ni la base ni le binaire ne sont remis",
                ),
            };
            Some((act, Phase::RollbackDatabase))
        }
        Phase::RollbackDatabase => {
            let act = match binary_backup(facts.backup_expected, facts.backup_present) {
                BinaryBackup::Restore => RollbackAct::RestoreBinary,
                BinaryBackup::Nothing => RollbackAct::Nothing,
                BinaryBackup::Lost => {
                    RollbackAct::Lost("la sauvegarde de l'ancien binaire a disparu")
                }
            };
            Some((act, Phase::RollbackBinary))
        }
        Phase::RollbackBinary => Some((RollbackAct::RestartService, Phase::Concluded)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(recover: bool) -> Job {
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
            requested_by: None,
            client_name: None,
            client_addr: None,
            recover,
        }
    }

    fn marker(phase: Phase, resumes: u32) -> Marker {
        Marker {
            phase,
            resumes,
            ..Marker::begin(&job(false))
        }
    }

    #[test]
    fn no_marker_starts_a_fresh_work_at_the_beginning() {
        let Entry::Begin(marker) = enter(Stored::Absent, &job(false)) else {
            panic!("un travail neuf");
        };
        assert_eq!(marker.phase, Phase::Started);
        assert_eq!(marker.resumes, 0);
        assert!(!marker.database_copy && !marker.backup_kept);
    }

    #[test]
    fn an_orphan_recovery_starts_at_the_check() {
        let Entry::Begin(marker) = enter(Stored::Absent, &job(true)) else {
            panic!("un travail neuf");
        };
        assert_eq!(marker.phase, Phase::Checking);
    }

    #[test]
    fn a_marker_of_another_version_is_not_this_work_and_guesses_nothing() {
        let mut other = marker(Phase::Checking, 2);
        other.version = "0.1.5".into();
        assert_eq!(enter(Stored::Found(other), &job(false)), Entry::Unreadable);
    }

    #[test]
    fn an_interrupted_work_resumes_where_it_stopped_with_one_more_resume() {
        for phase in [
            Phase::Started,
            Phase::Stopped,
            Phase::DatabaseSaved,
            Phase::Swapping,
            Phase::Swapped,
            Phase::Checking,
            Phase::RollingBack,
            Phase::RollbackStopped,
            Phase::RollbackDatabase,
            Phase::RollbackBinary,
        ] {
            let entry = enter(Stored::Found(marker(phase, 1)), &job(false));
            assert!(
                matches!(&entry, Entry::Resume(m) if m.phase == resume_phase(phase) && m.resumes == 2),
                "{phase:?} : {entry:?}"
            );
        }
    }

    #[test]
    fn a_rollback_interrupted_after_its_stop_resumes_at_its_stop_to_put_the_database_back_again() {
        // La machine a pu redémarrer : le nouveau binaire (encore en place) a repris la base.
        assert_eq!(resume_phase(Phase::RollbackStopped), Phase::RollingBack);
        assert_eq!(resume_phase(Phase::RollbackDatabase), Phase::RollingBack);
        // Une fois l'ancien binaire remis, la base ne se remet plus jamais.
        assert_eq!(resume_phase(Phase::RollbackBinary), Phase::RollbackBinary);
        assert_eq!(resume_phase(Phase::Checking), Phase::Checking);
    }

    #[test]
    fn the_binary_is_restored_when_its_flag_fell_and_its_backup_is_gone() {
        assert!(binary_restored(false, false));
        assert!(
            !binary_restored(false, true),
            "le renommage n'a pas eu lieu"
        );
        assert!(
            !binary_restored(true, false),
            "la sauvegarde est perdue, pas remise"
        );
        assert!(!binary_restored(true, true));
    }

    #[test]
    fn the_resumes_are_bounded() {
        let entry = enter(
            Stored::Found(marker(Phase::Checking, MAX_RESUMES)),
            &job(false),
        );
        assert!(matches!(entry, Entry::Exhausted(_)), "{entry:?}");
        let entry = enter(
            Stored::Found(marker(Phase::Checking, MAX_RESUMES - 1)),
            &job(false),
        );
        assert!(matches!(entry, Entry::Resume(_)), "{entry:?}");
    }

    #[test]
    fn the_launches_the_agent_failed_count_in_the_bound_and_a_started_supervisor_clears_them() {
        let mut failing = marker(Phase::Checking, 1);
        failing.launches = MAX_RESUMES - 1;
        assert_eq!(failing.attempts(), MAX_RESUMES);
        assert!(!failing.can_resume());
        assert!(matches!(
            enter(Stored::Found(failing), &job(false)),
            Entry::Exhausted(_)
        ));
        let mut once = marker(Phase::Checking, 0);
        once.launches = 1;
        assert!(matches!(
            enter(Stored::Found(once), &job(false)),
            Entry::Resume(m) if m.launches == 0 && m.resumes == 1
        ));
    }

    #[test]
    fn a_decided_result_is_only_written_not_redone() {
        let entry = enter(Stored::Found(marker(Phase::Concluded, 9)), &job(false));
        assert!(matches!(entry, Entry::Finish(_)), "{entry:?}");
    }

    #[test]
    fn an_abandoned_work_does_nothing_and_says_nothing_more() {
        let entry = enter(Stored::Found(marker(Phase::Abandoned, 0)), &job(false));
        assert_eq!(entry, Entry::Nothing);
    }

    #[test]
    fn an_abandon_killed_before_its_settling_is_settled_at_the_next_start() {
        let mut killed = marker(Phase::Abandoned, 3);
        killed.settled = false;
        assert!(matches!(
            enter(Stored::Found(killed), &job(false)),
            Entry::Settle(_)
        ));
    }

    #[test]
    fn an_unreadable_marker_guesses_nothing() {
        assert_eq!(enter(Stored::Unreadable, &job(false)), Entry::Unreadable);
    }

    #[test]
    fn the_swap_is_done_when_the_installed_binary_is_the_staged_one() {
        assert!(swap_done(true));
        assert!(!swap_done(false));
    }

    #[test]
    fn a_missing_database_copy_is_lost_only_when_one_was_taken() {
        assert_eq!(database_copy(true, true), DatabaseCopy::Restore);
        assert_eq!(database_copy(false, true), DatabaseCopy::Restore);
        assert_eq!(database_copy(false, false), DatabaseCopy::Nothing);
        assert_eq!(database_copy(true, false), DatabaseCopy::Lost);
    }

    #[test]
    fn a_lost_binary_backup_is_told_apart_from_a_binary_already_put_back() {
        assert_eq!(binary_backup(true, true), BinaryBackup::Restore);
        assert_eq!(binary_backup(false, true), BinaryBackup::Restore);
        assert_eq!(binary_backup(false, false), BinaryBackup::Nothing);
        assert_eq!(binary_backup(true, false), BinaryBackup::Lost);
    }

    #[test]
    fn the_service_may_be_stopped_by_the_supervisor_between_its_stop_and_its_check() {
        for phase in [
            Phase::Stopped,
            Phase::DatabaseSaved,
            Phase::Swapping,
            Phase::Swapped,
            Phase::RollingBack,
            Phase::RollbackStopped,
            Phase::RollbackDatabase,
            Phase::RollbackBinary,
        ] {
            assert!(phase.service_may_be_stopped(), "{phase:?}");
        }
        for phase in [
            Phase::Started,
            Phase::Checking,
            Phase::Concluded,
            Phase::Abandoned,
        ] {
            assert!(!phase.service_may_be_stopped(), "{phase:?}");
        }
    }

    #[test]
    fn the_marker_survives_the_json_round_trip_and_tolerates_added_fields() {
        let mut m = marker(Phase::RollbackDatabase, 2);
        m.reason = Some(UpdateReason::NoAnswer);
        let text = serde_json::to_string(&m).unwrap_or_default();
        assert_eq!(serde_json::from_str::<Marker>(&text).ok(), Some(m));
        let older = r#"{"version":"0.2.0","phase":"checking"}"#;
        assert!(serde_json::from_str::<Marker>(older).is_ok());
    }

    /// Joue le retour arrière d'après les faits du disque, comme le superviseur : rend les actes dans
    /// l'ordre.
    fn walk(mut facts: RollbackFacts) -> Vec<RollbackAct> {
        let mut phase = Phase::RollingBack;
        let mut acts = Vec::new();
        while let Some((act, next)) = rollback_step(phase, facts) {
            if matches!(act, RollbackAct::Lost(_)) {
                acts.push(act);
                break;
            }
            match act {
                RollbackAct::RestoreBinary => {
                    facts.backup_expected = false;
                    facts.backup_present = false;
                }
                RollbackAct::Nothing => {}
                _ => {}
            }
            if act != RollbackAct::Nothing {
                acts.push(act);
            }
            if next == Phase::Concluded {
                break;
            }
            phase = next;
        }
        acts
    }

    const FULL: RollbackFacts = RollbackFacts {
        database_expected: true,
        database_present: true,
        backup_expected: true,
        backup_present: true,
    };

    #[test]
    fn the_database_comes_back_before_the_binary() {
        let acts = walk(FULL);
        assert_eq!(
            acts,
            [
                RollbackAct::StopService,
                RollbackAct::RestoreDatabase,
                RollbackAct::RestoreBinary,
                RollbackAct::RestartService
            ]
        );
    }

    #[test]
    fn without_a_copy_the_database_is_left_alone() {
        let acts = walk(RollbackFacts {
            database_expected: false,
            database_present: false,
            ..FULL
        });
        assert_eq!(
            acts,
            [
                RollbackAct::StopService,
                RollbackAct::RestoreBinary,
                RollbackAct::RestartService
            ]
        );
    }

    #[test]
    fn once_the_old_binary_is_back_the_database_is_never_put_back_again() {
        // Interrompu après la remise du binaire (drapeau tombé, sauvegarde absente), même avec la copie
        // de la base encore là : la remise ne se rejoue pas par-dessus ce que l'ancien agent a écrit.
        let acts = walk(RollbackFacts {
            backup_expected: false,
            backup_present: false,
            ..FULL
        });
        assert_eq!(
            acts,
            [RollbackAct::StopService, RollbackAct::RestartService]
        );
    }

    #[test]
    fn a_lost_copy_puts_back_nothing() {
        let lost_db = walk(RollbackFacts {
            database_present: false,
            ..FULL
        });
        assert!(matches!(lost_db.last(), Some(RollbackAct::Lost(_))));
        assert!(
            !lost_db.contains(&RollbackAct::RestoreBinary),
            "{lost_db:?}"
        );
        let lost_backup = walk(RollbackFacts {
            backup_present: false,
            ..FULL
        });
        assert!(matches!(lost_backup.last(), Some(RollbackAct::Lost(_))));
    }

    #[test]
    fn only_the_rollback_phases_have_a_rollback_step() {
        for phase in [
            Phase::Started,
            Phase::Stopped,
            Phase::DatabaseSaved,
            Phase::Swapping,
            Phase::Swapped,
            Phase::Checking,
            Phase::Concluded,
            Phase::Abandoned,
        ] {
            assert_eq!(rollback_step(phase, FULL), None, "{phase:?}");
        }
    }
}

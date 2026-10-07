//! Le superviseur de mise à jour (BR-UPDATE-015, BR-UPDATE-018, BR-UPDATE-030 à 034) : il tourne
//! **détaché** de l'agent qu'il remplace (c'est une copie de l'ancien binaire, lancée hors du
//! groupe de contrôle du service), arrête le service, échange les binaires, redémarre, contrôle
//! `GET /hello` pendant 60 secondes, et **restaure l'ancien binaire** si le nouvel agent ne répond
//! pas avec la nouvelle version et le même certificat. Son résultat est un fichier : il survit à tout.
//!
//! **Rejouable (HRT-27).** Après chaque étape durable il écrit un marqueur (`update/phase.json`,
//! `fsync` du fichier et du dossier). Tué (par un signal, par le gestionnaire de mémoire) puis
//! relancé (systemd relance son unité transitoire en cas d'échec), il reprend là où il s'est
//! arrêté et conclut, sans refaire une étape déjà faite : l'échange des binaires et la remise de la
//! base ne se rejouent jamais. Passé `MAX_RESUMES` reprises, il abandonne (copies gardées) et plus
//! rien ne le relance. Le marqueur est illisible : il ne devine rien et ne touche à rien.
//!
//! Tout est synchrone et bloquant : le superviseur est un processus à part, sans serveur. Ce que
//! l'agent ne doit jamais devenir : un serveur sans agent. Quoi qu'il arrive, ce code remet un
//! binaire qui tourne, ou écrit « retour en arrière impossible » pour que l'administrateur le sache.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use hearth_proto::api::update::{UpdateOutcome, UpdateReason, UpdateStep};
use hearth_proto::fingerprint::Fingerprint;
use thiserror::Error;

use super::ports::{
    BinaryInstalled, Clock, FreeSpace, HelloProbe, InstallHost, ServiceManager, ServiceState,
    UpdateHost, UpdateHostError,
};
use super::update::format_time;
use crate::domain::install::Version;
use crate::domain::update::{
    Answer, Entry, Job, Marker, Phase, RollbackAct, RollbackFacts, Stored, SupervisorState,
    UpdateRecord, Verdict, check_space, check_verdict, enter, rollback_step, swap_done,
};

#[derive(Debug, Error)]
pub enum SuperviseError {
    #[error("travail du superviseur illisible : {0}")]
    Job(String),
    #[error(transparent)]
    Host(#[from] UpdateHostError),
}

/// Comment le superviseur a fini.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    /// Le résultat est écrit (réussite, retour arrière, échec).
    Concluded,
    /// Le service a été arrêté à la main pendant le contrôle : rien n'est défait, rien n'est
    /// conclu ; l'agent qui redémarrera reprendra (BR-UPDATE-028, BR-UPDATE-031). `outcome` et
    /// `reason` ne disent rien dans ce cas.
    Paused,
    /// Reprise abandonnée, copies gardées : le résultat « échec, retour arrière impossible » est
    /// écrit et journalisé, une seule fois (BR-UPDATE-033).
    Abandoned,
    /// Déjà abandonné par une exécution précédente : rien à faire, rien à dire. `outcome` et
    /// `reason` ne disent rien dans ce cas.
    Nothing,
}

/// Comment s'est terminé le travail du superviseur (le résultat complet est dans le fichier).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Supervised {
    pub outcome: UpdateOutcome,
    pub reason: Option<UpdateReason>,
    pub end: End,
}

impl Supervised {
    fn without_result(end: End) -> Self {
        Self {
            outcome: UpdateOutcome::Failed,
            reason: Some(UpdateReason::Interrupted),
            end,
        }
    }
}

pub struct Supervisor<'a> {
    pub host: &'a dyn UpdateHost,
    pub install: &'a dyn InstallHost,
    /// L'espace libre (un port à part : les tests n'interrogent jamais le vrai disque).
    pub space: &'a dyn FreeSpace,
    pub service: &'a dyn ServiceManager,
    pub probe: &'a dyn HelloProbe,
    pub clock: &'a dyn Clock,
}

/// Le travail, lu et vérifié une fois.
struct Plan {
    expected: Version,
    addr: SocketAddr,
    fingerprint: Fingerprint,
    window: Duration,
    poll: Duration,
}

impl Plan {
    fn of(job: &Job) -> Result<Self, SuperviseError> {
        let expected =
            Version::parse(&job.version).map_err(|error| SuperviseError::Job(error.to_string()))?;
        let addr: SocketAddr = job
            .probe_addr
            .parse()
            .map_err(|_| SuperviseError::Job(format!("adresse illisible : {}", job.probe_addr)))?;
        let fingerprint = Fingerprint::from_hex(&job.fingerprint)
            .map_err(|error| SuperviseError::Job(error.to_string()))?;
        Ok(Self {
            expected,
            addr,
            fingerprint,
            window: Duration::from_millis(job.check_window_ms),
            poll: Duration::from_millis(job.poll_ms.max(1)),
        })
    }
}

/// Comment le contrôle s'est terminé.
enum Checked {
    Verdict(Verdict),
    /// Le service est arrêté et ce n'est pas le superviseur qui l'a arrêté.
    StoppedOnPurpose,
}

impl Supervisor<'_> {
    /// Fait le travail décrit par `job`, ou le reprend où il s'est arrêté. Prend le verrou du
    /// superviseur pour toute sa durée : un second superviseur est refusé (`AlreadyRunning`).
    pub fn run(&self, job: &Job) -> Result<Supervised, SuperviseError> {
        let _lock = self.host.take_supervisor_lock()?;
        let plan = Plan::of(job)?;
        let stored = match self.host.read_marker() {
            Ok(None) => Stored::Absent,
            Ok(Some(marker)) => Stored::Found(marker),
            Err(error) => {
                tracing::error!(%error, "marqueur d'étape illisible");
                Stored::Unreadable
            }
        };
        let marker = match enter(stored, job) {
            Entry::Begin(mut marker) => {
                if job.recover {
                    // Reprise d'un orphelin : les copies qui comptent sont celles du disque.
                    marker.database_copy = self.host.database_copy_present();
                    marker.backup_kept = self.host.path_exists(&job.backup);
                }
                self.save(&marker)?;
                marker
            }
            Entry::Resume(marker) => {
                tracing::warn!(
                    phase = ?marker.phase,
                    resumes = marker.resumes,
                    "reprise du superviseur de mise à jour là où il s'est arrêté"
                );
                self.save(&marker)?;
                marker
            }
            Entry::Finish(marker) => return Ok(self.finish(job, marker)),
            Entry::Exhausted(marker) => {
                return Ok(self.abandon(job, marker, "trop de reprises", true));
            }
            Entry::Settle(marker) => return Ok(self.settle(job, marker)),
            Entry::Nothing => {
                // Abandon réglé, travail pas encore retiré (mort entre les deux) : il part maintenant.
                self.host.discard_work_files();
                return Ok(Supervised::without_result(End::Nothing));
            }
            Entry::Unreadable => return Ok(self.abandon_unreadable(job)),
        };
        self.drive(job, &plan, marker)
    }

    /// Les étapes, dans l'ordre, chacune suivie de son marqueur. Une étape est rejouable : ce que
    /// son marqueur n'a pas encore enregistré se retrouve dans les faits du disque.
    fn drive(
        &self,
        job: &Job,
        plan: &Plan,
        mut marker: Marker,
    ) -> Result<Supervised, SuperviseError> {
        loop {
            match marker.phase {
                // --- Le chemin « avant » -------------------------------------------------------
                Phase::Started => {
                    self.state(job, UpdateStep::Restart);
                    if marker.resumes == 0 {
                        // L'ancien agent finit d'annoncer « redémarrage » à ses clients avant
                        // d'être arrêté.
                        std::thread::sleep(Duration::from_millis(job.grace_ms));
                    }
                    // La place pour la copie de la base se contrôle AVANT d'arrêter le service : un
                    // refus prévisible ne coûte ni coupure ni redémarrage.
                    if let Err(detail) = self.check_space() {
                        tracing::error!(%detail, "copie de la base refusée, échange non tenté");
                        return Ok(self.conclude(
                            job,
                            marker,
                            UpdateOutcome::Failed,
                            Some(UpdateReason::Swap),
                        ));
                    }
                    if let Err(error) = self.service.stop() {
                        return Ok(self.forward_failed(job, marker, &error.to_string()));
                    }
                    self.advance(&mut marker, Phase::Stopped)?;
                }
                Phase::Stopped => {
                    // Service arrêté : aucune écriture en cours dans la base.
                    if let Err(error) = self.host.backup_database() {
                        return Ok(self.forward_failed(job, marker, &error.to_string()));
                    }
                    marker.database_copy = self.host.database_copy_present();
                    self.advance(&mut marker, Phase::DatabaseSaved)?;
                }
                Phase::DatabaseSaved => self.advance(&mut marker, Phase::Swapping)?,
                Phase::Swapping => {
                    // Rejoué après une interruption : le contenu du binaire en place dit si le
                    // renommage a eu lieu (`swap_done`). Un échange fait ne se refait jamais.
                    let done = swap_done(self.host.same_content(&job.staged, &job.binary));
                    if !done
                        && let Err(error) =
                            self.install
                                .install_binary(&job.staged, &job.binary, &job.backup)
                    {
                        return Ok(self.forward_failed(job, marker, &error.to_string()));
                    }
                    marker.backup_kept = self.host.path_exists(&job.backup);
                    self.advance(&mut marker, Phase::Swapped)?;
                }
                Phase::Swapped => {
                    self.state(job, UpdateStep::Check);
                    if let Err(error) = self.service.restart() {
                        tracing::error!(%error, "le nouvel agent ne démarre pas");
                        marker.reason = Some(UpdateReason::NoAnswer);
                        self.advance(&mut marker, Phase::RollingBack)?;
                    } else {
                        self.advance(&mut marker, Phase::Checking)?;
                    }
                }
                // --- Le contrôle ---------------------------------------------------------------
                Phase::Checking => {
                    self.state(job, UpdateStep::Check);
                    match self.check(plan) {
                        Checked::Verdict(Verdict::Succeeded) => {
                            return Ok(self.conclude(job, marker, UpdateOutcome::Succeeded, None));
                        }
                        // La boucle ne sort jamais sur « pas encore » ; si elle le faisait un jour,
                        // ce serait un retour arrière, jamais une réussite.
                        Checked::Verdict(Verdict::Wait) => {
                            marker.reason = Some(UpdateReason::NoAnswer);
                            self.advance(&mut marker, Phase::RollingBack)?;
                        }
                        Checked::Verdict(Verdict::Rollback(reason)) => {
                            marker.reason = Some(reason);
                            self.advance(&mut marker, Phase::RollingBack)?;
                        }
                        Checked::StoppedOnPurpose => return Ok(self.pause(&marker)),
                    }
                }
                // --- Le retour arrière : l'ordre est celui de `rollback_step` (BR-UPDATE-029) ----
                Phase::RollingBack
                | Phase::RollbackStopped
                | Phase::RollbackDatabase
                | Phase::RollbackBinary => {
                    let facts = RollbackFacts {
                        database_expected: marker.database_copy,
                        database_present: self.host.database_copy_present(),
                        backup_expected: marker.backup_kept,
                        backup_present: self.host.path_exists(&job.backup),
                    };
                    let Some((act, next)) = rollback_step(marker.phase, facts) else {
                        return Ok(self.abandon(
                            job,
                            marker,
                            "étape de retour arrière inconnue",
                            true,
                        ));
                    };
                    match act {
                        RollbackAct::StopService => {
                            tracing::warn!(reason = ?marker.reason, "retour à la version précédente");
                            if let Err(error) = self.service.stop() {
                                return Ok(self.abandon(job, marker, &error.to_string(), true));
                            }
                        }
                        RollbackAct::RestoreDatabase => {
                            if let Err(error) = self.host.restore_database() {
                                return Ok(self.abandon(job, marker, &error.to_string(), true));
                            }
                        }
                        RollbackAct::RestoreBinary => {
                            // Le drapeau tombe AVANT le renommage qui remet le binaire.
                            if marker.backup_kept {
                                marker.backup_kept = false;
                                self.save(&marker)?;
                            }
                            let installed = BinaryInstalled {
                                backup: Some(job.backup.clone()),
                            };
                            if let Err(error) = self.install.restore_binary(&job.binary, &installed)
                            {
                                return Ok(self.abandon(job, marker, &error.to_string(), true));
                            }
                        }
                        RollbackAct::Nothing => {}
                        RollbackAct::Lost(why) => {
                            return Ok(self.abandon(job, marker, why, true));
                        }
                        RollbackAct::RestartService => {
                            if let Err(error) = self.service.restart() {
                                // Binaire et base d'avant sont en place : rien d'autre à tenter ici.
                                return Ok(self.abandon(job, marker, &error.to_string(), false));
                            }
                            let (outcome, reason) = if self.previous_answers(job, plan) {
                                (UpdateOutcome::RolledBack, marker.reason)
                            } else {
                                (UpdateOutcome::Failed, Some(UpdateReason::RollbackFailed))
                            };
                            return Ok(self.conclude(job, marker, outcome, reason));
                        }
                    }
                    self.advance(&mut marker, next)?;
                }
                // --- La fin --------------------------------------------------------------------
                Phase::Concluded => return Ok(self.finish(job, marker)),
                Phase::Abandoned => return Ok(Supervised::without_result(End::Nothing)),
            }
        }
    }

    /// Le contrôle : interroge `GET /hello` jusqu'au verdict. Un service **arrêté** (par un
    /// administrateur : le superviseur ne l'arrête pas pendant le contrôle, et `Restart=always` ne
    /// laisse pas un échec à l'état arrêté) n'est jamais défait (BR-UPDATE-031).
    fn check(&self, plan: &Plan) -> Checked {
        let started = Instant::now();
        loop {
            let answer = self.ask(plan.addr, &plan.fingerprint, plan.poll);
            let verdict = check_verdict(started.elapsed(), &answer, plan.expected, plan.window);
            match verdict {
                Verdict::Succeeded | Verdict::Rollback(UpdateReason::IdentityChanged) => {
                    return Checked::Verdict(verdict);
                }
                _ if self.service.state() == ServiceState::Inactive => {
                    return Checked::StoppedOnPurpose;
                }
                Verdict::Wait => std::thread::sleep(plan.poll),
                other => return Checked::Verdict(other),
            }
        }
    }

    /// L'ancien agent doit répondre de nouveau, avec la version d'avant.
    fn previous_answers(&self, job: &Job, plan: &Plan) -> bool {
        let previous = Version::parse(&job.previous).ok();
        let started = Instant::now();
        loop {
            let ok = matches!(
                self.probe.hello(plan.addr, plan.poll.max(Duration::from_secs(1))),
                Ok(greeting) if previous.is_none_or(|v| greeting.version == v)
            );
            if ok {
                return true;
            }
            if started.elapsed() >= plan.window {
                return false;
            }
            std::thread::sleep(plan.poll);
        }
    }

    /// Une réponse de `GET /hello` rapportée au verdict : la version et l'identité.
    fn ask(&self, addr: SocketAddr, fingerprint: &Fingerprint, timeout: Duration) -> Answer {
        match self.probe.hello(addr, timeout.max(Duration::from_secs(1))) {
            Ok(greeting) => Answer::Hello {
                version: greeting.version,
                same_identity: greeting.fingerprint == *fingerprint,
            },
            Err(_) => Answer::None,
        }
    }

    /// Assez de place pour la copie de la base (deux fois sa taille, une marge) : sinon l'échange
    /// n'a pas lieu et l'ancien agent repart. Une place impossible à mesurer est un refus.
    fn check_space(&self) -> Result<(), String> {
        let free = self
            .space
            .free_bytes(&self.host.data_dir())
            .map_err(|error| error.to_string());
        check_space(self.host.database_size(), free).map_err(|refusal| refusal.to_string())
    }

    fn state(&self, job: &Job, step: UpdateStep) {
        let state = SupervisorState {
            version: job.version.clone(),
            step,
            previous: job.previous.clone(),
            requester: job.requester(),
        };
        if let Err(error) = self.host.write_state(&state) {
            tracing::warn!(%error, "étape du superviseur non écrite");
        }
    }

    // --- Le marqueur -----------------------------------------------------------------------

    fn save(&self, marker: &Marker) -> Result<(), SuperviseError> {
        self.host.write_marker(marker).map_err(SuperviseError::Host)
    }

    /// Passe à l'étape suivante et l'écrit, durablement. Une écriture qui échoue arrête tout (code de
    /// sortie non nul : systemd relance, le marqueur d'avant dit où reprendre).
    fn advance(&self, marker: &mut Marker, next: Phase) -> Result<(), SuperviseError> {
        marker.phase = next;
        self.save(marker)
    }

    // --- Les fins --------------------------------------------------------------------------

    /// Un échec avant ou pendant l'échange : l'ancien binaire est toujours en place, le service est
    /// relancé, la tentative est conclue.
    fn forward_failed(&self, job: &Job, marker: Marker, detail: &str) -> Supervised {
        tracing::error!(%detail, "échange des binaires impossible");
        let revived = self.service.restart().is_ok();
        self.conclude(
            job,
            marker,
            UpdateOutcome::Failed,
            Some(if revived {
                UpdateReason::Swap
            } else {
                UpdateReason::RollbackFailed
            }),
        )
    }

    /// Décide le résultat (marqueur `Concluded`, durable), puis l'écrit et nettoie.
    fn conclude(
        &self,
        job: &Job,
        mut marker: Marker,
        outcome: UpdateOutcome,
        reason: Option<UpdateReason>,
    ) -> Supervised {
        marker.phase = Phase::Concluded;
        marker.outcome = Some(outcome);
        marker.reason = reason;
        marker.at = Some(format_time(self.clock.now()));
        if let Err(error) = self.host.write_marker(&marker) {
            tracing::warn!(%error, "marqueur de fin non écrit");
        }
        self.finish(job, marker)
    }

    /// Écrit le résultat décidé, retire l'ancien binaire si c'est une réussite, nettoie. Rejouable :
    /// le résultat est dans le marqueur, il se réécrit à l'identique.
    fn finish(&self, job: &Job, marker: Marker) -> Supervised {
        let outcome = marker.outcome.unwrap_or(UpdateOutcome::Failed);
        let record = self.record(job, outcome, marker.reason, marker.at.clone());
        // Rejoué après une interruption : un résultat déjà écrit (et peut-être déjà annoncé par l'agent,
        // `reported`) n'est pas réécrit.
        let written = matches!(self.host.read_last(), Ok(Some(last)) if last.at == record.at
            && last.outcome == record.outcome);
        // Un résultat qui ne s'écrit pas est une erreur à tracer, jamais à subir.
        if !written && let Err(error) = self.host.write_last(&record) {
            tracing::error!(%error, "résultat de mise à jour non écrit");
        }
        if outcome == UpdateOutcome::Succeeded {
            self.install.discard_backup(&BinaryInstalled {
                backup: Some(job.backup.clone()),
            });
        }
        self.host.clear_staging();
        Supervised {
            outcome,
            reason: marker.reason,
            end: End::Concluded,
        }
    }

    fn record(
        &self,
        job: &Job,
        outcome: UpdateOutcome,
        reason: Option<UpdateReason>,
        at: Option<String>,
    ) -> UpdateRecord {
        UpdateRecord {
            version: Some(job.version.clone()),
            previous: job.previous.clone(),
            outcome,
            reason,
            at: at.unwrap_or_else(|| format_time(self.clock.now())),
            requested_by: job.requested_by.clone(),
            client_name: job.client_name.clone(),
            client_addr: job.client_addr.clone(),
            reported: false,
        }
    }

    /// Le service a été arrêté à la main pendant le contrôle : rien n'est défait. Le marqueur reste
    /// à `Checking`, le travail, les copies et la sauvegarde aussi : au prochain démarrage de
    /// l'agent, `classify_orphan` reprend (BR-UPDATE-028).
    fn pause(&self, marker: &Marker) -> Supervised {
        tracing::warn!(
            phase = ?marker.phase,
            "service arrêté à la main pendant le contrôle de la mise à jour : rien n'est défait ; \
             au prochain démarrage de l'agent, la mise à jour est reprise"
        );
        // Un arrêt voulu n'est pas une panne du superviseur : il ne compte pas dans ses reprises (deux
        // arrêts voulus de suite ne doivent pas faire déclarer « retour arrière impossible »).
        let mut waiting = marker.clone();
        waiting.resumes = 0;
        if let Err(error) = self.host.write_marker(&waiting) {
            tracing::warn!(%error, "marqueur non remis à zéro");
        }
        Supervised::without_result(End::Paused)
    }

    /// Reprise abandonnée, copies gardées (runbook). Ordre, pour qu'une mort au milieu ne répète rien :
    /// 1. le marqueur `Abandoned` (non « réglé », avec le résultat décidé et son instant), puis
    /// 2. la trace au journal système, **une seule fois** (jamais refaite : la reprise d'un abandon ne
    ///    journalise plus), puis
    /// 3. le règlement, idempotent (`settle`) : service relancé s'il a pu être arrêté par le
    ///    superviseur, résultat « échec, retour arrière impossible » écrit (une seule fois : même
    ///    instant), traces de travail retirées (l'agent ne relance pas une reprise), marqueur « réglé ».
    ///    Copies de la base et de l'ancien binaire gardées.
    ///
    /// `revive` : relancer le service s'il a pu être arrêté par le superviseur. Binaire et base y
    /// forment toujours un couple cohérent : la base est remise avant le binaire, jamais après.
    fn abandon(&self, job: &Job, mut marker: Marker, detail: &str, revive: bool) -> Supervised {
        marker.revive = revive && marker.phase.service_may_be_stopped();
        let (phase, resumes) = (marker.phase, marker.resumes);
        self.decide_abandon(&mut marker, UpdateReason::RollbackFailed);
        if let Err(error) = self.host.write_marker(&marker) {
            tracing::error!(%error, "marqueur d'abandon non écrit");
        }
        tracing::error!(
            %detail,
            phase = ?phase,
            resumes,
            "reprise de la mise à jour abandonnée, copies gardées (voir le runbook mettre-a-jour-agent)"
        );
        self.settle(job, marker)
    }

    /// Marqueur illisible : aucune étape ne se devine, rien n'est touché (ni binaire, ni base, ni
    /// service). Si l'ancien binaire est gardé, la reprise est à faire à la main.
    fn abandon_unreadable(&self, job: &Job) -> Supervised {
        let mut marker = Marker::begin(job);
        let reason = if self.host.path_exists(&job.backup) {
            UpdateReason::RollbackFailed
        } else {
            UpdateReason::Interrupted
        };
        self.decide_abandon(&mut marker, reason);
        if let Err(error) = self.host.write_marker(&marker) {
            tracing::error!(%error, "marqueur d'abandon non écrit");
        }
        tracing::error!(
            "marqueur d'étape illisible : reprise de la mise à jour abandonnée, copies gardées \
             (voir le runbook mettre-a-jour-agent)"
        );
        self.settle(job, marker)
    }

    fn decide_abandon(&self, marker: &mut Marker, reason: UpdateReason) {
        marker.phase = Phase::Abandoned;
        marker.outcome = Some(UpdateOutcome::Failed);
        marker.reason = Some(reason);
        marker.settled = false;
        marker.at = Some(format_time(self.clock.now()));
    }

    /// Règle un abandon décidé, sans rien journaliser : rejouable après une mort (BR-UPDATE-033).
    fn settle(&self, job: &Job, mut marker: Marker) -> Supervised {
        if marker.revive && self.service.state() != ServiceState::Active {
            let _ = self.service.restart();
        }
        let record = self.record(job, UpdateOutcome::Failed, marker.reason, marker.at.clone());
        let written = matches!(self.host.read_last(), Ok(Some(last)) if last.at == record.at
            && last.outcome == record.outcome);
        if !written && let Err(error) = self.host.write_last(&record) {
            tracing::error!(%error, "résultat de mise à jour non écrit");
        }
        // Le travail part EN DERNIER : tant qu'il est là, une relance règle l'abandon (marqueur non réglé)
        // ou le constate (réglé) ; sans lui, plus rien ne relance.
        marker.settled = true;
        if let Err(error) = self.host.write_marker(&marker) {
            tracing::error!(%error, "marqueur d'abandon non écrit");
        }
        self.host.discard_work_files();
        Supervised {
            outcome: UpdateOutcome::Failed,
            reason: marker.reason,
            end: End::Abandoned,
        }
    }
}

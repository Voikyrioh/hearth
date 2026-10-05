//! Le superviseur de mise à jour (BR-UPDATE-015, BR-UPDATE-018) : il tourne **détaché** de l'agent
//! qu'il remplace (c'est une copie de l'ancien binaire, lancée hors du groupe de contrôle du
//! service), arrête le service, échange les binaires, redémarre, contrôle `GET /hello` pendant
//! 60 secondes, et **restaure l'ancien binaire** si le nouvel agent ne répond pas avec la
//! nouvelle version et le même certificat. Son résultat est un fichier : il survit à tout.
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
    BinaryInstalled, Clock, HelloProbe, InstallHost, ServiceManager, UpdateHost, UpdateHostError,
};
use super::update::format_time;
use crate::domain::install::Version;
use crate::domain::update::{
    Answer, Job, RollbackStep, SupervisorState, UpdateRecord, Verdict, check_verdict, rollback_plan,
};

#[derive(Debug, Error)]
pub enum SuperviseError {
    #[error("travail du superviseur illisible : {0}")]
    Job(String),
    #[error(transparent)]
    Host(#[from] UpdateHostError),
}

/// Comment s'est terminé le travail du superviseur (le résultat complet est dans le fichier).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Supervised {
    pub outcome: UpdateOutcome,
    pub reason: Option<UpdateReason>,
}

pub struct Supervisor<'a> {
    pub host: &'a dyn UpdateHost,
    pub install: &'a dyn InstallHost,
    pub service: &'a dyn ServiceManager,
    pub probe: &'a dyn HelloProbe,
    pub clock: &'a dyn Clock,
}

impl Supervisor<'_> {
    /// Fait le travail décrit par `job`. Prend le verrou du superviseur pour toute sa durée.
    pub fn run(&self, job: &Job) -> Result<Supervised, SuperviseError> {
        let _lock = self.host.take_supervisor_lock()?;
        let expected =
            Version::parse(&job.version).map_err(|error| SuperviseError::Job(error.to_string()))?;
        let addr: SocketAddr = job
            .probe_addr
            .parse()
            .map_err(|_| SuperviseError::Job(format!("adresse illisible : {}", job.probe_addr)))?;
        let fingerprint = Fingerprint::from_hex(&job.fingerprint)
            .map_err(|error| SuperviseError::Job(error.to_string()))?;
        let window = Duration::from_millis(job.check_window_ms);
        let poll = Duration::from_millis(job.poll_ms.max(1));

        if job.recover {
            return self.recover(job, expected, addr, &fingerprint, window, poll);
        }
        self.state(job, UpdateStep::Restart);
        // L'ancien agent finit d'annoncer « redémarrage » à ses clients avant d'être arrêté.
        std::thread::sleep(Duration::from_millis(job.grace_ms));

        // 1. Arrêt, copie de la base (service arrêté : aucune écriture en cours), puis échange :
        // l'ancien binaire est gardé de côté (`job.backup`).
        let swapped = self
            .service
            .stop()
            .map_err(|e| e.to_string())
            .and_then(|()| self.check_space())
            .and_then(|()| self.host.backup_database().map_err(|e| e.to_string()))
            .and_then(|()| {
                self.install
                    .install_binary(&job.staged, &job.binary, &job.backup)
                    .map_err(|e| e.to_string())
            });
        let installed = match swapped {
            Ok(installed) => installed,
            Err(detail) => {
                tracing::error!(%detail, "échange des binaires impossible");
                // Rien n'a changé : l'ancien binaire est toujours en place.
                let revived = self.service.restart().is_ok();
                self.host.clear_staging();
                return Ok(self.conclude(
                    job,
                    UpdateOutcome::Failed,
                    Some(if revived {
                        UpdateReason::Swap
                    } else {
                        UpdateReason::RollbackFailed
                    }),
                ));
            }
        };

        // 2. Le nouvel agent démarre ; contrôle pendant la fenêtre.
        self.state(job, UpdateStep::Check);
        let _ = self.service.restart();
        let verdict = self.check(addr, &fingerprint, expected, window, poll);
        self.conclude_check(job, &installed, verdict, addr, window)
    }

    /// Reprise d'un travail orphelin (BR-UPDATE-028) : les binaires sont déjà échangés, rien ne
    /// s'arrête ni ne s'échange. Le binaire en place (celui de l'agent qui a lancé cette reprise)
    /// est contrôlé ; s'il ne tient pas, l'ancien revient, exactement.
    fn recover(
        &self,
        job: &Job,
        expected: Version,
        addr: SocketAddr,
        fingerprint: &Fingerprint,
        window: Duration,
        poll: Duration,
    ) -> Result<Supervised, SuperviseError> {
        self.state(job, UpdateStep::Check);
        let installed = BinaryInstalled {
            backup: self
                .host
                .path_exists(&job.backup)
                .then(|| job.backup.clone()),
        };
        let verdict = self.check(addr, fingerprint, expected, window, poll);
        self.conclude_check(job, &installed, verdict, addr, window)
    }

    /// Le contrôle : interroge `GET /hello` jusqu'au verdict.
    fn check(
        &self,
        addr: SocketAddr,
        fingerprint: &Fingerprint,
        expected: Version,
        window: Duration,
        poll: Duration,
    ) -> Verdict {
        let started = Instant::now();
        loop {
            let answer = self.ask(addr, fingerprint, poll);
            let verdict = check_verdict(started.elapsed(), &answer, expected, window);
            if verdict != Verdict::Wait {
                return verdict;
            }
            std::thread::sleep(poll);
        }
    }

    fn conclude_check(
        &self,
        job: &Job,
        installed: &BinaryInstalled,
        verdict: Verdict,
        addr: SocketAddr,
        window: Duration,
    ) -> Result<Supervised, SuperviseError> {
        match verdict {
            Verdict::Succeeded => {
                self.install.discard_backup(installed);
                self.host.clear_staging();
                Ok(self.conclude(job, UpdateOutcome::Succeeded, None))
            }
            // La boucle ne sort jamais sur « pas encore » ; si elle le faisait un jour, ce serait un
            // retour arrière, jamais une réussite.
            Verdict::Wait => {
                Ok(self.roll_back(job, installed, UpdateReason::NoAnswer, addr, window))
            }
            Verdict::Rollback(reason) => Ok(self.roll_back(job, installed, reason, addr, window)),
        }
    }

    /// Remet l'ancien binaire et relance le service. Le résultat dit si l'ancien agent répond de
    /// nouveau ; sinon `RollbackFailed` (l'ancien binaire reste à `job.backup`).
    fn roll_back(
        &self,
        job: &Job,
        installed: &BinaryInstalled,
        reason: UpdateReason,
        addr: SocketAddr,
        window: Duration,
    ) -> Supervised {
        tracing::warn!(?reason, "retour à la version précédente");
        // Le plan (domain) : base d'abord, puis binaire. La base n'est remise qu'ici, une seule fois.
        let restored = rollback_plan(self.host.database_copy_present())
            .into_iter()
            .try_for_each(|step| match step {
                RollbackStep::Stop => self.service.stop().map_err(|e| e.to_string()),
                RollbackStep::RestoreDatabase => {
                    self.host.restore_database().map_err(|e| e.to_string())
                }
                RollbackStep::RestoreBinary => self
                    .install
                    .restore_binary(&job.binary, installed)
                    .map_err(|e| e.to_string()),
                RollbackStep::Restart => self.service.restart().map_err(|e| e.to_string()),
            });
        if let Err(detail) = restored {
            tracing::error!(%detail, "retour en arrière impossible");
            return self.conclude(
                job,
                UpdateOutcome::Failed,
                Some(UpdateReason::RollbackFailed),
            );
        }
        self.host.clear_staging();
        // L'ancien agent doit répondre de nouveau, avec la version d'avant.
        let previous = Version::parse(&job.previous).ok();
        let poll = Duration::from_millis(job.poll_ms.max(1));
        let started = Instant::now();
        let answering = loop {
            let ok = matches!(
                self.probe.hello(addr, poll.max(Duration::from_secs(1))),
                Ok(greeting) if previous.is_none_or(|v| greeting.version == v)
            );
            if ok {
                break true;
            }
            if started.elapsed() >= window {
                break false;
            }
            std::thread::sleep(poll);
        };
        if answering {
            self.conclude(job, UpdateOutcome::RolledBack, Some(reason))
        } else {
            self.conclude(
                job,
                UpdateOutcome::Failed,
                Some(UpdateReason::RollbackFailed),
            )
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
    /// n'a pas lieu et l'ancien agent repart.
    fn check_space(&self) -> Result<(), String> {
        let needed = self.host.database_size().saturating_mul(2) + 1024 * 1024;
        match self.install.free_bytes(&self.host.data_dir()) {
            Ok(free) if free < needed => Err(format!(
                "espace disque insuffisant pour copier la base ({free} octets libres, {needed} nécessaires)"
            )),
            _ => Ok(()),
        }
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

    /// Écrit le résultat. Un résultat qui ne s'écrit pas est une erreur à tracer, jamais à subir.
    fn conclude(
        &self,
        job: &Job,
        outcome: UpdateOutcome,
        reason: Option<UpdateReason>,
    ) -> Supervised {
        let record = UpdateRecord {
            version: job.version.clone(),
            previous: job.previous.clone(),
            outcome,
            reason,
            at: format_time(self.clock.now()),
            requested_by: job.requested_by.clone(),
            client_name: job.client_name.clone(),
            client_addr: job.client_addr.clone(),
            reported: false,
        };
        if let Err(error) = self.host.write_last(&record) {
            tracing::error!(%error, "résultat de mise à jour non écrit");
        }
        Supervised { outcome, reason }
    }
}

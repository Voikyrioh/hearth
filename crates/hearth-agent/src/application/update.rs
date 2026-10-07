//! Cas d'usage « mettre l'agent à jour à distance » (BR-UPDATE-011 à 019, 024).
//!
//! `start` valide la demande (règles de `domain::update`) et rend tout de suite : la mise à jour
//! s'exécute côté serveur, dans une tâche détachée de la requête (une coupure réseau du client ne
//! l'arrête pas, BR-UPDATE-017). Elle télécharge en mémoire, vérifie la somme puis la signature
//! **avant d'écrire quoi que ce soit**, dépose le binaire, lance le superviseur détaché et laisse
//! le superviseur faire l'échange et le contrôle (`update_supervisor`). Chaque étape est diffusée
//! (`UpdateFeed`). Le résultat est un fichier (`UpdateHost::write_last`) : il survit au
//! redémarrage ; le nouvel agent, ou l'ancien revenu, l'écrit au journal d'activité et l'annonce
//! (`resume`).

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use hearth_proto::api::update::{
    AgentUpdateStatus, UpdateOutcome, UpdateProgress, UpdateReason, UpdateResult, UpdateStep,
};
use hearth_proto::fingerprint::Fingerprint;
use sha2::{Digest, Sha256};
use thiserror::Error;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tokio::sync::broadcast;

use super::ports::{
    AuditSink, Clock, Downloader, FetchError, SignatureVerifier, UpdateFeed, UpdateHost,
};
use crate::domain::accounts::Username;
use crate::domain::audit::{Actor, AuditAction, Origin, Outcome, Reason, Target};
use crate::domain::install::Version;
use crate::domain::text::hex;
use crate::domain::update::{
    CHECK_WINDOW, Job, Leftovers, MAX_BINARY_BYTES, MAX_RESUMES, Marker, Orphan, PercentTracker,
    Phase, Requester, STOP_GRACE, SupervisorState, UpdateInput, UpdateRecord, UpdateRefusal,
    UpdateTarget, classify_orphan, plan_update,
};

/// Combien de temps le service attend, par défaut, un superviseur qui devait travailler avant d'y
/// renoncer (il n'a pas démarré, ou il est mort sans écrire de résultat).
const SUPERVISOR_PATIENCE: Duration = Duration::from_secs(30);

/// Ce que l'agent sait de lui-même pour se mettre à jour.
#[derive(Debug, Clone)]
pub struct UpdateEnv {
    /// Le binaire installé (celui qui s'exécute).
    pub binary: PathBuf,
    /// Où l'ancien binaire est gardé pendant l'échange.
    pub backup: PathBuf,
    /// Adresse du contrôle `GET /hello` par le superviseur.
    pub probe_addr: SocketAddr,
    /// L'empreinte complète du certificat : elle ne doit pas changer.
    pub fingerprint: Fingerprint,
    /// Délais du superviseur ; ceux du protocole par défaut, raccourcis par les tests.
    pub timing: Timing,
    /// Les adresses locales et privées sont permises pour le téléchargement : les tests de bout en
    /// bout seulement (BR-UPDATE-027).
    pub allow_local_addresses: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct Timing {
    pub grace: Duration,
    pub check_window: Duration,
    pub poll: Duration,
    /// Pas de la surveillance du superviseur par l'agent.
    pub watch: Duration,
    /// Combien de temps la surveillance attend un superviseur absent avant de conclure d'après les
    /// traces laissées sur le disque.
    pub patience: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            grace: STOP_GRACE,
            check_window: CHECK_WINDOW,
            poll: crate::domain::update::CHECK_POLL,
            watch: Duration::from_secs(1),
            patience: SUPERVISOR_PATIENCE,
        }
    }
}

#[derive(Debug, Error)]
pub enum UpdateError {
    #[error(transparent)]
    Refused(#[from] UpdateRefusal),
    /// La signature n'est pas du minisign de la clé de l'agent (rien n'est téléchargé).
    #[error("La signature de la mise à jour est refusée. Rien n'a été téléchargé ni modifié.")]
    BadSignature,
}

#[derive(Default)]
struct Running {
    /// La mise à jour en cours dans ce processus, ou reprise après un redémarrage.
    progress: Option<UpdateProgress>,
    /// Qui a demandé la mise à jour en cours, pour la trace d'intention (`update/state.json`).
    requester: Requester,
}

/// Exécute un travail bloquant (fichiers, somme de 128 Mio, `systemd-run`, `--version`) hors des
/// threads de l'exécuteur : sur une machine à un cœur, l'agent continue de livrer ses étapes et ses
/// mesures. Un travail qui plante est un échec de dépôt.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, UpdateReason> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|_| UpdateReason::Staging)
}

pub struct UpdateService {
    current: Version,
    /// La mise à jour à distance est possible ici (ni installation gérée, ni sans systemd).
    allowed: bool,
    env: UpdateEnv,
    downloader: Arc<dyn Downloader>,
    verifier: Arc<dyn SignatureVerifier>,
    host: Arc<dyn UpdateHost>,
    feed: Arc<dyn UpdateFeed>,
    sink: Arc<dyn AuditSink>,
    clock: Arc<dyn Clock>,
    running: Mutex<Running>,
}

/// Les adaptateurs d'une mise à jour, d'un bloc.
pub struct UpdateAdapters {
    pub downloader: Arc<dyn Downloader>,
    pub verifier: Arc<dyn SignatureVerifier>,
    pub host: Arc<dyn UpdateHost>,
    pub feed: Arc<dyn UpdateFeed>,
}

impl UpdateService {
    pub fn new(
        current: Version,
        allowed: bool,
        env: UpdateEnv,
        adapters: UpdateAdapters,
        sink: Arc<dyn AuditSink>,
        clock: Arc<dyn Clock>,
    ) -> Arc<Self> {
        Arc::new(Self {
            current,
            allowed,
            env,
            downloader: adapters.downloader,
            verifier: adapters.verifier,
            host: adapters.host,
            feed: adapters.feed,
            sink,
            clock,
            running: Mutex::new(Running::default()),
        })
    }

    fn running(&self) -> std::sync::MutexGuard<'_, Running> {
        self.running.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// L'état de la mise à jour : la version, le mode, ce qui est en cours, le dernier résultat.
    pub fn status(&self) -> AgentUpdateStatus {
        let progress = self.progress();
        AgentUpdateStatus {
            current: self.current.to_string(),
            managed: !self.allowed,
            in_progress: progress.is_some(),
            progress,
            last: self.last(),
        }
    }

    /// Le dernier résultat (fichier : il survit au redémarrage).
    pub fn last(&self) -> Option<UpdateResult> {
        match self.host.read_last() {
            Ok(record) => record.map(|record| record.to_result()),
            Err(error) => {
                tracing::warn!(%error, "dernier résultat de mise à jour illisible");
                None
            }
        }
    }

    /// Ce qui est en cours : dans ce processus, sinon chez le superviseur (l'agent qui vient de
    /// redémarrer en plein contrôle ne l'a pas lancé, mais le voit).
    pub fn progress(&self) -> Option<UpdateProgress> {
        if let Some(progress) = self.running().progress.clone() {
            return Some(progress);
        }
        if self.supervised() {
            let state = self.host.read_state().ok().flatten();
            return Some(match state {
                Some(state) => UpdateProgress {
                    version: state.version,
                    step: state.step,
                    percent: None,
                    outcome: None,
                    reason: None,
                },
                None => UpdateProgress {
                    version: String::new(),
                    step: UpdateStep::Restart,
                    percent: None,
                    outcome: None,
                    reason: None,
                },
            });
        }
        None
    }

    /// S'abonne à la progression, puis rend l'état courant (dans cet ordre : aucun changement
    /// n'échappe entre les deux).
    pub fn subscribe(&self) -> (broadcast::Receiver<UpdateProgress>, Option<UpdateProgress>) {
        let receiver = self.feed.subscribe();
        (receiver, self.progress())
    }

    fn publish(&self, progress: UpdateProgress) {
        self.running().progress = Some(progress.clone());
        self.feed.publish(progress);
    }

    /// Écrit la trace d'intention (`update/state.json`) : si l'agent meurt, le démarrage suivant
    /// sait qu'une mise à jour était en cours et jusqu'où elle était allée (BR-UPDATE-028).
    fn write_intent(&self, version: &Version, step: UpdateStep) {
        let requester = self.running().requester.clone();
        let state = SupervisorState {
            version: version.to_string(),
            step,
            previous: self.current.to_string(),
            requester,
        };
        if let Err(error) = self.host.write_state(&state) {
            tracing::warn!(%error, "trace d'intention de la mise à jour non écrite");
        }
    }

    /// Une étape de CETTE mise à jour : trace d'intention (une seule source d'écriture de
    /// `state.json` côté agent), puis diffusion.
    fn step(&self, version: &Version, step: UpdateStep, percent: Option<u8>) {
        if percent.is_none_or(|percent| percent == 0) {
            self.write_intent(version, step);
        }
        self.announce(version, step, percent);
    }

    /// Diffuse sans écrire : l'agent revenu (ou qui surveille) relaie ce que le superviseur a écrit
    /// et ne réécrit jamais son `state.json`.
    fn announce(&self, version: &Version, step: UpdateStep, percent: Option<u8>) {
        self.publish(UpdateProgress {
            version: version.to_string(),
            step,
            percent,
            outcome: None,
            reason: None,
        });
    }

    /// Valide la demande et la lance côté serveur. Rend la première étape (`202`).
    ///
    /// Les refus sont consignés au journal d'activité (BR-UPDATE-024) : celui de « déjà en cours »
    /// l'est ici ; installation gérée et signature le sont par la couche d'accès, d'après le code
    /// d'erreur.
    pub async fn start(
        self: &Arc<Self>,
        actor: Actor,
        input: UpdateInput<'_>,
    ) -> Result<UpdateProgress, UpdateError> {
        let begun = {
            let mut running = self.running();
            let in_progress = running.progress.is_some() || self.supervised();
            match plan_update(
                self.current,
                self.allowed,
                in_progress,
                self.env.allow_local_addresses,
                input,
            ) {
                Err(refusal) => Err(UpdateError::Refused(refusal)),
                Ok(target) => match self.verifier.check_format(&target.signature) {
                    Err(_) => Err(UpdateError::BadSignature),
                    Ok(()) => {
                        let first = UpdateProgress {
                            version: target.version.to_string(),
                            step: UpdateStep::Download,
                            percent: None,
                            outcome: None,
                            reason: None,
                        };
                        running.progress = Some(first.clone());
                        running.requester = Requester {
                            by: actor.account.as_ref().map(ToString::to_string),
                            name: actor.origin.name().map(str::to_owned),
                            addr: actor.origin.addr().map(str::to_owned),
                        };
                        Ok((target, first))
                    }
                },
            }
        };
        let (target, first) = match begun {
            Ok(begun) => begun,
            Err(UpdateError::Refused(UpdateRefusal::InProgress)) => {
                let version = Version::parse(input.version).ok();
                self.sink
                    .record(
                        actor,
                        AuditAction::AgentUpdate,
                        version.map_or(Target::None, |v| Target::AgentVersion(v.to_string())),
                        Outcome::Failed(Reason::UpdateInProgress),
                    )
                    .await;
                return Err(UpdateError::Refused(UpdateRefusal::InProgress));
            }
            Err(other) => return Err(other),
        };
        // La trace d'intention est écrite avant tout le reste (BR-UPDATE-028).
        self.write_intent(&target.version, UpdateStep::Download);
        self.feed.publish(first.clone());
        let service = Arc::clone(self);
        tokio::spawn(async move { service.run(target, actor).await });
        Ok(first)
    }

    /// Toute la mise à jour, jusqu'au lancement du superviseur.
    async fn run(self: Arc<Self>, target: UpdateTarget, actor: Actor) {
        match self.execute(&target, &actor).await {
            Ok(()) => self.watch(target.version, requester_of(&actor)),
            Err(reason) => self.fail(&target, &actor, reason).await,
        }
    }

    async fn execute(&self, target: &UpdateTarget, actor: &Actor) -> Result<(), UpdateReason> {
        let version = target.version;
        // 1. Téléchargement, en mémoire.
        let tracker = Mutex::new(PercentTracker::new());
        let bytes = self
            .downloader
            .fetch(&target.url, MAX_BINARY_BYTES, &|received, total| {
                let percent = tracker
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .update(received, total);
                if let Some(percent) = percent {
                    self.step(&version, UpdateStep::Download, Some(percent));
                }
            })
            .await
            .map_err(|error| {
                // Le détail (l'adresse n'y est pas, jamais d'identifiant) va au journal de l'agent :
                // c'est ce que lit l'administrateur pour comprendre « injoignable ».
                tracing::warn!(%error, "téléchargement de la mise à jour impossible");
                match error {
                    FetchError::Unreachable(_) => UpdateReason::Unreachable,
                    FetchError::Failed(_) | FetchError::TooLarge => UpdateReason::DownloadFailed,
                }
            })?;

        // 2. Vérification : la somme, puis la signature. Rien n'est écrit avant. Le calcul porte sur
        // jusqu'à 128 Mio : hors de l'exécuteur.
        self.step(&version, UpdateStep::Verify, None);
        let bytes = Arc::new(bytes);
        let sum = {
            let bytes = Arc::clone(&bytes);
            blocking(move || hex(&Sha256::digest(&bytes[..]))).await?
        };
        if sum != target.sha256 {
            return Err(UpdateReason::BadChecksum);
        }
        {
            let (bytes, verifier, signature) = (
                Arc::clone(&bytes),
                Arc::clone(&self.verifier),
                target.signature.clone(),
            );
            blocking(move || verifier.verify(&bytes[..], &signature))
                .await?
                .map_err(|_| UpdateReason::BadSignature)?;
        }

        // 3. Dépôt, contrôle de la version annoncée, copie du superviseur, travail.
        self.step(&version, UpdateStep::Install, None);
        let staged = {
            let (bytes, host) = (Arc::clone(&bytes), Arc::clone(&self.host));
            blocking(move || host.stage(&bytes[..]))
                .await?
                .map_err(|_| UpdateReason::Staging)?
        };
        drop(bytes);
        let announced = {
            let (host, staged) = (Arc::clone(&self.host), staged.clone());
            blocking(move || host.staged_version(&staged))
                .await?
                .map_err(|_| UpdateReason::BadBinary)?
        };
        if announced != version {
            return Err(UpdateReason::BadBinary);
        }
        let supervisor = {
            let host = Arc::clone(&self.host);
            blocking(move || host.prepare_supervisor())
                .await?
                .map_err(|_| UpdateReason::Staging)?
        };
        let job = Job {
            version: version.to_string(),
            previous: self.current.to_string(),
            binary: self.env.binary.clone(),
            staged,
            backup: self.env.backup.clone(),
            probe_addr: self.env.probe_addr.to_string(),
            fingerprint: self.env.fingerprint.to_hex(),
            grace_ms: millis(self.env.timing.grace),
            check_window_ms: millis(self.env.timing.check_window),
            poll_ms: millis(self.env.timing.poll),
            requested_by: actor.account.as_ref().map(ToString::to_string),
            client_name: actor.origin.name().map(str::to_owned),
            client_addr: actor.origin.addr().map(str::to_owned),
            recover: false,
        };
        let job_path = {
            let host = Arc::clone(&self.host);
            blocking(move || {
                // Une nouvelle mise à jour repart de zéro : le marqueur d'une précédente (reprise
                // abandonnée, copies gardées) ne s'applique pas à elle (BR-UPDATE-033).
                host.discard_marker();
                host.write_job(&job)
            })
            .await?
            .map_err(|_| UpdateReason::Staging)?
        };

        // 4. Le superviseur, détaché : il arrête ce service, échange, redémarre, contrôle.
        self.step(&version, UpdateStep::Restart, None);
        let host = Arc::clone(&self.host);
        blocking(move || host.launch(&supervisor, &job_path))
            .await?
            .map_err(|error| {
                tracing::error!(%error, "lancement du superviseur de mise à jour impossible");
                UpdateReason::SupervisorLaunch
            })?;
        Ok(())
    }

    /// Un échec avant l'échange des binaires : l'agent n'a pas changé. Le résultat est écrit, la
    /// fin est diffusée, le journal en garde une entrée.
    async fn fail(&self, target: &UpdateTarget, actor: &Actor, reason: UpdateReason) {
        tracing::warn!(?reason, version = %target.version, "mise à jour de l'agent échouée");
        self.host.clear_staging();
        let record = UpdateRecord {
            version: Some(target.version.to_string()),
            previous: self.current.to_string(),
            outcome: UpdateOutcome::Failed,
            reason: Some(reason),
            at: self.now_text(),
            requested_by: actor.account.as_ref().map(ToString::to_string),
            client_name: actor.origin.name().map(str::to_owned),
            client_addr: actor.origin.addr().map(str::to_owned),
            reported: true,
        };
        if let Err(error) = self.host.write_last(&record) {
            tracing::error!(%error, "résultat de mise à jour non écrit");
        }
        self.sink
            .record(
                actor.clone(),
                AuditAction::AgentUpdate,
                Target::AgentVersion(target.version.to_string()),
                Outcome::Failed(audit_reason(UpdateOutcome::Failed, Some(reason))),
            )
            .await;
        self.finish(&record);
    }

    /// Clôt : l'état « en cours » tombe, la fin est diffusée.
    fn finish(&self, record: &UpdateRecord) {
        self.running().progress = None;
        self.feed.publish(UpdateProgress {
            // Sur le flux, une version inconnue est vide (`GET /agent/update/last` dit pourquoi).
            version: record.version.clone().unwrap_or_default(),
            step: UpdateStep::Done,
            percent: None,
            outcome: Some(record.outcome),
            reason: record.reason,
        });
    }

    /// Surveille le superviseur lancé par ce processus : diffuse ses étapes, annonce son résultat.
    /// L'ancien agent est arrêté par le superviseur ; la tâche n'est utile que si le superviseur
    /// ne l'arrête pas (échec avant l'arrêt) ou s'il revient (retour en arrière).
    fn watch(self: &Arc<Self>, version: Version, requester: Requester) {
        let service = Arc::clone(self);
        tokio::spawn(async move {
            let started = tokio::time::Instant::now();
            let mut seen: Option<UpdateStep> = None;
            loop {
                tokio::time::sleep(service.env.timing.watch).await;
                if service.supervised() {
                    if let Ok(Some(state)) = service.host.read_state()
                        && seen != Some(state.step)
                    {
                        seen = Some(state.step);
                        service.announce(&version, state.step, None);
                    }
                    continue;
                }
                if service.report_pending().await {
                    return;
                }
                if started.elapsed() > service.env.timing.patience {
                    service.give_up_on_supervisor(&version, &requester).await;
                    return;
                }
            }
        });
    }

    /// Au démarrage de l'agent : annonce le résultat d'une mise à jour que personne n'a encore
    /// annoncée (le nouvel agent après un échange réussi, l'ancien après un retour en arrière) ;
    /// ou reprend la surveillance d'un superviseur encore au travail.
    ///
    /// **Un travail laissé en cours est conclu** (BR-UPDATE-028) : serveur redémarré, agent tué
    /// pendant le téléchargement, superviseur tué après l'échange. Selon l'étape atteinte : abandon
    /// propre (`failed` / `interrupted`, dépôt nettoyé), ou reprise par un superviseur qui contrôle
    /// le binaire en place et, sinon, remet exactement l'ancien.
    pub async fn resume(self: &Arc<Self>) {
        // Une demande arrivée entre l'ouverture du serveur et ce démarrage a déjà écrit son
        // intention : ce n'est pas un orphelin.
        if self.running().progress.is_some() {
            return;
        }
        // Lu AVANT le résultat : un superviseur qui conclut entre les deux lectures est rattrapé
        // par `watch`, au lieu de laisser un résultat jamais annoncé.
        let supervising = self.supervised();
        if self.report_pending().await {
            return;
        }
        if supervising {
            let state = self.host.read_state().ok().flatten();
            let version = state
                .as_ref()
                .and_then(|state| Version::parse(&state.version).ok())
                .unwrap_or(self.current);
            let requester = state.as_ref().map(|state| state.requester.clone());
            let step = state.map_or(UpdateStep::Check, |state: SupervisorState| state.step);
            self.announce(&version, step, None);
            self.watch(version, requester.unwrap_or_default());
            return;
        }
        let leftovers = self.read_leftovers();
        let orphan = classify_orphan(&leftovers, false, self.current);
        self.conclude_orphan(orphan).await;
    }

    /// Un superviseur travaille, ou systemd va le relancer (HRT-27). **Un seul arbitre** : tant que c'est
    /// vrai, l'agent qui démarre surveille et ne touche ni au travail ni à la copie du superviseur ; sans
    /// cela, un agent qui démarre dans les 2 s entre la mort du superviseur et sa relance réécrirait le
    /// travail (`recover`) et la reprise suivante serait tenue pour « déjà tentée ».
    fn supervised(&self) -> bool {
        self.host.supervisor_running() || self.host.supervisor_pending()
    }

    /// Ce que le dossier `update/` contient. Illisible n'est pas absent : une trace qui ne se lit
    /// pas est un travail à conclure.
    fn read_leftovers(&self) -> Leftovers {
        let job = self.host.read_job();
        let state = self.host.read_state();
        let unreadable = job.is_err() || state.is_err();
        let job = job.ok().flatten();
        // « Reprises déjà tentées » est une donnée du disque, lue dans le marqueur d'étape : les reprises
        // du superviseur plus les lancements que l'agent a ratés. Sans marqueur (agent d'avant), un
        // travail déjà marqué `recover` compte pour toutes. Le domaine (`classify_orphan`) en décide.
        let recovery_attempts = match self.host.read_marker() {
            Ok(Some(marker)) => marker.attempts(),
            _ if job.as_ref().is_some_and(|job| job.recover) => MAX_RESUMES,
            _ => 0,
        };
        Leftovers {
            unreadable,
            state: state.ok().flatten(),
            job,
            backup_present: self.host.path_exists(&self.env.backup),
            recovery_attempts,
        }
    }

    /// Conclut ce qu'un travail laissé en cours (BR-UPDATE-028) a laissé derrière lui.
    async fn conclude_orphan(self: &Arc<Self>, orphan: Orphan) {
        match orphan {
            Orphan::None => {}
            Orphan::BeforeLaunch {
                version,
                previous,
                requester,
            } => self.abandon(version, previous, requester).await,
            Orphan::LaunchedNoSwap(job) => {
                self.abandon(job.version.clone(), job.previous.clone(), job.requester())
                    .await;
            }
            Orphan::AfterSwap(job) => self.recover(job).await,
            Orphan::RecoveryAlreadyTried(job) => {
                // Une reprise a déjà été lancée pour cet échange et n'a rien conclu : pas de
                // seconde, jamais de boucle. Les copies (ancien binaire, base d'avant) restent
                // pour la reprise à la main ; seules les traces de travail sont retirées.
                tracing::error!(
                    version = %job.version,
                    "la reprise de la mise à jour n'a rien conclu : à reprendre à la main (runbook), copies gardées"
                );
                self.host.discard_work_files();
                // Le marqueur le dit aussi : abandonné, réglé, plus rien ne reprend.
                if let Ok(Some(mut marker)) = self.host.read_marker() {
                    marker.phase = Phase::Abandoned;
                    marker.outcome = Some(UpdateOutcome::Failed);
                    marker.reason = Some(UpdateReason::RollbackFailed);
                    marker.settled = true;
                    let _ = self.host.write_marker(&marker);
                }
                let record = UpdateRecord {
                    version: Some(job.version.clone()),
                    previous: job.previous.clone(),
                    outcome: UpdateOutcome::Failed,
                    reason: Some(UpdateReason::RollbackFailed),
                    at: self.now_text(),
                    requested_by: job.requested_by.clone(),
                    client_name: job.client_name.clone(),
                    client_addr: job.client_addr.clone(),
                    reported: false,
                };
                if let Err(error) = self.host.write_last(&record) {
                    tracing::error!(%error, "résultat de mise à jour non écrit");
                }
                self.report(record).await;
            }
            Orphan::Completed {
                version,
                previous,
                requester,
            } => {
                self.conclude_found(version, previous, requester, UpdateOutcome::Succeeded, None)
                    .await;
            }
            Orphan::AlreadyRolledBack(job) => {
                // La version d'avant tourne déjà : conclu sans toucher à la base (qui a vécu
                // depuis), la sauvegarde de l'ancien binaire et la copie périmée sont retirées.
                if let Err(error) = self.host.remove_path(&job.backup) {
                    tracing::warn!(%error, "sauvegarde de l'ancien binaire non retirée");
                }
                self.conclude_found(
                    job.version.clone(),
                    job.previous.clone(),
                    job.requester(),
                    UpdateOutcome::RolledBack,
                    Some(UpdateReason::NoAnswer),
                )
                .await;
            }
            Orphan::ForeignVersion(job) => {
                // FIX:01M47N6Z485TWN2H770KQ5H80R : une version posée à la main n'est jamais
                // défaite. Ni reprise, ni remise de la base : la sauvegarde de l'ancien binaire et
                // la copie périmée de la base sont retirées avec les traces.
                tracing::warn!(
                    current = %self.current,
                    version = %job.version,
                    previous = %job.previous,
                    "une autre version que celles de la mise à jour tourne : mise à jour conclue sans retour arrière"
                );
                if let Err(error) = self.host.remove_path(&job.backup) {
                    tracing::warn!(%error, "sauvegarde de l'ancien binaire non retirée");
                }
                self.conclude_found(
                    job.version.clone(),
                    job.previous.clone(),
                    job.requester(),
                    UpdateOutcome::Failed,
                    Some(UpdateReason::Interrupted),
                )
                .await;
            }
            Orphan::Unreadable { backup_present } => {
                tracing::error!("trace de mise à jour illisible");
                self.host.discard_work_files();
                let (outcome, reason) = if backup_present {
                    // L'ancien binaire est gardé, ainsi que la copie de la base : reprise à la
                    // main (runbook), ni deviné ni écrasé.
                    (UpdateOutcome::Failed, UpdateReason::RollbackFailed)
                } else {
                    self.host.clear_staging();
                    (UpdateOutcome::Failed, UpdateReason::Interrupted)
                };
                let record = UpdateRecord {
                    // Une trace illisible ne dit pas quelle version était visée : absente, jamais un texte
                    // qui ressemble à une version.
                    version: None,
                    previous: self.current.to_string(),
                    outcome,
                    reason: Some(reason),
                    at: self.now_text(),
                    requested_by: None,
                    client_name: None,
                    client_addr: None,
                    reported: false,
                };
                if let Err(error) = self.host.write_last(&record) {
                    tracing::error!(%error, "résultat de mise à jour non écrit");
                }
                self.report(record).await;
            }
        }
    }

    // FIX:01M47PHYR8MD87HAXY9PARQXAN : la patience ne conclut plus « échec du superviseur » à
    // l'aveugle (docs/bugs/FIX-01M47PHYR8MD87HAXY9PARQXAN.md).
    /// La surveillance ne voit plus de superviseur et n'a rien à annoncer depuis trop longtemps.
    /// Ce n'est pas une raison de conclure « échec » à l'aveugle : les traces sur le disque
    /// disent où en est le travail (binaires déjà échangés, nouvelle version déjà en place...) et
    /// la décision est celle du démarrage (`classify_orphan`). Seul « le superviseur n'a jamais écrit
    /// son travail » est un échec du lancement ; un superviseur qui a travaillé sans rien échanger
    /// (ou dont le retour arrière est fait) est conclu comme au démarrage : `failed` / `interrupted`.
    async fn give_up_on_supervisor(self: &Arc<Self>, version: &Version, requester: &Requester) {
        let leftovers = self.read_leftovers();
        match classify_orphan(&leftovers, false, self.current) {
            Orphan::None | Orphan::BeforeLaunch { .. } => {
                let target_text = version.to_string();
                tracing::error!(version = %target_text, "le superviseur n'a pas donné de résultat");
                // Le demandeur est celui de la mise à jour (le journal ne l'attribue pas à la ligne
                // de commande), et le dépôt est nettoyé.
                self.host.clear_staging();
                let record = UpdateRecord {
                    version: Some(target_text),
                    previous: self.current.to_string(),
                    outcome: UpdateOutcome::Failed,
                    reason: Some(UpdateReason::SupervisorLaunch),
                    at: self.now_text(),
                    requested_by: requester.by.clone(),
                    client_name: requester.name.clone(),
                    client_addr: requester.addr.clone(),
                    reported: false,
                };
                let _ = self.host.write_last(&record);
                self.report(record).await;
            }
            other => self.conclude_orphan(other).await,
        }
    }

    /// Conclut une mise à jour interrompue avant l'échange : rien n'a changé sur le serveur.
    async fn abandon(&self, version: String, previous: String, requester: Requester) {
        tracing::warn!(%version, "mise à jour interrompue avant l'échange des binaires, abandonnée");
        let record = UpdateRecord {
            version: Some(version),
            previous,
            outcome: UpdateOutcome::Failed,
            reason: Some(UpdateReason::Interrupted),
            at: self.now_text(),
            requested_by: requester.by,
            client_name: requester.name,
            client_addr: requester.addr,
            reported: false,
        };
        // Le résultat d'abord, le nettoyage ensuite : un arrêt entre les deux ne fait pas disparaître
        // la tentative.
        if let Err(error) = self.host.write_last(&record) {
            tracing::error!(%error, "résultat de mise à jour non écrit");
        }
        self.host.clear_staging();
        self.report(record).await;
    }

    /// Conclut ce que les traces et la version qui tourne ont déjà décidé : résultat écrit, puis
    /// traces nettoyées, annonce.
    async fn conclude_found(
        &self,
        version: String,
        previous: String,
        requester: Requester,
        outcome: UpdateOutcome,
        reason: Option<UpdateReason>,
    ) {
        let record = UpdateRecord {
            version: Some(version),
            previous,
            outcome,
            reason,
            at: self.now_text(),
            requested_by: requester.by,
            client_name: requester.name,
            client_addr: requester.addr,
            reported: false,
        };
        if let Err(error) = self.host.write_last(&record) {
            tracing::error!(%error, "résultat de mise à jour non écrit");
        }
        self.host.clear_staging();
        self.report(record).await;
    }

    /// Reprend une mise à jour dont les binaires ont été échangés sans que personne conclue : un
    /// superviseur contrôle le binaire en place (cet agent) et, s'il ne tient pas, remet l'ancien.
    async fn recover(self: &Arc<Self>, mut job: Job) {
        let version = Version::parse(&job.version).unwrap_or(self.current);
        tracing::warn!(%version, "échange des binaires non conclu, reprise par un superviseur");
        job.recover = true;
        let requester = job.requester();
        let host = Arc::clone(&self.host);
        let backup = self.env.backup.clone();
        let launched = blocking(move || {
            // La tentative se compte AVANT le lancement, durablement : un lancement qui échoue (systemd-run
            // absent, refusé) avance le compteur comme un superviseur repris, et à la borne l'agent conclut
            // (BR-UPDATE-033). Sans marqueur, il est créé ici avec les copies que le disque garde.
            match host.read_marker() {
                Ok(Some(mut marker)) => {
                    marker.launches += 1;
                    host.write_marker(&marker)?;
                }
                Ok(None) => {
                    let mut marker = Marker::begin(&job);
                    marker.database_copy = host.database_copy_present();
                    marker.backup_kept = host.path_exists(&backup);
                    marker.launches = 1;
                    host.write_marker(&marker)?;
                }
                // Marqueur illisible : le superviseur l'abandonnera sans rien deviner.
                Err(_) => {}
            }
            // La copie déjà déposée est l'ANCIEN binaire (celui qu'on n'a pas à juger) : une reprise la
            // garde ; sans elle (machine redémarrée, copie perdue), elle vient du binaire courant.
            let supervisor = match host.existing_supervisor() {
                Some(existing) => existing,
                None => host.prepare_supervisor()?,
            };
            let job_path = host.write_job(&job)?;
            host.launch(&supervisor, &job_path)
        })
        .await;
        match launched {
            Ok(Ok(())) => {
                self.announce(&version, UpdateStep::Check, None);
                self.watch(version, requester);
            }
            Ok(Err(error)) => {
                tracing::error!(%error, "reprise de la mise à jour impossible : à reprendre à la main (runbook)");
            }
            Err(_) => tracing::error!("reprise de la mise à jour interrompue"),
        }
    }

    /// Annonce le dernier résultat s'il ne l'a pas encore été. `true` s'il y en avait un.
    async fn report_pending(&self) -> bool {
        match self.host.read_last() {
            Ok(Some(record)) if !record.reported => {
                self.report(record).await;
                true
            }
            Ok(_) => false,
            Err(error) => {
                tracing::warn!(%error, "dernier résultat de mise à jour illisible");
                false
            }
        }
    }

    /// Écrit le résultat au journal d'activité (une seule fois : `reported`), puis l'annonce.
    async fn report(&self, mut record: UpdateRecord) {
        let actor = Actor::new(
            record
                .requested_by
                .as_deref()
                .and_then(|name| Username::parse(name).ok()),
            match (&record.client_name, &record.client_addr) {
                (None, None) => Origin::CommandLine,
                (name, addr) => Origin::client(name.as_deref(), addr.as_deref().unwrap_or("")),
            },
        );
        let outcome = match record.outcome {
            UpdateOutcome::Succeeded => Outcome::Succeeded,
            other => Outcome::Failed(audit_reason(other, record.reason)),
        };
        self.sink
            .record(
                actor,
                AuditAction::AgentUpdate,
                record
                    .version
                    .clone()
                    .map_or(Target::None, Target::AgentVersion),
                outcome,
            )
            .await;
        record.reported = true;
        if let Err(error) = self.host.write_last(&record) {
            tracing::error!(%error, "résultat de mise à jour non marqué comme annoncé");
        }
        self.finish(&record);
    }

    fn now_text(&self) -> String {
        format_time(self.clock.now())
    }
}

/// La raison du journal d'activité, d'après le résultat d'une mise à jour.
pub fn audit_reason(outcome: UpdateOutcome, reason: Option<UpdateReason>) -> Reason {
    match (outcome, reason) {
        (UpdateOutcome::RolledBack, _) => Reason::RolledBack,
        (_, Some(UpdateReason::BadSignature | UpdateReason::BadChecksum)) => Reason::BadSignature,
        (_, Some(UpdateReason::Unreachable | UpdateReason::DownloadFailed)) => {
            Reason::DownloadFailed
        }
        _ => Reason::UpdateFailed,
    }
}

fn requester_of(actor: &Actor) -> Requester {
    Requester {
        by: actor.account.as_ref().map(ToString::to_string),
        name: actor.origin.name().map(str::to_owned),
        addr: actor.origin.addr().map(str::to_owned),
    }
}

pub(crate) fn format_time(at: OffsetDateTime) -> String {
    at.to_offset(time::UtcOffset::UTC)
        .format(&Rfc3339)
        .unwrap_or_default()
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

//! Démarrer, arrêter et redémarrer un service (HRT-64, ADR-0037, ADR-0038, ADR-0045) : trois actes
//! d'administration qui pilotent un service par son pilote (le moteur, ou le système du serveur pour un service
//! lancé par une unité), avec délai de grâce propre au service, verrou d'une seule action par service, arrêt
//! voulu mémorisé et issue relisible après une coupure.
//!
//! **Ce que fait [`ServiceLifecycle::request`], de façon SYNCHRONE** (la requête n'attend rien du moteur) :
//! le moteur est-il prêt (`BR-LIFE-019`), le service existe-t-il et est-il publié (`BR-LIFE-015`,
//! `BR-LIFE-024`), une action le tient-elle déjà (`BR-LIFE-013`), son état permet-il le geste
//! (`BR-LIFE-014`) ; puis l'opération est acceptée dans le journal (verrou pris) et son exécution part en
//! tâche de fond. Le droit de gérer les services et la confirmation de l'acte sont jugés avant, par la couche
//! d'accès et la couche `reauth`. La réponse est `202 { action_id, grace_s }` ; l'issue se lit dans
//! l'opération (`GET /operations/services/{id}`), dans `last_action` du service et dans le flux.
//!
//! **Ce que fait l'exécution** :
//! - **arrêter** : l'arrêt voulu est écrit dans `service_intents` AVANT l'ordre (`BR-LIFE-010`), puis l'ordre
//!   part avec le délai de grâce du service ; l'attente est ce délai plus 30 secondes, au moins 60
//!   (`BR-LIFE-021`) ; un arrêt qui a dû être forcé est dit (`BR-LIFE-009`) ;
//! - **démarrer** : l'arrêt voulu est oublié, l'ordre part, « En marche » vient du moteur (jamais par avance,
//!   `BR-LIFE-004`) ;
//! - **redémarrer** : un seul ordre du pilote ; si le service s'est arrêté mais n'a pas pu repartir (port pris
//!   entre-temps), il reste arrêté et la cause est dite (`BR-LIFE-012`, `BR-LIFE-006`).
//!
//! **Ce qui vient après la fin de l'action et ne bloque aucune autre** : la surveillance de 10 secondes d'un
//! service lancé (il s'est arrêté aussitôt : `BR-LIFE-005`) et de 30 secondes d'un service arrêté (il est reparti
//! tout seul : `BR-LIFE-011`). Elle pose sa cause, après coup, sur l'opération déjà terminée en succès et
//! l'écrit au journal d'activité ; elle s'arrête dès qu'un geste de Hearth reprend le service.
//!
//! **Jamais** : rejouer un ordre (`BR-LIFE-020`, `BR-LIFE-021`), convertir un geste en un autre
//! (`BR-LIFE-014`), recréer ou modifier le service (`BR-LIFE-002`), écrire dans une trace ou le journal un
//! texte du moteur, une variable, une commande ou un chemin.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use thiserror::Error;
use time::OffsetDateTime;
use tokio::time::Instant;

use super::ports::{
    AuditSink, Clock, ContainerEngine, EngineError, ServiceIntents, ServiceSupervisor, StopIntent,
    SupervisorError,
};
use super::service_ops::{AcceptRequest, AuditDraft, InterruptedAction, OpError, ServiceOps};
use super::services::{Driver, LifecycleTarget, ServicesRegistry};
use crate::domain::audit::{Actor, ClientName, LifecycleNote, Outcome, Reason, Target};
use crate::domain::ops::{
    Cause, Gesture, Holder, OpKind, OpStatus, RESTART_WATCH, Refusal, STARTUP_WATCH, Sighting,
    StateRefusal, StopEvidence, UndoInfo, WAIT_FLOOR, WAIT_MARGIN, Watch, cause_after, check_state,
    engine_ready, stop_was_forced, wait_budget_with,
};
use crate::domain::services::{
    EngineState, RunState, ServiceKey, ServiceStateKind, TransitionKind, UnitActivity, UnitLookup,
};

/// Les cadences de la surveillance qui suit une action. Les tests les raccourcissent.
#[derive(Debug, Clone, Copy)]
pub struct LifecycleTimings {
    /// Combien de temps un service lancé est surveillé (`BR-LIFE-005`).
    pub startup_watch: Duration,
    /// Combien de temps un service arrêté est surveillé (`BR-LIFE-011`).
    pub restart_watch: Duration,
    /// L'intervalle entre deux lectures pendant la surveillance.
    pub poll: Duration,
    /// Marge ajoutée à la fin d'une action pour l'exemption des démarrages demandés par Hearth
    /// (`BR-SVC-006`) : l'heure du moteur et celle de l'agent ne sont pas lues au même instant.
    pub exemption_margin: Duration,
    /// La marge ajoutée au délai d'arrêt pour l'attente d'une action (30 s, `BR-LIFE-021`).
    pub wait_margin: Duration,
    /// Le plancher de cette attente (60 s, `BR-LIFE-021`).
    pub wait_floor: Duration,
}

impl LifecycleTimings {
    pub const fn production() -> Self {
        Self {
            startup_watch: STARTUP_WATCH,
            restart_watch: RESTART_WATCH,
            poll: Duration::from_millis(500),
            exemption_margin: Duration::from_secs(2),
            wait_margin: WAIT_MARGIN,
            wait_floor: WAIT_FLOOR,
        }
    }

    /// L'attente d'une action sur un service dont le délai d'arrêt est `grace_s` (`BR-LIFE-021`).
    pub fn wait_for(&self, grace_s: u32) -> Duration {
        wait_budget_with(grace_s, self.wait_margin, self.wait_floor)
    }
}

/// Combien de surveillances de fin d'action tournent au plus en même temps : un serveur qui enchaîne les
/// actions sur beaucoup de services ne fait pas croître le travail de fond sans borne.
const MAX_WATCHES: usize = 64;

/// Ce que la requête reçoit quand l'action est acceptée.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Accepted {
    /// L'identifiant de l'opération (`GET /operations/services/{id}`).
    pub action_id: String,
    /// Le délai d'arrêt du service, en secondes, que le client affiche dans son décompte (`BR-LIFE-009`).
    pub grace_s: u32,
}

/// Pourquoi une demande est refusée tout de suite, avant que rien ne soit ordonné.
#[derive(Debug, Error)]
pub enum LifecycleError {
    /// Le moteur n'est pas prêt : le service n'a pas été modifié (`BR-LIFE-019`).
    #[error("le moteur n'est pas prêt")]
    EngineUnavailable(EngineState),
    /// Aucun service publié de cette clé (`BR-LIFE-015`, `BR-LIFE-024`).
    #[error("service inconnu")]
    NotFound,
    /// Le service est déjà dans l'état visé (`BR-LIFE-014`).
    #[error("déjà dans l'état demandé")]
    AlreadyInState(ServiceStateKind),
    /// L'état du service ne permet pas ce geste (`BR-LIFE-014`).
    #[error("état incompatible avec l'action")]
    NotDrivable(ServiceStateKind),
    /// Une autre action tient le service (`BR-LIFE-013`) : qui, et laquelle.
    #[error("une action est déjà en cours")]
    Busy { by: String, kind: OpKind },
    #[error("erreur interne : {0}")]
    Internal(String),
}

/// Ce que l'exécution d'une action a décidé de son issue.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Finish {
    status: OpStatus,
    cause: Option<Cause>,
    forced: bool,
    /// Ce qu'il faut surveiller encore une fois l'action terminée.
    watch: Option<Watch>,
}

impl Finish {
    fn done(forced: bool, watch: Watch) -> Self {
        Self {
            status: OpStatus::Succeeded,
            cause: forced.then_some(Cause::ForcedStop),
            forced,
            watch: Some(watch),
        }
    }

    fn failed(cause: Cause) -> Self {
        Self {
            status: OpStatus::Failed,
            cause: Some(cause),
            forced: false,
            watch: None,
        }
    }

    fn unknown(cause: Cause) -> Self {
        Self {
            status: OpStatus::Interrupted,
            cause: Some(cause),
            forced: false,
            watch: None,
        }
    }
}

/// Pourquoi un ordre n'a pas abouti, indépendamment du pilote (jamais un texte du moteur).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OrderFailure {
    /// Le service n'existe plus.
    Gone,
    /// Le pilote est injoignable : on ne sait pas si l'ordre est parti (`BR-LIFE-019`).
    Unreachable,
    /// Le pilote n'existe pas sur ce système : l'ordre n'est jamais parti.
    Unavailable,
    /// Le pilote a refusé.
    Refused,
    /// Un port que le service demande est déjà pris (`BR-LIFE-006`).
    PortInUse,
    /// Le pilote n'a pas rendu la main dans le délai (`BR-LIFE-021`).
    TimedOut,
}

impl OrderFailure {
    fn from_engine(error: EngineError) -> Self {
        match error {
            EngineError::NotFound => Self::Gone,
            EngineError::Unreachable | EngineError::Denied => Self::Unreachable,
            EngineError::Timeout => Self::TimedOut,
            EngineError::PortInUse => Self::PortInUse,
            EngineError::Refused
            | EngineError::Unsupported
            // Les erreurs de récupération et de création n'arrivent pas d'un ordre de cycle de vie.
            | EngineError::ImageNotFound
            | EngineError::TagNotFound
            | EngineError::ImagePrivateOrMissing
            | EngineError::RegistryUnreachable
            | EngineError::RegistryRateLimited
            | EngineError::PlatformMismatch
            | EngineError::DiskFull
            | EngineError::NameTaken => Self::Refused,
        }
    }

    fn from_supervisor(error: SupervisorError) -> Self {
        match error {
            SupervisorError::Unavailable => Self::Unavailable,
            SupervisorError::Refused => Self::Refused,
            SupervisorError::Timeout => Self::TimedOut,
        }
    }

    /// L'issue de l'opération. Un résultat qu'on ne connaît pas (pilote injoignable, pas de réponse à temps)
    /// est « interrompu », jamais « échoué » : l'état réel est relu quand le pilote répond (`BR-LIFE-019`,
    /// `BR-LIFE-021`). Un refus net est un échec, et le service n'a pas été modifié.
    fn finish(self) -> Finish {
        match self {
            Self::Gone => Finish::failed(Cause::ServiceNotFound),
            Self::Unreachable => Finish::unknown(Cause::EngineUnavailable),
            Self::Unavailable => Finish::failed(Cause::EngineUnavailable),
            Self::Refused => Finish::failed(Cause::EngineRefused),
            Self::PortInUse => Finish::failed(Cause::PortInUse),
            Self::TimedOut => Finish::unknown(Cause::Timeout),
        }
    }
}

/// L'étape annoncée tant que l'action dure, retirée quoi qu'il arrive (même si la tâche est abandonnée).
struct StepShown<'a> {
    registry: &'a ServicesRegistry,
    key: ServiceKey,
}

impl Drop for StepShown<'_> {
    fn drop(&mut self) {
        self.registry.end_transition(&self.key);
    }
}

/// Un compteur de surveillances qui se décompte quand la surveillance finit, ou est abandonnée.
struct WatchSeat(Arc<AtomicUsize>);

impl Drop for WatchSeat {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Tout ce qu'une exécution d'action emporte avec elle.
struct Job {
    id: String,
    gesture: Gesture,
    target: LifecycleTarget,
    actor: Actor,
    account: String,
    requested_at: OffsetDateTime,
}

pub struct ServiceLifecycle {
    registry: Arc<ServicesRegistry>,
    ops: Arc<ServiceOps>,
    engine: Arc<dyn ContainerEngine>,
    supervisor: Arc<dyn ServiceSupervisor>,
    intents: Arc<dyn ServiceIntents>,
    clock: Arc<dyn Clock>,
    timings: LifecycleTimings,
    watches: Arc<AtomicUsize>,
}

impl ServiceLifecycle {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        registry: Arc<ServicesRegistry>,
        ops: Arc<ServiceOps>,
        engine: Arc<dyn ContainerEngine>,
        supervisor: Arc<dyn ServiceSupervisor>,
        intents: Arc<dyn ServiceIntents>,
        clock: Arc<dyn Clock>,
        timings: LifecycleTimings,
    ) -> Arc<Self> {
        Arc::new(Self {
            registry,
            ops,
            engine,
            supervisor,
            intents,
            clock,
            timings,
            watches: Arc::new(AtomicUsize::new(0)),
        })
    }

    // -----------------------------------------------------------------------------------------
    // La demande : tout ce qui se juge sans rien attendre du moteur
    // -----------------------------------------------------------------------------------------

    /// Accepte (ou refuse tout de suite) un geste de cycle de vie sur un service. Le droit de gérer les
    /// services et la confirmation de l'acte ont déjà été jugés.
    ///
    /// L'ordre des vérifications est celui des spécifications : le moteur (`BR-LIFE-019`), le service
    /// (`BR-LIFE-015`), l'action en cours (`BR-LIFE-013`, avant l'état : un service en train de s'arrêter est
    /// « occupé », pas « incompatible »), l'état (`BR-LIFE-014`).
    pub async fn request(
        self: &Arc<Self>,
        gesture: Gesture,
        key: &ServiceKey,
        actor: Actor,
        op_key: &str,
    ) -> Result<Accepted, LifecycleError> {
        let engine_state = self.registry.engine_state();
        if !engine_ready(engine_state) {
            return Err(LifecycleError::EngineUnavailable(engine_state));
        }
        let target = self
            .registry
            .lifecycle_target(key)
            .ok_or(LifecycleError::NotFound)?;
        if let Some(busy) = self.busy_of(key) {
            return Err(busy);
        }
        check_state(gesture, target.state).map_err(|refusal| match refusal {
            StateRefusal::AlreadyInState(state) => LifecycleError::AlreadyInState(state),
            StateRefusal::NotDrivable(state) => LifecycleError::NotDrivable(state),
        })?;
        let plan = gesture
            .plan()
            .map_err(|error| LifecycleError::Internal(error.to_string()))?;
        let account = actor
            .account
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default();
        let id = self
            .ops
            .accept(AcceptRequest {
                op_key: op_key.to_owned(),
                service: key.clone(),
                service_name: target.name.clone(),
                kind: gesture.kind(),
                account: account.clone(),
                plan,
                forced: false,
                reserve_name: None,
                heavy_io: false,
            })
            .await
            .map_err(|error| match error {
                // Deux demandes simultanées : la seconde n'a pas vu la première dans l'index, le verrou en
                // mémoire (ou l'index unique de la base) la voit.
                OpError::Locked(Refusal::ServiceBusy { holder }) => LifecycleError::Busy {
                    by: self.holder_account(key),
                    kind: match holder {
                        Holder::Operation(kind) => kind,
                        Holder::FileWrites => gesture.kind(),
                    },
                },
                OpError::ServiceBusy => LifecycleError::Busy {
                    by: self.holder_account(key),
                    kind: gesture.kind(),
                },
                other => LifecycleError::Internal(other.to_string()),
            })?;
        if let Err(error) = self.ops.start(&id).await {
            // L'opération ne peut pas démarrer : elle est close, le verrou rendu, rien n'a été ordonné.
            journal(
                self.ops
                    .finish(&id, OpStatus::Failed, Some(Cause::StepFailed))
                    .await,
                "finish",
            );
            return Err(LifecycleError::Internal(error.to_string()));
        }
        let accepted = Accepted {
            action_id: id.clone(),
            grace_s: target.grace_s,
        };
        let job = Job {
            id,
            gesture,
            target,
            actor,
            account,
            requested_at: self.clock.now(),
        };
        let this = Arc::clone(self);
        tokio::spawn(async move { this.execute(job).await });
        Ok(accepted)
    }

    /// L'action qui tient ce service, d'après l'index du journal (visible de tous).
    fn busy_of(&self, key: &ServiceKey) -> Option<LifecycleError> {
        self.ops.with_index(|index| {
            index.busy(key).map(|open| LifecycleError::Busy {
                by: open.account.clone(),
                kind: open.kind,
            })
        })
    }

    fn holder_account(&self, key: &ServiceKey) -> String {
        self.ops.with_index(|index| {
            index
                .busy(key)
                .map(|open| open.account.clone())
                .unwrap_or_default()
        })
    }

    // -----------------------------------------------------------------------------------------
    // L'exécution
    // -----------------------------------------------------------------------------------------

    async fn execute(self: Arc<Self>, job: Job) {
        let started = Instant::now();
        let finish = {
            // L'étape annoncée ne survit pas à l'action, ni à la tâche si elle est abandonnée.
            let _step = StepShown {
                registry: &self.registry,
                key: job.target.key.clone(),
            };
            match job.gesture {
                Gesture::Stop => self.run_stop(&job, started).await,
                Gesture::Start => self.run_start(&job).await,
                Gesture::Restart => self.run_restart(&job, started).await,
            }
        };
        self.settle(&job, finish, started.elapsed()).await;
    }

    /// Arrêter : l'arrêt voulu est écrit AVANT l'ordre (`BR-LIFE-010`).
    async fn run_stop(&self, job: &Job, started: Instant) -> Finish {
        let key = &job.target.key;
        let intent = StopIntent {
            key: key.clone(),
            container_id: match &job.target.driver {
                Driver::Engine { container_id } => container_id.clone(),
                Driver::Supervisor { .. } => String::new(),
            },
            account: job.account.clone(),
            at: self.clock.now(),
        };
        if let Err(error) = self.intents.save(&intent).await {
            tracing::error!(%error, "arrêt voulu non écrit : l'ordre d'arrêt n'est pas donné");
            return Finish::failed(Cause::StepFailed);
        }
        self.registry.note_stop_requested(key, intent.at);
        if let Err(error) = self.ops.intent(&job.id, "stop", &UndoInfo::default()).await {
            tracing::error!(%error, "journal de l'action illisible : l'ordre d'arrêt n'est pas donné");
            self.forget_stop(key).await;
            return Finish::failed(Cause::StepFailed);
        }
        self.registry
            .begin_transition(key, TransitionKind::Stopping);
        let ordered_at = self.clock.now();
        let ordered = self.order(job).await;
        match ordered {
            Ok(()) => {
                journal(
                    self.ops.done(&job.id, "stop", &UndoInfo::default()).await,
                    "done",
                );
                let forced = self
                    .stop_was_forced(job, started.elapsed(), ordered_at)
                    .await;
                self.refresh(job).await;
                Finish::done(forced, Watch::after(job.gesture))
            }
            Err(failure) => {
                let finish = failure.finish();
                journal(self.ops.failed(&job.id, "stop").await, "failed");
                // L'ordre n'a pas eu lieu (refusé, pilote absent, service disparu) : rien n'a été arrêté, l'arrêt
                // voulu n'a pas lieu d'être. Un résultat inconnu le garde : l'arrêt a peut-être eu lieu.
                if finish.status == OpStatus::Failed {
                    self.forget_stop(key).await;
                }
                self.refresh(job).await;
                finish
            }
        }
    }

    /// Démarrer : l'arrêt voulu est oublié, « En marche » vient du moteur (`BR-LIFE-004`).
    async fn run_start(&self, job: &Job) -> Finish {
        let key = &job.target.key;
        if let Err(error) = self
            .ops
            .intent(&job.id, "start", &UndoInfo::default())
            .await
        {
            tracing::error!(%error, "journal de l'action illisible : l'ordre de démarrage n'est pas donné");
            return Finish::failed(Cause::StepFailed);
        }
        self.registry
            .begin_transition(key, TransitionKind::Starting);
        match self.order(job).await {
            Ok(()) => {
                journal(
                    self.ops.done(&job.id, "start", &UndoInfo::default()).await,
                    "done",
                );
                self.forget_stop(key).await;
                self.exempt_starts(job);
                self.refresh(job).await;
                Finish::done(false, Watch::after(job.gesture))
            }
            Err(failure) => {
                journal(self.ops.failed(&job.id, "start").await, "failed");
                self.refresh(job).await;
                failure.finish()
            }
        }
    }

    /// Redémarrer : un seul ordre du pilote. Si le service s'est arrêté mais n'a pas pu repartir, il reste
    /// arrêté (`BR-LIFE-012`) : sa sortie est celle de l'arrêt voulu, pas une erreur.
    async fn run_restart(&self, job: &Job, started: Instant) -> Finish {
        let key = &job.target.key;
        if let Err(error) = self
            .ops
            .intent(&job.id, "restart", &UndoInfo::default())
            .await
        {
            tracing::error!(%error, "journal de l'action illisible : l'ordre de redémarrage n'est pas donné");
            return Finish::failed(Cause::StepFailed);
        }
        self.registry
            .begin_transition(key, TransitionKind::Restarting);
        let ordered_at = self.clock.now();
        match self.order(job).await {
            Ok(()) => {
                journal(
                    self.ops
                        .done(&job.id, "restart", &UndoInfo::default())
                        .await,
                    "done",
                );
                self.forget_stop(key).await;
                self.exempt_starts(job);
                let forced = self
                    .stop_was_forced(job, started.elapsed(), ordered_at)
                    .await;
                self.refresh(job).await;
                Finish::done(forced, Watch::after(job.gesture))
            }
            Err(failure) => {
                journal(self.ops.failed(&job.id, "restart").await, "failed");
                let finish = failure.finish();
                // Arrêté puis pas reparti : l'arrêt est celui de Hearth.
                if finish.status == OpStatus::Failed
                    && matches!(
                        self.seen(&job.target.driver).await,
                        Sighting::Stopped | Sighting::Gone
                    )
                {
                    self.keep_stop(job).await;
                }
                self.exempt_starts(job);
                self.refresh(job).await;
                finish
            }
        }
    }

    /// Donne l'ordre au pilote du service, au plus pendant l'attente de l'action (`BR-LIFE-021`).
    async fn order(&self, job: &Job) -> Result<(), OrderFailure> {
        let grace = Duration::from_secs(u64::from(job.target.grace_s));
        let wait = self.timings.wait_for(job.target.grace_s);
        let work = async {
            match (&job.target.driver, job.gesture) {
                (Driver::Engine { container_id }, Gesture::Start) => self
                    .engine
                    .start(container_id)
                    .await
                    .map_err(OrderFailure::from_engine),
                (Driver::Engine { container_id }, Gesture::Stop) => self
                    .engine
                    .stop(container_id, grace)
                    .await
                    .map_err(OrderFailure::from_engine),
                (Driver::Engine { container_id }, Gesture::Restart) => self
                    .engine
                    .restart(container_id, grace)
                    .await
                    .map_err(OrderFailure::from_engine),
                (Driver::Supervisor { unit }, Gesture::Start) => self
                    .supervisor
                    .start(unit, wait)
                    .await
                    .map_err(OrderFailure::from_supervisor),
                (Driver::Supervisor { unit }, Gesture::Stop) => self
                    .supervisor
                    .stop(unit, wait)
                    .await
                    .map_err(OrderFailure::from_supervisor),
                (Driver::Supervisor { unit }, Gesture::Restart) => self
                    .supervisor
                    .restart(unit, wait)
                    .await
                    .map_err(OrderFailure::from_supervisor),
            }
        };
        tokio::time::timeout(wait, work)
            .await
            .unwrap_or(Err(OrderFailure::TimedOut))
    }

    /// L'arrêt qui vient d'aboutir a-t-il été forcé (`BR-LIFE-009`) ? Le jugement est celui du domaine
    /// ([`stop_was_forced`]) ; ici, seulement la preuve la plus proche du pilote, et la marque au journal.
    async fn stop_was_forced(
        &self,
        job: &Job,
        waited: Duration,
        ordered_at: OffsetDateTime,
    ) -> bool {
        let evidence = self.stop_evidence(job, waited, ordered_at).await;
        let forced = stop_was_forced(evidence, job.target.grace_s);
        if forced {
            journal(self.ops.mark_forced(&job.id).await, "forced");
        }
        forced
    }

    /// La preuve de l'arrêt qui vient d'avoir lieu, par pilote et par geste :
    /// - arrêter par le moteur : le code de sortie et l'attente de l'agent ; un conteneur déjà supprimé (il se
    ///   supprime à l'arrêt) n'a plus de code : l'attente seule ;
    /// - redémarrer par le moteur : le démarrage remet le code à zéro ; reste la fin de l'exécution arrêtée
    ///   (`FinishedAt`, que le démarrage garde) moins l'heure de l'ordre ; sans elle, rien ;
    /// - par le système : la durée de l'arrêt qu'il a mesurée lui-même, démarrage d'un redémarrage exclu ; à
    ///   défaut, l'attente de l'agent pour un arrêt, rien pour un redémarrage (elle compte le démarrage).
    async fn stop_evidence(
        &self,
        job: &Job,
        waited: Duration,
        ordered_at: OffsetDateTime,
    ) -> StopEvidence {
        match (&job.target.driver, job.gesture) {
            (Driver::Engine { container_id }, Gesture::Restart) => {
                match self.engine.inspect(container_id).await {
                    Ok(Some(observation)) => observation
                        .finished_at
                        .and_then(|end| Duration::try_from(end - ordered_at).ok())
                        .map_or(StopEvidence::Unknown, StopEvidence::Took),
                    _ => StopEvidence::Unknown,
                }
            }
            (Driver::Engine { container_id }, _) => match self.engine.inspect(container_id).await {
                Ok(Some(observation)) => StopEvidence::ExitCode {
                    code: observation.exit_code,
                    waited,
                },
                _ => StopEvidence::Took(waited),
            },
            (Driver::Supervisor { unit }, gesture) => {
                match self.supervisor.last_stop_duration(unit).await {
                    Some(took) => StopEvidence::Took(took),
                    None if gesture == Gesture::Restart => StopEvidence::Unknown,
                    None => StopEvidence::Took(waited),
                }
            }
        }
    }

    /// Ce que le pilote dit du service, maintenant.
    async fn seen(&self, driver: &Driver) -> Sighting {
        match driver {
            Driver::Engine { container_id } => match self.engine.inspect(container_id).await {
                Ok(Some(observation)) => match observation.run_state {
                    RunState::Running | RunState::Paused => Sighting::Running,
                    RunState::Restarting => Sighting::Restarting,
                    RunState::Created | RunState::Exited | RunState::Dead | RunState::Removing => {
                        Sighting::Stopped
                    }
                    RunState::Unknown => Sighting::Unknown,
                },
                Ok(None) => Sighting::Gone,
                Err(_) => Sighting::Unknown,
            },
            Driver::Supervisor { unit } => match self.supervisor.unit(unit).await {
                UnitLookup::Loaded(status) => match status.activity {
                    UnitActivity::Active => Sighting::Running,
                    UnitActivity::Activating => Sighting::Restarting,
                    UnitActivity::Inactive | UnitActivity::Failed | UnitActivity::Deactivating => {
                        Sighting::Stopped
                    }
                    UnitActivity::Other => Sighting::Unknown,
                },
                UnitLookup::NotLoaded => Sighting::Gone,
                UnitLookup::Unavailable => Sighting::Unknown,
            },
        }
    }

    /// Oublie l'arrêt voulu, en base puis en mémoire.
    async fn forget_stop(&self, key: &ServiceKey) {
        if let Err(error) = self.intents.clear(key).await {
            tracing::warn!(%error, "arrêt voulu non effacé");
        }
        self.registry.forget_stop_requested(key);
    }

    /// Retient l'arrêt voulu après coup : le service a été arrêté par un redémarrage de Hearth qui n'a pas pu
    /// repartir.
    async fn keep_stop(&self, job: &Job) {
        let intent = StopIntent {
            key: job.target.key.clone(),
            container_id: match &job.target.driver {
                Driver::Engine { container_id } => container_id.clone(),
                Driver::Supervisor { .. } => String::new(),
            },
            account: job.account.clone(),
            at: job.requested_at,
        };
        if let Err(error) = self.intents.save(&intent).await {
            tracing::warn!(%error, "arrêt voulu non écrit");
        }
        self.registry
            .note_stop_requested(&job.target.key, intent.at);
    }

    /// Les démarrages que Hearth vient de demander ne comptent pas pour « redémarre en boucle » (`BR-SVC-006`).
    fn exempt_starts(&self, job: &Job) {
        let margin = time::Duration::try_from(self.timings.exemption_margin)
            .unwrap_or(time::Duration::seconds(2));
        self.registry.exempt_starts(
            &job.target.key,
            job.requested_at - margin,
            self.clock.now() + margin,
        );
    }

    /// Relit le service tout de suite : ce que le client voit est ce que le moteur dit maintenant.
    async fn refresh(&self, job: &Job) {
        match &job.target.driver {
            Driver::Engine { container_id } => self.registry.reread(container_id).await,
            // Le système recrée le conteneur à chaque démarrage : seule la liste entière le retrouve.
            Driver::Supervisor { .. } => {
                let _ = self.registry.resync().await;
            }
        }
    }

    /// Clôt l'opération, la consigne au journal d'activité et aux traces, puis lance la surveillance qui suit.
    /// La trace dit l'opération, jamais le service : sa clé porte le nom d'une unité (`u_`).
    async fn settle(self: &Arc<Self>, job: &Job, finish: Finish, took: Duration) {
        // FIX:01M5NYA5W781YW5NFQ7W1JGR98 L'issue et l'entrée du journal d'activité sont écrites dans UNE
        // transaction, puis annoncées : une transition refusée n'écrit pas de ligne, et qui lit l'issue (flux
        // ou GET) relit un journal complet.
        let draft = self.draft(job, finish.status, finish.cause);
        match self
            .ops
            .finish_with(&job.id, finish.status, finish.cause, draft)
            .await
        {
            Ok(()) => {}
            Err(OpError::IllegalTransition | OpError::Stale) => tracing::info!(
                operation = %job.id,
                "action de service non close : l'opération a changé de statut entre-temps"
            ),
            Err(error) => tracing::error!(%error, "action de service non close dans le journal"),
        }
        tracing::info!(
            operation = %job.id,
            kind = job.gesture.step(),
            status = ?finish.status,
            cause = finish.cause.map(Cause::as_str),
            forced = finish.forced,
            duration_ms = u64::try_from(took.as_millis()).unwrap_or(u64::MAX),
            "action de cycle de vie terminée"
        );
        if let Some(watch) = finish.watch {
            self.watch_later(job, watch);
        }
    }

    /// Une entrée au journal d'activité : le compte, le service (son nom), le geste, l'issue et sa cause
    /// typée (`BR-LIFE-018`).
    fn draft(&self, job: &Job, status: OpStatus, cause: Option<Cause>) -> Option<AuditDraft> {
        let service = ClientName::parse(&job.target.name)?;
        let (outcome, note) = match (status, cause) {
            (OpStatus::Succeeded, Some(Cause::ForcedStop)) => {
                (Outcome::Succeeded, LifecycleNote::Forced)
            }
            (OpStatus::Succeeded, _) => (Outcome::Succeeded, LifecycleNote::Plain),
            (_, cause) => (
                Outcome::Failed(cause.map_or(Reason::Internal, reason_of)),
                LifecycleNote::Plain,
            ),
        };
        Some(AuditDraft {
            actor: job.actor.clone(),
            action: job.gesture.kind().audit_action(),
            target: Target::ServiceLifecycle { service, note },
            outcome,
        })
    }

    // -----------------------------------------------------------------------------------------
    // La surveillance qui suit une action terminée
    // -----------------------------------------------------------------------------------------

    fn watch_later(self: &Arc<Self>, job: &Job, watch: Watch) {
        if self.watches.fetch_add(1, Ordering::SeqCst) >= MAX_WATCHES {
            self.watches.fetch_sub(1, Ordering::SeqCst);
            tracing::warn!(
                operation = %job.id,
                "surveillance de fin d'action non lancée : {MAX_WATCHES} déjà en cours"
            );
            return;
        }
        let seat = WatchSeat(Arc::clone(&self.watches));
        let this = Arc::clone(self);
        let id = job.id.clone();
        let key = job.target.key.clone();
        let driver = job.target.driver.clone();
        let name = job.target.name.clone();
        let actor = job.actor.clone();
        let kind = job.gesture.kind();
        tokio::spawn(async move {
            let _seat = seat;
            this.watch(watch, &id, &key, &driver, &name, actor, kind)
                .await;
        });
    }

    /// Surveille le service jusqu'à la fin de la fenêtre, ou jusqu'à ce qu'un autre geste de Hearth le reprenne.
    #[allow(clippy::too_many_arguments)]
    async fn watch(
        &self,
        watch: Watch,
        id: &str,
        key: &ServiceKey,
        driver: &Driver,
        name: &str,
        actor: Actor,
        kind: OpKind,
    ) {
        let window = match watch {
            Watch::Started => self.timings.startup_watch,
            Watch::Stopped => self.timings.restart_watch,
        };
        let deadline = Instant::now() + window;
        while Instant::now() < deadline {
            tokio::time::sleep(self.timings.poll).await;
            // Un autre geste de Hearth a repris le service : ce qui arrive est son affaire.
            if !self.still_last(key, id) {
                return;
            }
            let Some(cause) = cause_after(watch, self.seen(driver).await) else {
                continue;
            };
            // Une cause n'en remplace pas une autre : la surveillance ne dit rien d'une action forcée.
            // FIX:01M5NYA5W781YW5NFQ7W1JGR98 La cause et son entrée du journal d'activité sont écrites dans
            // UNE transaction, puis annoncées.
            let Some(service) = ClientName::parse(name) else {
                return;
            };
            let draft = AuditDraft {
                actor,
                action: kind.audit_action(),
                target: Target::ServiceLifecycle {
                    service,
                    note: LifecycleNote::Plain,
                },
                outcome: Outcome::Failed(reason_of(cause)),
            };
            if let Err(error) = self.ops.annotate_with(id, cause, draft).await {
                tracing::warn!(%error, "constat de surveillance non noté");
            }
            return;
        }
    }

    /// L'opération `id` est-elle toujours la dernière du service, sans qu'une autre le tienne ?
    fn still_last(&self, key: &ServiceKey, id: &str) -> bool {
        self.ops.with_index(|index| {
            index.busy(key).is_none() && index.last_action(key).is_some_and(|last| last.id == id)
        })
    }
}

/// Une écriture du journal des opérations qui échoue ne fait pas échouer l'action (l'ordre est donné, ou ne
/// le sera pas), mais elle se voit : la reprise au démarrage lira une étape incomplète.
fn journal<T>(result: Result<T, OpError>, what: &'static str) {
    if let Err(error) = result {
        tracing::warn!(%error, write = what, "journal de l'action non écrit");
    }
}

/// La raison du journal d'activité d'une cause d'opération (liste fermée, `BR-LIFE-018`).
pub(crate) fn reason_of(cause: Cause) -> Reason {
    match cause {
        Cause::PortInUse => Reason::PortInUse,
        Cause::StoppedRightAfterStart => Reason::StoppedRightAfterStart,
        Cause::EngineUnavailable => Reason::EngineUnavailable,
        Cause::EngineRefused => Reason::EngineRefused,
        Cause::RestartedByHost => Reason::RestartedByHost,
        Cause::Timeout => Reason::Timeout,
        Cause::Interrupted => Reason::Interrupted,
        Cause::ServiceNotFound => Reason::ServiceNotFound,
        Cause::CancelledByAccount => Reason::CancelledByAccount,
        Cause::ImageNotFound => Reason::ImageNotFound,
        Cause::TagNotFound => Reason::TagNotFound,
        Cause::ImagePrivateOrMissing => Reason::ImagePrivateOrMissing,
        Cause::RegistryUnreachable => Reason::RegistryUnreachable,
        Cause::RegistryRateLimited => Reason::RegistryRateLimited,
        Cause::PlatformMismatch => Reason::PlatformMismatch,
        Cause::DiskFull => Reason::DiskFull,
        Cause::NameTaken => Reason::NameTaken,
        Cause::NotExecuted
        | Cause::RunsWithPrevious
        | Cause::RunsWithNew
        | Cause::IntentWithoutDone
        | Cause::StepFailed
        | Cause::TwoStates
        | Cause::StateUnknown
        | Cause::DecisionExpired
        | Cause::DecidedByAccount
        | Cause::ForcedStop => Reason::Internal,
    }
}

/// Les actions de cycle de vie restées en plein geste quand l'agent s'est arrêté, consignées au journal
/// d'activité (`BR-LIFE-018`, cause « interrompu par un redémarrage de l'agent ») : le compte, le service, le
/// geste. L'agent ne les reprend, ne les rejoue ni ne les défait (`BR-LIFE-020`).
pub async fn record_interrupted(sink: &dyn AuditSink, interrupted: &[InterruptedAction]) {
    for action in interrupted {
        let Some(service) = ClientName::parse(&action.service_name) else {
            continue;
        };
        let actor = crate::domain::accounts::Username::parse(&action.account)
            .ok()
            .map_or_else(Actor::system, |username| {
                Actor::new(Some(username), crate::domain::audit::Origin::System)
            });
        sink.record(
            actor,
            action.kind.audit_action(),
            Target::ServiceLifecycle {
                service,
                note: LifecycleNote::Plain,
            },
            Outcome::Failed(Reason::Interrupted),
        )
        .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_of_the_pilot_is_a_failure_and_an_unknown_result_is_never_one() {
        // BR-LIFE-019 : refusé = échoué, le service n'a pas été modifié ; injoignable ou sans réponse = inconnu.
        let case = |failure: OrderFailure, status, cause| {
            let finish = failure.finish();
            assert_eq!(
                (finish.status, finish.cause),
                (status, Some(cause)),
                "{failure:?}"
            );
            assert!(finish.watch.is_none() && !finish.forced);
        };
        case(
            OrderFailure::Refused,
            OpStatus::Failed,
            Cause::EngineRefused,
        );
        case(OrderFailure::PortInUse, OpStatus::Failed, Cause::PortInUse);
        case(OrderFailure::Gone, OpStatus::Failed, Cause::ServiceNotFound);
        case(
            OrderFailure::Unavailable,
            OpStatus::Failed,
            Cause::EngineUnavailable,
        );
        case(
            OrderFailure::Unreachable,
            OpStatus::Interrupted,
            Cause::EngineUnavailable,
        );
        case(
            OrderFailure::TimedOut,
            OpStatus::Interrupted,
            Cause::Timeout,
        );
    }

    #[test]
    fn every_engine_and_supervisor_error_has_an_outcome() {
        for (error, failure) in [
            (EngineError::NotFound, OrderFailure::Gone),
            (EngineError::Unreachable, OrderFailure::Unreachable),
            (EngineError::Denied, OrderFailure::Unreachable),
            (EngineError::Timeout, OrderFailure::TimedOut),
            (EngineError::PortInUse, OrderFailure::PortInUse),
            (EngineError::Refused, OrderFailure::Refused),
            (EngineError::Unsupported, OrderFailure::Refused),
        ] {
            assert_eq!(OrderFailure::from_engine(error), failure, "{error:?}");
        }
        for (error, failure) in [
            (SupervisorError::Unavailable, OrderFailure::Unavailable),
            (SupervisorError::Refused, OrderFailure::Refused),
            (SupervisorError::Timeout, OrderFailure::TimedOut),
        ] {
            assert_eq!(OrderFailure::from_supervisor(error), failure, "{error:?}");
        }
    }

    #[test]
    fn a_success_carries_the_forced_flag_and_the_watch_that_follows() {
        let plain = Finish::done(false, Watch::Started);
        assert_eq!(
            (plain.status, plain.cause, plain.forced, plain.watch),
            (OpStatus::Succeeded, None, false, Some(Watch::Started))
        );
        let forced = Finish::done(true, Watch::Stopped);
        assert_eq!(
            (forced.status, forced.cause, forced.forced, forced.watch),
            (
                OpStatus::Succeeded,
                Some(Cause::ForcedStop),
                true,
                Some(Watch::Stopped)
            )
        );
    }

    #[test]
    fn every_cause_of_the_lifecycle_has_its_fixed_reason_in_the_activity_log() {
        for (cause, reason) in [
            (Cause::PortInUse, Reason::PortInUse),
            (
                Cause::StoppedRightAfterStart,
                Reason::StoppedRightAfterStart,
            ),
            (Cause::EngineUnavailable, Reason::EngineUnavailable),
            (Cause::EngineRefused, Reason::EngineRefused),
            (Cause::RestartedByHost, Reason::RestartedByHost),
            (Cause::Timeout, Reason::Timeout),
            (Cause::Interrupted, Reason::Interrupted),
            (Cause::ServiceNotFound, Reason::ServiceNotFound),
            (Cause::StepFailed, Reason::Internal),
        ] {
            assert_eq!(reason_of(cause), reason, "{cause:?}");
        }
    }
}

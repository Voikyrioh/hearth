//! Supprimer un service créé par Hearth : le conteneur et sa description partent, les données restent sauf
//! demande explicite (HRT-85, ADR-0045, ADR-0046, BR-DEL-*).
//!
//! **Ce que fait [`ServiceDeletion::delete`], de façon SYNCHRONE** (rien n'est attendu du moteur au-delà d'une
//! lecture) : le moteur est prêt (`BR-DEL-022`), le service existe et n'est pas adopté (`BR-DEL-002`,
//! `BR-DEL-018`), aucune autre action ne le tient (`BR-DEL-021`), son état permet la suppression
//! (`BR-DEL-003`), le nom est retapé quand une case est cochée (`BR-DEL-011`). Puis l'opération `delete` est
//! acceptée (verrou exclusif du service, **nom réservé** : un déploiement du même nom est refusé tant que
//! dure la suppression, `BR-DEL-019`) et l'exécution part en tâche de fond. Réponse : `202 { operation_id }`.
//!
//! **L'exécution** suit le plan figé (`domain::ops::delete`) dans l'ordre de la spec (`BR-DEL-014`) :
//! 1. `stop` : si le service tourne, l'arrêt voulu est écrit AVANT l'ordre, puis l'arrêt propre avec le délai de
//!    grâce du service ; une coupure forcée est dite (`BR-DEL-013`). Annulable jusqu'ici ;
//! 2. `remove_container` (point de non-retour) : `ContainerEngine::remove_keeping_volumes`, qui ne force jamais
//!    et ne touche à aucun volume ; échec ou refus = le service n'est pas supprimé, sa description et ses
//!    données sont intactes (`BR-DEL-015`) ;
//! 3. `forget` : la description, ses valeurs scellées, l'arrêt voulu et les conteneurs consignés sont oubliés
//!    (`BR-DEL-004`), le suivi des phases d'un jeu aussi (HRT-197) ;
//! 4. `erase` (seulement si la case « données » est cochée) : les dossiers gérés par Hearth, un par un, JAMAIS
//!    avant le conteneur, jamais un dossier du serveur, jamais un dossier qu'un autre conteneur monte
//!    (`BR-DEL-006` à `BR-DEL-008`, rejugé juste avant d'effacer). Mesure bornée à 10 s, effacement à faible
//!    priorité, avec suivi (`BR-DEL-030`).
//!
//! Chaque étape qui détruit écrit son `intent` avant le geste : un agent qui redémarre au milieu laisse une
//! opération « à vérifier », nom réservé, **jamais reprise** (`BR-DEL-025`, `recover`). L'effacement ne se
//! rejoue pas : le dossier a pu être réutilisé entre-temps.
//!
//! **Jamais** : un chemin, un nom de fichier, une variable ou une commande dans une trace, une erreur ou le
//! journal d'activité (`BR-DEL-029`) ; un volume supprimé ; un conteneur coupé de force sans l'avoir dit.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use hearth_proto::api::service_delete::{
    DeleteAccepted, DeletePreview, DeleteRequest, FolderFate, FolderPreview, MEASURE_BUDGET_S,
};
use thiserror::Error;
use tokio::sync::Mutex as AsyncMutex;
use tokio::time::Instant;

use super::game_phase::GamePhases;
use super::ports::{
    Clock, ContainerEngine, EngineError, Erased, FsError, HostPaths, Measure, Resolved, SafeFs,
    ServiceContainers, ServiceIntents, StopIntent,
};
use super::service_lifecycle::reason_of;
use super::service_ops::{AcceptRequest, AuditDraft, OpError, ServiceOps};
use super::service_specs::ServiceSpecs;
use super::services::{Driver, LifecycleTarget, ServicesRegistry};
use crate::application::ports::Progress;
use crate::domain::audit::{Actor, ClientName, DeleteNote, ErasedData, Outcome, Reason, Target};
use crate::domain::files::RelPath;
use crate::domain::ops::delete::{op_plan, step};
use crate::domain::ops::{
    Cause, Holder, OpKind, OpStatus, Refusal, StopEvidence, UndoInfo, WAIT_FLOOR, WAIT_MARGIN,
    stop_was_forced, wait_budget_with,
};
use crate::domain::services::delete::{
    Fate, FileId, FolderUse, ManagedFolder, MountSplit, OwnFolder, fate_of, split_mounts,
};
use crate::domain::services::plan::image_reference;
use crate::domain::services::spec::{SpecBody, fold_name};
use crate::domain::services::{
    EngineState, KeyKind, RunState, ServiceKey, ServiceStateKind, TransitionKind,
};

/// Les durées d'une suppression. Les tests les raccourcissent.
#[derive(Debug, Clone, Copy)]
pub struct DeleteTimings {
    /// Le budget de mesure des dossiers, tous ensemble (`BR-DEL-009`).
    pub measure_budget: Duration,
    /// La marge et le plancher de l'attente d'un arrêt (`BR-LIFE-021`).
    pub stop_margin: Duration,
    pub stop_floor: Duration,
    /// Combien attendre au plus la suppression du conteneur par le moteur.
    pub remove_wait: Duration,
    /// L'intervalle entre deux signes de vie de l'effacement.
    pub progress_every: Duration,
    /// Combien de temps au plus pour lire les chemins qui jugent un dossier (liens, parents, montages des autres
    /// conteneurs) : un partage réseau mort ne fait jamais pendre la demande, le dossier est alors conservé.
    pub resolve_budget: Duration,
}

impl DeleteTimings {
    pub const fn production() -> Self {
        Self {
            measure_budget: Duration::from_secs(MEASURE_BUDGET_S),
            stop_margin: WAIT_MARGIN,
            stop_floor: WAIT_FLOOR,
            remove_wait: Duration::from_secs(60),
            progress_every: Duration::from_millis(500),
            resolve_budget: Duration::from_secs(10),
        }
    }
}

/// Ce dont le cas d'usage a besoin de la machine.
pub struct DeleteContext {
    /// `<données de l'agent>/services` : sous lui, un sous-dossier par service.
    pub services_root: String,
    pub timings: DeleteTimings,
}

/// Pourquoi une demande est refusée tout de suite, avant que rien ne soit ordonné.
#[derive(Debug, Error)]
pub enum DeleteError {
    #[error("le moteur n'est pas prêt")]
    EngineUnavailable(EngineState),
    /// Aucun service de cette clé que Hearth connaisse (`BR-DEL-018`).
    #[error("service inconnu")]
    NotFound,
    /// Hearth ne supprime jamais un service créé hors de Hearth (`BR-DEL-002`).
    #[error("service adopté")]
    Adopted,
    /// Une autre action tient le service (`BR-DEL-021`) : qui, et laquelle.
    #[error("une action est déjà en cours")]
    Busy { by: String, kind: OpKind },
    /// L'état ne permet pas la suppression (`BR-DEL-003`).
    #[error("état incompatible")]
    NotDeletable(ServiceStateKind),
    /// Le nom retapé n'est pas celui du service (`BR-DEL-011`).
    #[error("nom retapé faux")]
    NameMismatch,
    /// Les sauvegardes n'existent pas encore (HRT-97) : cocher la case est refusé, jamais ignoré.
    #[error("sauvegardes indisponibles")]
    BackupsUnavailable,
    #[error("erreur interne : {0}")]
    Internal(String),
}

/// Un refus, avec le nom du service quand il est publié (il va au journal d'activité : le service est déjà
/// dans la liste de tout compte, `BR-DEL-002`).
#[derive(Debug)]
pub struct DeleteFailed {
    pub error: DeleteError,
    pub name: Option<String>,
}

impl DeleteFailed {
    fn plain(error: DeleteError) -> Self {
        Self { error, name: None }
    }

    /// Une clé que Hearth ne sait pas lire est un service inconnu (`BR-ADOPT-004`).
    pub fn not_found() -> Self {
        Self::plain(DeleteError::NotFound)
    }

    fn named(error: DeleteError, name: &str) -> Self {
        Self {
            error,
            name: Some(name.to_owned()),
        }
    }
}

/// Ce que l'exécution a décidé de son issue.
#[derive(Debug)]
struct Finish {
    status: OpStatus,
    cause: Option<Cause>,
    note: DeleteNote,
}

impl Finish {
    fn failed(cause: Cause, note: DeleteNote) -> Self {
        Self {
            status: OpStatus::Failed,
            cause: Some(cause),
            note,
        }
    }

    /// Le résultat est inconnu (le moteur n'a pas répondu à l'ordre) : le compte relit l'état réel.
    fn unknown(cause: Cause, note: DeleteNote) -> Self {
        Self {
            status: OpStatus::Interrupted,
            cause: Some(cause),
            note,
        }
    }

    fn cancelled(note: DeleteNote) -> Self {
        Self {
            status: OpStatus::Cancelled,
            cause: Some(Cause::CancelledByAccount),
            note,
        }
    }

    fn done(left: u64, note: DeleteNote) -> Self {
        Self {
            status: OpStatus::Succeeded,
            cause: (left > 0).then_some(Cause::DataNotErased),
            note,
        }
    }
}

/// Un dossier à effacer, décidé à l'acceptation (rejugé avant l'effacement).
#[derive(Debug, Clone)]
struct EraseTarget {
    folder: ManagedFolder,
}

/// Tout ce qu'une exécution emporte avec elle. Aucun secret.
struct Job {
    id: String,
    key: ServiceKey,
    name: String,
    actor: Actor,
    containers: Vec<String>,
    grace_s: u32,
    erase: Vec<EraseTarget>,
}

pub struct ServiceDeletion {
    registry: Arc<ServicesRegistry>,
    ops: Arc<ServiceOps>,
    specs: Arc<ServiceSpecs>,
    engine: Arc<dyn ContainerEngine>,
    containers: Arc<dyn ServiceContainers>,
    intents: Arc<dyn ServiceIntents>,
    safe_fs: Arc<dyn SafeFs>,
    host_paths: Arc<dyn HostPaths>,
    phases: Arc<GamePhases>,
    clock: Arc<dyn Clock>,
    context: DeleteContext,
    /// L'agent s'arrête : un effacement en cours s'arrête au lot suivant et reste « à vérifier » (comme après un
    /// redémarrage), au lieu de faire attendre l'arrêt jusqu'à la fin ou au `SIGKILL`.
    stopping: Arc<AtomicBool>,
    /// Une mesure à la fois pour tout l'agent (conception technique 9.3).
    measure_gate: AsyncMutex<()>,
}

/// Ce qu'une demande a établi du service visé.
struct Subject {
    name: String,
    body: Option<SpecBody>,
    target: Option<LifecycleTarget>,
    containers: Vec<String>,
}

impl ServiceDeletion {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        registry: Arc<ServicesRegistry>,
        ops: Arc<ServiceOps>,
        specs: Arc<ServiceSpecs>,
        engine: Arc<dyn ContainerEngine>,
        containers: Arc<dyn ServiceContainers>,
        intents: Arc<dyn ServiceIntents>,
        safe_fs: Arc<dyn SafeFs>,
        host_paths: Arc<dyn HostPaths>,
        phases: Arc<GamePhases>,
        clock: Arc<dyn Clock>,
        context: DeleteContext,
    ) -> Arc<Self> {
        Arc::new(Self {
            registry,
            ops,
            specs,
            engine,
            containers,
            intents,
            safe_fs,
            host_paths,
            phases,
            clock,
            context,
            stopping: Arc::new(AtomicBool::new(false)),
            measure_gate: AsyncMutex::new(()),
        })
    }

    /// L'agent s'arrête (appelé à l'arrêt du serveur) : les effacements en cours s'arrêtent au lot suivant, leur
    /// opération reste en cours dans le journal et sera « à vérifier » au prochain démarrage, jamais reprise.
    pub fn begin_shutdown(&self) {
        self.stopping.store(true, Ordering::SeqCst);
    }

    // -----------------------------------------------------------------------------------------
    // Ce qui se juge sans rien attendre du moteur
    // -----------------------------------------------------------------------------------------

    /// Le moteur est prêt, le service existe et n'est pas adopté. Ne dit rien d'un service que Hearth ne connaît
    /// pas : `NotFound` (`BR-ADOPT-004`).
    async fn subject(&self, key: &ServiceKey) -> Result<Subject, DeleteFailed> {
        let engine_state = self.registry.engine_state();
        if engine_state != EngineState::Ready {
            return Err(DeleteFailed::plain(DeleteError::EngineUnavailable(
                engine_state,
            )));
        }
        let target = self.registry.lifecycle_target(key);
        if key.kind() != KeyKind::Hearth {
            // Un service publié qui n'est pas créé par Hearth est un service adopté : jamais supprimé.
            return Err(match target {
                Some(target) => DeleteFailed::named(DeleteError::Adopted, &target.name),
                None => DeleteFailed::plain(DeleteError::NotFound),
            });
        }
        let body = self
            .specs
            .body(key)
            .await
            .map_err(|error| DeleteFailed::plain(DeleteError::Internal(error.to_string())))?;
        let name = match (&target, &body) {
            (Some(target), _) => target.name.clone(),
            (None, Some(body)) => body.name.clone(),
            // Ni conteneur ni description : Hearth ne connaît plus ce service (`BR-DEL-018`).
            (None, None) => return Err(DeleteFailed::plain(DeleteError::NotFound)),
        };
        let mut containers = self
            .containers
            .of(key)
            .await
            .map_err(|error| DeleteFailed::plain(DeleteError::Internal(error.to_string())))?;
        if let Some(target) = &target
            && let Driver::Engine { container_id } = &target.driver
            && !containers.contains(container_id)
        {
            containers.push(container_id.clone());
        }
        Ok(Subject {
            name,
            body,
            target,
            containers,
        })
    }

    fn busy_of(&self, key: &ServiceKey) -> Option<DeleteError> {
        self.ops
            .busy(key)
            .map(|(by, kind)| DeleteError::Busy { by, kind })
    }

    /// Les dossiers du service rangés, et le sort de chacun d'après ce que les autres conteneurs montent.
    async fn judge_folders(&self, key: &ServiceKey, body: Option<&SpecBody>) -> Judged {
        let Some(body) = body else {
            return Judged::default();
        };
        let split = split_mounts(body, &self.context.services_root, key.as_str());
        let mut fates = Vec::with_capacity(split.managed.len());
        for folder in &split.managed {
            fates.push(self.fate_now(key, &folder.host_path).await);
        }
        Judged { split, fates }
    }

    /// Le sort d'un dossier géré MAINTENANT : la liste des autres conteneurs est lue à cet instant ; chemins
    /// canoniques, identité (périphérique, inode) du dossier ET de tous ses parents, des deux côtés, dans les deux
    /// sens (parent monté, enfant monté) ; au moindre doute (liste absente ou pas relue, chemin qu'on ne peut pas
    /// lire, lecture trop longue), le dossier est conservé (BR-DEL-008).
    async fn fate_now(&self, key: &ServiceKey, host_path: &str) -> Fate {
        let users = self.registry.folder_users(key);
        let paths = Arc::clone(&self.host_paths);
        let host_path = host_path.to_owned();
        let work = tokio::task::spawn_blocking(move || {
            let own = see(paths.as_ref(), &host_path, true);
            if own.unreadable {
                return Fate::Unverifiable;
            }
            let own = OwnFolder {
                paths: own.names,
                lineage: own.lineage,
            };
            let resolved: Option<Vec<FolderUse>> = users.map(|list| {
                list.into_iter()
                    .map(|mut other| {
                        let (mut sources, mut identities, mut ancestry) =
                            (Vec::new(), Vec::new(), Vec::new());
                        for source in &other.sources {
                            let seen = see(paths.as_ref(), source, false);
                            sources.extend(seen.names);
                            identities.extend(seen.lineage.first().copied());
                            ancestry.extend(seen.lineage);
                            if seen.unreadable {
                                // Un montage qu'on ne sait pas lire : on ne peut pas dire qu'il est ailleurs.
                                other.complete = false;
                                break;
                            }
                        }
                        other.sources = sources;
                        other.identities = identities;
                        other.ancestry = ancestry;
                        other
                    })
                    .collect()
            });
            fate_of(&own, resolved.as_deref())
        });
        match tokio::time::timeout(self.context.timings.resolve_budget, work).await {
            Ok(Ok(fate)) => fate,
            _ => Fate::Unverifiable,
        }
    }

    // -----------------------------------------------------------------------------------------
    // La fenêtre « Supprimer {nom} ? »
    // -----------------------------------------------------------------------------------------

    /// De quoi bâtir la fenêtre : les dossiers gérés, leur taille (10 secondes en tout), leur sort. Une consultation :
    /// rien n'est écrit.
    pub async fn preview(&self, key: &ServiceKey) -> Result<DeletePreview, DeleteFailed> {
        let subject = self.subject(key).await?;
        let judged = self.judge_folders(key, subject.body.as_ref()).await;
        let deadline = Instant::now() + self.context.timings.measure_budget;
        let mut in_time = true;
        let mut folders =
            Vec::with_capacity(judged.split.managed.len() + judged.split.unmanaged.len());
        for (folder, fate) in judged.split.managed.iter().zip(&judged.fates) {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let measure = if remaining.is_zero() {
                in_time = false;
                Some(Measure {
                    partial: true,
                    ..Measure::default()
                })
            } else {
                self.measure(key, &folder.rel, remaining).await
            };
            let (measured, unmeasured) = match measure {
                Some(measure) => (measure, false),
                None => (Measure::default(), true),
            };
            if measured.partial {
                in_time = false;
            }
            let (fate, shared_with, unmanaged) = match fate {
                Fate::Erase => (FolderFate::Erasable, Vec::new(), false),
                Fate::Shared {
                    services,
                    unmanaged,
                } => (FolderFate::KeptShared, services.clone(), *unmanaged),
                Fate::Unverifiable => (FolderFate::KeptUnverifiable, Vec::new(), false),
            };
            folders.push(FolderPreview {
                path: folder.host_path.clone(),
                bytes: measured.bytes_on_disk,
                items: measured.items,
                partial: measured.partial,
                unmeasured,
                fate,
                shared_with,
                shared_with_unmanaged: unmanaged,
            });
        }
        for path in &judged.split.unmanaged {
            folders.push(FolderPreview {
                path: path.clone(),
                bytes: 0,
                items: 0,
                partial: false,
                unmeasured: true,
                fate: FolderFate::KeptOutsideRoot,
                shared_with: Vec::new(),
                shared_with_unmanaged: false,
            });
        }
        let (running, grace_s) = match &subject.target {
            Some(target) => (
                matches!(
                    target.state,
                    ServiceStateKind::Running | ServiceStateKind::RestartLoop
                ),
                target.grace_s,
            ),
            None => (false, 0),
        };
        Ok(DeletePreview {
            name: subject.name,
            image: subject
                .body
                .as_ref()
                .map(|body| image_reference(&body.image))
                .unwrap_or_default(),
            running,
            stop_grace_s: grace_s,
            container_missing: subject.target.is_none(),
            folders,
            server_folders: judged.split.server_folders,
            volumes: self.registry.volume_count(key),
            measured_in_time: in_time,
            backups: 0,
        })
    }

    /// Mesure un dossier géré dans le temps qui reste. `None` : le dossier ou la racine n'existe plus, ou ne se
    /// lit pas.
    async fn measure(&self, key: &ServiceKey, rel: &RelPath, budget: Duration) -> Option<Measure> {
        // Une mesure à la fois pour tout l'agent, aussi bien pour la fenêtre que pour la taille journalisée.
        let _one_measure_at_a_time = self.measure_gate.lock().await;
        let fs = Arc::clone(&self.safe_fs);
        let (key, rel) = (key.clone(), rel.clone());
        let result =
            tokio::task::spawn_blocking(move || fs.open_service_root(&key)?.measure(&rel, budget))
                .await;
        match result {
            Ok(Ok(measure)) => Some(measure),
            Ok(Err(_)) | Err(_) => None,
        }
    }

    // -----------------------------------------------------------------------------------------
    // La demande
    // -----------------------------------------------------------------------------------------

    /// Accepte (ou refuse tout de suite) une suppression. Le droit de gérer les services et la confirmation de
    /// l'acte ont déjà été jugés.
    pub async fn delete(
        self: &Arc<Self>,
        key: &ServiceKey,
        request: DeleteRequest,
        actor: Actor,
        op_key: &str,
    ) -> Result<DeleteAccepted, DeleteFailed> {
        // Une clôture en cours sur ce service (commit, verrous, projection, annonce) se termine avant que l'on juge
        // « occupé » : une seule attente par demande, partagée avec `accept_after_wait`.
        self.ops.wait_closed(key).await;
        let subject = match self.subject(key).await {
            Ok(subject) => subject,
            // Une suppression en cours a déjà oublié le service (description et conteneur) mais tient encore son
            // verrou jusqu'à la fin de l'effacement : la seconde demande reçoit « une action est en cours », pas
            // « ce service n'existe plus » (BR-DEL-021).
            Err(failed) if matches!(failed.error, DeleteError::NotFound) => {
                return Err(match self.busy_of(key) {
                    Some(busy) => DeleteFailed::plain(busy),
                    None => failed,
                });
            }
            Err(failed) => return Err(failed),
        };
        let named = |error| DeleteFailed::named(error, &subject.name);
        // Dans l'ordre de la spec : l'action en cours avant l'état (`BR-DEL-021`, `BR-DEL-003`).
        if let Some(busy) = self.busy_of(key) {
            return Err(named(busy));
        }
        if let Some(target) = &subject.target
            && !deletable(target.state)
        {
            return Err(named(DeleteError::NotDeletable(target.state)));
        }
        // Les sauvegardes n'existent pas encore : une demande qui les coche n'est jamais ignorée en silence.
        if request.erase_backups {
            return Err(named(DeleteError::BackupsUnavailable));
        }
        // Une case cochée, le nom retapé, casse comprise (`BR-DEL-011`), même si le client est modifié.
        if request.erase_data && request.confirm_name.as_deref() != Some(subject.name.as_str()) {
            return Err(named(DeleteError::NameMismatch));
        }

        let judged = self.judge_folders(key, subject.body.as_ref()).await;
        let erase: Vec<EraseTarget> = if request.erase_data {
            judged
                .split
                .managed
                .iter()
                .zip(&judged.fates)
                .filter(|(_, fate)| **fate == Fate::Erase)
                .map(|(folder, _)| EraseTarget {
                    folder: folder.clone(),
                })
                .collect()
        } else {
            Vec::new()
        };
        let plan = op_plan(!erase.is_empty())
            .map_err(|error| named(DeleteError::Internal(error.to_string())))?;
        let account = account_of(&actor);
        let id = self
            .ops
            .accept_after_wait(AcceptRequest {
                op_key: op_key.to_owned(),
                service: key.clone(),
                service_name: subject.name.clone(),
                kind: OpKind::Delete,
                account: account.clone(),
                plan,
                forced: false,
                reserve_name: Some(fold_name(&subject.name)),
                heavy_io: false,
            })
            .await
            .map_err(|error| match error {
                // Deux demandes simultanées : la seconde n'a pas vu la première dans l'index.
                OpError::Locked(Refusal::ServiceBusy { holder }) => named(DeleteError::Busy {
                    by: self.holder_account(key),
                    kind: match holder {
                        Holder::Operation(kind) => kind,
                        Holder::FileWrites => OpKind::Delete,
                    },
                }),
                OpError::ServiceBusy => named(DeleteError::Busy {
                    by: self.holder_account(key),
                    kind: OpKind::Delete,
                }),
                other => named(DeleteError::Internal(other.to_string())),
            })?;
        if let Err(error) = self.ops.start(&id).await {
            journal(
                self.ops
                    .finish(&id, OpStatus::Failed, Some(Cause::StepFailed))
                    .await,
                "finish",
            );
            return Err(named(DeleteError::Internal(error.to_string())));
        }
        let job_actor = actor.clone();
        let job = Job {
            id: id.clone(),
            key: key.clone(),
            name: subject.name.clone(),
            actor,
            containers: subject.containers,
            grace_s: subject.target.as_ref().map_or(10, |target| target.grace_s),
            erase,
        };
        let this = Arc::clone(self);
        let work = tokio::spawn(async move { this.execute(job).await });
        // Une panique de l'exécution ne laisse jamais une opération « en cours » : elle est close, le verrou rendu.
        let watcher = Arc::clone(self);
        let (watched, name, actor_of_job) = (id.clone(), subject.name.clone(), job_actor);
        tokio::spawn(async move {
            if work.await.is_err() {
                tracing::error!(operation = %watched, "exécution d'une suppression interrompue par une panique");
                watcher
                    .close_after_panic(&watched, &name, actor_of_job)
                    .await;
            }
        });
        Ok(DeleteAccepted { operation_id: id })
    }

    fn holder_account(&self, key: &ServiceKey) -> String {
        self.ops.busy(key).map(|(by, _)| by).unwrap_or_default()
    }

    // -----------------------------------------------------------------------------------------
    // L'exécution
    // -----------------------------------------------------------------------------------------

    async fn execute(&self, job: Job) {
        let started = Instant::now();
        let mut note = DeleteNote::default();
        let Some(finish) = self.run(&job, &mut note).await else {
            // L'agent s'arrête au milieu de l'effacement : l'opération n'est pas close, son `intent` sans `done`
            // en fait une opération « à vérifier » au prochain démarrage (BR-DEL-025), jamais reprise.
            tracing::info!(operation = %job.id, "effacement arrêté par l'arrêt de l'agent : à vérifier au prochain démarrage");
            return;
        };
        self.settle(&job, finish, started.elapsed()).await;
    }

    async fn is_cancelling(&self, id: &str) -> bool {
        matches!(
            self.ops.snapshot(id).await,
            Ok(Some(snapshot)) if snapshot.record.status == OpStatus::Cancelling
        )
    }

    /// Écrit l'`intent` d'une étape. `Err` : l'issue à rendre (annulée si une annulation est passée avant,
    /// sinon un échec).
    async fn begin(&self, job: &Job, name: &str, note: DeleteNote) -> Result<(), Finish> {
        match self.ops.intent(&job.id, name, &UndoInfo::default()).await {
            Ok(_) => Ok(()),
            Err(OpError::Stale | OpError::OutOfOrder) if self.is_cancelling(&job.id).await => {
                Err(Finish::cancelled(note))
            }
            Err(error) => {
                tracing::error!(%error, step = name, "journal de la suppression illisible : l'étape n'est pas lancée");
                Err(Finish::failed(Cause::StepFailed, note))
            }
        }
    }

    /// `None` : l'effacement s'est arrêté avec l'agent, rien n'est clos.
    async fn run(&self, job: &Job, note: &mut DeleteNote) -> Option<Finish> {
        // 1. Arrêt, si le service tourne (BR-DEL-013).
        if let Err(finish) = self.begin(job, step::STOP, *note).await {
            return Some(finish);
        }
        if let Err(finish) = self.stop(job, note).await {
            return Some(finish);
        }
        journal(
            self.ops
                .done(&job.id, step::STOP, &UndoInfo::default())
                .await,
            "done",
        );
        if self.is_cancelling(&job.id).await {
            return Some(Finish::cancelled(*note));
        }

        // 2. Le conteneur : le point de non-retour (BR-DEL-014).
        if let Err(finish) = self.begin(job, step::REMOVE_CONTAINER, *note).await {
            return Some(finish);
        }
        if let Err(finish) = self.remove_containers(job, *note).await {
            return Some(finish);
        }
        journal(
            self.ops
                .done(&job.id, step::REMOVE_CONTAINER, &UndoInfo::default())
                .await,
            "done",
        );

        // 3. La description et tout ce qui s'y rattache (BR-DEL-004).
        if let Err(finish) = self.begin(job, step::FORGET, *note).await {
            return Some(finish);
        }
        if let Err(finish) = self.forget(job, *note).await {
            return Some(finish);
        }
        journal(
            self.ops
                .done(&job.id, step::FORGET, &UndoInfo::default())
                .await,
            "done",
        );

        // 4. Les données, APRÈS le conteneur, si la case était cochée (BR-DEL-014).
        let mut left = 0;
        if !job.erase.is_empty() {
            if let Err(finish) = self.begin(job, step::ERASE, *note).await {
                return Some(finish);
            }
            left = self.erase(job, note).await?;
            journal(
                self.ops
                    .done(&job.id, step::ERASE, &UndoInfo::default())
                    .await,
                "done",
            );
        }
        note.left = left;
        Some(Finish::done(left, *note))
    }

    /// Arrête les conteneurs du service qui tournent : l'arrêt voulu est écrit AVANT l'ordre (`BR-LIFE-010`), le
    /// délai de grâce est celui du service, une coupure forcée est dite (`BR-DEL-013`).
    async fn stop(&self, job: &Job, note: &mut DeleteNote) -> Result<(), Finish> {
        let mut running = Vec::new();
        for id in &job.containers {
            match self.engine.inspect(id).await {
                Ok(Some(observation)) => {
                    if matches!(
                        observation.run_state,
                        RunState::Running | RunState::Restarting | RunState::Paused
                    ) {
                        running.push(id.clone());
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    journal(self.ops.failed(&job.id, step::STOP).await, "failed");
                    return Err(Finish::failed(cause_of(error), *note));
                }
            }
        }
        if running.is_empty() {
            return Ok(());
        }
        let intent = StopIntent {
            key: job.key.clone(),
            container_id: running.first().cloned().unwrap_or_default(),
            account: account_of(&job.actor),
            at: self.clock.now(),
        };
        if let Err(error) = self.intents.save(&intent).await {
            tracing::error!(%error, "arrêt voulu non écrit : l'ordre d'arrêt n'est pas donné");
            journal(self.ops.failed(&job.id, step::STOP).await, "failed");
            return Err(Finish::failed(Cause::StepFailed, *note));
        }
        self.registry.note_stop_requested(&job.key, intent.at);
        self.registry
            .begin_transition(&job.key, TransitionKind::Stopping);
        let _shown = StepShown {
            registry: &self.registry,
            key: job.key.clone(),
        };
        let grace = Duration::from_secs(u64::from(job.grace_s));
        let wait = wait_budget_with(
            job.grace_s,
            self.context.timings.stop_margin,
            self.context.timings.stop_floor,
        );
        for id in &running {
            let started = Instant::now();
            let ordered = match tokio::time::timeout(wait, self.engine.stop(id, grace)).await {
                Ok(result) => result,
                Err(_) => Err(EngineError::Timeout),
            };
            match ordered {
                Ok(()) | Err(EngineError::NotFound) => {
                    let evidence = match self.engine.inspect(id).await {
                        Ok(Some(observation)) => StopEvidence::ExitCode {
                            code: observation.exit_code,
                            waited: started.elapsed(),
                        },
                        _ => StopEvidence::Took(started.elapsed()),
                    };
                    if stop_was_forced(evidence, job.grace_s) {
                        note.forced_stop = true;
                        journal(self.ops.mark_forced(&job.id).await, "forced");
                    }
                }
                Err(error) => {
                    journal(self.ops.failed(&job.id, step::STOP).await, "failed");
                    // Un refus net : rien n'a été arrêté, l'arrêt voulu n'a pas lieu d'être. Un résultat inconnu le
                    // garde : l'arrêt a peut-être eu lieu.
                    if matches!(error, EngineError::Refused | EngineError::Denied) {
                        self.forget_stop(&job.key).await;
                    }
                    return Err(Finish::failed(cause_of(error), *note));
                }
            }
        }
        for id in &running {
            self.registry.reread(id).await;
        }
        Ok(())
    }

    /// Supprime les conteneurs du service, sans toucher à un volume ni forcer (`remove_keeping_volumes`). Un
    /// refus ou un échec : le service n'est pas supprimé (`BR-DEL-015`).
    async fn remove_containers(&self, job: &Job, note: DeleteNote) -> Result<(), Finish> {
        for id in &job.containers {
            let removed = tokio::time::timeout(
                self.context.timings.remove_wait,
                self.engine.remove_keeping_volumes(id),
            )
            .await
            .unwrap_or(Err(EngineError::Timeout));
            match removed {
                Ok(()) | Err(EngineError::NotFound) => {
                    if let Err(error) = self.containers.forget_container(id).await {
                        tracing::warn!(%error, "conteneur supprimé non oublié");
                    }
                    self.registry.forget_hearth_container(id);
                }
                Err(error) => {
                    journal(
                        self.ops.failed(&job.id, step::REMOVE_CONTAINER).await,
                        "failed",
                    );
                    // Le moteur ne répond plus : l'ordre est peut-être parti, le résultat est inconnu. Tout le
                    // reste (description, données) est intact dans les deux cas.
                    return Err(
                        if matches!(error, EngineError::Unreachable | EngineError::Timeout) {
                            Finish::unknown(cause_of(error), note)
                        } else {
                            Finish::failed(cause_of(error), note)
                        },
                    );
                }
            }
        }
        Ok(())
    }

    /// Oublie la description (variables, commande et leurs valeurs scellées), les conteneurs consignés, l'arrêt
    /// voulu et le suivi des phases d'un jeu (`BR-DEL-004`, HRT-197).
    async fn forget(&self, job: &Job, note: DeleteNote) -> Result<(), Finish> {
        if let Err(error) = self.specs.forget(&job.key).await {
            tracing::error!(%error, "description du service supprimé non effacée");
            journal(self.ops.failed(&job.id, step::FORGET).await, "failed");
            return Err(Finish::failed(Cause::StepFailed, note));
        }
        if let Err(error) = self.containers.forget_service(&job.key).await {
            tracing::warn!(%error, "conteneurs consignés du service supprimé non oubliés");
        }
        self.forget_stop(&job.key).await;
        self.phases.forget(&job.key);
        Ok(())
    }

    async fn forget_stop(&self, key: &ServiceKey) {
        if let Err(error) = self.intents.clear(key).await {
            tracing::warn!(%error, "arrêt voulu non effacé");
        }
        self.registry.forget_stop_requested(key);
    }

    /// Efface les dossiers gérés un par un (`BR-DEL-006` à `BR-DEL-008`). Rend le nombre d'éléments qui n'ont pas
    /// pu être effacés, ou `None` si l'agent s'est arrêté en cours de route. Le sort de chaque dossier est rejugé juste avant : un conteneur créé entre-temps qui le
    /// monte le fait conserver.
    async fn erase(&self, job: &Job, note: &mut DeleteNote) -> Option<u64> {
        let timings = self.context.timings;
        let deadline = Instant::now() + timings.measure_budget;
        let mut left = 0_u64;
        let mut base = 0_u64;
        let mut folders = 0_u32;
        let mut bytes = 0_u64;
        let mut partial = false;
        let total = u64::try_from(job.erase.len()).unwrap_or(u64::MAX);
        let mut halted = false;
        for (index, target) in job.erase.iter().enumerate() {
            if self.stopping.load(Ordering::SeqCst) {
                halted = true;
                break;
            }
            if self.fate_now(&job.key, &target.folder.host_path).await != Fate::Erase {
                // Devenu partagé (ou impossible à vérifier) depuis la fenêtre : conservé, et dit comme ce qui
                // reste (`data_not_erased`), jamais passé sous silence.
                tracing::warn!(
                    "dossier d'un service supprimé conservé : devenu partagé ou non vérifiable"
                );
                left = left.saturating_add(1);
                continue;
            }
            // La taille d'avant l'effacement, pour le journal (`BR-DEL-029`), dans le budget qui reste.
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                partial = true;
            } else {
                match self.measure(&job.key, &target.folder.rel, remaining).await {
                    Some(measure) => {
                        bytes = bytes.saturating_add(measure.bytes_on_disk);
                        partial |= measure.partial;
                    }
                    None => partial = true,
                }
            }
            self.report(job, index, total, base).await;
            match self
                .erase_one(job, &target.folder.rel, index, total, base)
                .await
            {
                Ok(erased) => {
                    base = base.saturating_add(erased.removed);
                    left = left.saturating_add(erased.left);
                    folders += 1;
                    if erased.stopped {
                        halted = true;
                        break;
                    }
                    if let Some(error) = erased.first_error {
                        tracing::warn!(
                            ?error,
                            left = erased.left,
                            "données d'un service supprimé non effacées en entier"
                        );
                    }
                }
                // Le dossier n'existe plus : rien à effacer, pas une erreur (`BR-DEL-017`).
                Err(FsError::NotFound) => {}
                Err(error) => {
                    tracing::warn!(?error, "dossier d'un service supprimé non effacé");
                    left = left.saturating_add(1);
                }
            }
        }
        note.erased = Some(ErasedData {
            folders,
            bytes,
            partial,
        });
        if halted {
            return None;
        }
        // La racine du service ne part que si plus rien n'y est : un dossier conservé la garde (`BR-DEL-008`).
        if left == 0 {
            let fs = Arc::clone(&self.safe_fs);
            let key = job.key.clone();
            match tokio::task::spawn_blocking(move || fs.remove_empty_service_root(&key)).await {
                Ok(Ok(_)) => {}
                Ok(Err(error)) => tracing::warn!(?error, "racine du service supprimé non retirée"),
                Err(error) => tracing::warn!(%error, "retrait de la racine interrompu"),
            }
        }
        Some(left)
    }

    /// Efface un dossier sur un fil à part, avec un signe de vie régulier dans l'opération (`BR-DEL-030`).
    async fn erase_one(
        &self,
        job: &Job,
        rel: &RelPath,
        index: usize,
        total: u64,
        base: u64,
    ) -> Result<Erased, FsError> {
        let seen = Arc::new(AtomicU64::new(0));
        let (fs, key, rel, counter, stopping) = (
            Arc::clone(&self.safe_fs),
            job.key.clone(),
            rel.clone(),
            Arc::clone(&seen),
            Arc::clone(&self.stopping),
        );
        // Le geste tourne sur un fil créé pour lui (`erase_tree`) ; ce fil de tâche bloquante l'attend. Le suivi
        // rend `false` quand l'agent s'arrête : l'effacement s'arrête au lot suivant.
        let mut work = tokio::task::spawn_blocking(move || {
            let root = fs.open_service_root(&key)?;
            root.erase_tree(&rel, &mut |removed| {
                counter.store(removed, Ordering::Relaxed);
                !stopping.load(Ordering::SeqCst)
            })
        });
        loop {
            tokio::select! {
                finished = &mut work => {
                    return finished.unwrap_or(Err(FsError::Io(std::io::ErrorKind::Other)));
                }
                () = tokio::time::sleep(self.context.timings.progress_every) => {
                    self.report(job, index, total, base + seen.load(Ordering::Relaxed)).await;
                }
            }
        }
    }

    /// Le nombre d'éléments effacés jusqu'ici : ce que la bande « Opérations en cours » montre. Jamais un chemin.
    async fn report(&self, job: &Job, index: usize, total: u64, items: u64) {
        let percent = u8::try_from((u64::try_from(index).unwrap_or(0) * 100) / total.max(1)).ok();
        journal(
            self.ops
                .set_progress(
                    &job.id,
                    &Progress {
                        percent,
                        items_done: Some(items),
                        ..Progress::default()
                    },
                )
                .await,
            "progress",
        );
    }

    // -----------------------------------------------------------------------------------------
    // La fin
    // -----------------------------------------------------------------------------------------

    async fn settle(&self, job: &Job, finish: Finish, took: Duration) {
        // Une annulation arrivée pendant que l'étape échouait gagne : un statut « en annulation » ne devient
        // jamais « échoué » (diagramme d'états, ADR-0045).
        let finish = if finish.status == OpStatus::Failed && self.is_cancelling(&job.id).await {
            Finish::cancelled(finish.note)
        } else {
            finish
        };
        // L'issue et l'entrée du journal d'activité sont écrites dans UNE transaction, puis annoncées.
        match self
            .ops
            .finish_with(
                &job.id,
                finish.status,
                finish.cause,
                self.draft(job, &finish),
            )
            .await
        {
            Ok(()) => {}
            Err(OpError::IllegalTransition | OpError::Stale) => {
                tracing::info!(
                    operation = %job.id,
                    "suppression non close : l'opération a changé de statut entre-temps"
                );
                // Si l'autre changement l'a rendue terminale sans rendre ses verrous, ils sont rendus ici.
                self.ops.settle(&job.id, None).await;
            }
            Err(error) => {
                tracing::error!(%error, "suppression non close dans le journal");
                self.ops.settle(&job.id, None).await;
            }
        }
        // La trace dit l'opération, jamais le service ni un chemin.
        tracing::info!(
            operation = %job.id,
            status = ?finish.status,
            cause = finish.cause.map(Cause::as_str),
            left = finish.note.left,
            duration_ms = u64::try_from(took.as_millis()).unwrap_or(u64::MAX),
            "suppression terminée"
        );
    }

    /// Une panique de l'exécution : l'opération est close par le MÊME chemin que toute clôture (`finish_with` :
    /// transaction, entrée au journal d'activité, verrous rendus, annonce). Pendant `Cancelling`, la seule issue
    /// permise est « annulé » (un `failed` serait refusé et laisserait le verrou).
    async fn close_after_panic(&self, id: &str, name: &str, actor: Actor) {
        // Après l'`intent` d'un geste destructif (le conteneur, l'oubli, l'effacement), on ne sait pas où il en
        // était : l'opération est « à vérifier » comme après un redémarrage, le nom reste réservé, rien n'est
        // rejoué (BR-DEL-025). Seule une panique AVANT ce point (l'arrêt) laisse le service entier et se clôt.
        match self.ops.park_for_check(id).await {
            Ok(true) => {
                tracing::error!(operation = %id, "suppression interrompue par une panique après un geste destructif : à vérifier");
                return;
            }
            Ok(false) => {}
            Err(error) => {
                tracing::error!(%error, "suppression interrompue : classement « à vérifier » impossible");
            }
        }
        let cancelling = self.is_cancelling(id).await;
        let (status, cause) = if cancelling {
            (OpStatus::Cancelled, Cause::CancelledByAccount)
        } else {
            (OpStatus::Failed, Cause::StepFailed)
        };
        let draft = ClientName::parse(name).map(|service| AuditDraft {
            actor,
            action: OpKind::Delete.audit_action(),
            target: Target::ServiceDelete {
                service,
                note: DeleteNote::default(),
            },
            outcome: if cancelling {
                Outcome::Failed(reason_of(cause))
            } else {
                Outcome::Failed(Reason::Internal)
            },
        });
        // `finish_with` rend les verrous et annonce : rien à faire de plus quand il réussit.
        if let Err(error) = self.ops.finish_with(id, status, Some(cause), draft).await {
            tracing::error!(%error, "suppression interrompue non close dans le journal");
            self.ops.settle(id, None).await;
        }
    }

    /// Une entrée au journal d'activité (`BR-DEL-029`) : le compte, le service, ce qui a été effacé (des nombres),
    /// l'arrêt forcé, les données non effacées, l'issue et sa cause typée. Jamais un chemin.
    fn draft(&self, job: &Job, finish: &Finish) -> Option<AuditDraft> {
        let service = ClientName::parse(&job.name)?;
        let outcome = match (finish.status, finish.cause) {
            (OpStatus::Succeeded, _) => Outcome::Succeeded,
            (_, cause) => Outcome::Failed(cause.map_or(Reason::Internal, reason_of)),
        };
        Some(AuditDraft {
            actor: job.actor.clone(),
            action: OpKind::Delete.audit_action(),
            target: Target::ServiceDelete {
                service,
                note: finish.note,
            },
            outcome,
        })
    }
}

/// Ce que la requête a jugé du service et de ses dossiers.
#[derive(Default)]
struct Judged {
    split: MountSplit,
    /// Le sort de chaque dossier géré, dans l'ordre de `split.managed`.
    fates: Vec<Fate>,
}

/// L'état de `BR-DEL-003` : en marche, arrêté, en erreur, ou qui redémarre en boucle.
fn deletable(state: ServiceStateKind) -> bool {
    matches!(
        state,
        ServiceStateKind::Running
            | ServiceStateKind::Stopped
            | ServiceStateKind::Error
            | ServiceStateKind::RestartLoop
    )
}

/// La cause d'une erreur du moteur, dans la liste fermée (`BR-DEL-029`). Jamais un texte du moteur.
fn cause_of(error: EngineError) -> Cause {
    match error {
        EngineError::Unreachable | EngineError::Denied => Cause::EngineUnavailable,
        EngineError::Timeout => Cause::Timeout,
        EngineError::NotFound => Cause::ServiceNotFound,
        _ => Cause::EngineRefused,
    }
}

/// L'étape annoncée tant que l'arrêt dure, retirée quoi qu'il arrive.
struct StepShown<'a> {
    registry: &'a ServicesRegistry,
    key: ServiceKey,
}

impl Drop for StepShown<'_> {
    fn drop(&mut self) {
        self.registry.end_transition(&self.key);
    }
}

fn account_of(actor: &Actor) -> String {
    actor
        .account
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_default()
}

/// Une écriture du journal des opérations qui échoue ne fait pas échouer la suppression : l'ordre est donné, ou ne
/// le sera pas. Elle se voit : la reprise lira une étape incomplète.
fn journal<T>(result: Result<T, OpError>, what: &'static str) {
    if let Err(error) = result {
        tracing::warn!(%error, write = what, "journal de la suppression non écrit");
    }
}

/// Ce que la lecture d'un chemin du serveur établit : ses noms à comparer, sa lignée (lui, puis ses parents), et
/// si une lecture n'a pas pu se faire.
struct Seen {
    names: Vec<String>,
    lineage: Vec<FileId>,
    unreadable: bool,
}

/// Lit un chemin du serveur : le nom donné, sa forme canonique, et la forme canonique de son dossier parent suivie
/// de son dernier nom (le dossier lui-même peut ne pas exister). `own` : un dossier qui n'existe pas prend la
/// lignée de son parent (un parent monté ailleurs reste vu).
fn see(paths: &dyn HostPaths, path: &str, own: bool) -> Seen {
    let mut seen = Seen {
        names: vec![path.to_owned()],
        lineage: Vec::new(),
        unreadable: false,
    };
    let add = |seen: &mut Seen, candidate: String| {
        if !seen.names.contains(&candidate) {
            seen.names.push(candidate);
        }
    };
    let absent = match paths.resolve(path) {
        Resolved::Found { canonical, lineage } => {
            add(&mut seen, canonical);
            seen.lineage = lineage;
            false
        }
        Resolved::Absent => true,
        Resolved::Unreadable => {
            seen.unreadable = true;
            return seen;
        }
    };
    if let Some((parent, leaf)) = path.trim_end_matches('/').rsplit_once('/') {
        let parent = if parent.is_empty() { "/" } else { parent };
        match paths.resolve(parent) {
            Resolved::Found { canonical, lineage } => {
                add(
                    &mut seen,
                    format!("{}/{leaf}", canonical.trim_end_matches('/')),
                );
                if absent && own {
                    seen.lineage = lineage;
                }
            }
            Resolved::Absent => {}
            Resolved::Unreadable => seen.unreadable = true,
        }
    }
    seen
}

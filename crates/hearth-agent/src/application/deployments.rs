//! Déployer une image : vérifier, récupérer, créer, démarrer, surveiller (HRT-68, ADR-0039, ADR-0041,
//! ADR-0044, ADR-0045, BR-DEPLOY-*, BR-DEPLOY-038).
//!
//! **Ce que fait [`Deployments::deploy`], de façon SYNCHRONE** (rien n'est attendu du moteur au-delà d'une
//! lecture) : le moteur est prêt (`BR-DEPLOY-027`), la demande est jugée par le domaine (plan fermé, champs,
//! limites obligatoires), le nom est libre, les ports le sont ou leur réservation est cochée, deux déploiements
//! au plus tournent, l'espace est suffisant ou cochée. Puis l'opération `deploy` est acceptée dans le journal
//! (verrou exclusif de la clé, nom réservé), la description durable (révision 1, secrets scellés) est écrite
//! (`BR-DEPLOY-037`) et l'exécution part en tâche de fond. Réponse : `202 { operation_id, service_key }`.
//!
//! **L'exécution** suit le plan figé (`domain::services::deploy::op_plan`) : `check` (l'image est-elle là ?),
//! `pull` (progression, annulable), `create` (point de non-retour : le conteneur est créé DEPUIS la description
//! relue en base, étiquettes Hearth, `unless-stopped`, limites), `start`, `watch` (10 s). `create` et `start`
//! écrivent leur `intent` AVANT le geste : un agent qui s'arrête entre les deux laisse une opération « à
//! vérifier », jamais « tout va bien ». L'annulation (`DELETE /operations/services/{id}`) abandonne le flux de
//! téléchargement ; passé `create`, elle répond `TOO_LATE_TO_CANCEL`.
//!
//! **Un échec avant le lancement ne laisse RIEN** (`BR-DEPLOY-019`) : la description est retirée, le dossier de
//! données du service aussi, le conteneur créé puis refusé au démarrage est supprimé. Un résultat INCONNU (le
//! moteur ne répond plus après un ordre de création ou de démarrage) ne retire rien : l'opération est
//! « interrompue » et le compte relit l'état réel.
//!
//! **Jamais** : écrire une valeur de variable, un nom de variable ou la commande dans une trace, une erreur, le
//! journal d'activité ou une ligne de la base autre que la description scellée ; rejouer un ordre ; générer un
//! réglage hors du plan fermé.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use futures_util::StreamExt;
use hearth_proto::api::deploy::{
    CapacityHost, DEPLOY_TIMEOUT_S, DeployAccepted, DeployCheck, DeployCheckRequest, DeployPlace,
    DeployProtocol, DeploySpec, DiskCheck, EngineCapacity, LOW_DISK_BYTES,
    MAX_CONCURRENT_DEPLOYMENTS, MountCheck, MountStatus, NameStatus, PortCheck, PortStatus,
    reserve_bytes,
};
use thiserror::Error;
use tokio::time::Instant;

use super::folders::{FolderFacts, check_mount_folder};
use super::ports::{
    AuditSink, Clock, ContainerEngine, EngineError, EngineInfo, FreeSpace, HostFolders, HostPorts,
    IdGen, Ownership, Perms, PullProgress, SafeFs, ServiceContainers,
};
use super::service_lifecycle::reason_of;
use super::service_ops::{AcceptRequest, OpError, ServiceOps};
use super::service_specs::{ServiceSpecs, SpecServiceError};
use super::services::ServicesRegistry;
use crate::application::ports::Progress;
use crate::domain::audit::{Actor, ClientName, Outcome, Reason, Target};
use crate::domain::files::RelPath;
use crate::domain::ops::{Cause, OpKind, OpStatus, Refusal as LockRefusal, UndoInfo};
use crate::domain::services::deploy::{
    self, ImageRef, Invalid, Machine, Refusal, ValidDeploy, op_plan, step, validate, validate_name,
};
use crate::domain::services::plan::PlanContext;
use crate::domain::services::ports::{Listener, PortWorld, duplicates, suggest, verdict};
use crate::domain::services::spec::{SecretsAccess, fold_name};
use crate::domain::services::{EngineState, RunState, ServiceKey};

/// Les durées d'un déploiement. Les tests les raccourcissent.
#[derive(Debug, Clone, Copy)]
pub struct DeployTimings {
    /// Au-delà, le déploiement est arrêté (`BR-DEPLOY-034`).
    pub deadline: Duration,
    /// Combien de temps un service lancé est surveillé (`BR-DEPLOY-020`).
    pub watch: Duration,
    /// L'intervalle entre deux lectures pendant la surveillance.
    pub poll: Duration,
}

impl DeployTimings {
    pub const fn production() -> Self {
        Self {
            deadline: Duration::from_secs(DEPLOY_TIMEOUT_S),
            watch: Duration::from_secs(10),
            poll: Duration::from_millis(500),
        }
    }
}

/// Le dossier géré par Hearth et ce que l'agent sait de lui-même, pour juger un dossier du serveur
/// (`check_mount_folder`, `BR-DEPLOY-012`).
pub struct FolderGuard {
    pub host: Arc<dyn HostFolders>,
    pub facts: FolderFacts,
}

/// Ce dont le cas d'usage a besoin de la machine et de la configuration.
pub struct DeployContext {
    pub install_id: String,
    /// Le port de l'agent (`BR-DEPLOY-007`).
    pub agent_port: u16,
    /// `<données de l'agent>/services` : sous lui, un sous-dossier par service.
    pub services_root: String,
    pub folders: Option<FolderGuard>,
    pub timings: DeployTimings,
}

/// Pourquoi une demande de déploiement est refusée tout de suite, avant que rien ne soit créé. Aucune variante
/// ne porte une valeur reçue.
#[derive(Debug, Error)]
pub enum DeployError {
    #[error("le moteur n'est pas prêt")]
    EngineUnavailable(EngineState),
    #[error("champ invalide")]
    Invalid(Invalid),
    /// Un réglage hors du plan fermé (`BR-DEPLOY-035`).
    #[error("réglage non permis")]
    SettingNotAllowed,
    #[error("limites obligatoires")]
    LimitsRequired,
    #[error("dossier refusé")]
    FolderRefused { index: usize, reason: &'static str },
    #[error("nom pris")]
    NameTaken,
    /// Les données d'un service supprimé s'effacent encore (`BR-DEL-019`).
    #[error("nom en cours de suppression")]
    NameBeingDeleted,
    #[error("port tenu")]
    PortInUse {
        host_port: u16,
        protocol: DeployProtocol,
        status: PortStatus,
        holder: Option<String>,
        suggested: Option<u16>,
    },
    #[error("deux déploiements déjà en cours")]
    DeployLimit,
    /// L'espace libre est sous le seuil et la case n'est pas cochée (`BR-DEPLOY-018`).
    #[error("espace disque bas")]
    LowDisk { free_bytes: u64 },
    #[error("erreur interne : {0}")]
    Internal(String),
}

/// Une place de déploiement, rendue quand l'exécution finit, quoi qu'il arrive.
struct Slot(Arc<AtomicUsize>);

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// La ligne « Déploiement en cours » de la liste, retirée quoi qu'il arrive.
struct Shown {
    registry: Arc<ServicesRegistry>,
    key: ServiceKey,
}

impl Drop for Shown {
    fn drop(&mut self) {
        self.registry.end_deploying(&self.key);
    }
}

/// Tout ce qu'une exécution emporte avec elle. Aucun secret : les variables et la commande sont relues de la
/// description scellée au moment de créer.
struct Job {
    id: String,
    key: ServiceKey,
    name: String,
    repository: String,
    tag: String,
    account: String,
    actor: Actor,
    host_ports: Vec<u16>,
    env_count: u32,
    custom_command: bool,
    mounts: Vec<String>,
}

/// Ce que l'exécution a décidé de son issue.
#[derive(Debug)]
struct Finish {
    status: OpStatus,
    cause: Option<Cause>,
    /// Rien n'a été créé : la description et le dossier de données sont retirés (`BR-DEPLOY-019`).
    clean: bool,
}

impl Finish {
    fn failed(cause: Cause) -> Self {
        Self {
            status: OpStatus::Failed,
            cause: Some(cause),
            clean: true,
        }
    }

    /// Le résultat est inconnu : on ne retire rien, le compte relit l'état réel (`BR-DEPLOY-028`).
    fn unknown(cause: Cause) -> Self {
        Self {
            status: OpStatus::Interrupted,
            cause: Some(cause),
            clean: false,
        }
    }

    fn cancelled() -> Self {
        Self {
            status: OpStatus::Cancelled,
            cause: Some(Cause::CancelledByAccount),
            clean: true,
        }
    }

    fn done(cause: Option<Cause>) -> Self {
        Self {
            status: OpStatus::Succeeded,
            cause,
            clean: false,
        }
    }

    /// La cause d'un échec de récupération ou de création, et si le résultat est inconnu.
    fn from_engine(error: EngineError, may_have_sent: bool) -> Self {
        let (cause, unknown) = cause_of(error);
        if unknown && may_have_sent {
            Self::unknown(cause)
        } else {
            Self::failed(cause)
        }
    }
}

/// La cause d'une erreur du moteur, et si elle laisse le résultat inconnu (le moteur n'a pas répondu : l'ordre
/// est peut-être parti). Jamais un texte du moteur.
fn cause_of(error: EngineError) -> (Cause, bool) {
    match error {
        EngineError::ImageNotFound | EngineError::NotFound => (Cause::ImageNotFound, false),
        EngineError::TagNotFound => (Cause::TagNotFound, false),
        EngineError::ImagePrivateOrMissing => (Cause::ImagePrivateOrMissing, false),
        EngineError::RegistryUnreachable => (Cause::RegistryUnreachable, false),
        EngineError::RegistryRateLimited => (Cause::RegistryRateLimited, false),
        EngineError::PlatformMismatch => (Cause::PlatformMismatch, false),
        EngineError::DiskFull => (Cause::DiskFull, false),
        EngineError::NameTaken => (Cause::NameTaken, false),
        EngineError::PortInUse => (Cause::PortInUse, false),
        EngineError::Unreachable => (Cause::EngineUnavailable, true),
        EngineError::Timeout => (Cause::Timeout, true),
        EngineError::Denied => (Cause::EngineUnavailable, false),
        EngineError::Refused | EngineError::Unsupported => (Cause::EngineRefused, false),
    }
}

pub struct Deployments {
    registry: Arc<ServicesRegistry>,
    ops: Arc<ServiceOps>,
    specs: Arc<ServiceSpecs>,
    engine: Arc<dyn ContainerEngine>,
    host_ports: Arc<dyn HostPorts>,
    free_space: Arc<dyn FreeSpace>,
    safe_fs: Arc<dyn SafeFs>,
    containers: Arc<dyn ServiceContainers>,
    sink: Arc<dyn AuditSink>,
    ids: Arc<dyn IdGen>,
    clock: Arc<dyn Clock>,
    context: DeployContext,
    running: Arc<AtomicUsize>,
}

/// Les ports du serveur tels que le domaine les juge (`PortWorld`) : le moteur, les sockets en écoute, le port de
/// l'agent.
struct World<'a> {
    registry: &'a ServicesRegistry,
    listeners: Vec<Listener>,
    host: &'a dyn HostPorts,
    agent_port: u16,
}

impl PortWorld for World<'_> {
    fn claim(
        &self,
        port: u16,
        protocol: DeployProtocol,
    ) -> Option<crate::domain::services::ports::Claim> {
        self.registry.port_claim(port, protocol)
    }

    fn listener(&self, port: u16, protocol: DeployProtocol) -> Option<Listener> {
        self.listeners
            .iter()
            .find(|l| l.port == port && l.protocol == protocol)
            .cloned()
    }

    /// Le nom du programme n'est cherché que pour un port en conflit.
    fn program(&self, port: u16, protocol: DeployProtocol) -> Option<String> {
        self.host.program_of(port, protocol)
    }

    fn agent_port(&self) -> u16 {
        self.agent_port
    }
}

impl Deployments {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        registry: Arc<ServicesRegistry>,
        ops: Arc<ServiceOps>,
        specs: Arc<ServiceSpecs>,
        engine: Arc<dyn ContainerEngine>,
        host_ports: Arc<dyn HostPorts>,
        free_space: Arc<dyn FreeSpace>,
        safe_fs: Arc<dyn SafeFs>,
        containers: Arc<dyn ServiceContainers>,
        sink: Arc<dyn AuditSink>,
        ids: Arc<dyn IdGen>,
        clock: Arc<dyn Clock>,
        context: DeployContext,
    ) -> Arc<Self> {
        Arc::new(Self {
            registry,
            ops,
            specs,
            engine,
            host_ports,
            free_space,
            safe_fs,
            containers,
            sink,
            ids,
            clock,
            context,
            running: Arc::new(AtomicUsize::new(0)),
        })
    }

    // -----------------------------------------------------------------------------------------
    // Les bornes de la machine
    // -----------------------------------------------------------------------------------------

    /// `GET /engine/capacity` : le plafond de chaque curseur (lu sur la machine) et ce qui est déjà engagé.
    pub async fn capacity(&self) -> Result<EngineCapacity, DeployError> {
        self.ready()?;
        let info = self.engine.info().await.map_err(|error| {
            if error.engine_gone() || error == EngineError::Timeout {
                DeployError::EngineUnavailable(self.registry.engine_state())
            } else {
                DeployError::Internal("moteur illisible".into())
            }
        })?;
        Ok(EngineCapacity {
            host: CapacityHost {
                cpus: info.cpus,
                memory_bytes: info.memory_bytes,
                reserve_bytes: reserve_bytes(info.memory_bytes),
            },
            committed: self.registry.committed(),
        })
    }

    async fn machine(&self) -> (Option<Machine>, Option<EngineInfo>) {
        match self.engine.info().await {
            Ok(info) if info.cpus > 0 && info.memory_bytes > 0 => (
                Some(Machine {
                    cpus: info.cpus,
                    memory_bytes: info.memory_bytes,
                }),
                Some(info),
            ),
            Ok(info) => (None, Some(info)),
            Err(_) => (None, None),
        }
    }

    /// L'espace libre où le moteur range ses images (`BR-DEPLOY-018`).
    async fn disk(&self, info: Option<&EngineInfo>) -> Option<DiskCheck> {
        let dir = info?.storage_dir.clone()?;
        let free_space = self.free_space.clone();
        let free = tokio::task::spawn_blocking(move || free_space.free_bytes(Path::new(&dir)))
            .await
            .ok()?
            .ok()?;
        Some(DiskCheck {
            free_bytes: free,
            low: free < LOW_DISK_BYTES,
        })
    }

    fn ready(&self) -> Result<(), DeployError> {
        let state = self.registry.engine_state();
        if state == EngineState::Ready {
            Ok(())
        } else {
            Err(DeployError::EngineUnavailable(state))
        }
    }

    // -----------------------------------------------------------------------------------------
    // La vérification : une consultation, rien n'est écrit ni journalisé (BR-DEPLOY-033)
    // -----------------------------------------------------------------------------------------

    /// Les constats du formulaire, sans aucun secret. Les mêmes jugements que le lancement (`BR-DEPLOY-032`).
    pub async fn check(&self, request: &DeployCheckRequest) -> Result<DeployCheck, DeployError> {
        self.ready()?;
        let mut invalid_fields = Vec::new();
        let name = match validate_name(&request.name) {
            Err(_) => {
                invalid_fields.push("name".to_owned());
                NameStatus::Invalid
            }
            Ok(()) => self.name_status(&request.name).await?,
        };
        let image = match ImageRef::parse(&request.image, request.tag.as_deref()) {
            Ok(image) => Some(image),
            Err(Invalid { field, .. }) => {
                invalid_fields.push(field.to_owned());
                None
            }
        };
        let ports = self.check_ports(request).await;
        let mounts = self.check_mounts(request, &mut invalid_fields);
        let (_, info) = self.machine().await;
        let disk = self.disk(info.as_ref()).await;
        let image_present = match &image {
            Some(image) => self
                .engine
                .image_present(&image.reference())
                .await
                .unwrap_or(false),
            None => false,
        };
        Ok(DeployCheck {
            name,
            invalid_fields,
            ports: ports.into(),
            mounts: mounts.into(),
            disk,
            image_present,
        })
    }

    async fn name_status(&self, name: &str) -> Result<NameStatus, DeployError> {
        let folded = fold_name(name);
        if let Some(by) = self
            .ops
            .locks()
            .with(|table| table.name_reserved_by(&folded))
        {
            return Ok(if by == OpKind::Delete {
                NameStatus::BeingDeleted
            } else {
                NameStatus::Taken
            });
        }
        if self.registry.name_holder(name).is_some() {
            return Ok(NameStatus::Taken);
        }
        match self.specs_holds(&folded).await {
            Ok(true) => Ok(NameStatus::Taken),
            Ok(false) => Ok(NameStatus::Ok),
            Err(error) => Err(DeployError::Internal(error)),
        }
    }

    async fn specs_holds(&self, folded: &str) -> Result<bool, String> {
        self.specs
            .holder_of_name(folded)
            .await
            .map(|holder| holder.is_some())
            .map_err(|error| error.to_string())
    }

    async fn world(&self) -> World<'_> {
        let host = self.host_ports.clone();
        let listeners = tokio::task::spawn_blocking(move || host.listening())
            .await
            .ok()
            .flatten()
            .unwrap_or_default();
        World {
            registry: &self.registry,
            listeners,
            host: self.host_ports.as_ref(),
            agent_port: self.context.agent_port,
        }
    }

    async fn check_ports(&self, request: &DeployCheckRequest) -> Vec<PortCheck> {
        let world = self.world().await;
        Self::judge_ports_sync(&world, &request.ports)
    }

    /// Le constat de chaque port du formulaire. Le nom d'un programme se cherche par `/proc` : hors de la tâche
    /// asynchrone.
    fn judge_ports_sync(
        world: &World<'_>,
        ports: &[hearth_proto::api::deploy::DeployPort],
    ) -> Vec<PortCheck> {
        let dups = duplicates(ports);
        let in_form: std::collections::HashSet<(u16, DeployProtocol)> =
            ports.iter().map(|p| (p.host_port, p.protocol)).collect();
        // L'ordre de la demande est gardé ; un doublon est invalide dès sa première occurrence.
        ports
            .iter()
            .map(|port| {
                let dup = dups.contains(&(port.host_port, port.protocol));
                let found = verdict(world, port.host_port, port.protocol, dup);
                let suggested = (found.blocks() || found.status == PortStatus::ReservedByStopped)
                    .then(|| suggest(world, port.host_port, port.protocol, &in_form))
                    .flatten();
                PortCheck {
                    host_port: port.host_port,
                    protocol: port.protocol,
                    status: found.status,
                    holder: found.holder,
                    suggested,
                    privileged: port.host_port < 1024,
                }
            })
            .collect()
    }

    fn check_mounts(
        &self,
        request: &DeployCheckRequest,
        invalid_fields: &mut Vec<String>,
    ) -> Vec<MountCheck> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::with_capacity(request.mounts.len());
        for mount in &request.mounts {
            let status = match normalized(&mount.target) {
                Err(_) => MountStatus::Invalid,
                Ok(target) if !seen.insert(target.clone()) => MountStatus::Invalid,
                Ok(_) if mount.place == DeployPlace::Host => self.host_folder_status(mount),
                Ok(_) => MountStatus::Ok,
            };
            if status == MountStatus::Invalid && !invalid_fields.iter().any(|f| f == "mounts") {
                invalid_fields.push("mounts".to_owned());
            }
            out.push(MountCheck {
                target: mount.target.clone(),
                status,
            });
        }
        out
    }

    /// Un dossier du serveur autre que le dossier géré : protégé (`BR-DEPLOY-012`), sinon « pas encore
    /// disponible » (HRT-70). Les deux répondent par un refus.
    fn host_folder_status(&self, mount: &hearth_proto::api::deploy::DeployMount) -> MountStatus {
        match self.host_folder_reason(mount.host_path.as_deref()) {
            "protected" | "not_a_path" => MountStatus::Refused,
            _ => MountStatus::NotAvailable,
        }
    }

    fn host_folder_reason(&self, path: Option<&str>) -> &'static str {
        let (Some(path), Some(guard)) = (path, self.context.folders.as_ref()) else {
            return "not_available";
        };
        match check_mount_folder(guard.host.as_ref(), &guard.facts, path) {
            Ok(_) => "not_available",
            Err(crate::domain::services::FolderRefusal::Protected) => "protected",
            Err(crate::domain::services::FolderRefusal::NotAnAbsolutePath) => "not_a_path",
        }
    }

    // -----------------------------------------------------------------------------------------
    // Le lancement
    // -----------------------------------------------------------------------------------------

    /// Accepte (ou refuse tout de suite) un déploiement. Le droit de gérer les services et la confirmation de
    /// l'acte ont déjà été jugés.
    pub async fn deploy(
        self: &Arc<Self>,
        spec: DeploySpec,
        actor: Actor,
        op_key: &str,
    ) -> Result<DeployAccepted, DeployError> {
        self.ready()?;
        let (machine, info) = self.machine().await;
        let valid = validate(&spec, machine).map_err(|refusal| self.refusal(&spec, refusal))?;

        // Deux déploiements au plus, tous comptes confondus (BR-DEPLOY-023).
        if self.running.fetch_add(1, Ordering::SeqCst) >= MAX_CONCURRENT_DEPLOYMENTS {
            self.running.fetch_sub(1, Ordering::SeqCst);
            return Err(DeployError::DeployLimit);
        }
        let slot = Slot(Arc::clone(&self.running));

        let folded = valid.body.name_folded();
        match self.name_status(&valid.body.name).await? {
            NameStatus::Ok => {}
            NameStatus::BeingDeleted => return Err(DeployError::NameBeingDeleted),
            _ => return Err(DeployError::NameTaken),
        }
        self.check_ports_for(&valid).await?;
        let image = valid.image();
        let reference = image.reference();
        if let Some(disk) = self.disk(info.as_ref()).await
            && disk.low
            && !valid.accept.low_disk
            && !self.engine.image_present(&reference).await.unwrap_or(false)
        {
            return Err(DeployError::LowDisk {
                free_bytes: disk.free_bytes,
            });
        }

        let key = ServiceKey::for_hearth(&self.ids.new_id())
            .ok_or_else(|| DeployError::Internal("identifiant de service illisible".into()))?;
        let account = account_of(&actor);
        let plan = op_plan().map_err(|error| DeployError::Internal(error.to_string()))?;
        let id = self
            .ops
            .accept(AcceptRequest {
                op_key: op_key.to_owned(),
                service: key.clone(),
                service_name: valid.body.name.clone(),
                kind: OpKind::Deploy,
                account: account.clone(),
                plan,
                forced: false,
                reserve_name: Some(folded),
                heavy_io: false,
            })
            .await
            .map_err(|error| match error {
                OpError::Locked(LockRefusal::NameReserved { by }) => {
                    if by == OpKind::Delete {
                        DeployError::NameBeingDeleted
                    } else {
                        DeployError::NameTaken
                    }
                }
                other => DeployError::Internal(other.to_string()),
            })?;

        // La description durable, révision 1, à l'acceptation (BR-DEPLOY-037) : le conteneur sera créé depuis elle.
        if let Err(error) = self
            .specs
            .save(&key, None, valid.body.clone(), &valid.secrets, &account)
            .await
        {
            journal(
                self.ops
                    .finish(&id, OpStatus::Failed, Some(Cause::StepFailed))
                    .await,
                "finish",
            );
            return Err(match error {
                SpecServiceError::NameTaken => DeployError::NameTaken,
                other => DeployError::Internal(other.to_string()),
            });
        }
        if let Err(error) = self.ops.start(&id).await {
            journal(
                self.ops
                    .finish(&id, OpStatus::Failed, Some(Cause::StepFailed))
                    .await,
                "finish",
            );
            self.forget_spec(&key).await;
            return Err(DeployError::Internal(error.to_string()));
        }
        self.registry
            .begin_deploying(&key, &valid.body.name, &image.repository, &image.tag);
        let job = Job {
            id: id.clone(),
            key: key.clone(),
            name: valid.body.name.clone(),
            repository: image.repository.clone(),
            tag: image.tag.clone(),
            account,
            actor,
            host_ports: valid.body.ports.iter().map(|p| p.host_port).collect(),
            env_count: u32::try_from(valid.env_count()).unwrap_or(u32::MAX),
            custom_command: valid.has_command(),
            mounts: valid.body.mounts.iter().map(|m| m.source.clone()).collect(),
        };
        let shown = Shown {
            registry: Arc::clone(&self.registry),
            key: key.clone(),
        };
        let this = Arc::clone(self);
        // Les secrets de la demande ne vont pas dans la tâche : elle les relira de la description scellée.
        drop(valid);
        let work = tokio::spawn(async move {
            let _slot = slot;
            let _shown = shown;
            this.execute(job).await;
        });
        // Une panique de l'exécution ne laisse jamais une opération « en cours » : elle est close, le verrou rendu.
        let watcher = Arc::clone(self);
        let (watched_id, watched_key) = (id.clone(), key.clone());
        tokio::spawn(async move {
            if work.await.is_err() {
                tracing::error!(operation = %watched_id, "exécution d'un déploiement interrompue par une panique");
                journal(
                    watcher
                        .ops
                        .finish(&watched_id, OpStatus::Failed, Some(Cause::StepFailed))
                        .await,
                    "finish",
                );
                watcher.registry.end_deploying(&watched_key);
            }
        });
        Ok(DeployAccepted {
            operation_id: id,
            service_key: key.as_str().to_owned(),
        })
    }

    /// Un refus du domaine devient l'erreur typée que la route rend. Un dossier du serveur : jugé ici, parce que
    /// le domaine ne connaît pas le système de fichiers.
    fn refusal(&self, spec: &DeploySpec, refusal: Refusal) -> DeployError {
        match refusal {
            Refusal::Invalid(invalid) => DeployError::Invalid(invalid),
            Refusal::SettingNotAllowed => DeployError::SettingNotAllowed,
            Refusal::LimitsRequired => DeployError::LimitsRequired,
            Refusal::HostFolder { index } => DeployError::FolderRefused {
                index,
                reason: self.host_folder_reason(
                    spec.mounts
                        .get(index)
                        .and_then(|mount| mount.host_path.as_deref()),
                ),
            },
        }
    }

    /// Les ports demandés sont libres, ou réservés par un service arrêté et la case est cochée (`BR-DEPLOY-008`).
    async fn check_ports_for(&self, valid: &ValidDeploy) -> Result<(), DeployError> {
        if valid.ports.is_empty() {
            return Ok(());
        }
        let world = self.world().await;
        for check in Self::judge_ports_sync(&world, &valid.ports) {
            let blocks = match check.status {
                PortStatus::Free => false,
                PortStatus::ReservedByStopped => !valid.accept.reserved_ports,
                _ => true,
            };
            if blocks {
                return Err(DeployError::PortInUse {
                    host_port: check.host_port,
                    protocol: check.protocol,
                    status: check.status,
                    holder: check.holder,
                    suggested: check.suggested,
                });
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------------------------
    // L'exécution
    // -----------------------------------------------------------------------------------------

    async fn execute(&self, job: Job) {
        let started = Instant::now();
        let deadline = started + self.context.timings.deadline;
        let finish = match tokio::time::timeout_at(deadline, self.run(&job, deadline)).await {
            Ok(finish) => finish,
            // Trente minutes (BR-DEPLOY-034) : ce qui n'a pas été créé n'existe pas ; sinon le résultat est inconnu.
            Err(_) => {
                if self.created(&job.key).await {
                    Finish::unknown(Cause::Timeout)
                } else {
                    Finish::failed(Cause::Timeout)
                }
            }
        };
        self.settle(&job, finish, started.elapsed()).await;
    }

    /// L'agent a-t-il déjà créé un conteneur pour ce service ?
    async fn created(&self, key: &ServiceKey) -> bool {
        self.containers
            .of(key)
            .await
            .map(|ids| !ids.is_empty())
            .unwrap_or(true)
    }

    async fn is_cancelling(&self, id: &str) -> bool {
        matches!(
            self.ops.snapshot(id).await,
            Ok(Some(snapshot)) if snapshot.record.status == OpStatus::Cancelling
        )
    }

    /// Écrit l'`intent` d'une étape. `Err` : l'issue à rendre (annulée si une annulation est passée avant,
    /// sinon un échec).
    async fn begin(&self, job: &Job, name: &str) -> Result<(), Finish> {
        match self.ops.intent(&job.id, name, &UndoInfo::default()).await {
            Ok(_) => Ok(()),
            Err(OpError::Stale | OpError::OutOfOrder) if self.is_cancelling(&job.id).await => {
                Err(Finish::cancelled())
            }
            Err(error) => {
                tracing::error!(%error, step = name, "journal du déploiement illisible : l'étape n'est pas lancée");
                Err(Finish::failed(Cause::StepFailed))
            }
        }
    }

    async fn run(&self, job: &Job, deadline: Instant) -> Finish {
        // 1. Vérification : l'image est-elle déjà là ?
        if let Err(finish) = self.begin(job, step::CHECK).await {
            return finish;
        }
        let reference = format!("{}:{}", job.repository, job.tag);
        let present = match self.engine.image_present(&reference).await {
            Ok(present) => present,
            Err(error) => {
                journal(self.ops.failed(&job.id, step::CHECK).await, "failed");
                return Finish::from_engine(error, false);
            }
        };
        journal(
            self.ops
                .done(&job.id, step::CHECK, &UndoInfo::default())
                .await,
            "done",
        );
        if self.is_cancelling(&job.id).await {
            return Finish::cancelled();
        }

        // 2. Récupération, annulable. Une image déjà présente est utilisée telle quelle (BR-DEPLOY-018).
        if !present && let Err(finish) = self.pull(job, &reference, deadline).await {
            return finish;
        }

        // 3. Création, depuis la description relue en base (BR-DEPLOY-037). Point de non-retour.
        let plan = match self.plan_of(job).await {
            Ok(plan) => plan,
            Err(finish) => return finish,
        };
        if let Err(finish) = self.prepare_folders(job).await {
            return finish;
        }
        if let Err(finish) = self.begin(job, step::CREATE).await {
            return finish;
        }
        let container = match self.engine.create(&plan).await {
            Ok(id) => id,
            Err(error) => {
                journal(self.ops.failed(&job.id, step::CREATE).await, "failed");
                return Finish::from_engine(error, true);
            }
        };
        drop(plan);
        // Le conteneur est consigné AVANT tout : c'est la preuve qu'il est à Hearth, et ce qui permet de le retirer.
        if let Err(error) = self
            .containers
            .record(&job.key, &container, self.clock.now())
            .await
        {
            tracing::error!(%error, "conteneur créé non consigné : retiré");
            return self
                .remove_after_failure(job, &container, Cause::StepFailed)
                .await;
        }
        self.registry
            .register_hearth_container(&job.key, &container);
        self.registry.reread(&container).await;
        journal(
            self.ops
                .done(&job.id, step::CREATE, &UndoInfo::default())
                .await,
            "done",
        );

        // 4. Démarrage.
        if let Err(finish) = self.begin(job, step::START).await {
            return finish;
        }
        match self.engine.start(&container).await {
            Ok(()) => {
                journal(
                    self.ops
                        .done(&job.id, step::START, &UndoInfo::default())
                        .await,
                    "done",
                );
                self.registry.reread(&container).await;
            }
            Err(error) => {
                journal(self.ops.failed(&job.id, step::START).await, "failed");
                let (cause, unknown) = cause_of(error);
                // Le moteur ne répond plus : l'ordre est peut-être parti, rien n'est retiré.
                if unknown {
                    return Finish::unknown(cause);
                }
                return self.remove_after_failure(job, &container, cause).await;
            }
        }

        // 5. Surveillance de 10 secondes (BR-DEPLOY-020) : un arrêt aussitôt n'est pas un échec.
        if let Err(finish) = self.begin(job, step::WATCH).await {
            return finish;
        }
        let stopped = self.watch(&container).await;
        journal(
            self.ops
                .done(&job.id, step::WATCH, &UndoInfo::default())
                .await,
            "done",
        );
        Finish::done(stopped.then_some(Cause::StoppedRightAfterStart))
    }

    /// Récupère l'image, en suivant la progression, jusqu'à la fin, une annulation ou le délai.
    async fn pull(&self, job: &Job, reference: &str, deadline: Instant) -> Result<(), Finish> {
        self.begin(job, step::PULL).await?;
        let mut feed = self.ops.subscribe();
        let mut stream = self.engine.pull(reference);
        loop {
            tokio::select! {
                item = stream.next() => match item {
                    None => break,
                    Some(Ok(progress)) => {
                        let PullProgress { percent, bytes_done, bytes_total } = progress;
                        journal(
                            self.ops
                                .set_progress(
                                    &job.id,
                                    &Progress {
                                        percent,
                                        items_done: None,
                                        bytes_done: Some(bytes_done),
                                        bytes_total,
                                    },
                                )
                                .await,
                            "progress",
                        );
                    }
                    Some(Err(error)) => {
                        journal(self.ops.failed(&job.id, step::PULL).await, "failed");
                        return Err(Finish::from_engine(error, false));
                    }
                },
                change = feed.recv() => {
                    // Une annulation : le flux est abandonné (la connexion au moteur se ferme), rien n'est créé.
                    let cancelling = match change {
                        Ok(snapshot) => snapshot.record.id == job.id
                            && snapshot.record.status == OpStatus::Cancelling,
                        // Des changements perdus : on relit l'opération.
                        Err(_) => self.is_cancelling(&job.id).await,
                    };
                    if cancelling {
                        drop(stream);
                        return Err(Finish::cancelled());
                    }
                }
                () = tokio::time::sleep_until(deadline) => {
                    drop(stream);
                    return Err(Finish::failed(Cause::Timeout));
                }
            }
        }
        journal(
            self.ops
                .done(&job.id, step::PULL, &UndoInfo::default())
                .await,
            "done",
        );
        if self.is_cancelling(&job.id).await {
            return Err(Finish::cancelled());
        }
        Ok(())
    }

    /// Le plan fermé du conteneur, construit depuis la description relue en base.
    async fn plan_of(
        &self,
        job: &Job,
    ) -> Result<crate::domain::services::plan::ContainerPlan, Finish> {
        let read = self.specs.read(&job.key, None).await;
        let (revision, access) = match read {
            Ok(Some(read)) => read,
            Ok(None) | Err(_) => {
                tracing::error!("description du déploiement illisible : rien n'est créé");
                return Err(Finish::failed(Cause::StepFailed));
            }
        };
        let SecretsAccess::Opened(secrets) = access else {
            tracing::error!("secrets du déploiement illisibles : rien n'est créé");
            return Err(Finish::failed(Cause::StepFailed));
        };
        crate::domain::services::plan::plan_from_spec(
            &revision.body,
            &secrets,
            &PlanContext {
                key: &job.key,
                install_id: &self.context.install_id,
                created_by: &job.account,
                rev: revision.rev,
                managed_root: &self.context.services_root,
            },
        )
        .map_err(|error| {
            tracing::error!(%error, "plan de conteneur refusé : rien n'est créé");
            Finish::failed(Cause::StepFailed)
        })
    }

    /// Crée la racine du service et un sous-dossier par point de montage, sous le dossier géré par Hearth,
    /// SANS suivre un lien (`SafeFs`, `BR-DEPLOY-010`). Aucun dossier n'est monté d'ailleurs.
    async fn prepare_folders(&self, job: &Job) -> Result<(), Finish> {
        if job.mounts.is_empty() {
            return Ok(());
        }
        let fs = self.safe_fs.clone();
        let key = job.key.clone();
        let sources = job.mounts.clone();
        let result = tokio::task::spawn_blocking(move || -> Result<(), ()> {
            let root = fs.ensure_service_root(&key).map_err(|_| ())?;
            // Ni le propriétaire ni les droits du service ne sont ceux de l'agent : le programme du conteneur
            // y écrit sous son propre compte. Le dossier du service, lui, reste à l'agent seul (`0700`).
            let owner = Ownership::new(0, 0).ok_or(())?;
            let perms = Perms::new(owner, 0o777).ok_or(())?;
            for source in sources {
                let rel = RelPath::parse(&source).map_err(|_| ())?;
                root.mkdir(&rel, perms).map_err(|_| ())?;
            }
            Ok(())
        })
        .await;
        match result {
            Ok(Ok(())) => Ok(()),
            _ => {
                tracing::error!("dossiers de données non créés : rien n'est lancé");
                Err(Finish::failed(Cause::StepFailed))
            }
        }
    }

    /// Le conteneur créé a été refusé au démarrage : il est supprimé, et avec lui tout ce que le déploiement a
    /// laissé (aucun conteneur orphelin, `BR-DEPLOY-019`).
    async fn remove_after_failure(&self, job: &Job, container: &str, cause: Cause) -> Finish {
        match self.engine.remove(container).await {
            Ok(()) => {
                if let Err(error) = self.containers.forget_container(container).await {
                    tracing::warn!(%error, "conteneur retiré non oublié");
                }
                self.registry.forget_hearth_container(container);
                Finish::failed(cause)
            }
            Err(_) => {
                // Le conteneur est resté : il garde sa consignation (il est listé, supprimable), rien d'autre
                // n'est retiré sous lui.
                tracing::warn!(operation = %job.id, "conteneur d'un déploiement échoué non supprimé");
                Finish {
                    status: OpStatus::Failed,
                    cause: Some(cause),
                    clean: false,
                }
            }
        }
    }

    /// Surveille le service pendant la fenêtre : `true` s'il s'arrête (ou redémarre en boucle) aussitôt.
    async fn watch(&self, container: &str) -> bool {
        let timings = self.context.timings;
        let end = Instant::now() + timings.watch;
        while Instant::now() < end {
            tokio::time::sleep(timings.poll).await;
            match self.engine.inspect(container).await {
                Ok(Some(observation)) => {
                    if matches!(
                        observation.run_state,
                        RunState::Exited | RunState::Dead | RunState::Restarting
                    ) {
                        self.registry.reread(container).await;
                        return true;
                    }
                }
                Ok(None) => return true,
                Err(_) => {}
            }
        }
        false
    }

    // -----------------------------------------------------------------------------------------
    // La fin
    // -----------------------------------------------------------------------------------------

    async fn settle(&self, job: &Job, finish: Finish, took: Duration) {
        // Une annulation arrivée pendant que l'étape échouait gagne : un statut « en annulation » ne devient
        // jamais « échoué » (diagramme d'états, ADR-0045).
        let finish = if finish.clean
            && finish.status != OpStatus::Cancelled
            && self.is_cancelling(&job.id).await
        {
            Finish::cancelled()
        } else {
            finish
        };
        if finish.clean {
            self.forget_spec(&job.key).await;
            self.remove_folders(job).await;
        }
        // La ligne « Déploiement en cours » part AVANT la clôture : qui lit l'issue ne la voit plus dans la liste.
        self.registry.end_deploying(&job.key);
        if let Err(error) = self.ops.finish(&job.id, finish.status, finish.cause).await {
            tracing::error!(%error, "déploiement non clos dans le journal");
        }
        // La trace dit l'opération, jamais le service, l'image ni une variable.
        tracing::info!(
            operation = %job.id,
            status = ?finish.status,
            cause = finish.cause.map(Cause::as_str),
            duration_ms = u64::try_from(took.as_millis()).unwrap_or(u64::MAX),
            "déploiement terminé"
        );
        self.record(job, &finish).await;
    }

    /// Une entrée au journal d'activité (`BR-DEPLOY-025`) : le compte, le service, l'image et la version, les
    /// ports du serveur, le nombre de variables, la commande oui ou non, l'issue et sa cause typée.
    async fn record(&self, job: &Job, finish: &Finish) {
        let (Some(service), Some(image)) = (
            ClientName::parse(&job.name),
            ClientName::parse(&format!("{}:{}", job.repository, job.tag)),
        ) else {
            return;
        };
        let stopped = finish.cause == Some(Cause::StoppedRightAfterStart);
        let outcome = match (finish.status, finish.cause) {
            (OpStatus::Succeeded, _) => Outcome::Succeeded,
            (_, cause) => Outcome::Failed(cause.map_or(Reason::Internal, reason_of)),
        };
        self.sink
            .record(
                job.actor.clone(),
                OpKind::Deploy.audit_action(),
                Target::ServiceDeploy {
                    service,
                    image,
                    host_ports: job.host_ports.clone(),
                    env_count: job.env_count,
                    custom_command: job.custom_command,
                    stopped_right_after_start: stopped,
                },
                outcome,
            )
            .await;
    }

    async fn forget_spec(&self, key: &ServiceKey) {
        if let Err(error) = self.specs.forget(key).await {
            tracing::warn!(%error, "description d'un déploiement échoué non retirée");
        }
        if let Err(error) = self.containers.forget_service(key).await {
            tracing::warn!(%error, "conteneurs d'un déploiement échoué non oubliés");
        }
    }

    async fn remove_folders(&self, job: &Job) {
        if job.mounts.is_empty() {
            return;
        }
        let fs = self.safe_fs.clone();
        let key = job.key.clone();
        let result = tokio::task::spawn_blocking(move || fs.remove_service_root(&key)).await;
        match result {
            Ok(Ok(())) | Ok(Err(crate::application::ports::FsError::NotFound)) => {}
            _ => tracing::warn!("dossiers d'un déploiement échoué non retirés"),
        }
    }
}

/// Le chemin d'un dossier dans le service, normalisé comme le domaine.
fn normalized(target: &str) -> Result<String, ()> {
    deploy::normalize_target(target).map_err(|_| ())
}

fn account_of(actor: &Actor) -> String {
    actor
        .account
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_default()
}

/// Une écriture du journal des opérations qui échoue ne fait pas échouer le déploiement : l'ordre est donné, ou
/// ne le sera pas. Elle se voit : la reprise lira une étape incomplète.
fn journal<T>(result: Result<T, OpError>, what: &'static str) {
    if let Err(error) = result {
        tracing::warn!(%error, write = what, "journal du déploiement non écrit");
    }
}

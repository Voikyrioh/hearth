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
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use futures_util::StreamExt;
use hearth_proto::api::deploy::{
    CapacityHost, DEPLOY_TIMEOUT_S, DeployAccepted, DeployCheck, DeployCheckRequest, DeployPlace,
    DeployProtocol, DeploySource, DeploySpec, DiskCheck, EngineCapacity, LOW_DISK_BYTES,
    MAX_CONCURRENT_DEPLOYMENTS, MountCheck, MountStatus, NameStatus, PortCheck, PortStatus,
    reserve_bytes,
};
use hearth_proto::api::packs::{ModrinthPackView, PackGenreWire, PackInspectView};
use thiserror::Error;
use tokio::time::Instant;

use super::catalog::CatalogService;
use super::fiche_install::{FicheInstaller, InstallAudit, WorldPrep};
use super::folders::{FolderFacts, check_mount_folder};
use super::game_phase::GamePhases;
use super::pack_intake::{JudgedPack, PackError, PackIntake};
use super::ports::{
    Clock, ContainerEngine, DepositId, EngineError, EngineInfo, FreeSpace, HostFolders, HostPorts,
    IdGen, ImportDir, Ownership, Perms, PullProgress, SafeFs, SecretRandom, ServiceContainers,
};
use super::service_lifecycle::reason_of;
use super::service_ops::{AcceptRequest, AuditDraft, OpError, ServiceOps};
use super::service_specs::{ServiceSpecs, SpecServiceError};
use super::services::ServicesRegistry;
use super::world_import::copy::{Control, CopyError, CopyReport, copy_world};
use super::world_import::{Prepared as WorldPrepared, WorldImport};
use crate::application::ports::Progress;
use crate::domain::audit::{Actor, ClientName, Outcome, Reason, Target, WorldNote, WorldResult};
use crate::domain::catalog::loading::schema::fiche::PackGenre;
use crate::domain::files::RelPath;
use crate::domain::ops::{
    Cause, CreatedObject, ObjectKind, ObjectRoot, OpKind, OpStatus, Refusal as LockRefusal,
    UndoInfo,
};
use crate::domain::services::deploy::{
    self, ImageRef, Invalid, Machine, Refusal, ValidDeploy, op_plan, op_plan_world, step, validate,
    validate_name,
};
use crate::domain::services::from_fiche::{PackPlacement, ProducedFile};
use crate::domain::services::plan::{PlanContext, image_reference};
use crate::domain::services::ports::{Listener, PortWorld, duplicates, suggest, verdict};
use crate::domain::services::spec::{SecretsAccess, fold_name};
use crate::domain::services::{EngineState, RunState, ServiceKey};
use crate::domain::worlds::COPY_MAX;

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
    /// `holder` : le nom (à défaut l'image) du conteneur non adopté qui porte ce nom, s'il y en a un
    /// (`BR-ADOPT-022`, HRT-214).
    #[error("nom pris")]
    NameTaken { holder: Option<String> },
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
    /// Une sauvegarde, une restauration ou une reprise de monde tient déjà la place des opérations lourdes
    /// (`BR-MONDE-013`).
    #[error("opération lourde déjà en cours")]
    HeavyIoBusy,
    /// L'espace libre est sous le seuil et la case n'est pas cochée (`BR-DEPLOY-018`).
    #[error("espace disque bas")]
    LowDisk { free_bytes: u64 },
    /// La fiche n'est pas proposée par cet agent : inconnue, ou d'essai sur une construction de publication
    /// (HRT-101, BR-FICHE-002).
    #[error("fiche inconnue")]
    FicheUnknown,
    /// La fiche de l'agent n'est plus celle que le client a vue (version ou empreinte).
    #[error("fiche changée")]
    FicheChanged,
    /// Le pack déposé n'existe pas ou est refusé (HRT-124, BR-MOD-008, BR-MOD-016).
    #[error("pack refusé")]
    Pack(PackError),
    /// Un autre pack est en cours de lecture (BR-MOD-007) : réessayer.
    #[error("lecture de pack occupée")]
    PackBusy,
    /// Un contrat de la fiche n'est pas accepté (BR-FICHE-058) : les identifiants manquants.
    #[error("contrat non accepté")]
    AgreementRequired(Vec<String>),
    /// Une réponse est refusée : le réglage (s'il est connu de la fiche), la raison et au plus UN caractère,
    /// jamais la valeur.
    #[error("réponse refusée")]
    SettingRefused {
        id: Option<String>,
        reason: &'static str,
        character: Option<char>,
    },
    /// La machine est plus petite qu'un minimum MESURÉ (BR-FICHE-021).
    #[error("machine trop petite")]
    HostTooSmall {
        lack: &'static str,
        needed: u64,
        has: u64,
    },
    /// L'espace libre est sous le besoin MESURÉ de l'installation (BR-FICHE-023).
    #[error("espace insuffisant")]
    NotEnoughSpace {
        needed: u64,
        free: u64,
        missing: u64,
    },
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
    account: String,
    actor: Actor,
    host_ports: Vec<u16>,
    env_count: u32,
    custom_command: bool,
    mounts: Vec<String>,
    /// La référence de l'image telle que le moteur la reçoit : `dépôt:version`, avec `@sha256:...` quand la
    /// description épingle une empreinte (une fiche).
    reference: String,
    /// Les fichiers à produire avant la création (une fiche, BR-FICHE-024). Leur contenu peut porter un secret :
    /// type `Secret`, jamais affiché ni tracé.
    files: Vec<ProducedFile>,
    /// Ce que le journal dit d'une installation depuis une fiche (BR-FICHE-032).
    install: Option<InstallAudit>,
    /// Le monde existant à reprendre par copie vérifiée (HRT-105).
    world: Option<WorldJob>,
    /// Le pack déposé à ranger dans le dossier des packs avant la création (HRT-124).
    pack: Option<JobPack>,
    /// L'ordre de création est parti (ou va partir) : passé ce point, un délai dépassé laisse un résultat
    /// INCONNU, même si le conteneur n'est pas encore consigné (le moteur peut l'avoir créé sans que l'agent
    /// ait lu sa réponse).
    create_sent: AtomicBool,
}

/// Le fil de copie d'un monde. Si le futur qui l'attend est abandonné (délai, arrêt de l'agent), il est arrêté ET
/// attendu : la copie ne continue pas d'écrire dans un dossier d'import que `settle` retire aussitôt, et la place
/// `heavy_io` n'est pas rendue pendant qu'elle tourne. L'arrêt est vu à chaque morceau lu (256 Kio).
struct CopyWorker {
    control: Arc<Control>,
    handle: Option<std::thread::JoinHandle<Result<CopyReport, CopyError>>>,
}

impl CopyWorker {
    fn is_finished(&self) -> bool {
        self.handle
            .as_ref()
            .is_none_or(std::thread::JoinHandle::is_finished)
    }

    fn join(&mut self) -> Option<Result<CopyReport, CopyError>> {
        self.handle.take().and_then(|handle| handle.join().ok())
    }
}

impl Drop for CopyWorker {
    fn drop(&mut self) {
        self.control.cancel.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// La reprise d'un monde, telle que le travail l'emporte (HRT-105).
struct WorldJob {
    prepared: WorldPrepared,
    /// « Garder cette copie » d'avance (`BR-MONDE-009`).
    keep_if_changed: bool,
    /// Le dossier d'import tant qu'il n'est pas mis en place : retiré si l'opération ne va pas au bout.
    import: std::sync::Mutex<Option<ImportDir>>,
    /// Ce que la copie a fait, pour le journal d'activité (`BR-MONDE-023`).
    note: std::sync::Mutex<WorldProgress>,
}

#[derive(Debug, Clone, Copy)]
struct WorldProgress {
    files: u64,
    bytes: u64,
    result: WorldResult,
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
    ids: Arc<dyn IdGen>,
    clock: Arc<dyn Clock>,
    context: DeployContext,
    running: Arc<AtomicUsize>,
    /// L'installation depuis une fiche (HRT-101) : juge la demande et la traduit en description ordinaire.
    installer: FicheInstaller,
    /// Les phases d'un serveur de jeu installé depuis une fiche (BR-FICHE-028), en mémoire.
    phases: Arc<GamePhases>,
    /// Reprendre un monde existant (HRT-105) ; absent : la reprise est refusée.
    world_import: Option<Arc<WorldImport>>,
    /// Le jugement et le rangement des packs déposés (HRT-124) : posé une fois par la racine de composition ;
    /// sans lui, une installation qui porte un pack est refusée.
    packs: OnceLock<Arc<PackIntake>>,
    /// Un seul pack est lu à la fois : lire un pack relit toute l'archive (jusqu'à 4 Gio), donc un compte qui
    /// enchaîne les lectures ne doit pas occuper l'agent.
    pack_gate: Arc<tokio::sync::Semaphore>,
}

/// Le pack à ranger : le dépôt jugé, son emplacement fixe dans le dossier géré des packs, son empreinte.
struct JobPack {
    deposit: DepositId,
    place: PackPlacement,
    sha256: [u8; 32],
}

/// Les ports du serveur tels que le domaine les juge (`PortWorld`) : le moteur, les sockets en écoute, le port de
/// l'agent.
struct World {
    registry: Arc<ServicesRegistry>,
    listeners: Vec<Listener>,
    host: Arc<dyn HostPorts>,
    agent_port: u16,
}

impl PortWorld for World {
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
        ids: Arc<dyn IdGen>,
        clock: Arc<dyn Clock>,
        context: DeployContext,
        catalog: Arc<CatalogService>,
        random: Arc<dyn SecretRandom>,
        phases: Arc<GamePhases>,
        world_import: Option<Arc<WorldImport>>,
    ) -> Arc<Self> {
        let installer = FicheInstaller::new(catalog, random, Arc::clone(&clock));
        Arc::new(Self {
            registry,
            ops,
            specs,
            engine,
            host_ports,
            free_space,
            safe_fs,
            containers,
            ids,
            clock,
            context,
            running: Arc::new(AtomicUsize::new(0)),
            installer,
            phases,
            world_import,
            packs: OnceLock::new(),
            pack_gate: Arc::new(tokio::sync::Semaphore::new(1)),
        })
    }

    /// Ce que l'agent lit dans un pack déposé par ce compte (HRT-124, BR-MOD-007) : rien n'est rangé ni écrit. Un
    /// pack refusé est retiré de la zone d'attente aussitôt.
    pub async fn inspect_pack(
        &self,
        deposit: &str,
        actor: &Actor,
    ) -> Result<PackInspectView, DeployError> {
        Ok(pack_view(&self.judge_deposit(deposit, actor).await?))
    }

    /// Juge un dépôt, hors du fil asynchrone (la lecture relit toute l'archive), une lecture de pack à la fois.
    async fn judge_deposit(&self, deposit: &str, actor: &Actor) -> Result<JudgedPack, DeployError> {
        let intake = Arc::clone(self.packs.get().ok_or_else(pack_unavailable)?);
        let permit = self.pack_permit()?;
        let deposit = deposit.to_owned();
        let account = account_of(actor);
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            intake.judge(&deposit, &account)
        })
        .await
        .map_err(|_| DeployError::Internal("jugement du pack interrompu".into()))?
        .map_err(DeployError::Pack)
    }

    /// La place de lecture d'un pack : une seule à la fois, jamais d'attente (`PackBusy`).
    fn pack_permit(&self) -> Result<tokio::sync::OwnedSemaphorePermit, DeployError> {
        Arc::clone(&self.pack_gate)
            .try_acquire_owned()
            .map_err(|_| DeployError::PackBusy)
    }

    /// Branche le jugement des packs déposés (racine de composition, une seule fois).
    pub fn set_pack_intake(&self, intake: Arc<PackIntake>) {
        let _ = self.packs.set(intake);
    }

    /// Juge le pack déposé que la demande porte, hors du fil asynchrone (lit tout le fichier), et vérifie la place
    /// : le pack se range tel quel, puis le programme du jeu le déballe dans le dossier du service.
    async fn judge_pack(
        &self,
        spec: &DeploySpec,
        actor: &Actor,
        free_disk: Option<u64>,
    ) -> Result<Option<JudgedPack>, DeployError> {
        let Some(DeploySource::Fiche {
            pack: Some(pack), ..
        }) = &spec.source
        else {
            return Ok(None);
        };
        let judged = self.judge_deposit(&pack.deposit, actor).await?;
        if let Some(free) = free_disk {
            let needed = judged.size.saturating_add(judged.report.unpacked);
            if needed > free {
                return Err(DeployError::NotEnoughSpace {
                    needed,
                    free,
                    missing: needed - free,
                });
            }
        }
        Ok(Some(judged))
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

    /// L'espace libre sous ce chemin, lu hors de la tâche asynchrone. `None` : la mesure a échoué, jamais lue
    /// comme « beaucoup de place » ni comme « plein ».
    async fn free_bytes_at(&self, dir: String) -> Option<u64> {
        let free_space = self.free_space.clone();
        tokio::task::spawn_blocking(move || free_space.free_bytes(Path::new(&dir)))
            .await
            .ok()?
            .ok()
    }

    /// L'espace libre où le moteur range ses images (`BR-DEPLOY-018`).
    async fn disk(&self, info: Option<&EngineInfo>) -> Option<DiskCheck> {
        let dir = info?.storage_dir.clone()?;
        let free = self.free_bytes_at(dir).await?;
        Some(DiskCheck {
            free_bytes: free,
            low: free < LOW_DISK_BYTES,
        })
    }

    /// L'espace libre qu'une installation depuis une fiche oppose à son besoin (BR-FICHE-023) : le disque de la
    /// racine de données de Hearth ET celui où le moteur range ses images ; le plus petit des deux, car l'un OU
    /// l'autre sous le besoin refuse. Un disque illisible n'est pas jugé (jamais lu comme « beaucoup de place »
    /// ni comme « plein »).
    async fn install_free_space(&self, info: Option<&EngineInfo>) -> Option<u64> {
        let images = self.disk(info).await.map(|disk| disk.free_bytes);
        let data = self.free_bytes_at(self.context.services_root.clone()).await;
        match (images, data) {
            (Some(images), Some(data)) => Some(images.min(data)),
            (images, data) => images.or(data),
        }
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
        let (name, name_holder) = match validate_name(&request.name) {
            Err(_) => {
                invalid_fields.push("name".to_owned());
                (NameStatus::Invalid, None)
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
        let ports = self.judge_ports(request.ports.clone()).await?;
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
            name_holder,
            invalid_fields,
            ports: ports.into(),
            mounts: mounts.into(),
            disk,
            image_present,
        })
    }

    /// Le constat sur un nom, et, quand un conteneur non adopté le porte, son nom (à défaut son image) : de quoi
    /// dire à l'administrateur où regarder (`BR-ADOPT-022`, HRT-214).
    async fn name_status(&self, name: &str) -> Result<(NameStatus, Option<String>), DeployError> {
        let folded = fold_name(name);
        if let Some(by) = self
            .ops
            .locks()
            .with(|table| table.name_reserved_by(&folded))
        {
            return Ok((
                if by == OpKind::Delete {
                    NameStatus::BeingDeleted
                } else {
                    NameStatus::Taken
                },
                None,
            ));
        }
        if let Some((_, label)) = self.registry.name_claim(name) {
            return Ok((NameStatus::Taken, label));
        }
        match self.specs_holds(&folded).await {
            Ok(true) => Ok((NameStatus::Taken, None)),
            Ok(false) => Ok((NameStatus::Ok, None)),
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

    /// Le constat de chaque port du formulaire, jugé HORS de la tâche asynchrone : la lecture des sockets en écoute
    /// et la recherche du nom d'un programme parcourent `/proc` (tous les processus, pour chaque port en conflit).
    /// `Err` : le jugement n'a pas abouti, jamais lu comme « libre ».
    async fn judge_ports(
        &self,
        ports: Vec<hearth_proto::api::deploy::DeployPort>,
    ) -> Result<Vec<PortCheck>, DeployError> {
        let registry = Arc::clone(&self.registry);
        let host = Arc::clone(&self.host_ports);
        let agent_port = self.context.agent_port;
        tokio::task::spawn_blocking(move || {
            let world = World {
                registry,
                listeners: host.listening().unwrap_or_default(),
                host,
                agent_port,
            };
            Self::judge_ports_sync(&world, &ports)
        })
        .await
        .map_err(|_| DeployError::Internal("jugement des ports interrompu".into()))
    }

    fn judge_ports_sync(
        world: &World,
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
        // Reprendre un monde (HRT-105) : jugé AVANT la traduction, qui en prend le nom et la place à copier.
        let world = self.judge_world(&spec).await?;
        // Une fiche est traduite puis jugée comme une image à la main ; sinon le formulaire (BR-FICHE-013).
        let (valid, files, install, pack) = if spec.source.is_some() {
            let free = self.install_free_space(info.as_ref()).await;
            let prep = world.as_ref().map(|(prepared, _)| WorldPrep {
                name: &prepared.name,
                names_setting: &prepared.rules.names_setting,
                bytes: prepared.bytes,
            });
            let judged = self.judge_pack(&spec, &actor, free).await?;
            let prepared = self.installer.prepare(
                &spec,
                &actor,
                machine,
                free,
                prep.as_ref(),
                judged.as_ref().map(|j| j.report.genre),
            )?;
            let pack = match (judged, prepared.pack) {
                (Some(judged), Some(place)) => Some(JobPack {
                    deposit: judged.id,
                    place,
                    sha256: judged.sha256,
                }),
                (None, None) => None,
                // Un pack jugé sans emplacement (ou l'inverse) ne s'oublie jamais en silence.
                _ => {
                    return Err(DeployError::Internal(
                        "pack jugé sans emplacement ou emplacement sans pack".into(),
                    ));
                }
            };
            (prepared.valid, prepared.files, Some(prepared.audit), pack)
        } else {
            let valid = validate(&spec, machine).map_err(|refusal| self.refusal(&spec, refusal))?;
            (valid, Vec::new(), None, None)
        };

        // Deux déploiements au plus, tous comptes confondus (BR-DEPLOY-023).
        if self.running.fetch_add(1, Ordering::SeqCst) >= MAX_CONCURRENT_DEPLOYMENTS {
            self.running.fetch_sub(1, Ordering::SeqCst);
            return Err(DeployError::DeployLimit);
        }
        let slot = Slot(Arc::clone(&self.running));

        let folded = valid.body.name_folded();
        match self.name_status(&valid.body.name).await? {
            (NameStatus::Ok, _) => {}
            (NameStatus::BeingDeleted, _) => return Err(DeployError::NameBeingDeleted),
            (_, holder) => return Err(DeployError::NameTaken { holder }),
        }
        self.check_ports_for(&valid).await?;
        let image = valid.image();
        let reference = image_reference(&valid.body.image);
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
        let plan = if world.is_some() {
            op_plan_world()
        } else {
            op_plan()
        }
        .map_err(|error| DeployError::Internal(error.to_string()))?;
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
                // La reprise d'un monde partage la place des opérations lourdes (`BR-MONDE-013`).
                heavy_io: world.is_some(),
            })
            .await
            .map_err(|error| match error {
                OpError::Locked(LockRefusal::HeavyIoBusy { .. }) => DeployError::HeavyIoBusy,
                OpError::Locked(LockRefusal::NameReserved { by }) => {
                    if by == OpKind::Delete {
                        DeployError::NameBeingDeleted
                    } else {
                        DeployError::NameTaken { holder: None }
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
                SpecServiceError::NameTaken => DeployError::NameTaken { holder: None },
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
            account,
            actor,
            host_ports: valid.body.ports.iter().map(|p| p.host_port).collect(),
            env_count: u32::try_from(valid.env_count()).unwrap_or(u32::MAX),
            custom_command: valid.has_command(),
            mounts: valid.body.mounts.iter().map(|m| m.source.clone()).collect(),
            reference,
            files,
            install,
            world: world.map(|(prepared, keep_if_changed)| WorldJob {
                prepared,
                keep_if_changed,
                import: std::sync::Mutex::new(None),
                note: std::sync::Mutex::new(WorldProgress {
                    files: 0,
                    bytes: 0,
                    result: WorldResult::NotReached,
                }),
            }),
            pack,
            create_sent: AtomicBool::new(false),
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

    /// Juge la reprise d'un monde demandée par une fiche : le monde existe sous un dossier ouvert, c'est un monde de
    /// la fiche, sa forme est reprise par cette version, son nom est accepté. Rien n'est écrit.
    async fn judge_world(
        &self,
        spec: &DeploySpec,
    ) -> Result<Option<(WorldPrepared, bool)>, DeployError> {
        let Some(DeploySource::Fiche {
            id,
            world: Some(pick),
            ..
        }) = &spec.source
        else {
            return Ok(None);
        };
        let invalid = |reason| {
            DeployError::Invalid(Invalid {
                field: "source.world",
                reason,
            })
        };
        let Some(world_import) = &self.world_import else {
            return Err(invalid("unavailable"));
        };
        let prepared = world_import
            .prepare(id, &pick.world)
            .await
            .map_err(|error| invalid(error.reason()))?;
        Ok(Some((prepared, pick.keep_if_changed)))
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
        self.check_ports(valid.ports.clone(), valid.accept.reserved_ports)
            .await
    }

    /// Les ports donnés sont libres, ou réservés par un service arrêté et la case est cochée. Aussi le jugement des
    /// ports NOUVEAUX d'une modification (HRT-87, BR-EDIT-008) : l'appelant n'envoie pas ceux que le service tient
    /// déjà, qui ne sont pas un conflit avec lui-même.
    pub(crate) async fn check_ports(
        &self,
        ports: Vec<hearth_proto::api::deploy::DeployPort>,
        reserved_accepted: bool,
    ) -> Result<(), DeployError> {
        if ports.is_empty() {
            return Ok(());
        }
        for check in self.judge_ports(ports).await? {
            let blocks = match check.status {
                PortStatus::Free => false,
                PortStatus::ReservedByStopped => !reserved_accepted,
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
        // La copie d'un monde a son propre plafond (six heures, `BR-MONDE-011`) en plus de celui du déploiement.
        // Un budget fixe de cinq minutes reste après la copie pour créer, démarrer et surveiller.
        let copy = if job.world.is_some() {
            COPY_MAX + Duration::from_secs(300)
        } else {
            Duration::ZERO
        };
        let deadline = started + self.context.timings.deadline;
        let outer = deadline + copy;
        let finish = match tokio::time::timeout_at(outer, self.run(&job, deadline)).await {
            Ok(finish) => finish,
            // Trente minutes (BR-DEPLOY-034) : ce qui n'a pas été créé n'existe pas ; sinon le résultat est inconnu.
            // L'ordre de création parti suffit : le délai peut couper l'attente de sa réponse, le conteneur existe
            // peut-être sans être consigné, et rien ne doit être retiré sous lui (BR-DEPLOY-028).
            Err(_) => {
                if job.create_sent.load(Ordering::SeqCst) || self.created(&job.key).await {
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
        let reference = job.reference.clone();
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

        // 2 bis. Copie du monde repris, annulable (`BR-MONDE-013`) : dans un dossier d'import neuf, relue.
        if let Some(world) = &job.world
            && let Err(finish) = self.import_world(job, world, deadline + COPY_MAX).await
        {
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
        job.create_sent.store(true, Ordering::SeqCst);
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
                // Un serveur de jeu installé depuis une fiche : ses phases commencent à ce lancement, le premier
                // (BR-FICHE-028, BR-FICHE-056).
                if let Some(install) = &job.install {
                    self.phases
                        .begin(&job.key, &container, install.rule.clone(), true);
                }
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
        // Le monde repris est mis en place dans son dossier géré, une fois ce dossier créé et AVANT la création du
        // conteneur : renommage du dossier d'import, jamais par-dessus un contenu (`BR-MONDE-014`).
        let placement = match (&job.world, &self.world_import) {
            (Some(world), Some(world_import)) => world
                .import
                .lock()
                .ok()
                .and_then(|held| held.clone())
                .map(|import| {
                    (
                        Arc::clone(world_import.fs()),
                        import,
                        world.prepared.rules.folder_source.clone(),
                    )
                }),
            _ => None,
        };
        // Le pack déposé (HRT-124) : l'emplacement et l'empreinte jugés, rangés en lecture seule pour le conteneur.
        let pack = job.pack.as_ref().and_then(|pack| {
            let rel = RelPath::parse(&format!(
                "{}/{}",
                pack.place.folder_source, pack.place.file_name
            ))
            .ok()?;
            Some((pack.deposit.clone(), rel, pack.sha256))
        });
        let wants_pack = job.pack.is_some();
        let account = job.account.clone();
        let deposits = self.packs.get().map(|intake| Arc::clone(intake.deposits()));
        // Les fichiers produits (BR-FICHE-024) : octets, chemin, propriétaire et droits décidés par le domaine.
        let files: Vec<ProducedBytes> = job
            .files
            .iter()
            .map(|file| ProducedBytes {
                folder: file.folder_source.clone(),
                path: file.path.clone(),
                uid: file.owner_uid,
                gid: file.owner_gid,
                mode: file.mode,
                bytes: zeroize::Zeroizing::new(file.content.expose().as_bytes().to_vec()),
            })
            .collect();
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
            if let Some((world_fs, import, folder)) = &placement {
                world_fs.place(import, &key, folder).map_err(|_| ())?;
            }
            for file in files {
                let rel =
                    RelPath::parse(&format!("{}/{}", file.folder, file.path)).map_err(|_| ())?;
                // Les dossiers intermédiaires d'un chemin imbriqué, créés comme les autres.
                if let Some((_, parents)) = rel.split_last()
                    && parents.len() > 1
                {
                    let parent = RelPath::parse(&parents.join("/")).map_err(|_| ())?;
                    root.mkdir(&parent, perms).map_err(|_| ())?;
                }
                let file_perms = Ownership::new(file.uid, file.gid)
                    .and_then(|owner| Perms::new(owner, file.mode))
                    .ok_or(())?;
                root.write_atomic(&rel, &file.bytes, file_perms)
                    .map_err(|_| ())?;
            }
            if wants_pack {
                let (Some((deposit, rel, sha256)), Some(deposits)) = (&pack, &deposits) else {
                    return Err(());
                };
                deposits
                    .place(deposit, &account, &key, rel, sha256)
                    .map_err(|_| ())?;
            }
            Ok(())
        })
        .await;
        match result {
            Ok(Ok(())) => {
                // Mis en place : le dossier d'import n'existe plus sous son nom, il n'y a plus rien à retirer.
                if let Some(world) = &job.world
                    && let Ok(mut held) = world.import.lock()
                {
                    *held = None;
                }
                Ok(())
            }
            _ => {
                tracing::error!("dossiers de données non créés : rien n'est lancé");
                Err(Finish::failed(Cause::StepFailed))
            }
        }
    }

    /// Écrit l'`intent` d'une étape avec ce qu'elle a créé (le dossier d'import, `BR-MONDE-026`).
    async fn begin_with(&self, job: &Job, name: &str, undo: &UndoInfo) -> Result<(), Finish> {
        match self.ops.intent(&job.id, name, undo).await {
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

    fn note_world(&self, world: &WorldJob, files: u64, bytes: u64, result: WorldResult) {
        if let Ok(mut note) = world.note.lock() {
            *note = WorldProgress {
                files,
                bytes,
                result,
            };
        }
    }

    /// Copie le monde dans un dossier d'import neuf, sur un fil dédié à priorité basse, en suivant la progression
    /// et l'annulation, puis relit la copie (`BR-MONDE-004`, `009`, `010`, `011`, `013`). La source n'est que lue.
    async fn import_world(
        &self,
        job: &Job,
        world: &WorldJob,
        limit: Instant,
    ) -> Result<(), Finish> {
        let Some(world_import) = self.world_import.clone() else {
            return Err(Finish::failed(Cause::StepFailed));
        };
        // Le plan est refait ici : l'image s'est récupérée avant, le monde a pu changer depuis la demande.
        let plan = match world_import.replan(&world.prepared).await {
            Ok(plan) => plan,
            Err(_) => {
                self.note_world(world, 0, 0, WorldResult::Failed);
                return Err(Finish::failed(Cause::StepFailed));
            }
        };
        let total_bytes = plan.bytes;
        // Le monde a pu grossir depuis le contrôle d'espace de la demande : la différence est revérifiée
        // (`BR-MONDE-012`), sans case pour passer outre.
        let grown = plan.bytes.saturating_sub(world.prepared.bytes);
        if grown > 0
            && self
                .free_bytes_at(self.context.services_root.clone())
                .await
                .is_some_and(|free| free < grown)
        {
            self.note_world(world, 0, 0, WorldResult::Failed);
            return Err(Finish::failed(Cause::DiskFull));
        }
        let world_fs = Arc::clone(world_import.fs());
        let (op, owner) = (job.id.clone(), world.prepared.owner);
        let created = tokio::task::spawn_blocking({
            let world_fs = Arc::clone(&world_fs);
            move || world_fs.create_import(&op, owner)
        })
        .await;
        let (sink, import) = match created {
            Ok(Ok(made)) => made,
            _ => {
                self.note_world(world, 0, 0, WorldResult::Failed);
                return Err(Finish::failed(Cause::StepFailed));
            }
        };
        if let Ok(mut held) = world.import.lock() {
            *held = Some(import.clone());
        }
        // Le dossier d'import est nommé au journal AVANT la première donnée : un agent qui s'arrête pendant la copie
        // le retire au démarrage, après vérification de l'inode (`BR-MONDE-026`).
        let undo = UndoInfo {
            created: vec![CreatedObject {
                root: ObjectRoot::Staging,
                rel: import.rel.clone(),
                dev: import.dev,
                ino: import.ino,
                kind: ObjectKind::ImportDir,
            }],
        };
        self.begin_with(job, step::WORLD, &undo).await?;

        let control = Arc::new(Control::default());
        // Le plafond de six heures ne dépasse jamais ce qui reste au déploiement.
        let until = (std::time::Instant::now() + COPY_MAX).min(limit.into_std());
        let worker = {
            let (control, root, copy_fs) = (
                Arc::clone(&control),
                world.prepared.root.clone(),
                Arc::clone(&world_fs),
            );
            std::thread::Builder::new()
                .name("hearth-world-copy".into())
                .spawn(move || -> Result<CopyReport, CopyError> {
                    // Priorité basse pour ne pas ralentir les autres services (`BR-MONDE-011`).
                    copy_fs.lower_priority();
                    let source = copy_fs.open_source(&root).map_err(CopyError::Source)?;
                    copy_world(source.as_ref(), sink.as_ref(), &plan, &control, until)
                })
        };
        let Ok(handle) = worker else {
            journal(self.ops.failed(&job.id, step::WORLD).await, "failed");
            self.note_world(world, 0, 0, WorldResult::Failed);
            return Err(Finish::failed(Cause::StepFailed));
        };
        let mut worker = CopyWorker {
            control: Arc::clone(&control),
            handle: Some(handle),
        };
        let mut ticks = 0u32;
        while !worker.is_finished() {
            tokio::time::sleep(Duration::from_millis(250)).await;
            ticks += 1;
            if !ticks.is_multiple_of(4) {
                continue;
            }
            // Une fois par seconde : la progression, et l'annulation éventuelle.
            let (files, bytes) = (
                control.files_done.load(Ordering::SeqCst),
                control.bytes_done.load(Ordering::SeqCst),
            );
            let percent = (total_bytes > 0)
                .then(|| u8::try_from((bytes.saturating_mul(100) / total_bytes).min(100)).ok())
                .flatten();
            journal(
                self.ops
                    .set_progress(
                        &job.id,
                        &Progress {
                            percent,
                            items_done: Some(files),
                            bytes_done: Some(bytes),
                            bytes_total: Some(total_bytes),
                        },
                    )
                    .await,
                "progress",
            );
            if self.is_cancelling(&job.id).await {
                control.cancel.store(true, Ordering::SeqCst);
            }
        }
        let outcome = worker.join().ok_or(());
        let (files, bytes) = (
            control.files_done.load(Ordering::SeqCst),
            control.bytes_done.load(Ordering::SeqCst),
        );
        match outcome {
            Ok(Ok(report)) if report.changed == 0 => {
                self.note_world(world, report.files, report.bytes, WorldResult::Copied);
                journal(self.ops.done(&job.id, step::WORLD, &undo).await, "done");
                if self.is_cancelling(&job.id).await {
                    return Err(Finish::cancelled());
                }
                Ok(())
            }
            // Des fichiers ont changé pendant la copie : elle est gardée si le compte l'a demandé d'avance
            // (« Garder cette copie »), sinon retirée (« Recommencer », `BR-MONDE-009`).
            Ok(Ok(report)) if world.keep_if_changed => {
                self.note_world(world, report.files, report.bytes, WorldResult::KeptChanged);
                journal(self.ops.done(&job.id, step::WORLD, &undo).await, "done");
                if self.is_cancelling(&job.id).await {
                    return Err(Finish::cancelled());
                }
                Ok(())
            }
            Ok(Ok(report)) => {
                self.note_world(
                    world,
                    report.files,
                    report.bytes,
                    WorldResult::ChangedDiscarded,
                );
                journal(self.ops.failed(&job.id, step::WORLD).await, "failed");
                Err(Finish::failed(Cause::StepFailed))
            }
            Ok(Err(CopyError::Cancelled)) => {
                self.note_world(world, files, bytes, WorldResult::Cancelled);
                journal(self.ops.failed(&job.id, step::WORLD).await, "failed");
                Err(Finish::cancelled())
            }
            Ok(Err(CopyError::Incomplete)) => {
                self.note_world(world, files, bytes, WorldResult::Incomplete);
                journal(self.ops.failed(&job.id, step::WORLD).await, "failed");
                Err(Finish::failed(Cause::StepFailed))
            }
            Ok(Err(CopyError::TimedOut)) => {
                self.note_world(world, files, bytes, WorldResult::Failed);
                journal(self.ops.failed(&job.id, step::WORLD).await, "failed");
                Err(Finish::failed(Cause::Timeout))
            }
            Ok(Err(_)) | Err(_) => {
                self.note_world(world, files, bytes, WorldResult::Failed);
                journal(self.ops.failed(&job.id, step::WORLD).await, "failed");
                Err(Finish::failed(Cause::StepFailed))
            }
        }
    }

    /// Retire le dossier d'import s'il n'a pas été mis en place : seulement lui, vérifié par son inode.
    async fn discard_import(&self, job: &Job) {
        let (Some(world), Some(world_import)) = (&job.world, &self.world_import) else {
            return;
        };
        let import = world.import.lock().ok().and_then(|mut held| held.take());
        let Some(import) = import else {
            return;
        };
        let world_fs = Arc::clone(world_import.fs());
        let removed = tokio::task::spawn_blocking(move || world_fs.remove_import(&import)).await;
        if !matches!(removed, Ok(Ok(()))) {
            tracing::warn!(operation = %job.id, "dossier d'import non retiré");
        }
    }

    /// Ce que la reprise a fait, pour le journal d'activité : le dossier ouvert tel qu'il est écrit, des nombres.
    fn world_note(&self, job: &Job, finish: &Finish) -> Option<WorldNote> {
        let world = job.world.as_ref()?;
        let progress = *world.note.lock().ok()?;
        // Une copie réussie suivie d'un échec plus tard (création) garde son résultat ; une annulation ou une
        // erreur avant la fin de la copie est dite telle.
        let result = match (progress.result, finish.status) {
            (WorldResult::Copied | WorldResult::KeptChanged, OpStatus::Cancelled) => {
                WorldResult::Cancelled
            }
            (result, _) => result,
        };
        Some(WorldNote {
            folder: ClientName::parse(&world.prepared.folder)?,
            files: progress.files,
            bytes: progress.bytes,
            not_copied: world.prepared.not_copied,
            result,
        })
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
        // Un dossier d'import pas mis en place est retiré, quelle que soit l'issue (seul lui, `BR-MONDE-013`).
        self.discard_import(job).await;
        if finish.clean {
            self.forget_spec(&job.key).await;
            self.remove_folders(job).await;
        }
        // La ligne « Déploiement en cours » part AVANT la clôture : qui lit l'issue ne la voit plus dans la liste.
        self.registry.end_deploying(&job.key);
        // FIX:01M5NYA5W781YW5NFQ7W1JGR98 L'issue et l'entrée du journal d'activité sont écrites dans UNE
        // transaction, puis annoncées : une transition refusée (une annulation est passée entre-temps)
        // n'écrit pas de ligne, et qui lit l'issue (flux ou GET) relit un journal complet.
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
            Err(OpError::IllegalTransition | OpError::Stale) => tracing::info!(
                operation = %job.id,
                "déploiement non clos : l'opération a changé de statut entre-temps (annulation)"
            ),
            Err(error) => tracing::error!(%error, "déploiement non clos dans le journal"),
        }
        // La trace dit l'opération, jamais le service, l'image ni une variable.
        tracing::info!(
            operation = %job.id,
            status = ?finish.status,
            cause = finish.cause.map(Cause::as_str),
            duration_ms = u64::try_from(took.as_millis()).unwrap_or(u64::MAX),
            "déploiement terminé"
        );
    }

    /// Une entrée au journal d'activité (`BR-DEPLOY-025`) : le compte, le service, l'image et la version, les
    /// ports du serveur, le nombre de variables, la commande oui ou non, l'issue et sa cause typée.
    fn draft(&self, job: &Job, finish: &Finish) -> Option<AuditDraft> {
        let (Some(service), Some(image)) = (
            ClientName::parse(&job.name),
            ClientName::parse(&job.reference),
        ) else {
            return None;
        };
        let stopped = finish.cause == Some(Cause::StoppedRightAfterStart);
        let outcome = match (finish.status, finish.cause) {
            (OpStatus::Succeeded, _) => Outcome::Succeeded,
            (_, cause) => Outcome::Failed(cause.map_or(Reason::Internal, reason_of)),
        };
        // Un serveur de jeu installé depuis une fiche : la fiche, le NOMBRE de réponses, jamais une valeur
        // (BR-FICHE-032).
        let target = match &job.install {
            Some(install) => {
                let fiche = ClientName::parse(&install.fiche)?;
                Target::GameInstall {
                    service,
                    fiche,
                    version: install.version,
                    image,
                    host_ports: job.host_ports.clone(),
                    answers: install.answers,
                    cpu_millis: install.cpu_millis,
                    memory_bytes: install.memory_bytes,
                    agreements: install
                        .agreements
                        .iter()
                        .filter_map(|title| ClientName::parse(title))
                        .collect(),
                    world: self.world_note(job, finish),
                }
            }
            None => Target::ServiceDeploy {
                service,
                image,
                host_ports: job.host_ports.clone(),
                env_count: job.env_count,
                custom_command: job.custom_command,
                stopped_right_after_start: stopped,
            },
        };
        Some(AuditDraft {
            actor: job.actor.clone(),
            action: OpKind::Deploy.audit_action(),
            target,
            outcome,
        })
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

/// Un fichier produit, prêt à être écrit : ses octets peuvent porter un secret, ils sont effacés après l'écriture.
struct ProducedBytes {
    folder: String,
    path: String,
    uid: u32,
    gid: u32,
    mode: u32,
    /// Effacés de la mémoire quand le fichier est écrit ou abandonné, quelle que soit l'issue.
    bytes: zeroize::Zeroizing<Vec<u8>>,
}

/// Le chemin d'un dossier dans le service, normalisé comme le domaine.
fn normalized(target: &str) -> Result<String, ()> {
    deploy::normalize_target(target).map_err(|_| ())
}

fn pack_unavailable() -> DeployError {
    DeployError::Invalid(Invalid {
        field: "source.pack",
        reason: "unavailable",
    })
}

/// Ce que l'écran lit d'un pack jugé.
fn pack_view(judged: &JudgedPack) -> PackInspectView {
    let report = &judged.report;
    PackInspectView {
        genre: match report.genre {
            PackGenre::ServerPack => PackGenreWire::ServerPack,
            PackGenre::ModrinthPack => PackGenreWire::ModrinthPack,
        },
        size_bytes: judged.size,
        files: u64::try_from(report.files).unwrap_or(u64::MAX),
        mods: u64::try_from(report.mods).unwrap_or(u64::MAX),
        unpacked_bytes: report.unpacked,
        top_dirs: report.top_dirs.clone().into(),
        modrinth: report.modrinth.as_ref().map(|m| ModrinthPackView {
            name: m.name.clone(),
            version: m.version.clone(),
            minecraft: m.minecraft.clone(),
            loader: m.loader.as_ref().map(|(name, _)| name.clone()),
            loader_version: m.loader.as_ref().map(|(_, version)| version.clone()),
            downloads: u32::try_from(m.downloads).unwrap_or(u32::MAX),
            hosts: m.hosts.clone().into(),
        }),
    }
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

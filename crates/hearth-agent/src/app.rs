//! Racine de composition : charge la configuration, assemble adaptateurs, cas d'usage et
//! serveur, puis exécute la commande demandée. Seul endroit qui connaît les types concrets.

use std::future::Future;
use std::net::{SocketAddr, TcpListener};
use std::sync::Arc;
use std::time::Duration;

use hearth_proto::fingerprint::Fingerprint;
use thiserror::Error;

mod install;
mod update;

pub use install::local_admins;
pub use update::{Updating, run_supervisor, update_service};

use crate::application::accounts::AccountService;
use crate::application::attack_mode::AttackModeService;
use crate::application::audit::{AuditRecorder, AuditService, AuditTrail};
use crate::application::catalog::CatalogService;
use crate::application::deployments::{DeployContext, DeployTimings, Deployments, FolderGuard};
use crate::application::elevation::Elevations;
use crate::application::folders::FolderFacts;
use crate::application::game_phase::GamePhases;
use crate::application::game_settings::GameSettings;
use crate::application::hello::HelloService;
use crate::application::maintenance::MaintenanceService;
use crate::application::metrics::MetricsService;
use crate::application::operations::OperationService;
use crate::application::ports::{
    AuditFeed, AuditSink, BootInfo, ChallengeCrypto, Clock, ContainerEngine, CryptoError,
    FingerprintSecretError, FingerprintSecretStore, FreeSpace, GpuProbe, HashError,
    HostEnvironment, HostPorts, IdGen, IdentityError, IdentityStore, InstallHost, MonotonicClock,
    PasswordHasher, PublicIdentity, RequestFingerprinter, SafeFs, SecretRandom, SecurityFeed,
    ServiceObserver, ServiceSupervisor, Store, StoreError, SystemProbe, TokenGen,
};
use crate::application::secret_vault::{KeyStatus, SecretVault};
use crate::application::security::SecurityService;
use crate::application::service_adoption::ServiceAdoption;
use crate::application::service_lifecycle::{
    LifecycleTimings, ServiceLifecycle, record_interrupted,
};
use crate::application::service_logs::{LogSettings, ServiceLogs};
use crate::application::service_op_access::ServiceOpAccess;
use crate::application::service_ops::{InterruptedAction, OpLocks, RecoverySummary, ServiceOps};
use crate::application::service_specs::ServiceSpecs;
use crate::application::services::{EngineTimings, ServicesRegistry};
use crate::application::services_right::ServicesRightService;
use crate::application::sessions::SessionService;
use crate::application::trust::TrustService;
use crate::domain::ops::ObjectRoot;
use crate::domain::services::BrakeSettings;
use crate::entrypoint::account::{self, AccountCliError};
use crate::entrypoint::attack_mode as attack_mode_cli;
use crate::entrypoint::cli::{AccountAction, Cli, Command};
use crate::entrypoint::http::{self, AppState, ServerError, ServerHandle};
use crate::entrypoint::install::InstallCliError;
use crate::entrypoint::signal::shutdown_signal;
use crate::entrypoint::tasks::{self, BackgroundTask};
use crate::entrypoint::terminal::TerminalPasswords;
use crate::entrypoint::ws::{StreamContext, StreamSettings};
use crate::infrastructure::argon2::Argon2Hasher;
use crate::infrastructure::audit_feed::BroadcastAuditFeed;
use crate::infrastructure::clock::{SystemClock, SystemMonotonic};
use crate::infrastructure::config::{self, AgentConfig, CliOverrides, ConfigError};
use crate::infrastructure::created_objects::FsCreatedObjects;
use crate::infrastructure::crypto::{HmacChallengeCrypto, RingProofVerifier};
use crate::infrastructure::data_dir;
use crate::infrastructure::engine::BollardEngine;
use crate::infrastructure::fingerprint::{HmacFingerprinter, NoFingerprint};
use crate::infrastructure::fingerprint_secret::FileFingerprintSecretStore;
use crate::infrastructure::fs::{SERVICES_DIR, SystemSafeFs};
use crate::infrastructure::host::folders::SystemFolders;
use crate::infrastructure::host::listening_ports::SystemHostPorts;
use crate::infrastructure::ids::UlidGen;
use crate::infrastructure::install::SystemHost;
use crate::infrastructure::ops_observer::{
    EngineObserver, SpecRecordedContainers, UnobservedState,
};
use crate::infrastructure::random::{OsSecretRandom, OsTokenGen};
use crate::infrastructure::sealer::RingSealerFactory;
use crate::infrastructure::security_feed::BroadcastSecurityFeed;
use crate::infrastructure::services_key::FileServicesKeyStore;
use crate::infrastructure::sqlite::{
    Database, DatabaseError, ServiceDatabase, SqliteAccountRepo, SqliteAttackModeRepo,
    SqliteAuditRepo, SqliteDeviceRepo, SqliteGameSettingsRepo, SqliteKnownAddressRepo,
    SqliteLoginAttemptRepo, SqliteOpJournal, SqliteOperationRepo, SqliteServiceRepo,
    SqliteSessionRepo, SqliteSpecRepo, SqliteStore,
};
use crate::infrastructure::supervisor::{NoSupervisor, ProcSystemd};
use crate::infrastructure::system::gpu;
use crate::infrastructure::system::{
    ProcBootInfo, SysinfoProbe, SystemHostEnvironment, SystemMachineInfo,
};
use crate::infrastructure::tls::{self, FileIdentityStore, TlsError};

#[derive(Debug, Error)]
pub enum AppError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Identity(#[from] IdentityError),
    #[error(transparent)]
    Tls(#[from] TlsError),
    #[error(transparent)]
    FingerprintSecret(#[from] FingerprintSecretError),
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error(transparent)]
    Hash(#[from] HashError),
    #[error(transparent)]
    Account(#[from] AccountCliError),
    #[error(transparent)]
    Install(#[from] InstallCliError),
    #[error("mise à jour de l'agent : {0}")]
    Update(String),
    #[error("superviseur de mise à jour : {detail}")]
    Supervise { detail: String },
    #[error("chemin du binaire en cours d'exécution introuvable : {0}")]
    CurrentExe(std::io::Error),
    #[error(transparent)]
    Version(crate::domain::install::VersionError),
    #[error(transparent)]
    Server(#[from] ServerError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Crypto(#[from] CryptoError),
    #[error(transparent)]
    AttackMode(#[from] attack_mode_cli::AttackModeCliError),
    #[error("opérations de service : {0}")]
    ServiceOps(#[from] crate::application::service_ops::OpError),
    #[error("ouverture du port {addr} impossible : {source}")]
    Bind {
        addr: SocketAddr,
        source: std::io::Error,
    },
}

/// Les adaptateurs « du monde » dont dépendent les cas d'usage : hachage, horloge, identifiants,
/// hasard. Ceux de production par défaut ; les tests en injectent d'autres (horloge pilotée,
/// hachage à coût minimal).
pub struct Adapters {
    pub hasher: Arc<dyn PasswordHasher>,
    pub clock: Arc<dyn Clock>,
    pub ids: Arc<dyn IdGen>,
    pub tokens: Arc<dyn TokenGen>,
}

impl Adapters {
    pub fn production() -> Result<Self, AppError> {
        Ok(Self {
            hasher: Arc::new(Argon2Hasher::new()?),
            clock: Arc::new(SystemClock),
            ids: Arc::new(UlidGen),
            tokens: Arc::new(OsTokenGen),
        })
    }
}

/// Ce qui mesure la machine et alimente le flux temps réel : sondes, cadence d'échantillonnage,
/// délais du flux. Ceux de production par défaut ; les tests injectent des sondes
/// simulées, une cadence rapide et des délais courts.
pub struct Metering {
    pub system: Arc<dyn SystemProbe>,
    pub gpu: Arc<dyn GpuProbe>,
    /// Horloge murale qui date les échantillons.
    pub clock: Arc<dyn Clock>,
    /// Horloge monotone : cadence, fenêtres et ordre des échantillons.
    pub monotonic: Arc<dyn MonotonicClock>,
    pub period: Duration,
    pub stream: StreamSettings,
}

impl Metering {
    /// Sondes de la machine réelle. À appeler dans un runtime Tokio (la sonde NVIDIA lance son
    /// sous-processus).
    pub fn production() -> Self {
        Self {
            system: Arc::new(SysinfoProbe::new()),
            gpu: gpu::platform_probe(),
            clock: Arc::new(SystemClock),
            monotonic: Arc::new(SystemMonotonic::new()),
            period: tasks::SAMPLE_PERIOD,
            stream: StreamSettings::default(),
        }
    }
}

/// Ce qui parle au moteur de conteneurs (HRT-61) : le moteur lui-même (le port `ContainerEngine`), le
/// superviseur qui dit quelle unité systemd lance un conteneur (HRT-63, détection seule), ce que l'agent
/// sait du serveur et les cadences de la détection. Ceux de production par défaut (le socket de la
/// configuration) ; les tests injectent un faux moteur, un faux superviseur et des cadences courtes.
pub struct EngineParts {
    pub engine: Arc<dyn ContainerEngine>,
    pub supervisor: Arc<dyn ServiceSupervisor>,
    pub host: Arc<dyn HostEnvironment>,
    pub timings: EngineTimings,
    /// Plafond de lectures de journaux et frein d'ouvertures par compte (HRT-107, BR-SLOG-026 et 027).
    /// Les tests qui ne portent pas sur eux le détendent (`BrakeSettings::relaxed`).
    pub log_brake: BrakeSettings,
    /// La surveillance qui suit une action de cycle de vie (HRT-64) ; les tests la raccourcissent.
    pub lifecycle: LifecycleTimings,
    /// Les programmes qui écoutent des ports du serveur (HRT-68) ; les tests y posent un faux.
    pub host_ports: Arc<dyn HostPorts>,
    /// L'espace libre là où le moteur range ses images (HRT-68) ; les tests y posent un faux.
    pub free_space: Arc<dyn FreeSpace>,
    /// Les délais d'un déploiement (HRT-68) ; les tests les raccourcissent.
    pub deploy: DeployTimings,
    /// Les dossiers de données des services (HRT-145, HRT-68). `None` : le vrai, sous le dossier de données de
    /// l'agent ; les tests y posent un faux en mémoire.
    pub safe_fs: Option<Arc<dyn SafeFs>>,
    /// Les fiches de serveurs de jeu proposées et le hasard des secrets qu'elles font tirer (HRT-101).
    pub catalog: CatalogParts,
}

/// Ce qu'il faut au catalogue des fiches et à l'installation depuis une fiche (HRT-101, ADR-0050) : la porte de
/// la fiche d'essai et la source des octets d'un secret tiré par l'agent.
pub struct CatalogParts {
    /// Les fiches `preview` sont proposées (construction d'essai seulement, `HEARTH_PREVIEW_FICHES`).
    pub include_preview: bool,
    pub random: Arc<dyn SecretRandom>,
    /// Un catalogue donné (tests : fiches de fabrication) à la place des fiches embarquées.
    pub service: Option<Arc<CatalogService>>,
}

impl CatalogParts {
    /// La construction courante : la porte de `build_info`, le hasard du système.
    pub fn production() -> Self {
        Self {
            include_preview: crate::build_info::PREVIEW_FICHES,
            random: Arc::new(OsSecretRandom),
            service: None,
        }
    }
}

impl EngineParts {
    /// Le moteur du socket Unix de la configuration. Désactivé, le socket n'est jamais ouvert : la
    /// tâche de détection ne sonde pas (l'adaptateur est construit mais jamais appelé).
    pub fn production(config: &crate::infrastructure::config::EngineConfig) -> Self {
        // Sans systemd (ou hors Linux), il n'y a aucun superviseur à interroger.
        let supervisor: Arc<dyn ServiceSupervisor> = if SystemHostEnvironment.describe().systemd {
            Arc::new(ProcSystemd::system())
        } else {
            Arc::new(NoSupervisor)
        };
        Self {
            engine: Arc::new(BollardEngine::unix(config.socket.clone())),
            supervisor,
            host: Arc::new(SystemHostEnvironment),
            timings: EngineTimings::production(),
            log_brake: BrakeSettings::production(),
            lifecycle: LifecycleTimings::production(),
            host_ports: Arc::new(SystemHostPorts::new()),
            free_space: Arc::new(SystemHost),
            deploy: DeployTimings::production(),
            safe_fs: None,
            catalog: CatalogParts::production(),
        }
    }
}

/// Ce qu'il faut à l'identité d'appareil (HRT-22) et que seul le démarrage du serveur connaît :
/// l'empreinte du certificat que le client épingle (celle que la preuve signe) et l'horloge
/// monotone qui borne la vie d'un défi.
pub struct TrustParts {
    pub fingerprint: Fingerprint,
    pub monotonic: Arc<dyn MonotonicClock>,
    /// Ce que le noyau dit du démarrage en cours (mode attaque, HRT-25).
    pub boot: Arc<dyn BootInfo>,
    /// L'empreinte à clé des requêtes suivies (HRT-32) : secret propre à l'installation.
    pub fingerprinter: Arc<dyn RequestFingerprinter>,
}

/// Les cas d'usage assemblés sur une base ouverte.
pub struct Services {
    pub accounts: Arc<AccountService>,
    pub sessions: Arc<SessionService>,
    /// État de sécurité et alerte (HRT-24).
    pub security: Arc<SecurityService>,
    /// Le mode attaque (HRT-25) ; absent des services sans horloge monotone ni démarrage du noyau.
    pub attack: Option<Arc<AttackModeService>>,
    pub operations: Arc<OperationService>,
    pub maintenance: Arc<MaintenanceService>,
    /// Lecture et export du journal d'activité.
    pub audit: Arc<AuditService>,
    /// Écriture du journal hors transaction (refus et échecs relevés par le routeur).
    pub audit_sink: Arc<dyn AuditSink>,
    /// Le même, pour écrire les synthèses des événements répétés (`flush`).
    pub audit_recorder: Arc<AuditRecorder>,
    /// Le magasin et la diffusion du journal : l'adoption de services écrit sa ligne et son entrée de
    /// journal dans la même transaction (HRT-80).
    pub store: Arc<dyn Store>,
    pub trail: Arc<AuditTrail>,
}

/// Agent démarré : le serveur, la partie publique de son identité et ses tâches de fond.
pub struct RunningAgent {
    pub server: ServerHandle,
    pub identity: PublicIdentity,
    /// Purge périodique : arrêtée avec l'agent.
    pub purge: BackgroundTask,
    /// Échantillonneur des mesures : arrêté avec l'agent.
    pub sampler: BackgroundTask,
    /// Écriture des synthèses du journal : arrêtée avec l'agent.
    pub audit_flush: BackgroundTask,
    /// Fin des épisodes d'alerte (30 minutes sans échec) : arrêtée avec l'agent.
    pub alert_sweep: BackgroundTask,
    /// Mode attaque : reprise après la fenêtre de redémarrage, sortie automatique : arrêtée avec
    /// l'agent.
    pub attack_sweep: Option<BackgroundTask>,
    /// Détection du moteur de conteneurs et suivi des services (HRT-61) : arrêtée avec l'agent.
    pub services_task: BackgroundTask,
    /// Le registre des services (les tests y lisent l'état sans passer par HTTP).
    pub services: Arc<ServicesRegistry>,
    /// Le socle des services créés par Hearth (HRT-83) : description durable, secrets scellés, journal des
    /// opérations. Aucune route ne s'en sert encore.
    pub foundations: ServiceFoundations,
    /// Décisions expirées et opérations de service purgées : arrêtée avec l'agent.
    pub service_ops_sweep: BackgroundTask,
    /// Les sessions : l'agent EXIGE la confirmation des actes
    /// dès la construction du service : aucun réglage, aucune porte, même en test.
    pub sessions: Arc<SessionService>,
    /// Écrit les synthèses du journal en attente à l'arrêt (les tests le vident pour lire le journal).
    pub audit_recorder: Arc<AuditRecorder>,
    /// Les écritures d'alerte en vol, attendues à l'arrêt.
    security: Arc<SecurityService>,
}

impl RunningAgent {
    /// Sert jusqu'à `stop`, tâches de fond comprises.
    pub async fn run_until(self, stop: impl Future<Output = ()>) -> Result<(), ServerError> {
        let Self {
            server,
            purge: _purge,
            sampler: _sampler,
            audit_flush: _audit_flush,
            alert_sweep: _alert_sweep,
            attack_sweep: _attack_sweep,
            services_task: _services_task,
            service_ops_sweep: _service_ops_sweep,
            audit_recorder,
            security,
            ..
        } = self;
        let result = server.run_until(stop).await;
        // Les entrées de début d'alerte lancées hors du chemin des requêtes sont écrites avant de
        // rendre la main : un arrêt du service n'en perd aucune.
        security.settle().await;
        // Les synthèses en attente ne partent pas avec la tâche : écrites avant de rendre la main.
        audit_recorder.flush_all().await;
        result
    }
}

/// Le socle des services créés par Hearth (HRT-83, ADR-0044 et ADR-0045) : la description durable (secrets
/// scellés), le coffre de la clé, le journal des opérations et ses verrous. Construit au démarrage ; les
/// tickets qui écrivent les actes (HRT-64, HRT-68…) s'y branchent.
#[derive(Clone)]
pub struct ServiceFoundations {
    pub specs: Arc<ServiceSpecs>,
    pub vault: Arc<SecretVault>,
    pub ops: Arc<ServiceOps>,
    /// Les actions de cycle de vie et les déploiements que l'arrêt précédent a interrompus (HRT-64, HRT-68) : le
    /// démarrage de l'agent les consigne au journal d'activité.
    pub interrupted_actions: Vec<InterruptedAction>,
    /// Les déploiements interrompus avant la création de leur conteneur (HRT-68) : le démarrage retire leur
    /// description (elle garderait le nom) et leur dossier de données.
    pub abandoned_deploys: Vec<crate::domain::services::ServiceKey>,
}

/// Le socle SANS moteur : les opérations restées en plein geste sont classées avec l'observateur « illisible »
/// (`UnobservedState`), donc « à vérifier » plutôt que supposées bonnes. **Pour les tests** et l'assemblage
/// sans moteur ; l'agent démarre par [`service_foundations_observed`] avec l'observateur du vrai moteur.
pub async fn service_foundations(
    data_dir: &std::path::Path,
    database: &Database,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGen>,
) -> Result<ServiceFoundations, AppError> {
    service_foundations_observed(data_dir, database, clock, ids, &UnobservedState).await
}

/// Où l'agent range, sous son dossier de données, ce que les opérations créent pour elles-mêmes et qu'une
/// reprise ou une annulation retire (`FsCreatedObjects`). Valeurs par défaut du plan d'intégration des
/// serveurs de jeu (sauvegardes et dossier d'attente) : les lots qui écrivent ces objets (HRT-97, HRT-70) les
/// rendront configurables. Une racine qui n'existe pas encore ne retire rien (« déjà parti »).
const BACKUPS_DIR: &str = "backups";
const STAGING_DIR: &str = "staging";

/// Assemble le socle, DIT l'état de la clé des secrets, et classe les opérations restées en plein geste
/// (reprendre le verrou de celles à vérifier, retirer ce que les autres avaient créé pour elles-mêmes).
/// `observer` dit ce que le moteur montre du service de chaque opération non terminée.
///
/// - Aucune clé n'est créée ici : elle naît à la première nécessité.
/// - Une clé absente, fausse ou inutilisable n'arrête PAS l'agent : les services qui tournent continuent,
///   les secrets des services sont dits illisibles, rien n'est supprimé ni régénéré.
/// - Rien d'une opération interrompue n'est repris, rejoué ni défait.
pub async fn service_foundations_observed(
    data_dir: &std::path::Path,
    database: &Database,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGen>,
    observer: &dyn ServiceObserver,
) -> Result<ServiceFoundations, AppError> {
    let spec_repo = Arc::new(SqliteSpecRepo::new(database.pool().clone()));
    let vault = Arc::new(SecretVault::new(
        Arc::new(FileServicesKeyStore::new(data_dir)),
        spec_repo.clone(),
        Arc::new(RingSealerFactory),
    ));
    match vault.status().await {
        Ok(KeyStatus::Ready | KeyStatus::NotNeededYet) => {}
        Ok(KeyStatus::Unavailable(problem)) => tracing::error!(
            %problem,
            advice = problem.advice(),
            "les secrets des services sont illisibles : l'agent démarre, les services en cours ne sont pas touchés, aucune clé n'est régénérée"
        ),
        Err(error) => tracing::error!(
            %error,
            "l'état de la clé des secrets des services n'a pas pu être établi : l'agent démarre"
        ),
    }
    let specs = Arc::new(ServiceSpecs::new(spec_repo, vault.clone(), clock.clone()));
    let ops = Arc::new(ServiceOps::new(
        Arc::new(SqliteOpJournal::new(database.pool().clone())),
        Arc::new(OpLocks::new()),
        clock,
        ids,
        // Les racines des objets qu'une opération crée pour elle-même, revérifiés par leur inode avant d'être
        // retirés (jamais un chemin absolu venu de la base).
        Arc::new(FsCreatedObjects::new([
            (ObjectRoot::Backups, data_dir.join(BACKUPS_DIR)),
            (ObjectRoot::Staging, data_dir.join(STAGING_DIR)),
        ])),
    ));
    // La reprise ne doit JAMAIS empêcher l'agent de démarrer : c'est lui qui sert à réparer le serveur.
    // Si même la liste des opérations non terminées est illisible, les fonctions de service restent
    // refusées par l'index unique de la base, tout le reste fonctionne.
    let summary = match ops.recover_at_startup(observer).await {
        Ok(summary) => summary,
        Err(error) => {
            tracing::error!(
                %error,
                "journal des opérations de service illisible : l'agent démarre, aucune opération n'est reprise"
            );
            RecoverySummary::default()
        }
    };
    if summary.interrupted + summary.needs_attention + summary.locks_restored > 0 {
        tracing::warn!(
            interrupted = summary.interrupted,
            needs_attention = summary.needs_attention,
            locks_restored = summary.locks_restored,
            objects_removed = summary.objects_removed,
            objects_left = summary.objects_left,
            "opérations de service restées en plein geste : classées, rien n'est rejoué"
        );
    }
    Ok(ServiceFoundations {
        specs,
        vault,
        ops,
        interrupted_actions: summary.interrupted_actions,
        abandoned_deploys: summary.abandoned_deploys,
    })
}

/// Fusionne fichier, variables d'environnement et options de la ligne de commande.
pub fn load_config(cli: &Cli) -> Result<AgentConfig, AppError> {
    let overrides = CliOverrides {
        config_path: cli.config.clone(),
        data_dir: cli.data_dir.clone(),
    };
    Ok(config::load(&overrides, &|name| std::env::var(name).ok())?)
}

/// Charge l'identité de l'installation, en la créant à la première exécution.
pub fn load_identity(store: &dyn IdentityStore) -> Result<PublicIdentity, AppError> {
    Ok(store.load_or_create()?)
}

/// Assemble les cas d'usage sur la base ouverte, **sans** identité d'appareil : le service des
/// sessions se comporte comme avant, et les routes du défi et des postes répondent `404`
/// (sous-commandes `account`, qui n'ouvrent aucune session ; tests de liaison).
pub fn services(database: &Database, adapters: &Adapters) -> Services {
    assemble(database, adapters, None, None)
}

/// Comme `services`, avec le mode attaque sur les horloges et le noyau de cette machine : ce que la
/// sous-commande `attack-mode` assemble (aucune identité d'appareil, aucun réseau).
pub fn services_for_attack_mode(database: &Database, adapters: &Adapters) -> Services {
    assemble(
        database,
        adapters,
        None,
        Some((
            Arc::new(SystemMonotonic::new()),
            Arc::new(ProcBootInfo::new()),
        )),
    )
}

/// Comme `services`, avec l'identité d'appareil (HRT-22) : le défi, la preuve de clé, l'inscription
/// des postes. C'est ce que démarre le serveur.
pub fn services_with_trust(
    database: &Database,
    adapters: &Adapters,
    trust: TrustParts,
) -> Result<Services, AppError> {
    let crypto: Arc<dyn ChallengeCrypto> = Arc::new(HmacChallengeCrypto::new()?);
    let attack = (trust.monotonic.clone(), trust.boot.clone());
    Ok(assemble(
        database,
        adapters,
        Some((trust, crypto)),
        Some(attack),
    ))
}

fn assemble(
    database: &Database,
    adapters: &Adapters,
    trust: Option<(TrustParts, Arc<dyn ChallengeCrypto>)>,
    attack: Option<(Arc<dyn MonotonicClock>, Arc<dyn BootInfo>)>,
) -> Services {
    let pool = database.pool();
    // Sans identité d'appareil, pas de service HTTP : aucune requête suivie à reconnaître.
    let fingerprinter: Arc<dyn RequestFingerprinter> = match &trust {
        Some((parts, _)) => parts.fingerprinter.clone(),
        None => Arc::new(NoFingerprint),
    };
    let accounts_repo = Arc::new(SqliteAccountRepo::new(pool.clone()));
    let sessions_repo = Arc::new(SqliteSessionRepo::new(pool.clone()));
    let store: Arc<dyn Store> = Arc::new(SqliteStore::new(pool.clone()));
    let feed: Arc<dyn AuditFeed> = Arc::new(BroadcastAuditFeed::new());
    let maintenance = Arc::new(MaintenanceService::new(
        store.clone(),
        adapters.clock.clone(),
    ));
    let trail = Arc::new(AuditTrail::new(feed.clone(), maintenance.clone()));
    let recorder = Arc::new(AuditRecorder::new(
        store.clone(),
        adapters.clock.clone(),
        trail.clone(),
    ));
    // Un seul canal de tics : le mode attaque et l'alerte parlent au même flux.
    let security_feed: Arc<dyn SecurityFeed> = Arc::new(BroadcastSecurityFeed::new());
    let attack = attack.map(|(monotonic, boot)| {
        Arc::new(AttackModeService::new(
            Arc::new(SqliteAttackModeRepo::new(pool.clone())),
            store.clone(),
            adapters.clock.clone(),
            monotonic,
            boot,
            adapters.ids.clone(),
            trail.clone(),
            security_feed.clone(),
        ))
    });
    let security = SecurityService::new(
        Arc::new(SqliteLoginAttemptRepo::new(pool.clone())),
        accounts_repo.clone(),
        Arc::new(SqliteDeviceRepo::new(pool.clone())),
        store.clone(),
        adapters.clock.clone(),
        trail.clone(),
        security_feed,
    );
    let security = Arc::new(match &attack {
        Some(attack) => security.with_attack(attack.clone()),
        None => security,
    });
    let sessions = SessionService::new(
        accounts_repo.clone(),
        sessions_repo.clone(),
        Arc::new(SqliteLoginAttemptRepo::new(pool.clone())),
        Arc::new(SqliteKnownAddressRepo::new(pool.clone())),
        store.clone(),
        adapters.hasher.clone(),
        adapters.clock.clone(),
        adapters.ids.clone(),
        adapters.tokens.clone(),
        trail.clone(),
        recorder.clone(),
    )
    .with_security(security.clone());
    let sessions = match &attack {
        Some(attack) => sessions.with_attack(attack.clone()),
        None => sessions,
    };
    // L'élévation du mot de passe en administration (HRT-28) : en mémoire, sur l'horloge monotone,
    // seulement quand l'identité d'appareil existe (sans clé, aucun acte ne se confirme).
    let elevations = trust
        .as_ref()
        .map(|(parts, _)| Arc::new(Elevations::new(parts.monotonic.clone())));
    let sessions = match &elevations {
        Some(elevations) => sessions.with_elevations(elevations.clone()),
        None => sessions,
    };
    let account_service = AccountService::new(
        accounts_repo.clone(),
        sessions_repo.clone(),
        store.clone(),
        adapters.hasher.clone(),
        adapters.clock.clone(),
        adapters.ids.clone(),
        trail.clone(),
    );
    let account_service = match elevations {
        Some(elevations) => account_service.with_elevations(elevations),
        None => account_service,
    };
    let sessions = match trust {
        Some((parts, crypto)) => sessions.with_trust(Arc::new(TrustService::new(
            Arc::new(SqliteDeviceRepo::new(pool.clone())),
            store.clone(),
            Arc::new(RingProofVerifier),
            crypto,
            parts.monotonic,
            adapters.clock.clone(),
            adapters.ids.clone(),
            parts.fingerprint,
            trail.clone(),
        ))),
        None => sessions,
    };
    Services {
        accounts: Arc::new(account_service),
        sessions: Arc::new(sessions),
        security,
        attack,
        operations: Arc::new(OperationService::new(
            Arc::new(SqliteOperationRepo::new(pool.clone())),
            store.clone(),
            adapters.clock.clone(),
            fingerprinter,
        )),
        maintenance,
        audit: Arc::new(AuditService::new(
            Arc::new(SqliteAuditRepo::new(pool.clone())),
            feed.clone(),
        )),
        audit_sink: recorder.clone(),
        audit_recorder: recorder,
        store,
        trail,
    }
}

/// Assemble le service des comptes sur la base ouverte (sous-commandes `account`).
pub fn account_service(database: &Database) -> Result<Arc<AccountService>, AppError> {
    Ok(services(database, &Adapters::production()?).accounts)
}

/// Assemble le cas d'usage qui donne et retire le droit de gérer les services (BR-GRANT-005,
/// BR-GRANT-008) : pour la sous-commande `account services` et la question de l'installation, UNIQUEMENT.
/// Il n'est dans aucun `Services` que reçoit le serveur HTTP : aucune route ne peut l'appeler.
pub fn services_right_service(
    database: &Database,
    adapters: &Adapters,
) -> Arc<ServicesRightService> {
    let pool = database.pool();
    let store: Arc<dyn Store> = Arc::new(SqliteStore::new(pool.clone()));
    let maintenance = Arc::new(MaintenanceService::new(
        store.clone(),
        adapters.clock.clone(),
    ));
    let feed: Arc<dyn AuditFeed> = Arc::new(BroadcastAuditFeed::new());
    let trail = Arc::new(AuditTrail::new(feed, maintenance));
    Arc::new(ServicesRightService::new(
        Arc::new(SqliteAccountRepo::new(pool.clone())),
        store,
        adapters.clock.clone(),
        trail,
    ))
}

/// Ouvre le port et démarre le serveur HTTPS, avec les adaptateurs de production.
pub async fn start(config: &AgentConfig) -> Result<RunningAgent, AppError> {
    let database = Database::open_for_service(&config.data_dir).await?;
    start_with(config, &database, &Adapters::production()?).await
}

/// Démarre le serveur sur une base déjà ouverte (migrations appliquées) avec ces adaptateurs et
/// les sondes de la machine réelle.
pub async fn start_with(
    config: &AgentConfig,
    database: &ServiceDatabase,
    adapters: &Adapters,
) -> Result<RunningAgent, AppError> {
    start_with_metering(config, database, adapters, Metering::production()).await
}

/// Comme `start_with`, avec ces sondes et ces délais.
pub async fn start_with_metering(
    config: &AgentConfig,
    database: &ServiceDatabase,
    adapters: &Adapters,
    metering: Metering,
) -> Result<RunningAgent, AppError> {
    start_with_all(
        config,
        database,
        adapters,
        metering,
        Updating::production(config)?,
    )
    .await
}

/// Comme `start_with_metering`, avec ces adaptateurs de mise à jour (les tests en injectent des
/// faux).
pub async fn start_with_all(
    config: &AgentConfig,
    database: &ServiceDatabase,
    adapters: &Adapters,
    metering: Metering,
    updating: Updating,
) -> Result<RunningAgent, AppError> {
    start_full(
        config,
        database,
        adapters,
        metering,
        updating,
        Arc::new(ProcBootInfo::new()),
    )
    .await
}

/// Comme `start_with_all`, avec ce que le noyau dit du démarrage (identifiant, temps écoulé) : les tests
/// du mode attaque et de sa fenêtre de redémarrage en injectent un faux, jamais le vrai `/proc`.
pub async fn start_full(
    config: &AgentConfig,
    database: &ServiceDatabase,
    adapters: &Adapters,
    metering: Metering,
    updating: Updating,
    boot: Arc<dyn BootInfo>,
) -> Result<RunningAgent, AppError> {
    let engine = EngineParts::production(&config.engine);
    start_engined(config, database, adapters, metering, updating, boot, engine).await
}

/// Comme `start_full`, avec ce moteur de conteneurs (les tests injectent un faux, jamais le vrai
/// socket).
pub async fn start_engined(
    config: &AgentConfig,
    database: &ServiceDatabase,
    adapters: &Adapters,
    metering: Metering,
    updating: Updating,
    boot: Arc<dyn BootInfo>,
    engine: EngineParts,
) -> Result<RunningAgent, AppError> {
    // FIX:01M4D6KNXEFG8DH4K9JXBJ98MA : une copie de la base à moitié effacée par un arrêt brutal est finie au
    // démarrage du service, pas seulement à la mise à jour suivante.
    crate::infrastructure::update::finish_interrupted_erasures(&config.data_dir);
    let store = FileIdentityStore::new(&config.data_dir);
    let identity = load_identity(&store)?;
    let tls = tls::server_config(&store)?;
    // Le secret d'empreinte : lu, ou créé au premier démarrage. Un fichier inutilisable arrête
    // l'agent ici, avant d'ouvrir le port (jamais régénéré en silence).
    let fingerprinter = Arc::new(HmacFingerprinter::new(
        &FileFingerprintSecretStore::new(&config.data_dir).load_or_create()?,
    ));

    let addr = SocketAddr::new(config.listen_addr, config.port);
    let listener = TcpListener::bind(addr).map_err(|source| AppError::Bind { addr, source })?;

    let services = services_with_trust(
        database.database(),
        adapters,
        TrustParts {
            fingerprint: identity.fingerprint,
            monotonic: metering.monotonic.clone(),
            boot,
            fingerprinter,
        },
    )?;
    // Aucune exécution ne survit à un arrêt : les opérations restées « en cours » deviennent
    // « interrompues » avant d'accepter la moindre requête.
    let interrupted = services.operations.interrupt_running().await?;
    if interrupted > 0 {
        tracing::warn!(interrupted, "opérations interrompues par l'arrêt précédent");
    }
    // Le mode attaque : l'identifiant de démarrage du noyau est noté, et la fenêtre de redémarrage
    // s'ouvre si la machine vient de démarrer, avant d'accepter la moindre requête (BR-TRUST-020).
    if let Some(attack) = &services.attack {
        attack.on_start().await?;
    }
    // Le socle des services créés par Hearth : la clé des secrets est dite (jamais créée ici), les opérations
    // restées en plein geste sont classées avant d'accepter la moindre requête.
    // La reprise regarde le VRAI moteur (compte les conteneurs du service de chaque opération non terminée,
    // sans jamais lui écrire).
    // Un seul dépôt pour les services supervisés, les adoptions, les arrêts voulus (HRT-64) et les conteneurs que
    // l'agent a créés (HRT-68).
    let service_repo = Arc::new(SqliteServiceRepo::new(database.database().pool().clone()));
    let observer = EngineObserver::new(
        engine.engine.clone(),
        Arc::new(
            SpecRecordedContainers::new(Arc::new(SqliteSpecRepo::new(
                database.database().pool().clone(),
            )))
            .with_containers(service_repo.clone()),
        ),
    );
    let foundations = service_foundations_observed(
        &config.data_dir,
        database.database(),
        adapters.clock.clone(),
        adapters.ids.clone(),
        &observer,
    )
    .await?;
    let service_ops_sweep =
        tasks::spawn_service_ops_sweep(foundations.ops.clone(), tasks::SERVICE_OPS_SWEEP_PERIOD);
    let hello = HelloService::new(
        identity.install_id.clone(),
        config.managed,
        &SystemMachineInfo,
    );
    let metrics = Arc::new(MetricsService::new(
        metering.system,
        metering.gpu,
        metering.clock,
        metering.monotonic.clone(),
    ));
    // L'identité de la machine est lue avant de servir : les premières requêtes la trouvent prête.
    metrics.warm_up().await;
    let stream = StreamContext::new(metering.stream);
    let closing = stream.clone();
    let update = update_service(
        updating,
        addr,
        identity.fingerprint,
        services.audit_sink.clone(),
        adapters.clock.clone(),
    )?;
    let sessions = services.sessions.clone();
    let container_engine = engine.engine.clone();
    let supervisor = engine.supervisor.clone();
    let registry = ServicesRegistry::with_supervision(
        engine.engine,
        engine.supervisor,
        service_repo.clone(),
        engine.host.as_ref(),
        adapters.clock.clone(),
        metering.monotonic.clone(),
        config.engine.enabled,
        engine.timings,
    );
    // Le passage « à confirmer » et la chute d'une adoption sont consignés (auteur : le système).
    registry.set_audit(services.audit_sink.clone());
    // Les arrêts voulus depuis Hearth survivent à un redémarrage de l'agent (HRT-64, BR-LIFE-010).
    registry.set_intents(service_repo.clone());
    // Les conteneurs que l'agent a créés : seuls ceux-là sont « créés par Hearth » (ADR-0037, HRT-68).
    registry.set_containers(service_repo.clone());
    let service_lifecycle = ServiceLifecycle::new(
        registry.clone(),
        foundations.ops.clone(),
        container_engine.clone(),
        supervisor,
        service_repo.clone(),
        services.audit_sink.clone(),
        adapters.clock.clone(),
        engine.lifecycle,
    );
    // Les actions que l'arrêt précédent a interrompues sont consignées avant d'accepter la moindre requête :
    // le résultat en est inconnu, rien n'est rejoué (BR-LIFE-020).
    record_interrupted(
        services.audit_sink.as_ref(),
        &foundations.interrupted_actions,
    )
    .await;
    // Les déploiements interrompus avant la création de leur conteneur n'ont rien laissé : leur description et leur
    // dossier de données partent avant d'accepter la moindre requête (BR-DEPLOY-019).
    let safe_fs: Arc<dyn SafeFs> = engine
        .safe_fs
        .unwrap_or_else(|| Arc::new(SystemSafeFs::new(config.data_dir.clone())));
    for key in &foundations.abandoned_deploys {
        if let Err(error) = foundations.specs.forget(key).await {
            tracing::warn!(%error, "description d'un déploiement abandonné non retirée");
        }
        match safe_fs.remove_service_root(key) {
            Ok(()) | Err(crate::application::ports::FsError::NotFound) => {}
            Err(_) => tracing::warn!("dossier d'un déploiement abandonné non retiré"),
        }
    }
    let services_root = config
        .data_dir
        .join(SERVICES_DIR)
        .to_string_lossy()
        .into_owned();
    // Ce que l'agent sait de lui-même, pour juger un dossier du serveur (BR-DEPLOY-012) : sans l'un de ces faits,
    // aucun dossier du serveur n'est jugé (et tous restent refusés).
    let folders = FolderFacts::new(
        config.engine.socket.to_string_lossy(),
        "/var/lib/docker",
        services_root.clone(),
        std::iter::once(config.data_dir.to_string_lossy().into_owned())
            .chain(
                std::env::current_exe()
                    .ok()
                    .map(|exe| exe.to_string_lossy().into_owned()),
            )
            .collect(),
    )
    .ok()
    .map(|facts| FolderGuard {
        host: Arc::new(SystemFolders::new()),
        facts,
    });
    // Le catalogue des fiches : celles que cette construction propose, et les écartées, consignées une fois.
    let catalog = engine
        .catalog
        .service
        .clone()
        .unwrap_or_else(|| Arc::new(CatalogService::embedded(engine.catalog.include_preview)));
    catalog.record_refused(services.audit_sink.as_ref()).await;
    // Les réglages d'un serveur de jeu (HRT-108) : le fichier du jeu par `SafeFs`, les valeurs voulues en base.
    let game_settings = GameSettings::new(
        catalog.clone(),
        foundations.specs.clone(),
        Arc::new(SqliteGameSettingsRepo::new(database.pool().clone())),
        safe_fs.clone(),
        foundations.ops.locks().clone(),
        services.audit_sink.clone(),
        adapters.clock.clone(),
    );
    let deployments = Deployments::new(
        registry.clone(),
        foundations.ops.clone(),
        foundations.specs.clone(),
        container_engine.clone(),
        engine.host_ports,
        engine.free_space,
        safe_fs,
        service_repo.clone(),
        services.audit_sink.clone(),
        adapters.ids.clone(),
        adapters.clock.clone(),
        DeployContext {
            install_id: identity.install_id.to_string(),
            agent_port: config.port,
            services_root,
            folders,
            timings: engine.deploy,
        },
        catalog.clone(),
        engine.catalog.random.clone(),
        GamePhases::new(container_engine.clone(), adapters.clock.clone()),
    );
    let service_logs = Arc::new(ServiceLogs::new(
        registry.clone(),
        container_engine,
        adapters.clock.clone(),
        LogSettings {
            brake: engine.log_brake,
            ..LogSettings::default()
        },
    ));
    let service_adoption = Arc::new(ServiceAdoption::new(
        registry.clone(),
        services.store.clone(),
        adapters.clock.clone(),
        services.trail.clone(),
        config.services.adoption,
    ));
    let service_ops = Arc::new(ServiceOpAccess::new(
        foundations.ops.clone(),
        registry.clone(),
        services.store.clone(),
        adapters.clock.clone(),
        services.trail.clone(),
    ));
    let router = http::router(AppState {
        hello: Arc::new(hello),
        accounts: services.accounts,
        sessions: services.sessions,
        security: services.security.clone(),
        operations: services.operations,
        audit: services.audit,
        sink: services.audit_sink,
        metrics: metrics.clone(),
        update: update.clone(),
        services: registry.clone(),
        service_logs,
        service_adoption,
        service_ops,
        service_lifecycle,
        deployments,
        catalog,
        game_settings,
        stream,
        erasure_pending: database.erasure_pending(),
    });
    // À l'arrêt, les flux ouverts se ferment d'eux-mêmes avant que le serveur n'attende les connexions.
    let server = http::spawn(listener, tls, router)?.on_shutdown(move || closing.begin_shutdown());
    // Le résultat d'une mise à jour que personne n'a encore annoncé (le nouvel agent après un
    // échange, l'ancien après un retour en arrière) part au journal et au flux.
    tokio::spawn({
        let update = update.clone();
        async move { update.resume().await }
    });
    let purge = tasks::spawn_purge(services.maintenance, tasks::PURGE_PERIOD);
    let sampler = tasks::spawn_sampler(metrics, metering.period);
    let services_task = tasks::spawn_services(registry.clone());
    let audit_flush =
        tasks::spawn_audit_flush(services.audit_recorder.clone(), tasks::AUDIT_FLUSH_PERIOD);
    let alert_sweep =
        tasks::spawn_alert_sweep(services.security.clone(), tasks::ALERT_SWEEP_PERIOD);
    let attack_sweep = services
        .attack
        .map(|attack| tasks::spawn_attack_sweep(attack, tasks::ATTACK_SWEEP_PERIOD));
    Ok(RunningAgent {
        server,
        identity,
        purge,
        sampler,
        audit_flush,
        alert_sweep,
        attack_sweep,
        services_task,
        services: registry,
        foundations,
        service_ops_sweep,
        sessions,
        audit_recorder: services.audit_recorder,
        security: services.security,
    })
}

/// Exécute la commande demandée sur la ligne de commande.
pub async fn run(cli: Cli) -> Result<(), AppError> {
    // Installer et désinstaller observent avant d'écrire : ils ne passent pas par la création du
    // dossier de données ci-dessous.
    match cli.command() {
        Command::Install(args) => return install::run_install(&cli, &args).await,
        Command::Uninstall(args) => return install::run_uninstall(&cli, &args).await,
        Command::HashPassword { user } => return install::run_hash_password(&user).await,
        // BR-GRANT-008 : le droit de gérer les services ne se change qu'en root, vérifié AVANT de
        // créer le dossier de données ou d'ouvrir la base.
        Command::Account {
            action: AccountAction::Services { .. },
        } => account::require_root(SystemHost.is_privileged())?,
        Command::UpdateSupervise { job } => return run_supervisor(&job),
        Command::BuildInfo => {
            let key = crate::infrastructure::update::EMBEDDED_PUBLIC_KEY;
            println!(
                "cle: {}\nnotes: {}",
                key.lines().next().unwrap_or(""),
                crate::build_info::BUILD_NOTES
            );
            return Ok(());
        }
        _ => {}
    }
    let config = load_config(&cli)?;
    // Un seul endroit crée le dossier de données et en contrôle les droits, avant que la base
    // ou le magasin d'identité n'y écrive.
    data_dir::ensure(&config.data_dir).map_err(|source| DatabaseError::DataDir {
        path: config.data_dir.clone(),
        source,
    })?;
    match cli.command() {
        Command::Fingerprint => {
            let identity = load_identity(&FileIdentityStore::new(&config.data_dir))?;
            println!("{}", identity.fingerprint);
            Ok(())
        }
        Command::Account { action } => {
            let database = Database::open(&config.data_dir).await?;
            let adapters = Adapters::production()?;
            let service = account_service(&database)?;
            let rights = services_right_service(&database, &adapters);
            let passwords = TerminalPasswords::from_env(&|name| std::env::var(name).ok());
            account::execute(
                &action,
                &service,
                &rights,
                &passwords,
                SystemHost.is_privileged(),
                &mut std::io::stdout(),
            )
            .await?;
            Ok(())
        }
        Command::AttackMode { action } => {
            let database = Database::open(&config.data_dir).await?;
            let services = services_for_attack_mode(&database, &Adapters::production()?);
            let Some(attack) = services.attack else {
                return Ok(());
            };
            attack_mode_cli::execute(&action, &attack, &mut std::io::stdout()).await?;
            Ok(())
        }
        Command::Install(_)
        | Command::Uninstall(_)
        | Command::HashPassword { .. }
        | Command::UpdateSupervise { .. }
        | Command::BuildInfo => Ok(()),
        Command::Serve => {
            if !crate::build_info::BUILD_NOTES.is_empty() {
                tracing::warn!(
                    notes = crate::build_info::BUILD_NOTES,
                    "construction qui n'est pas une publication"
                );
            }
            // Migrations appliquées avant d'accepter la moindre connexion.
            let running = start(&config).await?;
            tracing::info!(
                addr = %running.server.local_addr(),
                fingerprint = %running.identity.fingerprint,
                install_id = %running.identity.install_id,
                "agent démarré"
            );
            running.run_until(shutdown_signal()).await?;
            tracing::info!("agent arrêté");
            Ok(())
        }
    }
}

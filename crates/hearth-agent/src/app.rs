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

pub use update::{Updating, run_supervisor, update_service};

use crate::application::accounts::AccountService;
use crate::application::attack_mode::AttackModeService;
use crate::application::audit::{AuditRecorder, AuditService, AuditTrail};
use crate::application::hello::HelloService;
use crate::application::maintenance::MaintenanceService;
use crate::application::metrics::MetricsService;
use crate::application::operations::OperationService;
use crate::application::ports::{
    AuditFeed, AuditSink, BootInfo, ChallengeCrypto, Clock, CryptoError, GpuProbe, HashError,
    IdGen, IdentityError, IdentityStore, MonotonicClock, PasswordHasher, PublicIdentity,
    SecurityFeed, Store, StoreError, SystemProbe, TokenGen,
};
use crate::application::security::SecurityService;
use crate::application::sessions::SessionService;
use crate::application::trust::TrustService;
use crate::entrypoint::account::{self, AccountCliError};
use crate::entrypoint::attack_mode as attack_mode_cli;
use crate::entrypoint::cli::{Cli, Command};
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
use crate::infrastructure::crypto::{HmacChallengeCrypto, RingProofVerifier};
use crate::infrastructure::data_dir;
use crate::infrastructure::ids::UlidGen;
use crate::infrastructure::random::OsTokenGen;
use crate::infrastructure::security_feed::BroadcastSecurityFeed;
use crate::infrastructure::sqlite::{
    Database, DatabaseError, SqliteAccountRepo, SqliteAttackModeRepo, SqliteAuditRepo,
    SqliteDeviceRepo, SqliteKnownAddressRepo, SqliteLoginAttemptRepo, SqliteOperationRepo,
    SqliteSessionRepo, SqliteStore,
};
use crate::infrastructure::system::gpu;
use crate::infrastructure::system::{ProcBootInfo, SysinfoProbe, SystemMachineInfo};
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

/// Ce qu'il faut à l'identité d'appareil (HRT-22) et que seul le démarrage du serveur connaît :
/// l'empreinte du certificat que le client épingle (celle que la preuve signe) et l'horloge
/// monotone qui borne la vie d'un défi.
pub struct TrustParts {
    pub fingerprint: Fingerprint,
    pub monotonic: Arc<dyn MonotonicClock>,
    /// Ce que le noyau dit du démarrage en cours (mode attaque, HRT-25).
    pub boot: Arc<dyn BootInfo>,
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
    /// Écrit les synthèses du journal en attente à l'arrêt.
    audit_recorder: Arc<AuditRecorder>,
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
        accounts: Arc::new(AccountService::new(
            accounts_repo.clone(),
            sessions_repo.clone(),
            store.clone(),
            adapters.hasher.clone(),
            adapters.clock.clone(),
            adapters.ids.clone(),
            trail.clone(),
        )),
        sessions: Arc::new(sessions),
        security,
        attack,
        operations: Arc::new(OperationService::new(
            Arc::new(SqliteOperationRepo::new(pool.clone())),
            store.clone(),
            adapters.clock.clone(),
        )),
        maintenance,
        audit: Arc::new(AuditService::new(
            Arc::new(SqliteAuditRepo::new(pool.clone())),
            feed.clone(),
        )),
        audit_sink: recorder.clone(),
        audit_recorder: recorder,
    }
}

/// Assemble le service des comptes sur la base ouverte (sous-commandes `account`).
pub fn account_service(database: &Database) -> Result<Arc<AccountService>, AppError> {
    Ok(services(database, &Adapters::production()?).accounts)
}

/// Ouvre le port et démarre le serveur HTTPS, avec les adaptateurs de production.
pub async fn start(config: &AgentConfig) -> Result<RunningAgent, AppError> {
    let database = Database::open(&config.data_dir).await?;
    start_with(config, &database, &Adapters::production()?).await
}

/// Démarre le serveur sur une base déjà ouverte (migrations appliquées) avec ces adaptateurs et
/// les sondes de la machine réelle.
pub async fn start_with(
    config: &AgentConfig,
    database: &Database,
    adapters: &Adapters,
) -> Result<RunningAgent, AppError> {
    start_with_metering(config, database, adapters, Metering::production()).await
}

/// Comme `start_with`, avec ces sondes et ces délais.
pub async fn start_with_metering(
    config: &AgentConfig,
    database: &Database,
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
    database: &Database,
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
    database: &Database,
    adapters: &Adapters,
    metering: Metering,
    updating: Updating,
    boot: Arc<dyn BootInfo>,
) -> Result<RunningAgent, AppError> {
    let store = FileIdentityStore::new(&config.data_dir);
    let identity = load_identity(&store)?;
    let tls = tls::server_config(&store)?;

    let addr = SocketAddr::new(config.listen_addr, config.port);
    let listener = TcpListener::bind(addr).map_err(|source| AppError::Bind { addr, source })?;

    let services = services_with_trust(
        database,
        adapters,
        TrustParts {
            fingerprint: identity.fingerprint,
            monotonic: metering.monotonic.clone(),
            boot,
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
    let hello = HelloService::new(
        identity.install_id.clone(),
        config.managed,
        &SystemMachineInfo,
    );
    let metrics = Arc::new(MetricsService::new(
        metering.system,
        metering.gpu,
        metering.clock,
        metering.monotonic,
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
        stream,
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
            let service = account_service(&database)?;
            let passwords = TerminalPasswords::from_env(&|name| std::env::var(name).ok());
            account::execute(&action, &service, &passwords, &mut std::io::stdout()).await?;
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

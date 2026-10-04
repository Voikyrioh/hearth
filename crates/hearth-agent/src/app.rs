//! Racine de composition : charge la configuration, assemble adaptateurs, cas d'usage et
//! serveur, puis exécute la commande demandée. Seul endroit qui connaît les types concrets.

use std::future::Future;
use std::net::{SocketAddr, TcpListener};
use std::sync::Arc;

use thiserror::Error;

use crate::application::accounts::AccountService;
use crate::application::audit::{AuditRecorder, AuditService, AuditTrail};
use crate::application::hello::HelloService;
use crate::application::maintenance::MaintenanceService;
use crate::application::operations::OperationService;
use crate::application::ports::{
    AuditFeed, AuditSink, Clock, HashError, IdGen, IdentityError, IdentityStore, PasswordHasher,
    PublicIdentity, Store, StoreError, TokenGen,
};
use crate::application::sessions::SessionService;
use crate::entrypoint::account::{self, AccountCliError};
use crate::entrypoint::cli::{Cli, Command};
use crate::entrypoint::http::{self, AppState, ServerError, ServerHandle};
use crate::entrypoint::signal::shutdown_signal;
use crate::entrypoint::tasks::{self, BackgroundTask};
use crate::entrypoint::terminal::TerminalPasswords;
use crate::infrastructure::argon2::Argon2Hasher;
use crate::infrastructure::audit_feed::BroadcastAuditFeed;
use crate::infrastructure::clock::SystemClock;
use crate::infrastructure::config::{self, AgentConfig, CliOverrides, ConfigError};
use crate::infrastructure::data_dir;
use crate::infrastructure::ids::UlidGen;
use crate::infrastructure::random::OsTokenGen;
use crate::infrastructure::sqlite::{
    Database, DatabaseError, SqliteAccountRepo, SqliteAuditRepo, SqliteLoginAttemptRepo,
    SqliteOperationRepo, SqliteSessionRepo, SqliteStore,
};
use crate::infrastructure::system::SystemMachineInfo;
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
    Server(#[from] ServerError),
    #[error(transparent)]
    Store(#[from] StoreError),
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

/// Les cas d'usage assemblés sur une base ouverte.
pub struct Services {
    pub accounts: Arc<AccountService>,
    pub sessions: Arc<SessionService>,
    pub operations: Arc<OperationService>,
    pub maintenance: Arc<MaintenanceService>,
    /// Lecture et export du journal d'activité.
    pub audit: Arc<AuditService>,
    /// Écriture du journal hors transaction (refus et échecs relevés par le routeur).
    pub audit_sink: Arc<dyn AuditSink>,
    /// Le même, pour écrire les synthèses des événements répétés (`flush`).
    pub audit_recorder: Arc<AuditRecorder>,
    /// Diffusion interne des entrées du journal, pour le flux temps réel.
    pub audit_feed: Arc<dyn AuditFeed>,
}

/// Agent démarré : le serveur, la partie publique de son identité et ses tâches de fond.
pub struct RunningAgent {
    pub server: ServerHandle,
    pub identity: PublicIdentity,
    /// Purge périodique : arrêtée avec l'agent.
    pub purge: BackgroundTask,
    /// Écriture des synthèses du journal : arrêtée avec l'agent.
    pub audit_flush: BackgroundTask,
    /// Écrit les synthèses du journal en attente à l'arrêt.
    audit_recorder: Arc<AuditRecorder>,
}

impl RunningAgent {
    /// Sert jusqu'à `stop`, tâches de fond comprises.
    pub async fn run_until(self, stop: impl Future<Output = ()>) -> Result<(), ServerError> {
        let Self {
            server,
            purge: _purge,
            audit_flush: _audit_flush,
            audit_recorder,
            ..
        } = self;
        let result = server.run_until(stop).await;
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

/// Assemble les cas d'usage sur la base ouverte.
pub fn services(database: &Database, adapters: &Adapters) -> Services {
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
        sessions: Arc::new(SessionService::new(
            accounts_repo,
            sessions_repo,
            Arc::new(SqliteLoginAttemptRepo::new(pool.clone())),
            store.clone(),
            adapters.hasher.clone(),
            adapters.clock.clone(),
            adapters.ids.clone(),
            adapters.tokens.clone(),
            trail.clone(),
        )),
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
        audit_feed: feed,
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

/// Démarre le serveur sur une base déjà ouverte (migrations appliquées) avec ces adaptateurs.
pub async fn start_with(
    config: &AgentConfig,
    database: &Database,
    adapters: &Adapters,
) -> Result<RunningAgent, AppError> {
    let store = FileIdentityStore::new(&config.data_dir);
    let identity = load_identity(&store)?;
    let tls = tls::server_config(&store)?;

    let addr = SocketAddr::new(config.listen_addr, config.port);
    let listener = TcpListener::bind(addr).map_err(|source| AppError::Bind { addr, source })?;

    let services = services(database, adapters);
    // Aucune exécution ne survit à un arrêt : les opérations restées « en cours » deviennent
    // « interrompues » avant d'accepter la moindre requête.
    let interrupted = services.operations.interrupt_running().await?;
    if interrupted > 0 {
        tracing::warn!(interrupted, "opérations interrompues par l'arrêt précédent");
    }
    let hello = HelloService::new(
        identity.install_id.clone(),
        config.managed,
        &SystemMachineInfo,
    );
    let router = http::router(AppState {
        hello: Arc::new(hello),
        accounts: services.accounts,
        sessions: services.sessions,
        operations: services.operations,
        audit: services.audit,
        sink: services.audit_sink,
    });
    let server = http::spawn(listener, tls, router)?;
    let purge = tasks::spawn_purge(services.maintenance, tasks::PURGE_PERIOD);
    let audit_flush =
        tasks::spawn_audit_flush(services.audit_recorder.clone(), tasks::AUDIT_FLUSH_PERIOD);
    Ok(RunningAgent {
        server,
        identity,
        purge,
        audit_flush,
        audit_recorder: services.audit_recorder,
    })
}

/// Exécute la commande demandée sur la ligne de commande.
pub async fn run(cli: Cli) -> Result<(), AppError> {
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
        Command::Serve => {
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

//! Racine de composition : charge la configuration, assemble adaptateurs, cas d'usage et
//! serveur, puis exécute la commande demandée. Seul endroit qui connaît les types concrets.

use std::net::{SocketAddr, TcpListener};
use std::sync::Arc;

use thiserror::Error;

use crate::application::accounts::AccountService;
use crate::application::hello::HelloService;
use crate::application::ports::{HashError, IdentityError, IdentityStore, PublicIdentity};
use crate::entrypoint::account::{self, AccountCliError};
use crate::entrypoint::cli::{Cli, Command};
use crate::entrypoint::http::{self, AppState, ServerError, ServerHandle};
use crate::entrypoint::signal::shutdown_signal;
use crate::entrypoint::terminal::TerminalPasswords;
use crate::infrastructure::argon2::Argon2Hasher;
use crate::infrastructure::clock::SystemClock;
use crate::infrastructure::config::{self, AgentConfig, CliOverrides, ConfigError};
use crate::infrastructure::ids::UlidGen;
use crate::infrastructure::sqlite::{
    Database, DatabaseError, SqliteAccountRepo, SqliteSessionRepo,
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
    #[error("ouverture du port {addr} impossible : {source}")]
    Bind {
        addr: SocketAddr,
        source: std::io::Error,
    },
}

/// Agent démarré : le serveur et la partie publique de son identité.
pub struct RunningAgent {
    pub server: ServerHandle,
    pub identity: PublicIdentity,
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

/// Assemble le service des comptes sur la base ouverte.
pub fn account_service(database: &Database) -> Result<AccountService, AppError> {
    Ok(AccountService::new(
        Arc::new(SqliteAccountRepo::new(database.pool().clone())),
        Arc::new(SqliteSessionRepo::new(database.pool().clone())),
        Arc::new(Argon2Hasher::new()?),
        Arc::new(SystemClock),
        Arc::new(UlidGen),
    ))
}

/// Ouvre le port et démarre le serveur HTTPS.
pub fn start(config: &AgentConfig) -> Result<RunningAgent, AppError> {
    let store = FileIdentityStore::new(&config.data_dir);
    let identity = load_identity(&store)?;
    let tls = tls::server_config(&store)?;

    let addr = SocketAddr::new(config.listen_addr, config.port);
    let listener = TcpListener::bind(addr).map_err(|source| AppError::Bind { addr, source })?;

    let hello = HelloService::new(
        identity.install_id.clone(),
        config.managed,
        &SystemMachineInfo,
    );
    let router = http::router(AppState {
        hello: Arc::new(hello),
    });
    let server = http::spawn(listener, tls, router)?;
    Ok(RunningAgent { server, identity })
}

/// Exécute la commande demandée sur la ligne de commande.
pub async fn run(cli: Cli) -> Result<(), AppError> {
    let config = load_config(&cli)?;
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
            // Migrations appliquées avant d'accepter la moindre connexion. HRT-04 passera la base
            // au routeur ; elle reste ouverte tant que l'agent tourne.
            let _database = Database::open(&config.data_dir).await?;
            let RunningAgent { server, identity } = start(&config)?;
            tracing::info!(
                addr = %server.local_addr(),
                fingerprint = %identity.fingerprint,
                install_id = %identity.install_id,
                "agent démarré"
            );
            server.run_until(shutdown_signal()).await?;
            tracing::info!("agent arrêté");
            Ok(())
        }
    }
}

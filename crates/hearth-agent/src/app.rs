//! Racine de composition : assemble les adaptateurs, les cas d'usage et le serveur.

use std::net::{SocketAddr, TcpListener};
use std::sync::Arc;

use thiserror::Error;

use crate::application::hello::HelloService;
use crate::application::ports::{Identity, IdentityError, IdentityStore};
use crate::entrypoint::http::{self, AppState, ServerError, ServerHandle};
use crate::infrastructure::config::{AgentConfig, ConfigError};
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
    Server(#[from] ServerError),
    #[error("ouverture du port {addr} impossible : {source}")]
    Bind {
        addr: SocketAddr,
        source: std::io::Error,
    },
    #[error("entrée/sortie : {0}")]
    Io(#[from] std::io::Error),
}

/// Charge l'identité de l'installation, en la créant à la première exécution.
pub fn load_identity(config: &AgentConfig) -> Result<Identity, AppError> {
    Ok(FileIdentityStore::new(&config.data_dir).load_or_create()?)
}

/// Ouvre le port et démarre le serveur HTTPS.
pub fn start(config: &AgentConfig, identity: &Identity) -> Result<ServerHandle, AppError> {
    let addr = SocketAddr::new(config.listen_addr, config.port);
    let listener = TcpListener::bind(addr).map_err(|source| AppError::Bind { addr, source })?;

    let hello = HelloService::new(
        identity.install_id.clone(),
        config.managed,
        Arc::new(SystemMachineInfo),
    );
    let router = http::router(AppState {
        hello: Arc::new(hello),
    });
    let tls = tls::server_config(identity)?;
    Ok(http::spawn(listener, tls, router)?)
}

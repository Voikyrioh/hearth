use std::future::Future;
use std::io;
use std::net::{SocketAddr, TcpListener};
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum_server::Handle;
use axum_server::tls_rustls::RustlsConfig;
use rustls::ServerConfig;
use thiserror::Error;
use tokio::task::JoinHandle;

/// Délai laissé aux requêtes en cours à l'arrêt.
const GRACE: Duration = Duration::from_secs(5);

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("serveur HTTPS : {0}")]
    Io(#[from] io::Error),
    #[error("tâche du serveur interrompue : {0}")]
    Task(#[from] tokio::task::JoinError),
}

/// Serveur HTTPS en cours d'exécution.
pub struct ServerHandle {
    local_addr: SocketAddr,
    handle: Handle<SocketAddr>,
    task: JoinHandle<io::Result<()>>,
}

/// Démarre le serveur sur un port déjà ouvert (port 0 accepté : voir [`ServerHandle::local_addr`]).
pub fn spawn(
    listener: TcpListener,
    tls: Arc<ServerConfig>,
    router: Router,
) -> Result<ServerHandle, ServerError> {
    listener.set_nonblocking(true)?;
    let local_addr = listener.local_addr()?;
    let handle = Handle::new();
    let server = axum_server::from_tcp_rustls(listener, RustlsConfig::from_config(tls))?
        .handle(handle.clone());
    let task = tokio::spawn(async move { server.serve(router.into_make_service()).await });
    Ok(ServerHandle {
        local_addr,
        handle,
        task,
    })
}

impl ServerHandle {
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Arrêt propre, puis attente de la fin du serveur.
    pub async fn shutdown(mut self) -> Result<(), ServerError> {
        self.handle.graceful_shutdown(Some(GRACE));
        (&mut self.task).await??;
        Ok(())
    }

    /// Sert jusqu'à `stop` (signal d'arrêt) ou jusqu'à la fin prématurée du serveur.
    pub async fn run_until(mut self, stop: impl Future<Output = ()>) -> Result<(), ServerError> {
        tokio::select! {
            result = &mut self.task => {
                result??;
                Ok(())
            }
            () = stop => self.shutdown().await,
        }
    }
}

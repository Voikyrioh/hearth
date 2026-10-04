use std::future::Future;
use std::io;
use std::net::{SocketAddr, TcpListener};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum_server::Handle;
use axum_server::accept::Accept;
use axum_server::tls_rustls::{RustlsAcceptor, RustlsConfig};
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
    /// Le serveur s'est arrêté sans qu'aucun arrêt n'ait été demandé.
    #[error("le serveur s'est arrêté sans signal d'arrêt")]
    StoppedUnexpectedly,
}

/// Enveloppe l'acceptation TLS pour journaliser (niveau debug) les poignées de main refusées :
/// version de protocole non prise en charge, client qui coupe, délai dépassé…
#[derive(Clone)]
struct LoggingAcceptor(RustlsAcceptor);

type AcceptFuture<S, T> = Pin<Box<dyn Future<Output = io::Result<(S, T)>> + Send>>;

impl<I, S> Accept<I, S> for LoggingAcceptor
where
    RustlsAcceptor: Accept<I, S>,
    <RustlsAcceptor as Accept<I, S>>::Future: Send + 'static,
    <RustlsAcceptor as Accept<I, S>>::Stream: Send + 'static,
    <RustlsAcceptor as Accept<I, S>>::Service: Send + 'static,
{
    type Stream = <RustlsAcceptor as Accept<I, S>>::Stream;
    type Service = <RustlsAcceptor as Accept<I, S>>::Service;
    type Future = AcceptFuture<Self::Stream, Self::Service>;

    fn accept(&self, stream: I, service: S) -> Self::Future {
        let inner = self.0.accept(stream, service);
        Box::pin(async move {
            let result = inner.await;
            if let Err(error) = &result {
                tracing::debug!(%error, "poignée de main TLS refusée");
            }
            result
        })
    }
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
    let acceptor = LoggingAcceptor(RustlsAcceptor::new(RustlsConfig::from_config(tls)));
    let server = axum_server::from_tcp(listener)?
        .acceptor(acceptor)
        .handle(handle.clone());
    let task = tokio::spawn(async move {
        server
            .serve(router.into_make_service_with_connect_info::<SocketAddr>())
            .await
    });
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

    /// Sert jusqu'à `stop` (arrêt demandé, `Ok`). Une fin du serveur avant `stop` est une
    /// erreur, même sans message : `StoppedUnexpectedly` ou l'erreur d'E/S d'origine.
    pub async fn run_until(mut self, stop: impl Future<Output = ()>) -> Result<(), ServerError> {
        tokio::select! {
            result = &mut self.task => {
                result??;
                Err(ServerError::StoppedUnexpectedly)
            }
            () = stop => self.shutdown().await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::IdentityStore;
    use crate::infrastructure::tls::{self, FileIdentityStore};

    fn started(dir: &std::path::Path) -> ServerHandle {
        let store = FileIdentityStore::new(dir);
        store.load_or_create().expect("identité");
        let tls = tls::server_config(&store).expect("tls");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        spawn(listener, tls, Router::new()).expect("spawn")
    }

    #[tokio::test]
    async fn a_server_that_ends_by_itself_is_an_error() {
        let dir = crate::infrastructure::data_dir::private_tempdir();
        let server = started(dir.path());
        server.handle.shutdown();
        let result = server.run_until(std::future::pending()).await;
        assert!(
            matches!(result, Err(ServerError::StoppedUnexpectedly)),
            "{result:?}"
        );
    }

    #[tokio::test]
    async fn a_requested_stop_is_not_an_error() {
        let dir = crate::infrastructure::data_dir::private_tempdir();
        let server = started(dir.path());
        server
            .run_until(std::future::ready(()))
            .await
            .expect("arrêt demandé");
    }
}

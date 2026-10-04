//! Signaux d'arrêt du processus.

use std::future::pending;

/// Se termine sur Ctrl+C, ou sur SIGTERM sous Unix.
///
/// Si un gestionnaire ne peut pas être installé, un avertissement est journalisé : l'agent ne
/// s'arrêtera alors pas proprement sur ce signal.
pub async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::warn!(%error, "gestionnaire Ctrl+C indisponible : arrêt propre impossible par ce signal");
            pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(error) => {
                tracing::warn!(%error, "gestionnaire SIGTERM indisponible : arrêt propre impossible par ce signal");
                pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = pending::<()>();

    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
    tracing::info!("signal d'arrêt reçu");
}

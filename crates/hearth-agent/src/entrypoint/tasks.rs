//! Tâches périodiques, supervisées : un passage qui échoue ou panique est journalisé, jamais
//! fatal, et le passage suivant a lieu à l'heure. Arrêtée avec la valeur qui la porte.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use tokio::task::JoinHandle;
use tokio::time::{MissedTickBehavior, interval};

use crate::application::maintenance::MaintenanceService;

/// Période de la purge (sessions expirées, traces anciennes, opérations de plus de 24 h).
pub const PURGE_PERIOD: Duration = Duration::from_secs(60 * 60);

/// Tâche de fond ; abandonner la valeur l'arrête.
pub struct BackgroundTask(JoinHandle<()>);

impl Drop for BackgroundTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Exécute `job` toutes les `period` (le premier passage a lieu tout de suite). Chaque passage
/// tourne dans sa propre tâche : une panique est vue comme une erreur de ce passage, pas de la
/// supervision.
pub fn every<F, Fut>(name: &'static str, period: Duration, job: F) -> BackgroundTask
where
    F: Fn() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    BackgroundTask(tokio::spawn(async move {
        let mut ticker = interval(period);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            if let Err(error) = tokio::spawn(job()).await {
                tracing::error!(task = name, %error, "passage interrompu, reprise au prochain");
            }
        }
    }))
}

/// Purge périodique du stockage.
pub fn spawn_purge(service: Arc<MaintenanceService>, period: Duration) -> BackgroundTask {
    every("purge", period, move || {
        let service = service.clone();
        async move {
            match service.purge().await {
                Ok(report) => tracing::debug!(?report, "purge faite"),
                Err(error) => {
                    tracing::warn!(%error, "purge impossible, reprise au prochain passage")
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    #[tokio::test]
    async fn a_panicking_pass_does_not_stop_the_next_ones() {
        let runs = Arc::new(AtomicU32::new(0));
        let counter = runs.clone();
        let _task = every("test", Duration::from_millis(10), move || {
            let counter = counter.clone();
            async move {
                let run = counter.fetch_add(1, Ordering::SeqCst);
                if run == 0 {
                    panic!("premier passage en échec");
                }
            }
        });
        tokio::time::sleep(Duration::from_millis(400)).await;
        assert!(runs.load(Ordering::SeqCst) >= 3);
    }

    #[tokio::test]
    async fn dropping_the_task_stops_it() {
        let runs = Arc::new(AtomicU32::new(0));
        let counter = runs.clone();
        let task = every("test", Duration::from_millis(10), move || {
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
            }
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
        drop(task);
        tokio::time::sleep(Duration::from_millis(50)).await;
        let after_stop = runs.load(Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(runs.load(Ordering::SeqCst), after_stop);
    }
}

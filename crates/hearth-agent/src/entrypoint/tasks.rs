//! Tâches périodiques, supervisées : un passage qui échoue ou panique est journalisé, jamais
//! fatal, et le passage suivant a lieu à l'heure. Arrêtée avec la valeur qui la porte.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use tokio::task::JoinHandle;
use tokio::time::{MissedTickBehavior, interval};

use crate::application::maintenance::MaintenanceService;
use crate::application::metrics::{MetricsError, MetricsService};

/// Période de la purge (sessions expirées, traces anciennes, opérations de plus de 24 h).
pub const PURGE_PERIOD: Duration = Duration::from_secs(60 * 60);

/// Période de l'échantillonnage : un échantillon par seconde (BR-DASH-002).
pub const SAMPLE_PERIOD: Duration = Duration::from_secs(1);

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

/// Échantillonneur : un échantillon par `period`. Une erreur de sonde perd cet échantillon, une
/// panique est vue par `every` comme l'échec de ce passage : dans les deux cas le suivant a lieu.
pub fn spawn_sampler(service: Arc<MetricsService>, period: Duration) -> BackgroundTask {
    every("sampler", period, move || {
        let service = service.clone();
        async move {
            match service.sample_once().await {
                Ok(()) => {}
                // Déjà journalisé une fois par épisode par le service : pas de bruit à chaque seconde.
                Err(MetricsError::Stalled | MetricsError::Skipped) => {}
                Err(error) => {
                    tracing::warn!(%error, "échantillon manquant, reprise au prochain passage");
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

    /// Sonde qui panique au premier passage, échoue au deuxième, puis mesure.
    struct Flaky(AtomicU32);

    impl crate::application::ports::SystemProbe for Flaky {
        fn identity(&self) -> crate::domain::machine::MachineIdentity {
            unreachable!("l'échantillonneur ne demande pas l'identité")
        }

        fn sample(
            &self,
        ) -> Result<crate::domain::metrics::SystemSample, crate::application::ports::ProbeError>
        {
            match self.0.fetch_add(1, Ordering::SeqCst) {
                0 => panic!("la sonde casse"),
                1 => Err(crate::application::ports::ProbeError::Unreadable(
                    "capteur muet".into(),
                )),
                call => Ok(crate::domain::metrics::SystemSample {
                    uptime_s: u64::from(call),
                    cpu: 1.0,
                    cores: vec![1.0],
                    mem: crate::domain::metrics::MemoryUsage {
                        used_bytes: 1,
                        total_bytes: 2,
                    },
                    disks: vec![],
                    net: None,
                    temps: vec![],
                }),
            }
        }
    }

    struct NoGpu;

    impl crate::application::ports::GpuProbe for NoGpu {
        fn detect(&self) -> Vec<crate::domain::machine::GpuIdentity> {
            Vec::new()
        }

        fn sample(&self) -> Vec<crate::domain::metrics::GpuReading> {
            Vec::new()
        }
    }

    struct RealTime;

    impl crate::application::ports::Clock for RealTime {
        fn now(&self) -> time::OffsetDateTime {
            time::OffsetDateTime::now_utc()
        }
    }

    struct Mono(std::time::Instant);

    impl crate::application::ports::MonotonicClock for Mono {
        fn elapsed(&self) -> time::Duration {
            time::Duration::try_from(self.0.elapsed()).unwrap_or(time::Duration::ZERO)
        }
    }

    #[tokio::test]
    async fn the_sampler_survives_a_probe_panic_and_a_probe_error() {
        let service = Arc::new(MetricsService::new(
            Arc::new(Flaky(AtomicU32::new(0))),
            Arc::new(NoGpu),
            Arc::new(RealTime),
            Arc::new(Mono(std::time::Instant::now())),
        ));
        let mut feed = service.subscribe();
        let _sampler = spawn_sampler(service.clone(), Duration::from_millis(10));
        // Le premier échantillon reçu est le troisième passage : les deux ratés n'ont rien arrêté.
        let sample = tokio::time::timeout(Duration::from_secs(5), feed.recv())
            .await
            .expect("l'échantillonneur continue")
            .expect("un échantillon");
        assert_eq!(sample.uptime_s, 2);
        let next = tokio::time::timeout(Duration::from_secs(5), feed.recv())
            .await
            .expect("et continue encore")
            .expect("un échantillon");
        assert_eq!(next.uptime_s, 3);
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

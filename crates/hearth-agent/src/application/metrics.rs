//! Cas d'usage des mesures : prendre un échantillon, le garder dans l'anneau d'une heure, le
//! diffuser aux abonnés, rendre l'identité de la machine et l'historique (BR-DASH-001, 002, 008,
//! 010). Les règles (taille de l'anneau, fenêtres, pas) sont dans `domain::metrics`.
//!
//! La cadence d'une seconde et la supervision sont celles de la tâche de fond
//! (`entrypoint::tasks::spawn_sampler`) : ce module ne sait pas attendre.

use std::sync::{Arc, Mutex, PoisonError};

use thiserror::Error;
use tokio::sync::broadcast;

use super::ports::{Clock, GpuProbe, ProbeError, SystemProbe};
use crate::domain::machine::MachineIdentity;
use crate::domain::metrics::{HistoryWindow, Ring, Sample};

/// Échantillons que garde un abonné avant d'en perdre : un abonné lent perd les plus anciens, il
/// ne retient jamais l'échantillonnage ni les autres abonnés.
pub const BROADCAST_CAPACITY: usize = 16;

#[derive(Debug, Error)]
pub enum MetricsError {
    #[error(transparent)]
    Probe(#[from] ProbeError),
    /// La sonde a paniqué : l'échantillon de cette seconde est perdu, le suivant aura lieu.
    #[error("la sonde a paniqué ou a été interrompue : {0}")]
    Interrupted(String),
}

pub struct MetricsService {
    probe: Arc<dyn SystemProbe>,
    gpu: Arc<dyn GpuProbe>,
    clock: Arc<dyn Clock>,
    ring: Mutex<Ring>,
    feed: broadcast::Sender<Arc<Sample>>,
}

impl MetricsService {
    pub fn new(probe: Arc<dyn SystemProbe>, gpu: Arc<dyn GpuProbe>, clock: Arc<dyn Clock>) -> Self {
        Self::with_capacity(probe, gpu, clock, BROADCAST_CAPACITY)
    }

    pub fn with_capacity(
        probe: Arc<dyn SystemProbe>,
        gpu: Arc<dyn GpuProbe>,
        clock: Arc<dyn Clock>,
        broadcast_capacity: usize,
    ) -> Self {
        Self {
            probe,
            gpu,
            clock,
            ring: Mutex::new(Ring::default()),
            feed: broadcast::channel(broadcast_capacity.max(1)).0,
        }
    }

    /// Identité de la machine à cet instant : la sonde système et les cartes graphiques
    /// détectées. Relue à chaque appel (un disque peut avoir été monté).
    pub async fn identity(&self) -> Result<MachineIdentity, MetricsError> {
        let probe = self.probe.clone();
        let gpu = self.gpu.clone();
        let (mut identity, gpus) =
            tokio::task::spawn_blocking(move || (probe.identity(), gpu.detect()))
                .await
                .map_err(|error| MetricsError::Interrupted(error.to_string()))?;
        identity.gpus = gpus;
        Ok(identity)
    }

    /// L'historique d'une fenêtre, rééchantillonné au pas de la fenêtre.
    pub fn history(&self, window: HistoryWindow) -> Vec<Sample> {
        let now = self.clock.now();
        // Copie sous le verrou, rééchantillonnage hors du verrou : le verrou n'est jamais tenu
        // pendant un calcul.
        let raw = self
            .ring
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .since(now - window.span());
        crate::domain::metrics::resample(&raw, window.step_s())
    }

    /// S'abonne aux prochains échantillons. S'abonner **avant** de lire l'historique : aucun
    /// échantillon n'échappe, ceux déjà dans l'historique se reconnaissent à leur date.
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<Sample>> {
        self.feed.subscribe()
    }

    /// Prend un échantillon, le garde, le diffuse. Une erreur ou une panique de la sonde perd cet
    /// échantillon seulement : l'anneau et les abonnés sont intacts, l'appel suivant mesure.
    pub async fn sample_once(&self) -> Result<(), MetricsError> {
        let probe = self.probe.clone();
        let gpu = self.gpu.clone();
        let (system, gpus) = tokio::task::spawn_blocking(move || (probe.sample(), gpu.sample()))
            .await
            .map_err(|error| MetricsError::Interrupted(error.to_string()))?;
        let sample = Arc::new(Sample::new(self.clock.now(), system?, gpus));
        self.ring
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Sample::clone(&sample));
        // Sans abonné, l'envoi échoue : ce n'est pas une erreur.
        let _ = self.feed.send(sample);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use time::{Duration, OffsetDateTime};

    use super::*;
    use crate::domain::machine::{CpuIdentity, GpuIdentity, MachineIdentity, OsIdentity};
    use crate::domain::metrics::{GpuReading, MemoryUsage, SystemSample};

    struct ManualClock(Mutex<OffsetDateTime>);

    impl ManualClock {
        fn new() -> Arc<Self> {
            Arc::new(Self(Mutex::new(
                OffsetDateTime::UNIX_EPOCH + Duration::seconds(1_790_000_000),
            )))
        }

        fn advance(&self, seconds: i64) {
            *self.0.lock().unwrap() += Duration::seconds(seconds);
        }
    }

    impl Clock for ManualClock {
        fn now(&self) -> OffsetDateTime {
            *self.0.lock().unwrap()
        }
    }

    /// Sonde simulée : `cpu` croît à chaque appel ; elle échoue ou panique au gré des réglages.
    #[derive(Default)]
    struct FakeProbe {
        calls: AtomicU32,
        fail_on: Option<u32>,
        panic_on: Option<u32>,
    }

    impl SystemProbe for FakeProbe {
        fn identity(&self) -> MachineIdentity {
            MachineIdentity {
                name: "forge".into(),
                os: OsIdentity {
                    name: "NixOS".into(),
                    version: None,
                    kernel: None,
                    arch: "x86_64".into(),
                },
                cpu: CpuIdentity {
                    model: "cpu".into(),
                    physical_cores: None,
                    logical_cores: 2,
                    frequency_mhz: None,
                },
                memory_total_bytes: 1_000,
                disks: vec![],
                gpus: vec![],
                has_temperature_sensors: false,
            }
        }

        fn sample(&self) -> Result<SystemSample, ProbeError> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            if self.panic_on == Some(call) {
                panic!("la sonde casse");
            }
            if self.fail_on == Some(call) {
                return Err(ProbeError::Unreadable("capteur muet".into()));
            }
            Ok(SystemSample {
                uptime_s: 10,
                cpu: call as f32,
                cores: vec![call as f32, call as f32],
                mem: MemoryUsage {
                    used_bytes: 1,
                    total_bytes: 1_000,
                },
                disks: vec![],
                net: None,
                temps: vec![],
            })
        }
    }

    struct FakeGpu(Vec<GpuIdentity>);

    impl GpuProbe for FakeGpu {
        fn detect(&self) -> Vec<GpuIdentity> {
            self.0.clone()
        }

        fn sample(&self) -> Vec<GpuReading> {
            self.0
                .iter()
                .map(|gpu| GpuReading {
                    name: gpu.name.clone(),
                    load_percent: Some(5.0),
                    memory_used_bytes: None,
                    memory_total_bytes: gpu.memory_total_bytes,
                    temp_c: None,
                })
                .collect()
        }
    }

    fn service(
        probe: FakeProbe,
        gpus: Vec<GpuIdentity>,
        clock: &Arc<ManualClock>,
    ) -> MetricsService {
        MetricsService::new(Arc::new(probe), Arc::new(FakeGpu(gpus)), clock.clone())
    }

    #[tokio::test]
    async fn a_sample_is_stamped_by_the_clock_kept_and_broadcast() {
        let clock = ManualClock::new();
        let metrics = service(FakeProbe::default(), vec![], &clock);
        let mut feed = metrics.subscribe();
        metrics.sample_once().await.unwrap();
        let received = feed.recv().await.unwrap();
        assert_eq!(received.at, clock.now());
        assert_eq!(received.cpu, 0.0);
        assert_eq!(metrics.history(HistoryWindow::OneMinute).len(), 1);
        // Une minute plus tard, l'échantillon sort de la fenêtre d'une minute, pas de l'anneau.
        clock.advance(61);
        assert_eq!(metrics.history(HistoryWindow::OneMinute).len(), 0);
        assert_eq!(metrics.history(HistoryWindow::FiveMinutes).len(), 1);
    }

    #[tokio::test]
    async fn gpu_readings_join_the_sample() {
        let clock = ManualClock::new();
        let gpu = GpuIdentity {
            name: "RTX".into(),
            memory_total_bytes: Some(8),
        };
        let metrics = service(FakeProbe::default(), vec![gpu], &clock);
        let mut feed = metrics.subscribe();
        metrics.sample_once().await.unwrap();
        let sample = feed.recv().await.unwrap();
        assert_eq!(sample.gpus.len(), 1);
        assert_eq!(sample.gpus[0].load_percent, Some(5.0));
        assert_eq!(sample.gpus[0].temp_c, None);
    }

    #[tokio::test]
    async fn the_identity_adds_the_detected_gpus_and_their_capability() {
        let clock = ManualClock::new();
        let without = service(FakeProbe::default(), vec![], &clock);
        let identity = without.identity().await.unwrap();
        assert!(!identity.capabilities().gpu);
        let gpu = GpuIdentity {
            name: "RTX".into(),
            memory_total_bytes: None,
        };
        let with = service(FakeProbe::default(), vec![gpu], &clock);
        let identity = with.identity().await.unwrap();
        assert!(identity.capabilities().gpu);
        assert_eq!(identity.gpus[0].name, "RTX");
    }

    #[tokio::test]
    async fn a_probe_error_loses_one_sample_and_the_next_one_is_taken() {
        let clock = ManualClock::new();
        let probe = FakeProbe {
            fail_on: Some(1),
            ..FakeProbe::default()
        };
        let metrics = service(probe, vec![], &clock);
        let mut feed = metrics.subscribe();
        metrics.sample_once().await.unwrap();
        clock.advance(1);
        assert!(matches!(
            metrics.sample_once().await,
            Err(MetricsError::Probe(_))
        ));
        clock.advance(1);
        metrics.sample_once().await.unwrap();
        clock.advance(1);
        // Deux échantillons reçus et gardés : le raté n'a laissé aucune trace.
        assert_eq!(feed.recv().await.unwrap().cpu, 0.0);
        assert_eq!(feed.recv().await.unwrap().cpu, 2.0);
        assert_eq!(metrics.history(HistoryWindow::OneMinute).len(), 2);
    }

    #[tokio::test]
    async fn a_probe_panic_does_not_stop_the_sampler() {
        let clock = ManualClock::new();
        let probe = FakeProbe {
            panic_on: Some(0),
            ..FakeProbe::default()
        };
        let metrics = service(probe, vec![], &clock);
        assert!(matches!(
            metrics.sample_once().await,
            Err(MetricsError::Interrupted(_))
        ));
        clock.advance(1);
        metrics.sample_once().await.unwrap();
        clock.advance(1);
        assert_eq!(metrics.history(HistoryWindow::OneMinute).len(), 1);
    }

    #[tokio::test]
    async fn a_slow_subscriber_loses_samples_without_blocking_the_others() {
        let clock = ManualClock::new();
        let metrics = MetricsService::with_capacity(
            Arc::new(FakeProbe::default()),
            Arc::new(FakeGpu(vec![])),
            clock.clone(),
            2,
        );
        let mut slow = metrics.subscribe();
        let mut fast = metrics.subscribe();
        for _ in 0..6 {
            // Chaque envoi est immédiat, quel que soit l'état du lent.
            tokio::time::timeout(std::time::Duration::from_secs(5), metrics.sample_once())
                .await
                .expect("l'échantillonnage n'attend personne")
                .unwrap();
            clock.advance(1);
            assert!(fast.recv().await.is_ok(), "le rapide reçoit tout");
        }
        // Le lent n'a rien lu : il apprend combien il en a perdu, puis reçoit les plus récents.
        assert!(matches!(
            slow.recv().await,
            Err(broadcast::error::RecvError::Lagged(4))
        ));
        assert_eq!(slow.recv().await.unwrap().cpu, 4.0);
        assert_eq!(slow.recv().await.unwrap().cpu, 5.0);
    }

    #[tokio::test]
    async fn the_hour_window_is_averaged_by_ten_seconds() {
        let clock = ManualClock::new();
        let metrics = service(FakeProbe::default(), vec![], &clock);
        // Aligne l'horloge sur un multiple de 10 s, puis 20 échantillons à 1 s.
        let start = clock.now().unix_timestamp();
        clock.advance(10 - start.rem_euclid(10));
        for _ in 0..20 {
            metrics.sample_once().await.unwrap();
            clock.advance(1);
        }
        let hour = metrics.history(HistoryWindow::OneHour);
        assert_eq!(hour.len(), 2);
        let minute = metrics.history(HistoryWindow::OneMinute);
        assert_eq!(minute.len(), 20);
    }
}

//! Cas d'usage des mesures : prendre un échantillon, le garder dans l'anneau d'une heure, le
//! diffuser aux abonnés, rendre l'identité de la machine et l'historique (BR-DASH-001, 002, 008,
//! 010). Les règles (taille de l'anneau, fenêtres, pas, délais) sont dans `domain::metrics`.
//!
//! La cadence d'une seconde et la supervision sont celles de la tâche de fond
//! (`entrypoint::tasks::spawn_sampler`) : ce module ne sait pas attendre. Les fenêtres et l'ordre
//! des échantillons reposent sur l'horloge **monotone** ; l'horloge murale ne date que
//! l'échantillon.
//!
//! Une sonde qui ne revient pas ne bloque rien : l'échantillon est abandonné après
//! `SAMPLE_TIMEOUT`, aucun second échantillon n'est lancé tant que le premier n'est pas revenu
//! (pas d'empilement de fils bloquants), et l'identité servie est celle du cache.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration as StdDuration;

use thiserror::Error;
use time::Duration;
use tokio::sync::broadcast;

use super::ports::{Clock, GpuProbe, MonotonicClock, ProbeError, SystemProbe};
use crate::domain::machine::MachineIdentity;
use crate::domain::metrics::{
    HistoryWindow, IDENTITY_MAX_AGE, Ring, SAMPLE_TIMEOUT, Sample, window_samples,
};

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
    /// La sonde n'est pas revenue à temps : cet échantillon est abandonné (déjà journalisé).
    #[error("la sonde ne répond pas")]
    Stalled,
    /// La sonde n'est pas encore revenue du passage précédent : on ne l'appelle pas une seconde
    /// fois.
    #[error("la sonde est encore occupée par le passage précédent")]
    Skipped,
}

/// Un seul appel de sonde en cours à la fois, même si l'appelant a abandonné l'attente.
#[derive(Clone, Default)]
struct Flight(Arc<AtomicBool>);

struct FlightGuard(Arc<AtomicBool>);

impl Drop for FlightGuard {
    fn drop(&mut self) {
        // Aussi en cas de panique de la sonde : le déroulement libère le droit d'appeler.
        self.0.store(false, Ordering::SeqCst);
    }
}

impl Flight {
    fn try_acquire(&self) -> Option<FlightGuard> {
        (!self.0.swap(true, Ordering::SeqCst)).then(|| FlightGuard(self.0.clone()))
    }
}

pub struct MetricsService {
    probe: Arc<dyn SystemProbe>,
    gpu: Arc<dyn GpuProbe>,
    clock: Arc<dyn Clock>,
    mono: Arc<dyn MonotonicClock>,
    ring: Mutex<Ring>,
    feed: broadcast::Sender<Arc<Sample>>,
    sample_timeout: StdDuration,
    sampling: Flight,
    /// Un épisode de blocage est déjà journalisé : pas de répétition à chaque seconde.
    stalled: AtomicBool,
    identity_flight: Flight,
    identity: tokio::sync::Mutex<Option<(Duration, MachineIdentity)>>,
}

impl MetricsService {
    pub fn new(
        probe: Arc<dyn SystemProbe>,
        gpu: Arc<dyn GpuProbe>,
        clock: Arc<dyn Clock>,
        mono: Arc<dyn MonotonicClock>,
    ) -> Self {
        Self {
            probe,
            gpu,
            clock,
            mono,
            ring: Mutex::new(Ring::default()),
            feed: broadcast::channel(BROADCAST_CAPACITY).0,
            sample_timeout: SAMPLE_TIMEOUT,
            sampling: Flight::default(),
            stalled: AtomicBool::new(false),
            identity_flight: Flight::default(),
            identity: tokio::sync::Mutex::new(None),
        }
    }

    /// Capacité du canal de diffusion (tests de l'abonné lent).
    pub fn with_broadcast_capacity(mut self, capacity: usize) -> Self {
        self.feed = broadcast::channel(capacity.max(1)).0;
        self
    }

    /// Délai accordé à la sonde pour un échantillon ou une identité.
    pub fn with_sample_timeout(mut self, timeout: StdDuration) -> Self {
        self.sample_timeout = timeout;
        self
    }

    /// Identité de la machine : celle du cache, rafraîchie au plus toutes les
    /// `IDENTITY_MAX_AGE`. Une sonde qui ne répond pas ne bloque pas : on sert le cache, périmé
    /// s'il le faut (erreur seulement s'il n'y en a jamais eu).
    pub async fn identity(&self) -> Result<MachineIdentity, MetricsError> {
        let mut cache = self.identity.lock().await;
        let now = self.mono.elapsed();
        if let Some((at, identity)) = cache.as_ref()
            && now - *at < IDENTITY_MAX_AGE
        {
            return Ok(identity.clone());
        }
        let refreshed = match self.identity_flight.try_acquire() {
            Some(guard) => {
                let probe = self.probe.clone();
                let gpu = self.gpu.clone();
                let task = tokio::task::spawn_blocking(move || {
                    let _guard = guard;
                    let mut identity = probe.identity();
                    identity.gpus = gpu.detect();
                    identity
                });
                tokio::time::timeout(self.sample_timeout, task)
                    .await
                    .ok()
                    .and_then(Result::ok)
            }
            None => None,
        };
        match refreshed {
            Some(identity) => {
                *cache = Some((now, identity.clone()));
                Ok(identity)
            }
            None => match cache.as_ref() {
                Some((_, stale)) => Ok(stale.clone()),
                None => Err(MetricsError::Stalled),
            },
        }
    }

    /// L'historique d'une fenêtre, rééchantillonné au pas de la fenêtre.
    pub fn history(&self, window: HistoryWindow) -> Vec<Arc<Sample>> {
        let now = self.mono.elapsed();
        // Copie des `Arc` sous le verrou, moyennes hors du verrou : le verrou n'est jamais tenu
        // pendant un calcul.
        let raw = self
            .ring
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .since(now, window.span());
        window_samples(raw, window)
    }

    /// S'abonne aux prochains échantillons. S'abonner **avant** de lire l'historique : aucun
    /// échantillon n'échappe, ceux déjà dans l'historique se reconnaissent à leur instant
    /// monotone.
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<Sample>> {
        self.feed.subscribe()
    }

    /// Prend un échantillon, le garde, le diffuse. Une erreur, une panique ou un blocage de la
    /// sonde perd cet échantillon seulement : l'anneau et les abonnés sont intacts, l'appel
    /// suivant mesure (ou, si la sonde n'est pas revenue, saute son tour).
    pub async fn sample_once(&self) -> Result<(), MetricsError> {
        let Some(guard) = self.sampling.try_acquire() else {
            return Err(MetricsError::Skipped);
        };
        let probe = self.probe.clone();
        let gpu = self.gpu.clone();
        let task = tokio::task::spawn_blocking(move || {
            let _guard = guard;
            (probe.sample(), gpu.sample())
        });
        let (system, gpus) = match tokio::time::timeout(self.sample_timeout, task).await {
            Err(_) => {
                // Journal une fois par épisode ; le fil bloqué garde son droit d'appel jusqu'à
                // son retour.
                if !self.stalled.swap(true, Ordering::SeqCst) {
                    tracing::warn!(
                        timeout_s = self.sample_timeout.as_secs_f32(),
                        "la sonde système ne répond pas : échantillons sautés jusqu'à son retour"
                    );
                }
                return Err(MetricsError::Stalled);
            }
            Ok(joined) => joined.map_err(|error| MetricsError::Interrupted(error.to_string()))?,
        };
        if self.stalled.swap(false, Ordering::SeqCst) {
            tracing::info!("la sonde système répond de nouveau");
        }
        let sample = Arc::new(Sample::new(
            self.clock.now(),
            self.mono.elapsed(),
            system?,
            gpus,
        ));
        self.ring
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(sample.clone());
        // Sans abonné, l'envoi échoue : ce n'est pas une erreur.
        let _ = self.feed.send(sample);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU32;

    use time::OffsetDateTime;

    use super::*;
    use crate::domain::machine::{CpuIdentity, GpuIdentity, MachineIdentity, OsIdentity};
    use crate::domain::metrics::{DiskUsage, GpuReading, MemoryUsage, SystemSample};

    /// Les deux horloges, pilotées à la main : la murale peut reculer, la monotone jamais.
    struct Time {
        wall: Mutex<OffsetDateTime>,
        mono: Mutex<Duration>,
    }

    impl Time {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                wall: Mutex::new(OffsetDateTime::UNIX_EPOCH + Duration::seconds(1_790_000_000)),
                mono: Mutex::new(Duration::seconds(5)),
            })
        }

        fn advance(&self, seconds: i64) {
            *self.wall.lock().unwrap() += Duration::seconds(seconds);
            *self.mono.lock().unwrap() += Duration::seconds(seconds);
        }

        /// L'horloge murale recule (synchronisation, réglage) ; le temps monotone avance.
        fn wall_jumps_back(&self, seconds: i64) {
            *self.wall.lock().unwrap() -= Duration::seconds(seconds);
            *self.mono.lock().unwrap() += Duration::seconds(1);
        }
    }

    impl Clock for Time {
        fn now(&self) -> OffsetDateTime {
            *self.wall.lock().unwrap()
        }
    }

    impl MonotonicClock for Time {
        fn elapsed(&self) -> Duration {
            *self.mono.lock().unwrap()
        }
    }

    /// Sonde simulée : `cpu` croît à chaque appel ; elle échoue, panique ou dort au gré des
    /// réglages.
    #[derive(Default)]
    struct FakeProbe {
        calls: AtomicU32,
        identities: AtomicU32,
        fail_on: Option<u32>,
        panic_on: Option<u32>,
        /// Dort (fil bloquant) pendant cet appel : une sonde qui ne revient pas.
        sleep_on: Option<(u32, StdDuration)>,
        /// Comme le verrou de la vraie sonde : un échantillon qui dort bloque l'identité.
        busy: Mutex<()>,
        /// Disques montés à cet instant : le test les change entre deux échantillons.
        disks: Mutex<Vec<DiskUsage>>,
    }

    impl SystemProbe for FakeProbe {
        fn identity(&self) -> MachineIdentity {
            let _held = self.busy.lock().unwrap_or_else(PoisonError::into_inner);
            self.identities.fetch_add(1, Ordering::SeqCst);
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
            let _held = self.busy.lock().unwrap_or_else(PoisonError::into_inner);
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            if self.panic_on == Some(call) {
                panic!("la sonde casse");
            }
            if let Some((on, duration)) = self.sleep_on
                && on == call
            {
                std::thread::sleep(duration);
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
                disks: self.disks.lock().unwrap().clone(),
                net: None,
                temps: vec![],
            })
        }
    }

    struct FakeGpu(Mutex<Vec<GpuIdentity>>);

    impl FakeGpu {
        fn new(gpus: Vec<GpuIdentity>) -> Arc<Self> {
            Arc::new(Self(Mutex::new(gpus)))
        }
    }

    impl GpuProbe for FakeGpu {
        fn detect(&self) -> Vec<GpuIdentity> {
            self.0.lock().unwrap().clone()
        }

        fn sample(&self) -> Vec<GpuReading> {
            self.detect()
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

    fn service(probe: FakeProbe, gpus: Vec<GpuIdentity>, time: &Arc<Time>) -> MetricsService {
        MetricsService::new(
            Arc::new(probe),
            FakeGpu::new(gpus),
            time.clone(),
            time.clone(),
        )
    }

    /// Attend qu'une condition devienne vraie (délai large : seul un vrai blocage échoue).
    async fn eventually(what: &str, condition: impl Fn() -> bool) {
        let started = std::time::Instant::now();
        while !condition() {
            assert!(
                started.elapsed() < StdDuration::from_secs(20),
                "délai dépassé : {what}"
            );
            tokio::time::sleep(StdDuration::from_millis(5)).await;
        }
    }

    #[tokio::test]
    async fn a_sample_is_stamped_by_both_clocks_kept_and_broadcast() {
        let time = Time::new();
        let metrics = service(FakeProbe::default(), vec![], &time);
        let mut feed = metrics.subscribe();
        metrics.sample_once().await.unwrap();
        let received = feed.recv().await.unwrap();
        assert_eq!(received.at, time.now());
        assert_eq!(received.mono, time.elapsed());
        assert_eq!(received.cpu, 0.0);
        assert_eq!(metrics.history(HistoryWindow::OneMinute).len(), 1);
        // Une minute plus tard, l'échantillon sort de la fenêtre d'une minute, pas de l'anneau.
        time.advance(61);
        assert_eq!(metrics.history(HistoryWindow::OneMinute).len(), 0);
        assert_eq!(metrics.history(HistoryWindow::FiveMinutes).len(), 1);
    }

    #[tokio::test]
    async fn the_ring_and_the_subscribers_share_the_same_sample() {
        let time = Time::new();
        let metrics = service(FakeProbe::default(), vec![], &time);
        let mut feed = metrics.subscribe();
        metrics.sample_once().await.unwrap();
        let broadcast = feed.recv().await.unwrap();
        let kept = metrics.history(HistoryWindow::OneMinute);
        assert!(Arc::ptr_eq(&broadcast, &kept[0]), "aucune copie profonde");
    }

    #[tokio::test]
    async fn gpu_readings_join_the_sample() {
        let time = Time::new();
        let gpu = GpuIdentity {
            name: "RTX".into(),
            memory_total_bytes: Some(8),
        };
        let metrics = service(FakeProbe::default(), vec![gpu], &time);
        let mut feed = metrics.subscribe();
        metrics.sample_once().await.unwrap();
        let sample = feed.recv().await.unwrap();
        assert_eq!(sample.gpus.len(), 1);
        assert_eq!(sample.gpus[0].load_percent, Some(5.0));
        assert_eq!(sample.gpus[0].temp_c, None);
    }

    #[tokio::test]
    async fn the_identity_adds_the_detected_gpus_and_their_capability() {
        let time = Time::new();
        let without = service(FakeProbe::default(), vec![], &time);
        let identity = without.identity().await.unwrap();
        assert!(!identity.capabilities().gpu);
        let gpu = GpuIdentity {
            name: "RTX".into(),
            memory_total_bytes: None,
        };
        let with = service(FakeProbe::default(), vec![gpu], &time);
        let identity = with.identity().await.unwrap();
        assert!(identity.capabilities().gpu);
        assert_eq!(identity.gpus[0].name, "RTX");
    }

    #[tokio::test]
    async fn the_identity_is_cached_and_refreshed_at_most_every_thirty_seconds() {
        let time = Time::new();
        let probe = Arc::new(FakeProbe::default());
        let gpus = FakeGpu::new(vec![GpuIdentity {
            name: "RTX".into(),
            memory_total_bytes: None,
        }]);
        let metrics = MetricsService::new(probe.clone(), gpus.clone(), time.clone(), time.clone());
        for _ in 0..50 {
            assert!(metrics.identity().await.unwrap().capabilities().gpu);
        }
        assert_eq!(probe.identities.load(Ordering::SeqCst), 1);
        // La carte disparaît un instant (relance de `nvidia-smi`) : le cache la garde.
        gpus.0.lock().unwrap().clear();
        time.advance(29);
        assert!(metrics.identity().await.unwrap().capabilities().gpu);
        assert_eq!(probe.identities.load(Ordering::SeqCst), 1);
        // Passé le délai, l'identité est relue.
        time.advance(2);
        assert!(!metrics.identity().await.unwrap().capabilities().gpu);
        assert_eq!(probe.identities.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn a_probe_error_loses_one_sample_and_the_next_one_is_taken() {
        let time = Time::new();
        let probe = FakeProbe {
            fail_on: Some(1),
            ..FakeProbe::default()
        };
        let metrics = service(probe, vec![], &time);
        let mut feed = metrics.subscribe();
        metrics.sample_once().await.unwrap();
        time.advance(1);
        assert!(matches!(
            metrics.sample_once().await,
            Err(MetricsError::Probe(_))
        ));
        time.advance(1);
        metrics.sample_once().await.unwrap();
        time.advance(1);
        // Deux échantillons reçus et gardés : le raté n'a laissé aucune trace.
        assert_eq!(feed.recv().await.unwrap().cpu, 0.0);
        assert_eq!(feed.recv().await.unwrap().cpu, 2.0);
        assert_eq!(metrics.history(HistoryWindow::OneMinute).len(), 2);
    }

    #[tokio::test]
    async fn a_probe_panic_does_not_stop_the_sampler() {
        let time = Time::new();
        let probe = FakeProbe {
            panic_on: Some(0),
            ..FakeProbe::default()
        };
        let metrics = service(probe, vec![], &time);
        assert!(matches!(
            metrics.sample_once().await,
            Err(MetricsError::Interrupted(_))
        ));
        time.advance(1);
        metrics.sample_once().await.unwrap();
        time.advance(1);
        assert_eq!(metrics.history(HistoryWindow::OneMinute).len(), 1);
    }

    #[tokio::test]
    async fn a_probe_that_sleeps_skips_turns_without_stacking_blocked_threads() {
        let time = Time::new();
        let probe = Arc::new(FakeProbe {
            sleep_on: Some((0, StdDuration::from_millis(600))),
            ..FakeProbe::default()
        });
        let metrics = MetricsService::new(
            probe.clone(),
            FakeGpu::new(vec![]),
            time.clone(),
            time.clone(),
        )
        .with_sample_timeout(StdDuration::from_millis(50));
        // La sonde dort : l'échantillon est abandonné après le délai.
        assert!(matches!(
            metrics.sample_once().await,
            Err(MetricsError::Stalled)
        ));
        // Tant qu'elle n'est pas revenue, les tours suivants sont sautés : aucun second appel.
        for _ in 0..5 {
            assert!(matches!(
                metrics.sample_once().await,
                Err(MetricsError::Skipped)
            ));
        }
        assert_eq!(probe.calls.load(Ordering::SeqCst), 1, "pas d'empilement");
        // Elle revient : la mesure reprend.
        let mut taken = false;
        for _ in 0..400 {
            time.advance(1);
            if metrics.sample_once().await.is_ok() {
                taken = true;
                break;
            }
            tokio::time::sleep(StdDuration::from_millis(10)).await;
        }
        assert!(taken, "la mesure doit reprendre quand la sonde revient");
        assert_eq!(metrics.history(HistoryWindow::FiveMinutes).len(), 1);
    }

    #[tokio::test]
    async fn the_identity_is_served_from_the_cache_while_the_probe_is_stuck() {
        let time = Time::new();
        let probe = Arc::new(FakeProbe {
            sleep_on: Some((1, StdDuration::from_millis(1_000))),
            ..FakeProbe::default()
        });
        let metrics = MetricsService::new(
            probe.clone(),
            FakeGpu::new(vec![]),
            time.clone(),
            time.clone(),
        )
        .with_sample_timeout(StdDuration::from_millis(50));
        let first = metrics.identity().await.unwrap();
        metrics.sample_once().await.unwrap();
        time.advance(1);
        // Le deuxième échantillon dort : le tour est abandonné.
        assert!(matches!(
            metrics.sample_once().await,
            Err(MetricsError::Stalled)
        ));
        // Cache périmé (le délai est passé) et sonde occupée : on sert le cache, au bout du délai
        // accordé, sans s'accrocher à la sonde.
        time.advance(60);
        assert_eq!(metrics.identity().await.unwrap(), first);
        // Et un autre appel ne lance pas une seconde lecture bloquée.
        assert_eq!(metrics.identity().await.unwrap(), first);
        assert_eq!(probe.identities.load(Ordering::SeqCst), 1);
        eventually("la sonde est revenue", || {
            metrics.sampling.try_acquire().is_some()
        })
        .await;
    }

    #[tokio::test]
    async fn a_wall_clock_that_goes_back_does_not_silence_or_reorder_the_samples() {
        let time = Time::new();
        let metrics = service(FakeProbe::default(), vec![], &time);
        let mut feed = metrics.subscribe();
        for _ in 0..3 {
            metrics.sample_once().await.unwrap();
            // L'horloge murale recule d'une heure à chaque tour ; la monotone avance.
            time.wall_jumps_back(3_600);
        }
        let received: Vec<_> = (0..3).map(|_| feed.try_recv().unwrap()).collect();
        assert!(received.windows(2).all(|pair| pair[1].at < pair[0].at));
        assert!(received.windows(2).all(|pair| pair[1].mono > pair[0].mono));
        // L'historique les contient tous, dans l'ordre monotone.
        let history = metrics.history(HistoryWindow::OneMinute);
        assert_eq!(history.len(), 3);
        assert!(history.windows(2).all(|pair| pair[1].mono > pair[0].mono));
        // Et l'envoi aux abonnés se décide sur la monotone.
        assert!(crate::domain::stream::is_new(
            Some(received[0].mono),
            received[1].mono
        ));
    }

    #[tokio::test]
    async fn a_slow_subscriber_loses_samples_without_blocking_the_others() {
        let time = Time::new();
        let metrics = service(FakeProbe::default(), vec![], &time).with_broadcast_capacity(2);
        let mut slow = metrics.subscribe();
        let mut fast = metrics.subscribe();
        for _ in 0..6 {
            // Chaque envoi est immédiat, quel que soit l'état du lent.
            tokio::time::timeout(StdDuration::from_secs(30), metrics.sample_once())
                .await
                .expect("l'échantillonnage n'attend personne")
                .unwrap();
            time.advance(1);
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
    async fn a_mounted_or_removed_disk_shows_in_the_next_sample() {
        let time = Time::new();
        let probe = Arc::new(FakeProbe::default());
        let metrics = MetricsService::new(
            probe.clone(),
            FakeGpu::new(vec![]),
            time.clone(),
            time.clone(),
        );
        let disk = |mount: &str| DiskUsage {
            name: format!("dev{mount}"),
            mount: mount.into(),
            used_bytes: 1,
            total_bytes: 2,
        };
        let mounts = |window| -> Vec<Vec<String>> {
            metrics
                .history(window)
                .iter()
                .map(|sample| sample.disks.iter().map(|d| d.mount.clone()).collect())
                .collect()
        };

        *probe.disks.lock().unwrap() = vec![disk("/")];
        metrics.sample_once().await.unwrap();
        time.advance(1);
        // Un disque est monté.
        *probe.disks.lock().unwrap() = vec![disk("/"), disk("/mnt/usb")];
        metrics.sample_once().await.unwrap();
        time.advance(1);
        // Puis retiré.
        *probe.disks.lock().unwrap() = vec![disk("/")];
        metrics.sample_once().await.unwrap();
        time.advance(1);
        assert_eq!(
            mounts(HistoryWindow::OneMinute),
            [vec!["/"], vec!["/", "/mnt/usb"], vec!["/"]]
        );
    }

    #[tokio::test]
    async fn the_hour_window_is_averaged_by_ten_seconds_of_the_monotonic_clock() {
        let time = Time::new();
        let metrics = service(FakeProbe::default(), vec![], &time);
        // Aligne l'horloge monotone sur un multiple de 10 s, puis 20 échantillons à 1 s.
        let start = time.elapsed().whole_seconds();
        time.advance(10 - start.rem_euclid(10));
        for _ in 0..20 {
            metrics.sample_once().await.unwrap();
            time.advance(1);
        }
        let hour = metrics.history(HistoryWindow::OneHour);
        assert_eq!(hour.len(), 2);
        let minute = metrics.history(HistoryWindow::OneMinute);
        assert_eq!(minute.len(), 20);
    }
}

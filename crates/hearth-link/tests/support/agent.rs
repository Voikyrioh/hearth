//! Un vrai agent démarré dans le processus du test : vrai TLS 1.3, vraie base SQLite temporaire,
//! vrai flux WebSocket ; seules les sondes de mesure sont simulées (un échantillon toutes les
//! 20 ms) et l'horloge de l'agent est pilotable (pour faire expirer une session).
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use hearth_agent::app::{self, Adapters, Metering, RunningAgent, Services};
use hearth_agent::application::ports::{
    Clock as AgentClock, GpuProbe, HashError, PasswordHasher, ProbeError, SystemProbe,
};
use hearth_agent::domain::accounts::{PlainPassword, Role, Username};
use hearth_agent::domain::audit::{Actor, Origin};
use hearth_agent::domain::machine::{
    CpuIdentity, DiskIdentity, GpuIdentity, MachineIdentity, OsIdentity,
};
use hearth_agent::domain::metrics::{DiskUsage, GpuReading, MemoryUsage, SystemSample};
use hearth_agent::domain::secret::Secret as AgentSecret;
use hearth_agent::entrypoint::ws::StreamSettings;
use hearth_agent::infrastructure::argon2::Argon2Hasher;
use hearth_agent::infrastructure::clock::{SystemClock, SystemMonotonic};
use hearth_agent::infrastructure::config::AgentConfig;
use hearth_agent::infrastructure::ids::UlidGen;
use hearth_agent::infrastructure::random::OsTokenGen;
use hearth_agent::infrastructure::sqlite::Database;
use tempfile::TempDir;
use time::{Duration as TimeDuration, OffsetDateTime};

pub const PASSWORD: &str = "Correct-Horse-9";

/// Qui demande, pour le journal d'activité : l'administrateur d'un poste du réseau.
fn by() -> &'static Actor {
    static BY: std::sync::OnceLock<Actor> = std::sync::OnceLock::new();
    BY.get_or_init(|| {
        Actor::new(
            Some(Username::parse("root").unwrap()),
            Origin::client(Some("poste/1.0"), "10.0.0.7"),
        )
    })
}

pub struct TestClock(Mutex<OffsetDateTime>);

impl TestClock {
    pub fn advance(&self, by: TimeDuration) {
        if let Ok(mut now) = self.0.lock() {
            *now += by;
        }
    }
}

impl AgentClock for TestClock {
    fn now(&self) -> OffsetDateTime {
        self.0
            .lock()
            .map(|now| *now)
            .unwrap_or(OffsetDateTime::UNIX_EPOCH)
    }
}

/// Hacheur à coût minimal dont la vérification peut être ralentie : une requête lente reste « en
/// cours » côté agent le temps de couper le lien.
pub struct SlowHasher {
    inner: Argon2Hasher,
    pub delay_ms: AtomicU64,
}

#[async_trait]
impl PasswordHasher for SlowHasher {
    async fn hash(&self, password: &PlainPassword) -> Result<AgentSecret, HashError> {
        self.inner.hash(password).await
    }

    async fn verify(&self, password: &AgentSecret, hash: &AgentSecret) -> Result<bool, HashError> {
        let delay = self.delay_ms.load(Ordering::SeqCst);
        if delay > 0 {
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
        self.inner.verify(password, hash).await
    }

    fn decoy_hash(&self) -> &AgentSecret {
        self.inner.decoy_hash()
    }
}

#[derive(Default)]
struct FakeSystem {
    calls: AtomicU32,
}

impl SystemProbe for FakeSystem {
    fn identity(&self) -> MachineIdentity {
        MachineIdentity {
            name: "forge-test".into(),
            os: OsIdentity {
                name: "TestOS".into(),
                version: Some("1.0".into()),
                kernel: None,
                arch: "x86_64".into(),
            },
            cpu: CpuIdentity {
                model: "Test CPU".into(),
                physical_cores: Some(2),
                logical_cores: 4,
                frequency_mhz: None,
            },
            memory_total_bytes: 16 << 30,
            disks: vec![DiskIdentity {
                name: "/dev/test".into(),
                mount: "/".into(),
                fs: Some("ext4".into()),
                total_bytes: 100 << 30,
                removable: false,
            }],
            gpus: vec![],
            has_temperature_sensors: false,
        }
    }

    fn sample(&self) -> Result<SystemSample, ProbeError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(SystemSample {
            uptime_s: 1_000 + u64::from(call),
            cpu: (call % 100) as f32,
            cores: vec![(call % 100) as f32; 4],
            mem: MemoryUsage {
                used_bytes: 4 << 30,
                total_bytes: 16 << 30,
            },
            disks: vec![DiskUsage {
                name: "/dev/test".into(),
                mount: "/".into(),
                used_bytes: 40 << 30,
                total_bytes: 100 << 30,
            }],
            net: None,
            temps: vec![],
        })
    }
}

struct NoGpu;

impl GpuProbe for NoGpu {
    fn detect(&self) -> Vec<GpuIdentity> {
        vec![]
    }

    fn sample(&self) -> Vec<GpuReading> {
        vec![]
    }
}

/// Un échantillon toutes les 20 ms ; la session est revérifiée toutes les 50 ms.
fn metering() -> Metering {
    Metering {
        system: Arc::new(FakeSystem::default()),
        gpu: Arc::new(NoGpu),
        clock: Arc::new(SystemClock),
        monotonic: Arc::new(SystemMonotonic::new()),
        period: Duration::from_millis(20),
        stream: StreamSettings {
            auth_timeout: Duration::from_millis(800),
            idle_timeout: Duration::from_secs(10),
            session_check_period: Duration::from_millis(50),
            send_timeout: Duration::from_secs(5),
            max_pending_total: 32,
            max_pending_per_address: 16,
            max_total: 64,
            max_per_account: 8,
            min_subscribe_interval: Duration::from_millis(10),
        },
    }
}

pub struct TestAgent {
    dir: TempDir,
    db: Database,
    pub clock: Arc<TestClock>,
    pub hasher: Arc<SlowHasher>,
    adapters: Adapters,
    pub services: Services,
    running: Option<RunningAgent>,
    pub addr: SocketAddr,
}

impl TestAgent {
    /// Nouvelle installation : nouveau dossier, donc nouvelle identité (nouveau certificat).
    pub async fn install() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(dir.path()).await.unwrap();
        let clock = Arc::new(TestClock(Mutex::new(
            OffsetDateTime::UNIX_EPOCH + TimeDuration::seconds(1_790_000_000),
        )));
        let hasher = Arc::new(SlowHasher {
            inner: Argon2Hasher::with_cost(8, 1, 1).unwrap(),
            delay_ms: AtomicU64::new(0),
        });
        let adapters = Adapters {
            hasher: hasher.clone(),
            clock: clock.clone(),
            ids: Arc::new(UlidGen),
            tokens: Arc::new(OsTokenGen),
        };
        let services = app::services(&db, &adapters);
        let mut agent = Self {
            dir,
            db,
            clock,
            hasher,
            adapters,
            services,
            running: None,
            addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        };
        agent.start().await;
        agent
    }

    async fn start(&mut self) {
        let config = AgentConfig {
            listen_addr: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            data_dir: self.dir.path().to_owned(),
            managed: false,
        };
        let running = app::start_with_metering(&config, &self.db, &self.adapters, metering())
            .await
            .unwrap();
        self.addr = running.server.local_addr();
        self.running = Some(running);
    }

    /// Arrêt de l'agent (le dossier de données, donc l'identité, est conservé).
    pub async fn stop(&mut self) {
        if let Some(running) = self.running.take() {
            running.server.shutdown().await.unwrap();
        }
    }

    /// Redémarrage : même dossier, même identité, nouveau port.
    pub async fn restart(&mut self) {
        self.stop().await;
        self.start().await;
    }

    pub async fn create_account(&self, username: &str, role: Role) {
        self.services
            .accounts
            .create(username, AgentSecret::from(PASSWORD), role, by())
            .await
            .unwrap();
    }

    pub async fn revoke_sessions(&self, username: &str) {
        let account = self.services.accounts.find(username).await.unwrap();
        self.services
            .accounts
            .revoke_sessions(&account.id, by())
            .await
            .unwrap();
    }

    pub async fn sessions_open(&self, username: &str) -> usize {
        self.services
            .accounts
            .list()
            .await
            .unwrap()
            .into_iter()
            .find(|summary| summary.account.username.as_str() == username)
            .map(|summary| summary.sessions_open)
            .unwrap_or(0)
    }

    pub async fn interrupt_running(&self) -> u64 {
        self.services.operations.interrupt_running().await.unwrap()
    }
}

//! Outils communs des tests d'intégration : un vrai agent, un mandataire à pannes, un
//! `LinkManager` aux seuils réduits (les mêmes scénarios, 6 fois plus vite : 3 s devient 0,5 s,
//! 30 s devient 5 s, les délais de reconnexion suivent).
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

pub mod agent;
pub mod proxy;

use std::collections::BTreeSet;
use std::net::IpAddr;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use hearth_agent::domain::accounts::Role;
use hearth_link::adapters::{
    FileServerStore, FileSnapshotStore, HttpTransport, HttpTransportConfig, MemoryVault, OsRng,
    SystemClock,
};
use hearth_link::domain::event::{Event, StateInfo};
use hearth_link::domain::secret::Secret;
use hearth_link::domain::server::ServerId;
use hearth_link::domain::state::{LinkState, Thresholds};
use hearth_link::domain::time::{Mono, WallTime};
use hearth_link::ports::Clock;
use hearth_link::ports::net_watcher::{NetError, NetWatcher};
use hearth_link::{LinkConfig, LinkManager, NewServer, Ports};
use hearth_proto::fingerprint::Fingerprint;
use tempfile::TempDir;

pub use agent::{PASSWORD, TestAgent};
pub use proxy::FaultProxy;

/// Facteur de réduction des durées du produit.
pub const SCALE: u32 = 6;

pub fn fast_config() -> LinkConfig {
    LinkConfig {
        thresholds: Thresholds::scaled(SCALE),
        heartbeat_period: Duration::from_millis(333),
        attempt_timeout: Duration::from_millis(1_500),
        request_timeout: Duration::from_millis(1_500),
        execute_timeout: Duration::from_secs(8),
        net_poll_period: Duration::from_millis(60),
        wake_check_period: Duration::from_millis(50),
        snapshot_save_period: Duration::from_millis(200),
        restart_delay: Duration::from_millis(50),
        recheck_delay: Duration::from_millis(100),
        ..LinkConfig::default()
    }
}

/// Réseau scripté : la liste des adresses locales est celle qu'on y met.
pub struct ScriptedNet(Mutex<BTreeSet<IpAddr>>);

impl ScriptedNet {
    pub fn new() -> Self {
        Self(Mutex::new(["192.168.1.20".parse().unwrap()].into()))
    }

    pub fn set(&self, addresses: &[&str]) {
        *self.0.lock().unwrap() = addresses.iter().map(|a| a.parse().unwrap()).collect();
    }
}

#[async_trait]
impl NetWatcher for ScriptedNet {
    async fn addresses(&self) -> Result<BTreeSet<IpAddr>, NetError> {
        Ok(self.0.lock().unwrap().clone())
    }
}

/// Horloge dont la date murale peut sauter (veille du PC) sans que l'horloge monotone bouge.
pub struct JumpClock {
    inner: SystemClock,
    jump_ms: AtomicI64,
}

impl JumpClock {
    pub fn new() -> Self {
        Self {
            inner: SystemClock::new(),
            jump_ms: AtomicI64::new(0),
        }
    }

    pub fn jump(&self, by: Duration) {
        self.jump_ms
            .fetch_add(i64::try_from(by.as_millis()).unwrap(), Ordering::SeqCst);
    }
}

impl Clock for JumpClock {
    fn mono(&self) -> Mono {
        self.inner.mono()
    }

    fn wall(&self) -> WallTime {
        WallTime::from_millis(self.inner.wall().as_millis() + self.jump_ms.load(Ordering::SeqCst))
    }
}

/// Tout ce qui s'est passé, daté, relu à volonté.
#[derive(Clone, Default)]
pub struct Recorder {
    log: Arc<Mutex<Vec<(Instant, Event)>>>,
}

impl Recorder {
    pub fn spawn(mut stream: hearth_link::EventStream) -> Self {
        let recorder = Self::default();
        let log = recorder.log.clone();
        tokio::spawn(async move {
            while let Some(event) = stream.recv().await {
                log.lock().unwrap().push((Instant::now(), event));
            }
        });
        recorder
    }

    /// Position actuelle du journal : les attentes ne regardent que ce qui suit.
    pub fn mark(&self) -> usize {
        self.log.lock().unwrap().len()
    }

    pub fn since(&self, mark: usize) -> Vec<(Instant, Event)> {
        self.log
            .lock()
            .unwrap()
            .iter()
            .skip(mark)
            .cloned()
            .collect()
    }

    /// Les états affichés depuis `mark`, sans répétition consécutive (une même valeur annoncée
    /// deux fois, avec une nouvelle date de tentative, n'est pas un changement d'état).
    pub fn states_since(&self, mark: usize) -> Vec<LinkState> {
        let mut states: Vec<LinkState> = self
            .since(mark)
            .into_iter()
            .filter_map(|(_, event)| match event {
                Event::State { info, .. } => Some(info.state),
                _ => None,
            })
            .collect();
        states.dedup();
        states
    }

    /// Attend qu'un événement vérifie `predicate` ; le rend avec l'instant où il est arrivé.
    pub async fn wait_for(
        &self,
        mark: usize,
        what: &str,
        limit: Duration,
        predicate: impl Fn(&Event) -> bool,
    ) -> (Instant, Event) {
        let deadline = Instant::now() + limit;
        loop {
            if let Some(found) = self.since(mark).into_iter().find(|(_, e)| predicate(e)) {
                return found;
            }
            assert!(
                Instant::now() < deadline,
                "délai dépassé en attendant : {what}. États vus : {:?}",
                self.states_since(mark)
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    pub async fn wait_state(&self, mark: usize, state: LinkState, limit: Duration) -> Instant {
        self.wait_for(
            mark,
            &format!("état {state:?}"),
            limit,
            |event| matches!(event, Event::State { info, .. } if info.state == state),
        )
        .await
        .0
    }

    /// Attend que les mesures arrivent de nouveau (le flux vit).
    pub async fn wait_metrics(&self, mark: usize, limit: Duration) {
        self.wait_for(mark, "des mesures", limit, |e| {
            matches!(e, Event::Metrics { .. })
        })
        .await;
    }
}

pub struct World {
    pub agent: TestAgent,
    pub proxy: FaultProxy,
    pub manager: LinkManager,
    pub vault: Arc<MemoryVault>,
    pub id: ServerId,
    pub recorder: Recorder,
    pub net: Arc<ScriptedNet>,
    pub clock: Arc<JumpClock>,
    pub fingerprint: Fingerprint,
    pub dir: TempDir,
}

pub struct Options {
    pub remember: bool,
    pub role: Role,
    pub config: LinkConfig,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            remember: false,
            role: Role::Admin,
            config: fast_config(),
        }
    }
}

/// Un `LinkManager` complet sur ce dossier et ce coffre : carnet et dernières vues en fichiers,
/// transport réel, horloge et réseau fournis.
pub async fn start_manager(
    dir: &std::path::Path,
    vault: Arc<MemoryVault>,
    net: Arc<ScriptedNet>,
    clock: Arc<JumpClock>,
    config: LinkConfig,
) -> LinkManager {
    LinkManager::start(
        Ports {
            transport: Arc::new(transport()),
            vault,
            servers: Arc::new(FileServerStore::new(dir.join("servers.json"))),
            snapshots: Arc::new(FileSnapshotStore::new(dir.join("snapshots"))),
            clock,
            rng: Arc::new(OsRng::default()),
            net,
            extra_sink: None,
        },
        config,
    )
    .await
    .unwrap()
}

pub fn transport() -> HttpTransport {
    HttpTransport::new(HttpTransportConfig {
        connect_timeout: Duration::from_millis(1_000),
        request_timeout: Duration::from_millis(1_500),
        send_timeout: Duration::from_millis(1_000),
        client_name: "poste-test/0.1".into(),
    })
}

impl World {
    /// Agent installé, compte « marie », mandataire, `LinkManager` connecté.
    pub async fn connected(options: Options) -> Self {
        let agent = TestAgent::install().await;
        agent.create_account("marie", options.role).await;
        let proxy = FaultProxy::start(agent.addr).await;
        let dir = tempfile::tempdir().unwrap();
        let vault = Arc::new(MemoryVault::new());
        let net = Arc::new(ScriptedNet::new());
        let clock = Arc::new(JumpClock::new());
        let manager = start_manager(
            dir.path(),
            vault.clone(),
            net.clone(),
            clock.clone(),
            options.config,
        )
        .await;
        let recorder = Recorder::spawn(manager.subscribe());

        let probe = manager.probe("127.0.0.1", proxy.port()).await.unwrap();
        let id = manager
            .add_server(NewServer {
                name: "Forge".into(),
                color: "#7aa2f7".into(),
                host: "127.0.0.1".into(),
                port: proxy.port(),
                fingerprint: probe.fingerprint,
                mac_addresses: probe.hello.mac_addresses.clone(),
            })
            .await
            .unwrap();
        let mark = recorder.mark();
        manager
            .login(&id, "marie", Secret::from(PASSWORD), options.remember)
            .await
            .unwrap();
        recorder
            .wait_state(mark, LinkState::Connected, Duration::from_secs(5))
            .await;
        recorder.wait_metrics(mark, Duration::from_secs(5)).await;
        Self {
            agent,
            proxy,
            manager,
            vault,
            id,
            recorder,
            net,
            clock,
            fingerprint: probe.fingerprint,
            dir,
        }
    }

    pub fn state(&self) -> StateInfo {
        self.manager.state(&self.id).unwrap()
    }
}

pub const WAIT: Duration = Duration::from_secs(10);

/// Durée réelle d'une durée du produit à l'échelle des tests.
pub fn scaled(real: Duration) -> Duration {
    real / SCALE
}

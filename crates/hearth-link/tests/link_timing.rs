//! Le gestionnaire du lien aux SEUILS DU PRODUIT (3 s, 30 s, délais 0,5 s à 30 s), en temps virtuel
//! (`start_paused`) : aucune attente réelle, des dizaines de secondes de coupure en un instant
//! (HRT-18, critères « reconnexion automatique : intervalles progressifs respectés » et « plusieurs
//! serveurs »). La machine à états pure est prouvée ligne par ligne dans `domain/state/tests.rs` ;
//! ici on prouve que la TÂCHE du gestionnaire tient ces échéances et ces délais.
//!
//! Le transport est scripté : chaque hôte a un interrupteur « joignable », un flux qui répond aux
//! battements tant qu'il l'est et se ferme sinon, et le journal des instants des tentatives.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeSet, HashMap};
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use hearth_link::adapters::{MemoryVault, TokioClock};
use hearth_link::domain::event::Event;
use hearth_link::domain::pending_ops::{OperationId, PendingOp};
use hearth_link::domain::secret::Secret;
use hearth_link::domain::server::{LastKnown, ServerId, ServerRecord};
use hearth_link::domain::state::LinkState;
use hearth_link::domain::time::WallTime;
use hearth_link::ports::net_watcher::{NetError, NetWatcher};
use hearth_link::ports::server_store::StoreError;
use hearth_link::ports::transport::{
    ApiRequest, ApiResponse, Frame, Probed, StreamConn, Target, Transport, TransportError,
};
use hearth_link::ports::{OperationStore, Rng, ServerStore, SnapshotStore};
use hearth_link::{LinkConfig, LinkManager, NewServer, Ports};
use hearth_proto::api::accounts::{AccountInfo, RoleName};
use hearth_proto::api::hello::{ApiRange, HelloResponse};
use hearth_proto::api::machine::{Capabilities, CpuInfo, MachineResponse, OsInfo};
use hearth_proto::api::operations::OperationResponse;
use hearth_proto::api::sessions::{LoginRequest, LoginResponse};
use hearth_proto::fingerprint::Fingerprint;
use hearth_proto::stream::{ClientMessage, ServerMessage};
use tokio::time::Instant;

// ── Le monde scripté ────────────────────────────────────────────────────────────────────────

/// Un hôte : joignable ou non, et les instants où une tentative de flux l'a visé.
#[derive(Default)]
struct Host {
    up: AtomicBool,
    attempts: Mutex<Vec<Instant>>,
}

#[derive(Default)]
struct Hosts(Mutex<HashMap<String, Arc<Host>>>);

impl Hosts {
    fn host(&self, name: &str) -> Arc<Host> {
        self.0
            .lock()
            .unwrap()
            .entry(name.to_owned())
            .or_insert_with(|| {
                Arc::new(Host {
                    up: AtomicBool::new(true),
                    ..Host::default()
                })
            })
            .clone()
    }
}

struct Scripted(Arc<Hosts>);

fn machine() -> MachineResponse {
    MachineResponse {
        name: "scripted".into(),
        os: OsInfo {
            name: "OS".into(),
            version: None,
            kernel: None,
            arch: "x86_64".into(),
        },
        cpu: CpuInfo {
            model: "CPU".into(),
            physical_cores: None,
            logical_cores: 4,
            frequency_mhz: None,
        },
        memory_total_bytes: 1 << 30,
        disks: vec![],
        gpus: vec![],
        capabilities: Capabilities {
            gpu: false,
            temps: false,
        },
    }
}

#[async_trait]
impl Transport for Scripted {
    async fn hello(&self, target: &Target) -> Result<Probed, TransportError> {
        if !self.0.host(&target.host).up.load(Ordering::SeqCst) {
            return Err(TransportError::Connect("injoignable".into()));
        }
        Ok(Probed {
            fingerprint: Fingerprint::from_bytes([1; 32]),
            hello: HelloResponse {
                product: "hearth".into(),
                agent_version: "0.0.0".into(),
                api: ApiRange { min: 1, max: 1 },
                machine_name: "scripted".into(),
                install_id: "00".into(),
                managed: false,
                mac_addresses: vec![],
            },
        })
    }

    async fn login(
        &self,
        target: &Target,
        _request: &LoginRequest,
    ) -> Result<LoginResponse, TransportError> {
        if !self.0.host(&target.host).up.load(Ordering::SeqCst) {
            return Err(TransportError::Connect("injoignable".into()));
        }
        Ok(LoginResponse {
            token: "ab".repeat(32),
            expires_at: "2026-11-03T10:30:15.25Z".into(),
            account: AccountInfo {
                id: "A".into(),
                username: "marie".into(),
                role: RoleName::Admin,
            },
        })
    }

    async fn logout(&self, _target: &Target, _token: &Secret) -> Result<(), TransportError> {
        Ok(())
    }

    async fn request(
        &self,
        _target: &Target,
        _token: &Secret,
        _request: &ApiRequest,
    ) -> Result<ApiResponse, TransportError> {
        Err(TransportError::Timeout)
    }

    async fn operation(
        &self,
        _target: &Target,
        _token: &Secret,
        _id: &OperationId,
    ) -> Result<OperationResponse, TransportError> {
        Err(TransportError::Timeout)
    }

    async fn open_stream(&self, target: &Target) -> Result<Box<dyn StreamConn>, TransportError> {
        let host = self.0.host(&target.host);
        host.attempts.lock().unwrap().push(Instant::now());
        if !host.up.load(Ordering::SeqCst) {
            return Err(TransportError::Connect("injoignable".into()));
        }
        Ok(Box::new(Stream {
            host,
            snapshot_sent: false,
            replies: Vec::new(),
        }))
    }
}

/// Un flux qui envoie l'instantané, répond aux battements, et se ferme dès que l'hôte tombe.
struct Stream {
    host: Arc<Host>,
    snapshot_sent: bool,
    replies: Vec<u64>,
}

#[async_trait]
impl StreamConn for Stream {
    async fn send(&mut self, message: &ClientMessage) -> Result<(), TransportError> {
        if let ClientMessage::Ping { n } = message {
            self.replies.push(*n);
        }
        Ok(())
    }

    async fn recv(&mut self) -> Result<Frame, TransportError> {
        loop {
            if !self.host.up.load(Ordering::SeqCst) {
                return Err(TransportError::Closed(None));
            }
            if !self.snapshot_sent {
                self.snapshot_sent = true;
                return Ok(Frame::Message(Box::new(ServerMessage::Snapshot {
                    machine: machine(),
                    history: vec![],
                })));
            }
            if let Some(n) = self.replies.pop() {
                return Ok(Frame::Message(Box::new(ServerMessage::Pong { n })));
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

/// Aucun aléa : `200` donne un facteur de 1000 pour mille (voir `domain::backoff`).
struct NoJitter;

impl Rng for NoJitter {
    fn next_u32(&self) -> u32 {
        200
    }
}

#[derive(Default)]
struct Net;

#[async_trait]
impl NetWatcher for Net {
    async fn addresses(&self) -> Result<BTreeSet<IpAddr>, NetError> {
        Ok(BTreeSet::new())
    }
}

#[derive(Default)]
struct MemStore {
    servers: Mutex<HashMap<ServerId, ServerRecord>>,
    views: Mutex<HashMap<ServerId, LastKnown>>,
    operations: Mutex<HashMap<ServerId, Vec<PendingOp>>>,
}

#[async_trait]
impl OperationStore for MemStore {
    async fn load(
        &self,
        id: &ServerId,
    ) -> Result<hearth_link::ports::operation_store::LoadedOperations, StoreError> {
        Ok(hearth_link::ports::operation_store::LoadedOperations {
            operations: self
                .operations
                .lock()
                .unwrap()
                .get(id)
                .cloned()
                .unwrap_or_default(),
            damaged: false,
        })
    }
    async fn save(&self, id: &ServerId, operations: &[PendingOp]) -> Result<(), StoreError> {
        self.operations
            .lock()
            .unwrap()
            .insert(id.clone(), operations.to_vec());
        Ok(())
    }
    async fn remove(&self, id: &ServerId) -> Result<(), StoreError> {
        self.operations.lock().unwrap().remove(id);
        Ok(())
    }
}

#[async_trait]
impl ServerStore for MemStore {
    async fn list(&self) -> Result<Vec<ServerRecord>, StoreError> {
        Ok(self.servers.lock().unwrap().values().cloned().collect())
    }
    async fn save(&self, record: &ServerRecord) -> Result<(), StoreError> {
        self.servers
            .lock()
            .unwrap()
            .insert(record.id.clone(), record.clone());
        Ok(())
    }
    async fn remove(&self, id: &ServerId) -> Result<(), StoreError> {
        self.servers.lock().unwrap().remove(id);
        Ok(())
    }
}

#[async_trait]
impl SnapshotStore for MemStore {
    async fn load(&self, id: &ServerId) -> Result<Option<LastKnown>, StoreError> {
        Ok(self.views.lock().unwrap().get(id).cloned())
    }
    async fn save(&self, id: &ServerId, view: &LastKnown) -> Result<(), StoreError> {
        self.views.lock().unwrap().insert(id.clone(), view.clone());
        Ok(())
    }
    async fn remove(&self, id: &ServerId) -> Result<(), StoreError> {
        self.views.lock().unwrap().remove(id);
        Ok(())
    }
}

// ── Le banc ─────────────────────────────────────────────────────────────────────────────────

type Timeline = Arc<Mutex<Vec<(ServerId, LinkState, Instant)>>>;

struct Bench {
    manager: LinkManager,
    hosts: Arc<Hosts>,
    timeline: Timeline,
}

impl Bench {
    async fn start() -> Self {
        let hosts = Arc::new(Hosts::default());
        let store = Arc::new(MemStore::default());
        let manager = LinkManager::start(
            Ports {
                transport: Arc::new(Scripted(hosts.clone())),
                vault: Arc::new(MemoryVault::new()),
                servers: store.clone(),
                snapshots: store.clone(),
                operations: store,
                clock: Arc::new(TokioClock::new(WallTime::from_millis(1_790_000_000_000))),
                rng: Arc::new(NoJitter),
                net: Arc::new(Net),
                extra_sink: None,
            },
            LinkConfig::default(),
        )
        .await
        .unwrap();
        let timeline: Timeline = Arc::default();
        let recorded = timeline.clone();
        let mut events = manager.subscribe();
        tokio::spawn(async move {
            while let Some(event) = events.recv().await {
                // Un événement d'état est aussi émis quand seul un compteur change : on ne garde que
                // les CHANGEMENTS d'état affiché.
                if let Event::State { server, info } = event {
                    let mut seen = recorded.lock().unwrap();
                    let last = seen.iter().rev().find(|(id, _, _)| *id == server);
                    if last.is_none_or(|(_, state, _)| *state != info.state) {
                        seen.push((server, info.state, Instant::now()));
                    }
                }
            }
        });
        Self {
            manager,
            hosts,
            timeline,
        }
    }

    /// Un serveur ajouté puis connecté (mot de passe mémorisé : la reconnexion est silencieuse).
    async fn connected_server(&self, host: &str) -> ServerId {
        let id = self
            .manager
            .add_server(NewServer {
                name: host.into(),
                color: "#fff".into(),
                host: host.into(),
                port: 7341,
                fingerprint: Fingerprint::from_bytes([1; 32]),
                mac_addresses: vec![],
            })
            .await
            .unwrap();
        self.manager
            .login(&id, "marie", Secret::from("Correct-Horse-9"), true)
            .await
            .unwrap();
        self.wait_state(&id, LinkState::Connected).await;
        id
    }

    async fn wait_state(&self, id: &ServerId, wanted: LinkState) {
        for _ in 0..3_000 {
            if self.manager.state(id).unwrap().state == wanted {
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!("jamais {wanted:?} en 300 s virtuelles");
    }

    /// Les changements d'état d'un serveur depuis `since`, avec leur instant.
    fn states_since(&self, id: &ServerId, since: Instant) -> Vec<(LinkState, Instant)> {
        self.timeline
            .lock()
            .unwrap()
            .iter()
            .filter(|(server, _, at)| server == id && *at >= since)
            .map(|(_, state, at)| (*state, *at))
            .collect()
    }
}

fn near(actual: Duration, expected_ms: u64, tolerance_ms: u64) -> bool {
    let actual = actual.as_millis() as u64;
    actual.abs_diff(expected_ms) <= tolerance_ms
}

// ── Les scénarios ───────────────────────────────────────────────────────────────────────────

#[tokio::test(start_paused = true)]
async fn the_link_shows_reconnecting_at_3_seconds_and_offline_at_30_then_comes_back() {
    let bench = Bench::start().await;
    let id = bench.connected_server("a.test").await;
    let host = bench.hosts.host("a.test");

    let cut = Instant::now();
    host.up.store(false, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_secs(40)).await;
    let states = bench.states_since(&id, cut);
    let kinds: Vec<LinkState> = states.iter().map(|(state, _)| *state).collect();
    assert_eq!(kinds, [LinkState::Reconnecting, LinkState::Offline]);
    assert!(
        near(states[0].1 - cut, 3_000, 300),
        "Reconnexion à {:?}",
        states[0].1 - cut
    );
    assert!(
        near(states[1].1 - cut, 30_000, 300),
        "Hors ligne à {:?}",
        states[1].1 - cut
    );

    // Le serveur revient : « Connecté » à la première tentative qui suit (30 s au plus).
    let back = Instant::now();
    host.up.store(true, Ordering::SeqCst);
    bench.wait_state(&id, LinkState::Connected).await;
    assert!(Instant::now() - back <= Duration::from_secs(31));
}

#[tokio::test(start_paused = true)]
async fn the_attempts_follow_0_5_1_2_4_8_15_30_30_seconds_and_never_stop() {
    let bench = Bench::start().await;
    let id = bench.connected_server("a.test").await;
    let host = bench.hosts.host("a.test");
    host.attempts.lock().unwrap().clear();
    let id_again = id.clone();

    host.up.store(false, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_secs(200)).await;
    let mut attempts = host.attempts.lock().unwrap().clone();
    // La vérification faite une fois au seuil de 3 s ne fait pas partie de la suite des délais.
    let first = attempts[0];
    attempts.retain(|at| !near(*at - first, 3_000, 100));
    let gaps: Vec<Duration> = attempts.windows(2).map(|w| w[1] - w[0]).collect();
    let expected = [500, 1_000, 2_000, 4_000, 8_000, 15_000, 30_000, 30_000];
    assert!(gaps.len() >= expected.len(), "{gaps:?}");
    for (gap, want) in gaps.iter().zip(expected) {
        assert!(near(*gap, want, 150), "{gaps:?}");
    }
    // Ensuite, 30 s entre deux tentatives, sans fin.
    for gap in &gaps[expected.len()..] {
        assert!(near(*gap, 30_000, 150), "{gaps:?}");
    }
    assert_eq!(
        bench.manager.state(&id_again).unwrap().state,
        LinkState::Offline
    );
}

/// FIX:01M4CJEQS88NZWCQ129XDP91MT : l'écran ne montre « Reconnexion » qu'après 3 s de coupure continue,
/// quelles que soient les tentatives en dessous (planifiées à 0,5 / 1,5 / 3,5 s).
#[tokio::test(start_paused = true)]
async fn a_cut_healed_at_2_9_seconds_shows_nothing() {
    let bench = Bench::start().await;
    let id = bench.connected_server("a.test").await;
    let host = bench.hosts.host("a.test");

    let cut = Instant::now();
    host.up.store(false, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(2_900)).await;
    host.up.store(true, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_secs(10)).await;
    let shown: Vec<_> = bench
        .states_since(&id, cut)
        .into_iter()
        .map(|(state, at)| (state, at - cut))
        .collect();
    assert!(shown.is_empty(), "{shown:?}");
    assert_eq!(
        bench.manager.state(&id).unwrap().state,
        LinkState::Connected
    );
}

#[tokio::test(start_paused = true)]
async fn a_cut_healed_at_3_1_seconds_shows_reconnecting_then_connected() {
    let bench = Bench::start().await;
    let id = bench.connected_server("a.test").await;
    let host = bench.hosts.host("a.test");

    let cut = Instant::now();
    host.up.store(false, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(3_100)).await;
    host.up.store(true, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_secs(10)).await;
    let shown: Vec<LinkState> = bench
        .states_since(&id, cut)
        .into_iter()
        .map(|(state, _)| state)
        .collect();
    assert_eq!(shown, [LinkState::Reconnecting, LinkState::Connected]);
}

#[tokio::test(start_paused = true)]
async fn each_of_several_servers_has_its_own_clock_and_state() {
    let bench = Bench::start().await;
    let a = bench.connected_server("a.test").await;
    let b = bench.connected_server("b.test").await;
    let c = bench.connected_server("c.test").await;

    // Seul « a » tombe : « b » et « c » ne changent jamais.
    let cut = Instant::now();
    bench.hosts.host("a.test").up.store(false, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_secs(40)).await;
    assert_eq!(bench.manager.state(&a).unwrap().state, LinkState::Offline);
    assert!(bench.states_since(&b, cut).is_empty());
    assert!(bench.states_since(&c, cut).is_empty());

    // « b » tombe 10 s plus tard et revient avant 30 s : « Reconnexion », jamais « Hors ligne », pendant
    // que « a » reste « Hors ligne ».
    let b_cut = Instant::now();
    bench.hosts.host("b.test").up.store(false, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_secs(10)).await;
    bench.hosts.host("b.test").up.store(true, Ordering::SeqCst);
    bench.wait_state(&b, LinkState::Connected).await;
    let b_states: Vec<LinkState> = bench
        .states_since(&b, b_cut)
        .into_iter()
        .map(|(state, _)| state)
        .collect();
    assert_eq!(b_states, [LinkState::Reconnecting, LinkState::Connected]);
    assert_eq!(bench.manager.state(&a).unwrap().state, LinkState::Offline);
    assert!(bench.states_since(&c, cut).is_empty());
}

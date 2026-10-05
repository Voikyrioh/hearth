//! Suivi des actions sur disque (persister PUIS envoyer), déconnexion contre une fin de session,
//! reprise après panique, équité de la boucle : transport simulé et stockage simulé, temps réel.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use hearth_link::adapters::{MemoryVault, OsRng, SystemClock};
use hearth_link::domain::event::Event;
use hearth_link::domain::pending_ops::{OperationId, Outcome, PendingOp};
use hearth_link::domain::secret::Secret;
use hearth_link::domain::server::{LastKnown, ServerId, ServerRecord};
use hearth_link::domain::state::{LinkState, Reason};
use hearth_link::ports::net_watcher::{NetError, NetWatcher};
use hearth_link::ports::operation_store::LoadedOperations;
use hearth_link::ports::server_store::StoreError;
use hearth_link::ports::transport::{
    ApiRequest, ApiResponse, Frame, Method, Probed, StreamConn, Target, Transport, TransportError,
};
use hearth_link::ports::{EventSink, OperationStore, ServerStore, SnapshotStore, Vault as _};
use hearth_link::{
    ActionOutcome, ActionRequest, LinkConfig, LinkError, LinkManager, NewServer, Ports,
};
use hearth_proto::api::accounts::{AccountInfo, RoleName};
use hearth_proto::api::hello::{ApiRange, HelloResponse};
use hearth_proto::api::machine::{Capabilities, CpuInfo, MachineResponse, OsInfo};
use hearth_proto::api::operations::{OperationResponse, OperationStatus};
use hearth_proto::api::sessions::{LoginRequest, LoginResponse};
use hearth_proto::error::{ErrorCode, ErrorDetail};
use hearth_proto::fingerprint::Fingerprint;
use hearth_proto::stream::{ClientMessage, ServerMessage, SessionNotice};
use serde_json::json;

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

// ── Transport simulé ────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum StreamMode {
    /// Un signe de vie toutes les 30 ms.
    Quiet,
    /// Des trames sans fin, sans jamais attendre.
    Flood,
}

struct Script {
    requests: AtomicUsize,
    logins: AtomicUsize,
    opens: AtomicUsize,
    pings: AtomicUsize,
    mode: Mutex<StreamMode>,
    /// Si posé : la prochaine lecture rend « session expirée » (une seule fois).
    expire_next: AtomicBool,
    /// Ce que la requête voit du disque au moment où elle part.
    on_request: Mutex<Option<Box<dyn Fn() + Send>>>,
    request_hangs: AtomicBool,
    lookup: Mutex<OperationStatus>,
    flood: tokio::sync::Semaphore,
    wake: tokio::sync::Notify,
    snapshot_next: AtomicBool,
}

impl Script {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            requests: AtomicUsize::new(0),
            logins: AtomicUsize::new(0),
            opens: AtomicUsize::new(0),
            pings: AtomicUsize::new(0),
            mode: Mutex::new(StreamMode::Quiet),
            expire_next: AtomicBool::new(false),
            on_request: Mutex::new(None),
            request_hangs: AtomicBool::new(false),
            lookup: Mutex::new(OperationStatus::Succeeded),
            flood: tokio::sync::Semaphore::new(1 << 40),
            wake: tokio::sync::Notify::new(),
            snapshot_next: AtomicBool::new(false),
        })
    }
}

struct Mock(Arc<Script>);

fn machine() -> MachineResponse {
    MachineResponse {
        name: "mock".into(),
        os: OsInfo {
            name: "OS".into(),
            version: None,
            kernel: None,
            arch: "x86_64".into(),
        },
        cpu: CpuInfo {
            model: "CPU".into(),
            physical_cores: None,
            logical_cores: 1,
            frequency_mhz: None,
        },
        memory_total_bytes: 1,
        disks: vec![],
        gpus: vec![],
        capabilities: Capabilities {
            gpu: false,
            temps: false,
        },
    }
}

#[async_trait]
impl Transport for Mock {
    async fn hello(&self, _: &Target) -> Result<Probed, TransportError> {
        Ok(Probed {
            fingerprint: Fingerprint::from_bytes([7; 32]),
            hello: HelloResponse {
                product: "hearth".into(),
                agent_version: "0".into(),
                api: ApiRange { min: 1, max: 1 },
                machine_name: "mock".into(),
                install_id: "00".into(),
                managed: false,
                mac_addresses: vec![],
            },
        })
    }

    async fn login(&self, _: &Target, _: &LoginRequest) -> Result<LoginResponse, TransportError> {
        let n = self.0.logins.fetch_add(1, Ordering::SeqCst);
        Ok(LoginResponse {
            token: format!("{:064x}", n + 1),
            expires_at: "x".into(),
            account: AccountInfo {
                id: "A".into(),
                username: "marie".into(),
                role: RoleName::Admin,
            },
        })
    }

    async fn logout(&self, _: &Target, _: &Secret) -> Result<(), TransportError> {
        Ok(())
    }

    async fn request(
        &self,
        _: &Target,
        _: &Secret,
        _: &ApiRequest,
    ) -> Result<ApiResponse, TransportError> {
        self.0.requests.fetch_add(1, Ordering::SeqCst);
        if let Some(hook) = self.0.on_request.lock().unwrap().as_ref() {
            hook();
        }
        if self.0.request_hangs.load(Ordering::SeqCst) {
            std::future::pending::<()>().await;
        }
        Ok(ApiResponse {
            status: 200,
            body: json!({}),
            replayed: false,
        })
    }

    async fn operation(
        &self,
        _: &Target,
        _: &Secret,
        id: &OperationId,
    ) -> Result<OperationResponse, TransportError> {
        Ok(OperationResponse {
            id: id.as_str().into(),
            kind: "PUT /x".into(),
            status: *self.0.lookup.lock().unwrap(),
            result: None,
        })
    }

    async fn open_stream(&self, _: &Target) -> Result<Box<dyn StreamConn>, TransportError> {
        self.0.opens.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(Stream {
            script: self.0.clone(),
            first: true,
        }))
    }
}

struct Stream {
    script: Arc<Script>,
    first: bool,
}

#[async_trait]
impl StreamConn for Stream {
    async fn send(&mut self, message: &ClientMessage) -> Result<(), TransportError> {
        if matches!(message, ClientMessage::Ping { .. }) {
            self.script.pings.fetch_add(1, Ordering::SeqCst);
        }
        Ok(())
    }

    async fn recv(&mut self) -> Result<Frame, TransportError> {
        if std::mem::take(&mut self.first) {
            return Ok(Frame::Message(Box::new(ServerMessage::Snapshot {
                machine: machine(),
                history: vec![],
            })));
        }
        if !self.script.expire_next.load(Ordering::SeqCst)
            && !self.script.snapshot_next.load(Ordering::SeqCst)
        {
            tokio::select! {
                () = tokio::time::sleep(ms(30)) => {}
                () = self.script.wake.notified() => {}
            }
        }
        if self.script.snapshot_next.swap(false, Ordering::SeqCst) {
            return Ok(Frame::Message(Box::new(ServerMessage::Snapshot {
                machine: machine(),
                history: vec![],
            })));
        }
        if self.script.expire_next.swap(false, Ordering::SeqCst) {
            return Ok(Frame::Message(Box::new(ServerMessage::Session {
                kind: SessionNotice::Expired,
            })));
        }
        let flood = *self.script.mode.lock().unwrap() == StreamMode::Flood;
        if flood {
            // Prête tout de suite, à chaque appel (le sémaphore consomme le budget coopératif de
            // Tokio : la tâche cède de temps en temps, comme sur une vraie socket pleine).
            self.script.flood.acquire().await.unwrap().forget();
        } else {
            tokio::time::sleep(ms(30)).await;
        }
        Ok(Frame::Other)
    }
}

// ── Stockage simulé ─────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
enum Disk {
    Normal,
    /// Chaque écriture des opérations échoue (disque plein).
    Failing,
    /// Chaque écriture des opérations ne finit jamais.
    Hanging,
    /// Chaque écriture des opérations dure ce temps.
    Slow(Duration),
}

struct Store {
    disk: Mutex<Disk>,
    servers: Mutex<Vec<ServerRecord>>,
    operations: Mutex<Vec<PendingOp>>,
    /// Instant de la fin de la dernière écriture des opérations.
    written_at: Mutex<Option<Instant>>,
}

impl Store {
    fn new(disk: Disk) -> Arc<Self> {
        Arc::new(Self {
            disk: Mutex::new(disk),
            servers: Mutex::new(Vec::new()),
            operations: Mutex::new(Vec::new()),
            written_at: Mutex::new(None),
        })
    }

    fn tracked(&self) -> usize {
        self.operations.lock().unwrap().len()
    }
}

#[async_trait]
impl ServerStore for Store {
    async fn list(&self) -> Result<Vec<ServerRecord>, StoreError> {
        Ok(self.servers.lock().unwrap().clone())
    }
    async fn save(&self, record: &ServerRecord) -> Result<(), StoreError> {
        let mut servers = self.servers.lock().unwrap();
        servers.retain(|s| s.id != record.id);
        servers.push(record.clone());
        Ok(())
    }
    async fn remove(&self, id: &ServerId) -> Result<(), StoreError> {
        self.servers.lock().unwrap().retain(|s| &s.id != id);
        Ok(())
    }
}

#[async_trait]
impl SnapshotStore for Store {
    async fn load(&self, _: &ServerId) -> Result<Option<LastKnown>, StoreError> {
        Ok(None)
    }
    async fn save(&self, _: &ServerId, _: &LastKnown) -> Result<(), StoreError> {
        Ok(())
    }
    async fn remove(&self, _: &ServerId) -> Result<(), StoreError> {
        Ok(())
    }
}

#[async_trait]
impl OperationStore for Store {
    async fn load(&self, _: &ServerId) -> Result<LoadedOperations, StoreError> {
        Ok(LoadedOperations {
            operations: self.operations.lock().unwrap().clone(),
            damaged: false,
        })
    }
    async fn save(&self, _: &ServerId, operations: &[PendingOp]) -> Result<(), StoreError> {
        let disk = *self.disk.lock().unwrap();
        match disk {
            Disk::Failing => return Err(StoreError("disque plein".into())),
            Disk::Hanging => std::future::pending::<()>().await,
            Disk::Slow(delay) => tokio::time::sleep(delay).await,
            Disk::Normal => {}
        }
        *self.operations.lock().unwrap() = operations.to_vec();
        *self.written_at.lock().unwrap() = Some(Instant::now());
        Ok(())
    }
    async fn remove(&self, _: &ServerId) -> Result<(), StoreError> {
        self.operations.lock().unwrap().clear();
        Ok(())
    }
}

struct NoNet;

#[async_trait]
impl NetWatcher for NoNet {
    async fn addresses(&self) -> Result<BTreeSet<IpAddr>, NetError> {
        Ok(BTreeSet::new())
    }
}

/// Destination d'événements qui panique une fois, quand on l'arme (panique d'une tâche).
#[derive(Default)]
struct Bomb {
    armed: AtomicBool,
}

impl EventSink for Bomb {
    fn emit(&self, _: Event) {
        if self.armed.swap(false, Ordering::SeqCst) {
            panic!("panique volontaire du test");
        }
    }
}

/// Destination d'événements qui retient la tâche du serveur (un fil bloqué) au prochain
/// événement : de quoi mettre une trame et une commande en attente ensemble.
#[derive(Default)]
struct Gate {
    block_next: AtomicBool,
}

impl EventSink for Gate {
    fn emit(&self, _: Event) {
        if self.block_next.swap(false, Ordering::SeqCst) {
            std::thread::sleep(ms(500));
        }
    }
}

fn config() -> LinkConfig {
    LinkConfig {
        heartbeat_period: ms(100),
        persist_timeout: ms(600),
        restart_delay: ms(20),
        net_poll_period: Duration::from_secs(3_600),
        wake_check_period: Duration::from_secs(3_600),
        ..LinkConfig::default()
    }
}

struct Rig {
    manager: LinkManager,
    script: Arc<Script>,
    store: Arc<Store>,
    vault: Arc<MemoryVault>,
}

async fn start(
    script: &Arc<Script>,
    store: &Arc<Store>,
    vault: &Arc<MemoryVault>,
    bomb: Option<Arc<dyn EventSink>>,
) -> LinkManager {
    LinkManager::start(
        Ports {
            transport: Arc::new(Mock(script.clone())),
            vault: vault.clone(),
            servers: store.clone(),
            snapshots: store.clone(),
            operations: store.clone(),
            clock: Arc::new(SystemClock::new()),
            rng: Arc::new(OsRng::default()),
            net: Arc::new(NoNet),
            extra_sink: bomb,
        },
        config(),
    )
    .await
    .unwrap()
}

async fn connected(disk: Disk, remember: bool) -> (Rig, ServerId) {
    connected_with(disk, remember, None).await
}

async fn connected_with(
    disk: Disk,
    remember: bool,
    sink: Option<Arc<dyn EventSink>>,
) -> (Rig, ServerId) {
    let script = Script::new();
    let store = Store::new(disk);
    let vault = Arc::new(MemoryVault::new());
    let manager = start(&script, &store, &vault, sink).await;
    let id = manager
        .add_server(NewServer {
            name: "Mock".into(),
            color: "#fff".into(),
            host: "mock.test".into(),
            port: 7341,
            fingerprint: Fingerprint::from_bytes([7; 32]),
            mac_addresses: vec![],
        })
        .await
        .unwrap();
    manager
        .login(&id, "marie", Secret::from("Correct-Horse-9"), remember)
        .await
        .unwrap();
    wait_state(&manager, &id, LinkState::Connected).await;
    (
        Rig {
            manager,
            script,
            store,
            vault,
        },
        id,
    )
}

async fn wait_state(manager: &LinkManager, id: &ServerId, state: LinkState) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while manager.state(id).unwrap().state != state {
        assert!(
            Instant::now() < deadline,
            "état {state:?} attendu, vu {:?}",
            manager.state(id)
        );
        tokio::time::sleep(ms(10)).await;
    }
}

fn change_password() -> ActionRequest {
    ActionRequest {
        method: Method::Put,
        path: "/me/password".into(),
        body: Some(json!({ "current": "a", "password": "b" })),
    }
}

// ── Persister PUIS envoyer ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_failed_write_means_the_action_is_not_sent_and_the_caller_is_told() {
    let (rig, id) = connected(Disk::Failing, false).await;
    let result = rig.manager.execute(&id, change_password()).await;
    assert_eq!(result.unwrap_err(), LinkError::TrackingUnavailable);
    assert_eq!(
        rig.script.requests.load(Ordering::SeqCst),
        0,
        "aucune requête n'est partie"
    );
    assert_eq!(rig.store.tracked(), 0);
}

#[tokio::test]
async fn a_write_that_never_ends_does_not_send_the_action_either() {
    let (rig, id) = connected(Disk::Hanging, false).await;
    let started = Instant::now();
    let result = rig.manager.execute(&id, change_password()).await;
    assert_eq!(result.unwrap_err(), LinkError::TrackingUnavailable);
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "borné par le délai court"
    );
    assert_eq!(rig.script.requests.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_slow_write_delays_the_request_until_it_is_done() {
    let (rig, id) = connected(Disk::Slow(ms(300)), false).await;
    let sent_at: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
    let slot = sent_at.clone();
    *rig.script.on_request.lock().unwrap() = Some(Box::new(move || {
        *slot.lock().unwrap() = Some(Instant::now());
    }));
    let started = Instant::now();
    let outcome = rig.manager.execute(&id, change_password()).await.unwrap();
    assert!(matches!(
        outcome,
        ActionOutcome::Completed { status: 200, .. }
    ));
    let sent = sent_at.lock().unwrap().expect("la requête est partie");
    let written = rig
        .store
        .written_at
        .lock()
        .unwrap()
        .expect("le suivi est écrit");
    assert!(
        sent >= written,
        "la requête n'est partie qu'après l'écriture du suivi"
    );
    assert!(
        sent - started >= ms(250),
        "elle a bien attendu l'écriture lente"
    );
}

#[tokio::test]
async fn a_crash_right_after_the_send_leaves_the_operation_on_disk_for_the_restart() {
    let (rig, id) = connected(Disk::Normal, false).await;
    // La requête ne répond jamais ; au moment où elle part, le suivi est déjà sur disque.
    rig.script.request_hangs.store(true, Ordering::SeqCst);
    let store = rig.store.clone();
    let seen_on_disk = Arc::new(AtomicUsize::new(usize::MAX));
    let seen = seen_on_disk.clone();
    *rig.script.on_request.lock().unwrap() = Some(Box::new(move || {
        seen.store(store.tracked(), Ordering::SeqCst);
    }));
    let manager = rig.manager.clone();
    let task_id = id.clone();
    let call = tokio::spawn(async move { manager.execute(&task_id, change_password()).await });
    let deadline = Instant::now() + Duration::from_secs(3);
    while rig.script.requests.load(Ordering::SeqCst) == 0 {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(ms(10)).await;
    }
    assert_eq!(
        seen_on_disk.load(Ordering::SeqCst),
        1,
        "suivi écrit avant l'envoi"
    );
    // Plantage : tout s'arrête sans rien nettoyer.
    call.abort();
    rig.manager.shutdown().await;

    // Redémarrage : l'opération est retrouvée et relue au premier retour du lien.
    let script = Script::new();
    let manager = start(&script, &rig.store, &rig.vault, None).await;
    let mut events = manager.subscribe();
    let (_, outcome) = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(Event::Operation { outcome, .. }) = events.recv().await {
                return ((), outcome);
            }
        }
    })
    .await
    .expect("l'opération suivie est annoncée après le redémarrage");
    assert!(
        matches!(outcome, Outcome::DoneDuringOutage { .. }),
        "{outcome:?}"
    );
}

// ── Déconnexion contre fin de session ───────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_session_end_frame_racing_a_logout_never_triggers_a_silent_reconnection() {
    let gate = Arc::new(Gate::default());
    let (rig, id) = connected_with(Disk::Normal, true, Some(gate.clone())).await;
    let logins = rig.script.logins.load(Ordering::SeqCst);
    // La tâche du serveur est retenue sur un événement (un instantané) ; pendant ce temps la
    // trame de fin de session ET la commande de déconnexion arrivent. Au réveil, la trame est
    // lue la première (`biased`) : sans la lecture de `signed_out`, elle relancerait une session.
    gate.block_next.store(true, Ordering::SeqCst);
    rig.script.snapshot_next.store(true, Ordering::SeqCst);
    rig.script.wake.notify_waiters();
    tokio::time::sleep(ms(150)).await;
    rig.script.expire_next.store(true, Ordering::SeqCst);
    rig.manager.logout(&id).await.unwrap();
    tokio::time::sleep(ms(900)).await;
    assert_eq!(
        rig.script.logins.load(Ordering::SeqCst),
        logins,
        "aucune reconnexion silencieuse"
    );
    let info = rig.manager.state(&id).unwrap();
    assert_eq!(info.state, LinkState::SessionExpired);
    assert_eq!(info.reason, Some(Reason::UserDisconnected));
    assert!(
        rig.vault
            .get(&id, hearth_link::ports::vault::SecretKind::Token)
            .unwrap()
            .is_none()
    );
}

// ── Reprise après panique ───────────────────────────────────────────────────────────────────

fn record(signed_out: bool) -> ServerRecord {
    ServerRecord {
        id: ServerId::parse("srv").unwrap(),
        name: "Mock".into(),
        color: "#fff".into(),
        host: "mock.test".into(),
        port: 7341,
        fingerprint: Fingerprint::from_bytes([7; 32]),
        username: "marie".into(),
        remember: true,
        mac_addresses: vec![],
        last_contact_at: None,
        signed_out,
    }
}

#[tokio::test]
async fn after_a_panic_a_disconnected_server_stays_disconnected() {
    let script = Script::new();
    let store = Store::new(Disk::Normal);
    store.servers.lock().unwrap().push(record(true));
    let vault = Arc::new(MemoryVault::new());
    vault
        .put(
            &ServerId::parse("srv").unwrap(),
            hearth_link::ports::vault::SecretKind::Password,
            &Secret::from("p"),
        )
        .unwrap();
    let bomb = Arc::new(Bomb::default());
    bomb.armed.store(true, Ordering::SeqCst);
    let manager = start(&script, &store, &vault, Some(bomb as Arc<dyn EventSink>)).await;
    let id = ServerId::parse("srv").unwrap();
    tokio::time::sleep(ms(600)).await;
    assert_eq!(
        manager.task_restarts(&id).unwrap(),
        1,
        "la tâche a bien paniqué une fois"
    );
    let info = manager.state(&id).unwrap();
    assert_eq!(info.state, LinkState::SessionExpired);
    assert_eq!(info.reason, Some(Reason::UserDisconnected));
    assert_eq!(
        script.logins.load(Ordering::SeqCst),
        0,
        "aucune reconnexion silencieuse"
    );
    assert_eq!(script.opens.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn after_a_panic_a_connected_server_restarts_offline_and_retries() {
    let script = Script::new();
    let store = Store::new(Disk::Normal);
    store.servers.lock().unwrap().push(record(false));
    let vault = Arc::new(MemoryVault::new());
    vault
        .put(
            &ServerId::parse("srv").unwrap(),
            hearth_link::ports::vault::SecretKind::Token,
            &Secret::from("t"),
        )
        .unwrap();
    let bomb = Arc::new(Bomb::default());
    bomb.armed.store(true, Ordering::SeqCst);
    let manager = start(&script, &store, &vault, Some(bomb as Arc<dyn EventSink>)).await;
    let id = ServerId::parse("srv").unwrap();
    wait_state(&manager, &id, LinkState::Connected).await;
    assert_eq!(manager.task_restarts(&id).unwrap(), 1);
}

// ── Équité de la boucle ─────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_flooding_the_stream_starves_neither_commands_nor_the_heartbeat() {
    let (rig, id) = connected(Disk::Normal, false).await;
    *rig.script.mode.lock().unwrap() = StreamMode::Flood;
    tokio::time::sleep(ms(300)).await;
    let pings = rig.script.pings.load(Ordering::SeqCst);
    // Une commande passe malgré le flot ininterrompu de trames.
    let outcome = tokio::time::timeout(
        Duration::from_secs(5),
        rig.manager.execute(&id, change_password()),
    )
    .await
    .expect("la commande n'est pas affamée par le flot de trames")
    .unwrap();
    assert!(matches!(outcome, ActionOutcome::Completed { .. }));
    tokio::time::sleep(ms(600)).await;
    assert!(
        rig.script.pings.load(Ordering::SeqCst) >= pings + 3,
        "le battement continue pendant le flot"
    );
    assert_eq!(rig.manager.state(&id).unwrap().state, LinkState::Connected);
    let _ = ErrorDetail {
        code: ErrorCode::Busy,
        message: String::new(),
        details: json!({}),
    };
}

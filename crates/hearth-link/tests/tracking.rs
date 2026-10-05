//! Suivi des actions sur disque (persister PUIS envoyer), déconnexion contre une fin de session,
//! reprise après panique, équité de la boucle : transport simulé et stockage simulé.
//!
//! Déterminisme (HRT-12) : aucune assertion ne dépend de la vitesse de la machine. Les seuils de
//! silence du lien et le délai d'écriture du suivi sont très larges (une machine saturée ne peut
//! pas les franchir par hasard) ; chaque scénario attend un FAIT observable (porte atteinte,
//! compteur, état) et non une durée ; les délais de garde (`GUARD`) ne servent qu'à ne pas
//! laisser un test bloqué à jamais.

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
use hearth_link::ports::vault::{SecretKind, VaultError};
use hearth_link::ports::{EventSink, OperationStore, ServerStore, SnapshotStore, Vault};
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

/// Délai de garde : un test bloqué échoue au bout de ce temps, jamais une assertion de vitesse.
const GUARD: Duration = Duration::from_secs(60);

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
    /// Si posé : la prochaine lecture du flux échoue (le lien tombe), une seule fois.
    fail_stream: AtomicBool,
    /// Si posé : la relecture d'une opération répond 404 (l'agent ne l'a jamais reçue).
    lookup_missing: AtomicBool,
    /// Si posé : `login` attend d'être relâché (connexion en vol).
    login_hold: AtomicBool,
    login_in_flight: AtomicBool,
    /// Rôle que le serveur donne au compte à la prochaine connexion.
    next_role: Mutex<RoleName>,
    /// Si posé : `logout` attend d'être relâché (déconnexion en vol).
    logout_hold: AtomicBool,
    logout_in_flight: AtomicBool,
    logouts: AtomicUsize,
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
            fail_stream: AtomicBool::new(false),
            lookup_missing: AtomicBool::new(false),
            login_hold: AtomicBool::new(false),
            login_in_flight: AtomicBool::new(false),
            next_role: Mutex::new(RoleName::Admin),
            logout_hold: AtomicBool::new(false),
            logout_in_flight: AtomicBool::new(false),
            logouts: AtomicUsize::new(0),
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
        if self.0.login_hold.load(Ordering::SeqCst) {
            self.0.login_in_flight.store(true, Ordering::SeqCst);
            while self.0.login_hold.load(Ordering::SeqCst) {
                tokio::time::sleep(ms(5)).await;
            }
        }
        Ok(LoginResponse {
            token: format!("{:064x}", n + 1),
            expires_at: "x".into(),
            account: AccountInfo {
                id: "A".into(),
                username: "marie".into(),
                role: *self.0.next_role.lock().unwrap(),
            },
        })
    }

    async fn logout(&self, _: &Target, _: &Secret) -> Result<(), TransportError> {
        self.0.logouts.fetch_add(1, Ordering::SeqCst);
        if self.0.logout_hold.load(Ordering::SeqCst) {
            self.0.logout_in_flight.store(true, Ordering::SeqCst);
            while self.0.logout_hold.load(Ordering::SeqCst) {
                tokio::time::sleep(ms(5)).await;
            }
        }
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
        if self.0.lookup_missing.load(Ordering::SeqCst) {
            return Err(TransportError::Api(
                hearth_link::ports::transport::ApiError {
                    status: 404,
                    code: None,
                    details: json!({}),
                    retry_after_s: None,
                },
            ));
        }
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
        if self.script.fail_stream.swap(false, Ordering::SeqCst) {
            return Err(TransportError::Closed(None));
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
    /// L'écriture attend que le test ouvre la porte (`Store::gate_open`) : aucune horloge.
    Gated,
}

struct Store {
    disk: Mutex<Disk>,
    servers: Mutex<Vec<ServerRecord>>,
    operations: Mutex<Vec<PendingOp>>,
    /// Instant de la fin de la dernière écriture des opérations.
    written_at: Mutex<Option<Instant>>,
    /// `Disk::Gated` : une écriture attend à la porte / la porte est ouverte.
    gate_reached: AtomicBool,
    gate_open: AtomicBool,
}

impl Store {
    fn new(disk: Disk) -> Arc<Self> {
        Arc::new(Self {
            disk: Mutex::new(disk),
            servers: Mutex::new(Vec::new()),
            operations: Mutex::new(Vec::new()),
            written_at: Mutex::new(None),
            gate_reached: AtomicBool::new(false),
            gate_open: AtomicBool::new(false),
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
            Disk::Gated => {
                self.gate_reached.store(true, Ordering::SeqCst);
                while !self.gate_open.load(Ordering::SeqCst) {
                    tokio::time::sleep(ms(2)).await;
                }
            }
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
/// événement : de quoi mettre une trame et une commande en attente ensemble. La tâche est
/// relâchée par le test (`release`), jamais par une durée.
#[derive(Default)]
struct Gate {
    block_next: AtomicBool,
    reached: AtomicBool,
    release: AtomicBool,
}

impl EventSink for Gate {
    fn emit(&self, _: Event) {
        if self.block_next.swap(false, Ordering::SeqCst) {
            self.reached.store(true, Ordering::SeqCst);
            while !self.release.load(Ordering::SeqCst) {
                std::thread::sleep(ms(1));
            }
        }
    }
}

fn config() -> LinkConfig {
    config_with(Duration::from_secs(120))
}

/// `persist_timeout` : très large par défaut (un disque simulé retenu par le test ne doit jamais
/// atteindre le délai) ; court seulement pour le test dont c'est précisément l'objet.
fn config_with(persist_timeout: Duration) -> LinkConfig {
    let hour = Duration::from_secs(3_600);
    LinkConfig {
        thresholds: hearth_link::domain::state::Thresholds {
            silence: hour,
            reconnecting_after: hour,
            offline_after: hour * 2,
            ..hearth_link::domain::state::Thresholds::default()
        },
        heartbeat_period: ms(100),
        persist_timeout,
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
    start_with(script, store, vault, bomb, config()).await
}

async fn start_with(
    script: &Arc<Script>,
    store: &Arc<Store>,
    vault: &Arc<MemoryVault>,
    bomb: Option<Arc<dyn EventSink>>,
    config: LinkConfig,
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
        config,
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
    connected_config(disk, remember, sink, config()).await
}

async fn connected_config(
    disk: Disk,
    remember: bool,
    sink: Option<Arc<dyn EventSink>>,
    config: LinkConfig,
) -> (Rig, ServerId) {
    let script = Script::new();
    let store = Store::new(disk);
    let vault = Arc::new(MemoryVault::new());
    let manager = start_with(&script, &store, &vault, sink, config).await;
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
    let deadline = Instant::now() + GUARD;
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
    // Délai d'écriture court : c'est lui qu'on éprouve (plus il est dépassé, mieux c'est : aucune
    // assertion de durée, seulement « l'appel se termine, en TrackingSlow »).
    let (rig, id) = connected_config(Disk::Hanging, false, None, config_with(ms(300))).await;
    let result = tokio::time::timeout(GUARD, rig.manager.execute(&id, change_password()))
        .await
        .expect("l'appel est borné par le délai d'écriture");
    assert_eq!(result.unwrap_err(), LinkError::TrackingSlow);
    assert_eq!(rig.script.requests.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_slow_write_delays_the_request_until_it_is_done() {
    // Écriture retenue à une porte ouverte par le test : aucune horloge dans le scénario.
    let (rig, id) = connected(Disk::Gated, false).await;
    // Ce que le disque contient au moment où la requête part (le suivi doit déjà y être).
    let seen_on_disk = Arc::new(AtomicUsize::new(usize::MAX));
    let seen = seen_on_disk.clone();
    let store = rig.store.clone();
    *rig.script.on_request.lock().unwrap() = Some(Box::new(move || {
        seen.store(store.tracked(), Ordering::SeqCst);
    }));
    let manager = rig.manager.clone();
    let task_id = id.clone();
    let call = tokio::spawn(async move { manager.execute(&task_id, change_password()).await });
    wait_until("écriture du suivi à la porte", || {
        rig.store.gate_reached.load(Ordering::SeqCst)
    })
    .await;
    // Tant que la porte est fermée (le disque n'a pas fini), la requête ne peut pas partir : on
    // laisse les tâches tourner à volonté, c'est un ordre de causalité et non une durée.
    for _ in 0..50 {
        tokio::task::yield_now().await;
    }
    assert_eq!(
        rig.script.requests.load(Ordering::SeqCst),
        0,
        "la requête attend l'écriture du suivi"
    );
    assert_eq!(rig.store.tracked(), 0, "rien d'écrit porte fermée");
    rig.store.gate_open.store(true, Ordering::SeqCst);
    let outcome = tokio::time::timeout(GUARD, call)
        .await
        .expect("l'action se termine une fois l'écriture finie")
        .unwrap()
        .unwrap();
    assert!(matches!(
        outcome,
        ActionOutcome::Completed { status: 200, .. }
    ));
    assert_eq!(
        seen_on_disk.load(Ordering::SeqCst),
        1,
        "la requête n'est partie qu'après l'écriture du suivi"
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
    wait_until("la requête est partie", || {
        rig.script.requests.load(Ordering::SeqCst) > 0
    })
    .await;
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
    let (_, outcome) = tokio::time::timeout(GUARD, async {
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
    // `notify_one` garde un permis : la lecture du flux le prend même si elle n'attend pas encore.
    rig.script.wake.notify_one();
    wait_until("tâche retenue par la destination d'événements", || {
        gate.reached.load(Ordering::SeqCst)
    })
    .await;
    rig.script.expire_next.store(true, Ordering::SeqCst);
    // La déconnexion démarre pendant que la tâche est retenue ; le test ne la relâche qu'une fois
    // la déconnexion notée.
    let logout = {
        let manager = rig.manager.clone();
        let task_id = id.clone();
        tokio::spawn(async move { manager.logout(&task_id).await })
    };
    // Précondition de la course : la déconnexion est notée (le carnet dit « déconnecté », sous
    // verrou, avant même l'envoi de la commande). Tant que ce n'est pas fait, on ne relâche pas.
    wait_until("déconnexion notée au carnet", || {
        rig.manager.servers().iter().any(|record| record.signed_out)
    })
    .await;
    gate.release.store(true, Ordering::SeqCst);
    tokio::time::timeout(GUARD, logout)
        .await
        .expect("la déconnexion se termine")
        .unwrap()
        .unwrap();
    // Le fait attendu : l'état final est « déconnecté par l'utilisateur » ; l'absence de session
    // silencieuse se lit au compteur de connexions, relevé une fois l'état atteint.
    wait_until("état déconnecté par l'utilisateur", || {
        let info = rig.manager.state(&id).unwrap();
        info.state == LinkState::SessionExpired && info.reason == Some(Reason::UserDisconnected)
    })
    .await;
    assert_eq!(
        rig.script.logins.load(Ordering::SeqCst),
        logins,
        "aucune reconnexion silencieuse"
    );
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
        role: None,
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
    wait_until("la tâche a paniqué puis repris", || {
        manager.task_restarts(&id).unwrap() == 1
    })
    .await;
    wait_until("état déconnecté après la reprise", || {
        manager.state(&id).unwrap().state == LinkState::SessionExpired
    })
    .await;
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
    let pings = rig.script.pings.load(Ordering::SeqCst);
    // Une commande passe malgré le flot ininterrompu de trames.
    let outcome = tokio::time::timeout(GUARD, rig.manager.execute(&id, change_password()))
        .await
        .expect("la commande n'est pas affamée par le flot de trames")
        .unwrap();
    assert!(matches!(outcome, ActionOutcome::Completed { .. }));
    // Le battement continue pendant le flot : trois battements de plus, quel que soit le temps
    // que cela prend sur la machine.
    wait_until("le battement continue pendant le flot", || {
        rig.script.pings.load(Ordering::SeqCst) >= pings + 3
    })
    .await;
    assert_eq!(rig.manager.state(&id).unwrap().state, LinkState::Connected);
    let _ = ErrorDetail {
        code: ErrorCode::Busy,
        message: String::new(),
        details: json!({}),
    };
}

// ── Suivi d'une action retenue par le disque, lien coupé (review PR 12) ──────────────────────

#[tokio::test]
async fn a_link_cut_while_the_tracking_is_written_ends_as_not_executed_exactly_once() {
    // Écriture du suivi retenue à une porte, ouverte par le test : aucune horloge dans le scénario.
    let (rig, id) = connected(Disk::Normal, false).await;
    *rig.store.disk.lock().unwrap() = Disk::Gated;
    rig.script.lookup_missing.store(true, Ordering::SeqCst);
    let mut events = rig.manager.subscribe();
    let manager = rig.manager.clone();
    let task_id = id.clone();
    let call = tokio::spawn(async move { manager.execute(&task_id, change_password()).await });
    // L'écriture est arrivée à la porte : la requête n'est pas partie. Le lien tombe maintenant.
    wait_until("écriture du suivi à la porte", || {
        rig.store.gate_reached.load(Ordering::SeqCst)
    })
    .await;
    rig.script.fail_stream.store(true, Ordering::SeqCst);
    let outcome = tokio::time::timeout(GUARD, call)
        .await
        .expect("l'appelant est libéré par la coupure")
        .unwrap()
        .unwrap();
    let ActionOutcome::ResultUnknown { id: operation } = outcome else {
        panic!("résultat inconnu attendu, vu {outcome:?}");
    };
    // On relâche l'écriture ; le lien revient ; l'agent ne connaît pas l'opération.
    rig.store.gate_open.store(true, Ordering::SeqCst);
    let announced = tokio::time::timeout(GUARD, async {
        loop {
            if let Some(Event::Operation { id, outcome, .. }) = events.recv().await {
                return (id, outcome);
            }
        }
    })
    .await
    .expect("l'issue est annoncée au retour du lien");
    assert_eq!(announced, (operation, Outcome::NotExecuted));
    // Plus aucune autre issue : le suivi est soldé, rien d'autre n'attend dans le flux.
    wait_until("suivi soldé sur disque", || rig.store.tracked() == 0).await;
    wait_state(&rig.manager, &id, LinkState::Connected).await;
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
    while let Some(event) = events.try_recv() {
        assert!(
            !matches!(event, Event::Operation { .. }),
            "une seule issue, pas {event:?}"
        );
    }
    assert_eq!(
        rig.script.requests.load(Ordering::SeqCst),
        0,
        "la requête n'est jamais partie"
    );
}

/// Attend une condition sans horloge de scénario (simple surveillance, garde de `GUARD`).
async fn wait_until(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + GUARD;
    while !condition() {
        assert!(Instant::now() < deadline, "attendu : {what}");
        tokio::time::sleep(ms(2)).await;
    }
}

// ── Déconnexion : le réseau hors verrou (review PR 12, round 2) ──────────────────────────────

#[tokio::test]
async fn a_slow_logout_does_not_hold_back_a_removal() {
    let (rig, id) = connected(Disk::Normal, true).await;
    rig.script.logout_hold.store(true, Ordering::SeqCst);
    let manager = rig.manager.clone();
    let task_id = id.clone();
    let logout = tokio::spawn(async move { manager.logout(&task_id).await });
    wait_until("déconnexion en vol", || {
        rig.script.logout_in_flight.load(Ordering::SeqCst)
    })
    .await;
    // La suppression n'attend pas le serveur injoignable : elle se termine alors que le réseau tient.
    tokio::time::timeout(GUARD, rig.manager.remove_server(&id))
        .await
        .expect("la suppression ne reste pas derrière l'appel réseau")
        .unwrap();
    rig.script.logout_hold.store(false, Ordering::SeqCst);
    assert_eq!(logout.await.unwrap().unwrap_err(), LinkError::UnknownServer);
    assert!(rig.vault.get(&id, SecretKind::Token).unwrap().is_none());
    assert!(rig.store.servers.lock().unwrap().is_empty());
}

// ── Déconnexion lente contre reconnexion (FIX-01M46G7Z0ZP43T53M2F5KG4VKS) ─────────────────────

#[tokio::test]
async fn a_slow_logout_never_erases_the_token_of_a_login_that_came_in_between() {
    let (rig, id) = connected(Disk::Normal, true).await;
    rig.script.logout_hold.store(true, Ordering::SeqCst);
    let manager = rig.manager.clone();
    let task_id = id.clone();
    let logout = tokio::spawn(async move { manager.logout(&task_id).await });
    wait_until("déconnexion en vol", || {
        rig.script.logout_in_flight.load(Ordering::SeqCst)
    })
    .await;
    // Pendant l'appel réseau de la déconnexion, l'utilisateur se reconnecte : jeton neuf au coffre.
    rig.manager
        .login(&id, "marie", Secret::from("Correct-Horse-9"), true)
        .await
        .unwrap();
    let fresh = rig.vault.get(&id, SecretKind::Token).unwrap().unwrap();
    rig.script.logout_hold.store(false, Ordering::SeqCst);
    logout.await.unwrap().unwrap();
    assert_eq!(
        rig.vault.get(&id, SecretKind::Token).unwrap(),
        Some(fresh),
        "le jeton de la nouvelle session est intact"
    );
    assert!(
        !rig.store.servers.lock().unwrap()[0].signed_out,
        "le carnet dit toujours « connecté »"
    );
    wait_state(&rig.manager, &id, LinkState::Connected).await;
}

// ── Suppression contre connexion en vol (review PR 12, bloquant 2) ───────────────────────────

#[tokio::test]
async fn a_server_removed_while_a_login_is_in_flight_is_never_written_back() {
    let (rig, id) = connected(Disk::Normal, true).await;
    rig.script.login_hold.store(true, Ordering::SeqCst);
    let manager = rig.manager.clone();
    let task_id = id.clone();
    let login = tokio::spawn(async move {
        manager
            .login(&task_id, "marie", Secret::from("Correct-Horse-9"), true)
            .await
    });
    wait_until("la connexion est en vol", || {
        rig.script.login_in_flight.load(Ordering::SeqCst)
    })
    .await;
    // Suppression pendant l'échange avec le réseau, puis la connexion repart.
    rig.manager.remove_server(&id).await.unwrap();
    rig.script.login_hold.store(false, Ordering::SeqCst);
    let result = login.await.unwrap();
    assert_eq!(result.unwrap_err(), LinkError::UnknownServer);
    assert!(rig.vault.get(&id, SecretKind::Token).unwrap().is_none());
    assert!(rig.vault.get(&id, SecretKind::Password).unwrap().is_none());
    assert!(rig.store.servers.lock().unwrap().is_empty(), "carnet vide");
    // Au prochain lancement rien ne revient.
    rig.manager.shutdown().await;
    let script = Script::new();
    let manager = start(&script, &rig.store, &rig.vault, None).await;
    assert!(manager.servers().is_empty());
}

// ── Coffre qui refuse d'effacer (review PR 12) ───────────────────────────────────────────────

/// Coffre dont l'effacement du mot de passe échoue.
struct StubbornVault(MemoryVault);

impl Vault for StubbornVault {
    fn get(&self, s: &ServerId, k: SecretKind) -> Result<Option<Secret>, VaultError> {
        self.0.get(s, k)
    }
    fn put(&self, s: &ServerId, k: SecretKind, v: &Secret) -> Result<(), VaultError> {
        self.0.put(s, k, v)
    }
    fn delete(&self, s: &ServerId, k: SecretKind) -> Result<(), VaultError> {
        if k == SecretKind::Password {
            return Err(VaultError("refusé".into()));
        }
        self.0.delete(s, k)
    }
}

#[tokio::test]
async fn a_vault_that_refuses_to_erase_keeps_the_server_and_says_so() {
    let script = Script::new();
    let store = Store::new(Disk::Normal);
    let vault = Arc::new(StubbornVault(MemoryVault::new()));
    let manager = LinkManager::start(
        Ports {
            transport: Arc::new(Mock(script.clone())),
            vault: vault.clone(),
            servers: store.clone(),
            snapshots: store.clone(),
            operations: store.clone(),
            clock: Arc::new(SystemClock::new()),
            rng: Arc::new(OsRng::default()),
            net: Arc::new(NoNet),
            extra_sink: None,
        },
        config(),
    )
    .await
    .unwrap();
    let id = manager
        .add_server(NewServer {
            name: "Mock".into(),
            color: "1".into(),
            host: "mock.test".into(),
            port: 7341,
            fingerprint: Fingerprint::from_bytes([7; 32]),
            mac_addresses: vec![],
        })
        .await
        .unwrap();
    manager
        .login(&id, "marie", Secret::from("Correct-Horse-9"), true)
        .await
        .unwrap();
    let error = manager.remove_server(&id).await.unwrap_err();
    assert!(matches!(error, LinkError::Vault(_)), "{error:?}");
    // Rien n'est dit « supprimé » : le serveur est toujours là, avec son mot de passe.
    assert_eq!(manager.servers().len(), 1);
    assert_eq!(store.servers.lock().unwrap().len(), 1);
    assert!(vault.get(&id, SecretKind::Password).unwrap().is_some());
    wait_state(&manager, &id, LinkState::Connected).await;
}

// ── Rôle rafraîchi à la reconnexion silencieuse (review PR 12) ───────────────────────────────

#[tokio::test]
async fn a_silent_reconnection_refreshes_the_role_of_the_account() {
    let (rig, id) = connected(Disk::Normal, true).await;
    assert_eq!(
        rig.manager.servers()[0].role,
        Some(RoleName::Admin),
        "rôle de la connexion"
    );
    *rig.script.next_role.lock().unwrap() = RoleName::Readonly;
    rig.script.expire_next.store(true, Ordering::SeqCst);
    wait_until("reconnexion silencieuse", || {
        rig.script.logins.load(Ordering::SeqCst) >= 2
    })
    .await;
    wait_state(&rig.manager, &id, LinkState::Connected).await;
    assert_eq!(rig.manager.servers()[0].role, Some(RoleName::Readonly));
    let saved = rig.store.servers.lock().unwrap()[0].role;
    assert_eq!(saved, Some(RoleName::Readonly), "gardé au carnet");
}

// ── Première connexion : un coffre qui refuse ne laisse ni secret ni serveur (round 2) ────────

/// Coffre qui refuse d'écrire.
struct ClosedVault;

impl Vault for ClosedVault {
    fn get(&self, _: &ServerId, _: SecretKind) -> Result<Option<Secret>, VaultError> {
        Ok(None)
    }
    fn put(&self, _: &ServerId, _: SecretKind, _: &Secret) -> Result<(), VaultError> {
        Err(VaultError("fermé".into()))
    }
    fn delete(&self, _: &ServerId, _: SecretKind) -> Result<(), VaultError> {
        Ok(())
    }
}

#[tokio::test]
async fn a_vault_that_refuses_to_write_leaves_no_server_and_closes_the_new_session() {
    let script = Script::new();
    let store = Store::new(Disk::Normal);
    let manager = LinkManager::start(
        Ports {
            transport: Arc::new(Mock(script.clone())),
            vault: Arc::new(ClosedVault),
            servers: store.clone(),
            snapshots: store.clone(),
            operations: store.clone(),
            clock: Arc::new(SystemClock::new()),
            rng: Arc::new(OsRng::default()),
            net: Arc::new(NoNet),
            extra_sink: None,
        },
        config(),
    )
    .await
    .unwrap();
    let new = NewServer {
        name: "Mock".into(),
        color: "1".into(),
        host: "mock.test".into(),
        port: 7341,
        fingerprint: Fingerprint::from_bytes([7; 32]),
        mac_addresses: vec![],
    };
    let error = manager
        .add_and_login(new, "marie", Secret::from("Correct-Horse-9"), true)
        .await
        .unwrap_err();
    assert!(matches!(error, LinkError::Vault(_)), "{error:?}");
    assert!(manager.servers().is_empty());
    assert!(store.servers.lock().unwrap().is_empty());
    assert_eq!(
        script.logouts.load(Ordering::SeqCst),
        1,
        "la session obtenue est refermée côté serveur"
    );
}

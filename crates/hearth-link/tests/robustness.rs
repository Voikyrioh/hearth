//! Robustesse : un transport simulé qui renvoie des erreurs et des réponses aléatoires, souvent
//! absurdes (nombres hors normes, listes énormes, messages dans le désordre, silences sans fin),
//! pendant des milliers d'itérations d'appels de l'interface. La bibliothèque ne doit ni paniquer
//! (aucune tâche relancée), ni bloquer (chaque appel rend sous délai), ni grossir sans borne.
//!
//! Le temps est virtuel (`start_paused`) : des heures de coupures en quelques secondes.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeSet, HashMap};
use std::net::IpAddr;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use hearth_link::adapters::{MemoryVault, TokioClock};
use hearth_link::domain::pending_ops::OperationId;
use hearth_link::domain::pending_ops::PendingOp;
use hearth_link::domain::secret::Secret;
use hearth_link::domain::server::{HISTORY_CAP, LastKnown, ServerId, ServerRecord};
use hearth_link::domain::state::Thresholds;
use hearth_link::domain::time::{Mono, WallTime};
use hearth_link::ports::net_watcher::{NetError, NetWatcher};
use hearth_link::ports::server_store::StoreError;
use hearth_link::ports::transport::{
    ApiError, ApiRequest, ApiResponse, Frame, Method, Probed, StreamConn, Target, Transport,
    TransportError,
};
use hearth_link::ports::{Clock, OperationStore, Rng, ServerStore, SnapshotStore};
use hearth_link::{ActionRequest, LinkConfig, LinkManager, NewServer, Ports};
use hearth_proto::api::accounts::{AccountInfo, RoleName};
use hearth_proto::api::audit::{AuditEventItem, AuditOrigin, OriginKindName, OutcomeName};
use hearth_proto::api::hello::{ApiRange, HelloResponse};
use hearth_proto::api::machine::{Capabilities, CpuInfo, MachineResponse, OsInfo};
use hearth_proto::api::metrics::{MemorySample, Sample};
use hearth_proto::api::operations::{OperationResponse, OperationStatus};
use hearth_proto::api::sessions::{LoginRequest, LoginResponse};
use hearth_proto::error::{ErrorCode, ErrorDetail};
use hearth_proto::fingerprint::Fingerprint;
use hearth_proto::stream::{ClientMessage, ServerMessage, SessionNotice};
use serde_json::{Value, json};

// ── Hasard déterministe ─────────────────────────────────────────────────────────────────────

struct Dice(Mutex<u64>);

impl Dice {
    fn new(seed: u64) -> Arc<Self> {
        Arc::new(Self(Mutex::new(
            seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1,
        )))
    }

    fn next(&self) -> u64 {
        let mut state = self.0.lock().unwrap();
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        *state
    }

    fn below(&self, n: u64) -> u64 {
        self.next() % n.max(1)
    }

    fn chance(&self, percent: u64) -> bool {
        self.below(100) < percent
    }
}

impl Rng for Dice {
    fn next_u32(&self) -> u32 {
        (self.next() >> 16) as u32
    }
}

// ── Données absurdes ────────────────────────────────────────────────────────────────────────

const CODES: [ErrorCode; 20] = [
    ErrorCode::Unauthenticated,
    ErrorCode::InvalidCredentials,
    ErrorCode::SessionExpired,
    ErrorCode::SessionRevoked,
    ErrorCode::ForbiddenRole,
    ErrorCode::OperationInProgress,
    ErrorCode::ValidationError,
    ErrorCode::IncompatibleVersion,
    ErrorCode::TooManyAttempts,
    ErrorCode::UsernameTaken,
    ErrorCode::WeakPassword,
    ErrorCode::WrongPassword,
    ErrorCode::LastAdmin,
    ErrorCode::Conflict,
    ErrorCode::PayloadTooLarge,
    ErrorCode::IdempotencyKeyReused,
    ErrorCode::Busy,
    ErrorCode::InternalError,
    ErrorCode::NotFound,
    ErrorCode::MethodNotAllowed,
];

fn float(dice: &Dice) -> f32 {
    match dice.below(8) {
        0 => f32::NAN,
        1 => f32::INFINITY,
        2 => f32::NEG_INFINITY,
        3 => -1.0,
        4 => f32::MAX,
        _ => dice.below(100) as f32,
    }
}

fn big(dice: &Dice) -> u64 {
    if dice.chance(20) {
        u64::MAX
    } else {
        dice.next()
    }
}

fn sample(dice: &Dice) -> Sample {
    Sample {
        at: if dice.chance(30) {
            "pas une date".into()
        } else {
            "2026-10-04T10:30:15.250Z".into()
        },
        uptime_s: big(dice),
        cpu: float(dice),
        cores: (0..dice.below(70)).map(|_| float(dice)).collect(),
        mem: MemorySample {
            used_bytes: big(dice),
            total_bytes: big(dice),
        },
        disks: vec![],
        net: None,
        gpus: vec![],
        temps: vec![],
    }
}

fn machine() -> MachineResponse {
    MachineResponse {
        name: "chaos".into(),
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

fn deep_json(dice: &Dice, depth: u32) -> Value {
    if depth == 0 || dice.chance(30) {
        return json!(dice.next());
    }
    json!({ "k": deep_json(dice, depth - 1), "list": [deep_json(dice, depth - 1)], "s": "é\u{0}\n" })
}

fn message(dice: &Dice, first: bool) -> ServerMessage {
    if first && dice.chance(80) {
        return ServerMessage::Snapshot {
            machine: machine(),
            history: (0..dice.below(900)).map(|_| sample(dice)).collect(),
        };
    }
    match dice.below(10) {
        0 | 1 => ServerMessage::Snapshot {
            machine: machine(),
            history: (0..dice.below(900)).map(|_| sample(dice)).collect(),
        },
        2..=4 => ServerMessage::Metrics(sample(dice)),
        5 => ServerMessage::Pong { n: big(dice) },
        6 => ServerMessage::Audit {
            event: AuditEventItem {
                id: big(dice) as i64,
                at: "n'importe quoi".repeat(dice.below(50) as usize),
                account: if dice.chance(50) {
                    None
                } else {
                    Some("é".into())
                },
                origin: AuditOrigin {
                    kind: OriginKindName::Cli,
                    name: None,
                    addr: None,
                    text: "x".repeat(dice.below(4_000) as usize),
                },
                action: "login".into(),
                action_label: "Connexion".into(),
                target: None,
                outcome: OutcomeName::Failed,
                reason: Some("raison".into()),
                repeat_count: dice.below(1_000_000) as u32,
            },
        },
        7 => ServerMessage::Session {
            kind: if dice.chance(50) {
                SessionNotice::Expired
            } else {
                SessionNotice::Revoked
            },
        },
        _ => ServerMessage::Error(ErrorDetail {
            code: CODES[dice.below(CODES.len() as u64) as usize],
            message: "x".repeat(dice.below(2_000) as usize),
            details: deep_json(dice, 4),
        }),
    }
}

fn failure(dice: &Dice) -> TransportError {
    match dice.below(8) {
        0 => TransportError::Connect("refusé".into()),
        1 => TransportError::Timeout,
        2 => TransportError::Io("reset".into()),
        3 => TransportError::Closed(if dice.chance(50) { Some(1008) } else { None }),
        4 => TransportError::FingerprintMismatch {
            presented: Fingerprint::from_bytes([dice.below(256) as u8; 32]),
        },
        5 => TransportError::Protocol("n'importe quoi".into()),
        _ => TransportError::Api(ApiError {
            status: [400u16, 401, 404, 409, 422, 426, 429, 500, 503, 0, 999]
                [dice.below(11) as usize],
            code: if dice.chance(15) {
                None
            } else {
                Some(CODES[dice.below(CODES.len() as u64) as usize])
            },
            details: deep_json(dice, 3),
            retry_after_s: if dice.chance(50) {
                Some(big(dice))
            } else {
                None
            },
        }),
    }
}

// ── Transport chaotique ─────────────────────────────────────────────────────────────────────

struct Chaos {
    dice: Arc<Dice>,
}

#[async_trait]
impl Transport for Chaos {
    async fn hello(&self, _target: &Target) -> Result<Probed, TransportError> {
        let dice = &self.dice;
        wait(dice).await;
        if dice.chance(25) {
            return Err(failure(dice));
        }
        Ok(Probed {
            fingerprint: Fingerprint::from_bytes([7; 32]),
            hello: HelloResponse {
                product: "hearth".into(),
                agent_version: "0.0.0".into(),
                api: if dice.chance(10) {
                    ApiRange { min: 9, max: 3 }
                } else {
                    ApiRange { min: 1, max: 1 }
                },
                machine_name: "chaos".into(),
                install_id: "00".into(),
                managed: false,
                mac_addresses: vec![],
            },
        })
    }

    async fn login(
        &self,
        _target: &Target,
        _request: &LoginRequest,
    ) -> Result<LoginResponse, TransportError> {
        let dice = &self.dice;
        wait(dice).await;
        if dice.chance(35) {
            return Err(failure(dice));
        }
        Ok(LoginResponse {
            token: format!("{:064x}", dice.next()),
            expires_at: "2026-11-03T10:30:15.25Z".into(),
            account: AccountInfo {
                id: "A".into(),
                username: "marie".into(),
                role: RoleName::Admin,
            },
        })
    }

    async fn logout(&self, _target: &Target, _token: &Secret) -> Result<(), TransportError> {
        wait(&self.dice).await;
        if self.dice.chance(40) {
            Err(failure(&self.dice))
        } else {
            Ok(())
        }
    }

    async fn request(
        &self,
        _target: &Target,
        _token: &Secret,
        _request: &ApiRequest,
    ) -> Result<ApiResponse, TransportError> {
        let dice = &self.dice;
        if dice.chance(10) {
            // Une requête qui ne répond jamais : seul le délai de l'appelant la libère.
            std::future::pending::<()>().await;
        }
        wait(dice).await;
        if dice.chance(40) {
            return Err(failure(dice));
        }
        Ok(ApiResponse {
            status: [200u16, 201, 204, 400, 401, 409, 422, 500, 503, 0, 65_535]
                [dice.below(11) as usize],
            body: if dice.chance(30) {
                Value::Null
            } else {
                deep_json(dice, 5)
            },
            replayed: dice.chance(20),
        })
    }

    async fn operation(
        &self,
        _target: &Target,
        _token: &Secret,
        id: &OperationId,
    ) -> Result<OperationResponse, TransportError> {
        let dice = &self.dice;
        wait(dice).await;
        if dice.chance(40) {
            return Err(failure(dice));
        }
        Ok(OperationResponse {
            id: id.as_str().to_owned(),
            kind: "PUT /x".into(),
            status: [
                OperationStatus::Running,
                OperationStatus::Succeeded,
                OperationStatus::Failed,
                OperationStatus::Interrupted,
            ][dice.below(4) as usize],
            result: if dice.chance(50) {
                None
            } else {
                Some(deep_json(dice, 4))
            },
        })
    }

    async fn open_stream(&self, _target: &Target) -> Result<Box<dyn StreamConn>, TransportError> {
        let dice = &self.dice;
        wait(dice).await;
        if dice.chance(35) {
            return Err(failure(dice));
        }
        Ok(Box::new(ChaosStream {
            dice: dice.clone(),
            first: true,
        }))
    }
}

async fn wait(dice: &Dice) {
    tokio::time::sleep(Duration::from_millis(dice.below(900))).await;
}

struct ChaosStream {
    dice: Arc<Dice>,
    first: bool,
}

#[async_trait]
impl StreamConn for ChaosStream {
    async fn send(&mut self, _message: &ClientMessage) -> Result<(), TransportError> {
        let dice = &self.dice;
        match dice.below(20) {
            0 => Err(failure(dice)),
            1 => {
                std::future::pending::<()>().await;
                Ok(())
            }
            _ => Ok(()),
        }
    }

    async fn recv(&mut self) -> Result<Frame, TransportError> {
        let dice = self.dice.clone();
        let first = std::mem::take(&mut self.first);
        match dice.below(100) {
            0..=69 => {
                tokio::time::sleep(Duration::from_millis(dice.below(700))).await;
                Ok(Frame::Message(Box::new(message(&dice, first))))
            }
            70..=74 => Ok(Frame::Other),
            75..=79 => {
                tokio::time::sleep(Duration::from_millis(dice.below(3_000))).await;
                Err(TransportError::Closed(None))
            }
            80..=84 => Err(failure(&dice)),
            // Gel : plus un seul message, sans erreur.
            85..=88 => std::future::pending().await,
            _ => Ok(Frame::Message(Box::new(message(&dice, false)))),
        }
    }
}

// ── Stockage en mémoire, réseau scripté, horloge qui saute ─────────────────────────────────

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

#[derive(Default)]
struct Net(Mutex<BTreeSet<IpAddr>>);

#[async_trait]
impl NetWatcher for Net {
    async fn addresses(&self) -> Result<BTreeSet<IpAddr>, NetError> {
        Ok(self.0.lock().unwrap().clone())
    }
}

struct JumpingClock {
    inner: TokioClock,
    jump_ms: AtomicI64,
}

impl Clock for JumpingClock {
    fn mono(&self) -> Mono {
        self.inner.mono()
    }
    fn wall(&self) -> WallTime {
        WallTime::from_millis(self.inner.wall().as_millis() + self.jump_ms.load(Ordering::SeqCst))
    }
}

// ── Le test ─────────────────────────────────────────────────────────────────────────────────

/// Un appel de l'interface rend toujours, sous délai : jamais de blocage indéfini.
async fn bounded<T>(what: &str, call: impl std::future::Future<Output = T>) -> T {
    match tokio::time::timeout(Duration::from_secs(300), call).await {
        Ok(value) => value,
        Err(_) => panic!("blocage : {what} n'a pas rendu en 300 s virtuelles"),
    }
}

async fn run(seed: u64, iterations: u32) {
    let dice = Dice::new(seed);
    let store = Arc::new(MemStore::default());
    let net = Arc::new(Net::default());
    let clock = Arc::new(JumpingClock {
        inner: TokioClock::new(WallTime::from_millis(1_790_000_000_000)),
        jump_ms: AtomicI64::new(0),
    });
    let config = LinkConfig {
        thresholds: Thresholds::default(),
        ..LinkConfig::default()
    };
    let manager = LinkManager::start(
        Ports {
            transport: Arc::new(Chaos { dice: dice.clone() }),
            vault: Arc::new(MemoryVault::new()),
            servers: store.clone(),
            snapshots: store.clone(),
            operations: store.clone(),
            clock: clock.clone(),
            rng: dice.clone(),
            net: net.clone(),
            extra_sink: None,
        },
        config,
    )
    .await
    .unwrap();
    let mut events = manager.subscribe();
    let seen = Arc::new(Mutex::new(HashMap::<String, u64>::new()));
    let tally = seen.clone();
    let drain = tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            let key = match &event {
                hearth_link::domain::event::Event::State { info, .. } => {
                    format!("{:?}", info.state)
                }
                hearth_link::domain::event::Event::Operation { outcome, .. } => {
                    format!(
                        "op:{}",
                        format!("{outcome:?}")
                            .split([' ', '{', '('])
                            .next()
                            .unwrap_or("")
                    )
                }
                other => {
                    format!("{:?}", std::mem::discriminant(other)).replace("Discriminant", "ev")
                }
            };
            *tally.lock().unwrap().entry(key).or_default() += 1;
        }
    });

    let mut ids: Vec<ServerId> = Vec::new();
    for step in 0..iterations {
        match dice.below(14) {
            // Ajouter un serveur (jamais plus de 4).
            0 if ids.len() < 4 => {
                let port = 7_000 + ids.len() as u16 + (step % 50) as u16 * 10;
                let added = bounded(
                    "add_server",
                    manager.add_server(NewServer {
                        name: format!("srv{step}"),
                        color: "#fff".into(),
                        host: "chaos.test".into(),
                        port,
                        fingerprint: Fingerprint::from_bytes([7; 32]),
                        mac_addresses: vec![],
                    }),
                )
                .await;
                if let Ok(id) = added {
                    ids.push(id);
                }
            }
            1 | 2 => {
                if let Some(id) = pick(&ids, &dice) {
                    let remember = dice.chance(50);
                    let password = if dice.chance(80) {
                        "Correct-Horse-9"
                    } else {
                        ""
                    };
                    let _ = bounded(
                        "login",
                        manager.login(id, "marie", Secret::from(password), remember),
                    )
                    .await;
                }
            }
            3 => {
                if let Some(id) = pick(&ids, &dice) {
                    let _ = bounded("probe", manager.probe("chaos.test", 7341)).await;
                    let _ = manager.retry_now(id);
                }
            }
            4..=6 => {
                if let Some(id) = pick(&ids, &dice) {
                    let action = ActionRequest {
                        method: Method::Put,
                        path: "/me/password".into(),
                        body: Some(deep_json(&dice, 3)),
                    };
                    let _ = bounded("execute", manager.execute_raw(id, action)).await;
                }
            }
            7 => {
                if let Some(id) = pick(&ids, &dice) {
                    let _ = bounded("logout", manager.logout(id)).await;
                }
            }
            8 => {
                if let Some(id) = pick(&ids, &dice) {
                    let _ = bounded(
                        "accept_fingerprint",
                        manager.accept_fingerprint(
                            id,
                            Fingerprint::from_bytes([dice.below(256) as u8; 32]),
                        ),
                    )
                    .await;
                }
            }
            9 => {
                // Changement de réseau.
                let mut addresses = net.0.lock().unwrap();
                addresses.clear();
                if dice.chance(70) {
                    addresses.insert(format!("10.0.0.{}", dice.below(250) + 1).parse().unwrap());
                }
            }
            10 => {
                // Réveil du PC (saut de l'horloge murale).
                clock
                    .jump_ms
                    .fetch_add(dice.below(7_200_000) as i64, Ordering::SeqCst);
            }
            11 => {
                if let Some(id) = pick(&ids, &dice) {
                    let _ = manager.state(id);
                    if let Ok(Some(view)) = bounded("last_known", manager.last_known(id)).await {
                        assert!(view.history.len() <= HISTORY_CAP, "dernière vue non bornée");
                    }
                }
            }
            12 if ids.len() > 1 && dice.chance(30) => {
                let id = ids.remove(dice.below(ids.len() as u64) as usize);
                let _ = bounded("remove_server", manager.remove_server(&id)).await;
            }
            _ => {}
        }
        // Le temps passe : de quelques millisecondes à une minute.
        let pause = match dice.below(10) {
            0..=5 => dice.below(300),
            6..=8 => dice.below(5_000),
            _ => dice.below(60_000),
        };
        tokio::time::sleep(Duration::from_millis(pause)).await;
    }

    for id in &ids {
        assert_eq!(
            manager.task_restarts(id).unwrap(),
            0,
            "une tâche du lien a paniqué (graine {seed})"
        );
        let _ = manager.state(id).unwrap();
        if let Some(view) = bounded("last_known", manager.last_known(id)).await.unwrap() {
            assert!(view.history.len() <= HISTORY_CAP);
        }
    }
    bounded("shutdown", manager.shutdown()).await;
    drain.abort();
    let seen = seen.lock().unwrap();
    eprintln!("graine {seed} : {seen:?}");
    // Le chaos a bien traversé les états : sinon le test ne prouverait rien.
    for state in ["Connected", "Reconnecting", "Offline"] {
        assert!(
            seen.get(state).copied().unwrap_or(0) > 0,
            "jamais « {state} » (graine {seed}) : {seen:?}"
        );
    }
}

fn pick<'a>(ids: &'a [ServerId], dice: &Dice) -> Option<&'a ServerId> {
    if ids.is_empty() {
        None
    } else {
        ids.get(dice.below(ids.len() as u64) as usize)
    }
}

#[tokio::test(start_paused = true)]
async fn random_errors_and_malformed_answers_never_panic_nor_block_seed_1() {
    run(1, 2_500).await;
}

#[tokio::test(start_paused = true)]
async fn random_errors_and_malformed_answers_never_panic_nor_block_seed_2() {
    run(2, 2_500).await;
}

#[tokio::test(start_paused = true)]
async fn random_errors_and_malformed_answers_never_panic_nor_block_seed_3() {
    run(0xDEAD_BEEF, 2_500).await;
}

#[tokio::test(start_paused = true)]
async fn a_server_that_always_fails_keeps_being_retried_with_bounded_state() {
    // Chaque tentative échoue : deux jours de coupure.
    struct Dead;
    #[async_trait]
    impl Transport for Dead {
        async fn hello(&self, _: &Target) -> Result<Probed, TransportError> {
            Err(TransportError::Timeout)
        }
        async fn login(
            &self,
            _: &Target,
            _: &LoginRequest,
        ) -> Result<LoginResponse, TransportError> {
            Err(TransportError::Timeout)
        }
        async fn logout(&self, _: &Target, _: &Secret) -> Result<(), TransportError> {
            Err(TransportError::Timeout)
        }
        async fn request(
            &self,
            _: &Target,
            _: &Secret,
            _: &ApiRequest,
        ) -> Result<ApiResponse, TransportError> {
            Err(TransportError::Timeout)
        }
        async fn operation(
            &self,
            _: &Target,
            _: &Secret,
            _: &OperationId,
        ) -> Result<OperationResponse, TransportError> {
            Err(TransportError::Timeout)
        }
        async fn open_stream(&self, _: &Target) -> Result<Box<dyn StreamConn>, TransportError> {
            Err(TransportError::Connect("mort".into()))
        }
    }
    let store = Arc::new(MemStore::default());
    let vault = Arc::new(MemoryVault::new());
    let clock = Arc::new(TokioClock::new(WallTime::from_millis(1_790_000_000_000)));
    let record = ServerRecord {
        id: ServerId::parse("srv").unwrap(),
        name: "mort".into(),
        color: "#000".into(),
        host: "dead.test".into(),
        port: 7341,
        fingerprint: Fingerprint::from_bytes([1; 32]),
        username: "marie".into(),
        remember: true,
        mac_addresses: vec![],
        last_contact_at: None,
        signed_out: false,
        role: None,
    };
    ServerStore::save(&*store, &record).await.unwrap();
    // Une session mémorisée : le client essaie de se connecter au démarrage.
    use hearth_link::ports::vault::{SecretKind, Vault as _};
    vault
        .put(&record.id, SecretKind::Token, &Secret::from("t"))
        .unwrap();
    let manager = LinkManager::start(
        Ports {
            transport: Arc::new(Dead),
            vault,
            servers: store.clone(),
            snapshots: store.clone(),
            operations: store,
            clock,
            rng: Dice::new(9),
            net: Arc::new(Net::default()),
            extra_sink: None,
        },
        LinkConfig::default(),
    )
    .await
    .unwrap();
    let mut events = manager.subscribe();
    let count = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let counted = count.clone();
    let counter = tokio::spawn(async move {
        while events.recv().await.is_some() {
            counted.fetch_add(1, Ordering::SeqCst);
        }
    });
    tokio::time::sleep(Duration::from_secs(2 * 24 * 3600)).await;
    let info = manager.state(&record.id).unwrap();
    assert_eq!(info.state, hearth_link::domain::state::LinkState::Offline);
    assert!(
        info.next_retry_at.is_some(),
        "les tentatives continuent après deux jours"
    );
    assert!(info.failed_attempts > 5_000, "{}", info.failed_attempts);
    manager.shutdown().await;
    counter.abort();
    // Pas d'avalanche d'événements : un par tentative.
    let events = count.load(Ordering::SeqCst);
    assert!(events < 8_000, "{events} événements en deux jours");
}

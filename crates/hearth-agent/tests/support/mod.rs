//! Outils communs aux tests d'intégration : base temporaire, horloge et identifiants
//! déterministes, hacheur qui compte ses appels, sessions écrites par la fonction d'écriture de
//! production (`SessionTx::insert`, donc au format de production).
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

pub mod api;
pub mod crash;
pub mod device;
pub mod https;
pub mod probe;
pub mod tmp;
pub mod update;
pub mod ws;

use async_trait::async_trait;
use hearth_agent::application::accounts::{AccountService, AccountView};
use hearth_agent::application::attack_mode::AttackModeService;
use hearth_agent::application::audit::AuditService;
use hearth_agent::application::maintenance::MaintenanceService;
use hearth_agent::application::operations::OperationService;
use hearth_agent::application::ports::{
    AuditFeed, BootInfo, Clock, HashError, IdGen, PasswordHasher, SecurityFeed, Store, TokenGen,
};
use hearth_agent::application::security::SecurityService;
use hearth_agent::application::sessions::{ClientInfo, SessionService};
use hearth_agent::application::trust::TrustService;
use hearth_agent::domain::accounts::{AccountId, PlainPassword, Role, Username};
use hearth_agent::domain::audit::{Actor, Origin};
use hearth_agent::domain::secret::Secret;
use hearth_agent::domain::session_token::SessionToken;
use hearth_agent::domain::sessions::{Session, SessionId};
use hearth_agent::domain::trust::{DeviceId, NewDevice};
use hearth_agent::infrastructure::argon2::Argon2Hasher;
use hearth_agent::infrastructure::audit_feed::BroadcastAuditFeed;
use hearth_agent::infrastructure::crypto::{HmacChallengeCrypto, RingProofVerifier};
use hearth_agent::infrastructure::random::OsTokenGen;
use hearth_agent::infrastructure::sqlite::{
    Database, SqliteAccountRepo, SqliteAttackModeRepo, SqliteAuditRepo, SqliteDeviceRepo,
    SqliteKnownAddressRepo, SqliteLoginAttemptRepo, SqliteOperationRepo, SqliteSessionRepo,
    SqliteStore,
};
use time::{Duration, OffsetDateTime};
use tmp::TestDir;

pub const PASSWORD: &str = "Correct-Horse-9";

/// L'empreinte du certificat du « serveur » des tests : celle que les preuves signent.
pub const SERVER_FINGERPRINT: [u8; 32] = [0xa5; 32];

/// Horloge monotone pilotée : les défis expirent quand le test le décide.
pub struct TestMonotonic(AtomicU64);

impl TestMonotonic {
    pub fn advance(&self, by: Duration) {
        self.0.fetch_add(
            u64::try_from(by.whole_milliseconds()).unwrap_or(0),
            Ordering::SeqCst,
        );
    }
}

impl hearth_agent::application::ports::MonotonicClock for TestMonotonic {
    fn elapsed(&self) -> Duration {
        Duration::milliseconds(i64::try_from(self.0.load(Ordering::SeqCst)).unwrap_or(0))
    }
}

/// Vérificateur de signatures qui compte ses appels : prouve que deux chemins de connexion font le
/// même travail coûteux.
pub struct CountingVerifier {
    inner: RingProofVerifier,
    pub calls: AtomicU64,
}

impl CountingVerifier {
    pub fn calls(&self) -> u64 {
        self.calls.load(Ordering::SeqCst)
    }
}

impl hearth_agent::application::ports::ProofVerifier for CountingVerifier {
    fn verify(&self, algorithm: &str, public_key: &[u8], message: &[u8], signature: &[u8]) -> bool {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner.verify(algorithm, public_key, message, signature)
    }
}

/// Ce que le « noyau » des tests dit du démarrage : un identifiant et un temps écoulé que le test
/// pilote (jamais le vrai `/proc`). Au départ : le démarrage `boot-1`, la machine tourne depuis dix
/// heures, donc aucune fenêtre de redémarrage.
pub struct TestBoot {
    id: Mutex<Option<String>>,
    uptime: Mutex<Duration>,
}

impl TestBoot {
    pub fn set_id(&self, id: Option<&str>) {
        *self.id.lock().unwrap() = id.map(str::to_owned);
    }

    pub fn set_uptime(&self, uptime: Duration) {
        *self.uptime.lock().unwrap() = uptime;
    }

    pub fn uptime(&self) -> Duration {
        *self.uptime.lock().unwrap()
    }
}

impl BootInfo for TestBoot {
    fn boot_id(&self) -> Option<String> {
        self.id.lock().unwrap().clone()
    }

    fn uptime(&self) -> Duration {
        *self.uptime.lock().unwrap()
    }
}

pub struct TestClock(Mutex<OffsetDateTime>);

impl TestClock {
    pub fn advance(&self, by: Duration) {
        if let Ok(mut now) = self.0.lock() {
            *now += by;
        }
    }
}

impl Clock for TestClock {
    fn now(&self) -> OffsetDateTime {
        self.0
            .lock()
            .map(|now| *now)
            .unwrap_or(OffsetDateTime::UNIX_EPOCH)
    }
}

pub struct SequentialIds(AtomicU64);

impl SequentialIds {
    pub fn starting_at(first: u64) -> Self {
        Self(AtomicU64::new(first))
    }
}

impl IdGen for SequentialIds {
    fn new_id(&self) -> String {
        format!("ID{:08}", self.0.fetch_add(1, Ordering::SeqCst))
    }
}

/// Hacheur de test (coût minimal) qui compte les vérifications, et parmi elles celles faites
/// contre le haché factice : prouve que deux chemins de connexion passent par le même appel.
pub struct CountingHasher {
    inner: Argon2Hasher,
    pub verifications: AtomicU64,
    pub against_decoy: AtomicU64,
    /// Attente (ms) avant chaque vérification : simule un calcul long pour couper le client
    /// pendant l'exécution.
    pub delay_ms: AtomicU64,
}

impl CountingHasher {
    pub fn new() -> Self {
        Self {
            inner: Argon2Hasher::with_cost(8, 1, 1).expect("paramètres"),
            verifications: AtomicU64::new(0),
            against_decoy: AtomicU64::new(0),
            delay_ms: AtomicU64::new(0),
        }
    }

    pub fn verifications(&self) -> u64 {
        self.verifications.load(Ordering::SeqCst)
    }

    pub fn against_decoy(&self) -> u64 {
        self.against_decoy.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl PasswordHasher for CountingHasher {
    async fn hash(&self, password: &PlainPassword) -> Result<Secret, HashError> {
        self.inner.hash(password).await
    }

    async fn verify(&self, password: &Secret, hash: &Secret) -> Result<bool, HashError> {
        self.verifications.fetch_add(1, Ordering::SeqCst);
        let delay = self.delay_ms.load(Ordering::SeqCst);
        if delay > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
        }
        if hash.expose() == self.inner.decoy_hash().expose() {
            self.against_decoy.fetch_add(1, Ordering::SeqCst);
        }
        self.inner.verify(password, hash).await
    }

    fn decoy_hash(&self) -> &Secret {
        self.inner.decoy_hash()
    }
}

pub struct Env {
    pub dir: TestDir,
    pub db: Database,
    pub clock: Arc<TestClock>,
    pub hasher: Arc<CountingHasher>,
    pub service: Arc<AccountService>,
    pub sessions: Arc<SessionService>,
    pub operations: Arc<OperationService>,
    pub maintenance: Arc<MaintenanceService>,
    pub audit: Arc<AuditService>,
    pub audit_sink: Arc<dyn hearth_agent::application::ports::AuditSink>,
    pub audit_recorder: Arc<hearth_agent::application::audit::AuditRecorder>,
    pub feed: Arc<BroadcastAuditFeed>,
    pub trail: Arc<hearth_agent::application::audit::AuditTrail>,
    /// L'identité d'appareil, branchée comme en production sur `sessions`.
    pub trust: Arc<TrustService>,
    /// L'alerte, branchée comme en production sur `sessions` (HRT-24).
    pub security: Arc<SecurityService>,
    pub monotonic: Arc<TestMonotonic>,
    /// L'élévation du mot de passe en administration, branchée comme en production (HRT-28).
    pub elevations: Arc<hearth_agent::application::elevation::Elevations>,
    pub verifier: Arc<CountingVerifier>,
    /// Le mode attaque, branché comme en production sur `sessions` et `security` (HRT-25) ; éteint
    /// tant qu'un test ne l'allume pas.
    pub attack: Arc<AttackModeService>,
    /// Le démarrage du « noyau » des tests.
    pub boot: Arc<TestBoot>,
}

pub const CLIENT_ADDR: &str = "10.0.0.7";

pub fn client() -> ClientInfo {
    client_at(CLIENT_ADDR)
}

pub fn client_at(addr: &str) -> ClientInfo {
    ClientInfo {
        name: "poste/1.0".into(),
        addr: addr.into(),
    }
}

/// Qui demande, dans les tests des cas d'usage : un administrateur depuis un poste du réseau.
pub fn by() -> &'static Actor {
    static BY: std::sync::OnceLock<Actor> = std::sync::OnceLock::new();
    BY.get_or_init(|| {
        Actor::new(
            Some(Username::parse("root").unwrap()),
            Origin::client(Some("poste/1.0"), CLIENT_ADDR),
        )
    })
}

pub fn start_time() -> OffsetDateTime {
    OffsetDateTime::UNIX_EPOCH + Duration::seconds(1_790_000_000)
}

pub async fn env() -> Env {
    let dir = tmp::tempdir().expect("dossier temporaire");
    let db = Database::open(dir.path()).await.expect("base");
    let clock = Arc::new(TestClock(Mutex::new(start_time())));
    let hasher = Arc::new(CountingHasher::new());
    let ids = Arc::new(SequentialIds(AtomicU64::new(1)));
    let store: Arc<dyn Store> = Arc::new(SqliteStore::new(db.pool().clone()));
    let accounts = Arc::new(SqliteAccountRepo::new(db.pool().clone()));
    let session_repo = Arc::new(SqliteSessionRepo::new(db.pool().clone()));
    let tokens: Arc<dyn TokenGen> = Arc::new(OsTokenGen);
    let feed = Arc::new(BroadcastAuditFeed::new());
    let maintenance = Arc::new(MaintenanceService::new(store.clone(), clock.clone()));
    let trail = Arc::new(hearth_agent::application::audit::AuditTrail::new(
        feed.clone() as Arc<dyn AuditFeed>,
        maintenance.clone(),
    ));
    let audit_recorder = Arc::new(hearth_agent::application::audit::AuditRecorder::new(
        store.clone(),
        clock.clone(),
        trail.clone(),
    ));
    let audit_sink: Arc<dyn hearth_agent::application::ports::AuditSink> = audit_recorder.clone();
    let monotonic = Arc::new(TestMonotonic(AtomicU64::new(1_000)));
    // L'élévation du mot de passe en administration (HRT-28), branchée comme en production.
    let elevations = Arc::new(hearth_agent::application::elevation::Elevations::new(
        monotonic.clone(),
    ));
    let service = Arc::new(
        AccountService::new(
            accounts.clone(),
            session_repo.clone(),
            store.clone(),
            hasher.clone(),
            clock.clone(),
            ids.clone(),
            trail.clone(),
        )
        .with_elevations(elevations.clone()),
    );
    let verifier = Arc::new(CountingVerifier {
        inner: RingProofVerifier,
        calls: AtomicU64::new(0),
    });
    let trust = Arc::new(TrustService::new(
        Arc::new(SqliteDeviceRepo::new(db.pool().clone())),
        store.clone(),
        verifier.clone(),
        Arc::new(HmacChallengeCrypto::new().expect("hasard")),
        monotonic.clone(),
        clock.clone(),
        ids.clone(),
        hearth_proto::fingerprint::Fingerprint::from_bytes(SERVER_FINGERPRINT),
        trail.clone(),
    ));
    let security_feed: Arc<dyn SecurityFeed> =
        Arc::new(hearth_agent::infrastructure::security_feed::BroadcastSecurityFeed::new());
    let boot = Arc::new(TestBoot {
        id: Mutex::new(Some("boot-1".to_owned())),
        uptime: Mutex::new(Duration::hours(10)),
    });
    let attack = Arc::new(AttackModeService::new(
        Arc::new(SqliteAttackModeRepo::new(db.pool().clone())),
        store.clone(),
        clock.clone(),
        monotonic.clone(),
        boot.clone(),
        ids.clone(),
        trail.clone(),
        security_feed.clone(),
    ));
    let security = Arc::new(
        SecurityService::new(
            Arc::new(SqliteLoginAttemptRepo::new(db.pool().clone())),
            accounts.clone(),
            Arc::new(SqliteDeviceRepo::new(db.pool().clone())),
            store.clone(),
            clock.clone(),
            trail.clone(),
            security_feed,
        )
        .with_attack(attack.clone()),
    );
    let sessions = Arc::new(
        SessionService::new(
            accounts,
            session_repo,
            Arc::new(SqliteLoginAttemptRepo::new(db.pool().clone())),
            Arc::new(SqliteKnownAddressRepo::new(db.pool().clone())),
            store.clone(),
            hasher.clone(),
            clock.clone(),
            ids,
            tokens,
            trail.clone(),
            audit_sink.clone(),
        )
        .with_trust(trust.clone())
        .with_security(security.clone())
        .with_attack(attack.clone())
        .with_elevations(elevations.clone()),
    );
    // Les bancs d'essai d'avant HRT-30 envoient des actes bruts : régime « accepte », demandé par son nom.
    // `admin_reauth.rs` rétablit l'exigence (`accept_unconfirmed_acts_for_tests(false)`).
    sessions.accept_unconfirmed_acts_for_tests(true);
    let operations = Arc::new(OperationService::new(
        Arc::new(SqliteOperationRepo::new(db.pool().clone())),
        store.clone(),
        clock.clone(),
    ));
    let audit = Arc::new(AuditService::new(
        Arc::new(SqliteAuditRepo::new(db.pool().clone())),
        feed.clone(),
    ));
    Env {
        dir,
        db,
        clock,
        hasher,
        service,
        sessions,
        operations,
        maintenance,
        audit,
        audit_sink,
        audit_recorder,
        feed,
        trail,
        trust,
        security,
        monotonic,
        elevations,
        verifier,
        attack,
        boot,
    }
}

/// Un service des sessions SANS identité d'appareil : le comportement d'un agent qui n'a pas la
/// fonction (sous-commandes `account`), et celui de tous les tests d'avant HRT-22.
pub fn plain_sessions(env: &Env) -> SessionService {
    let pool = env.db.pool().clone();
    let sessions = SessionService::new(
        Arc::new(SqliteAccountRepo::new(pool.clone())),
        Arc::new(SqliteSessionRepo::new(pool.clone())),
        Arc::new(SqliteLoginAttemptRepo::new(pool.clone())),
        Arc::new(SqliteKnownAddressRepo::new(pool.clone())),
        Arc::new(SqliteStore::new(pool)),
        env.hasher.clone(),
        env.clock.clone(),
        Arc::new(SequentialIds::starting_at(10_000)),
        Arc::new(OsTokenGen),
        env.trail.clone(),
        env.audit_sink.clone(),
    );
    sessions.accept_unconfirmed_acts_for_tests(true);
    sessions
}

pub fn secret(value: &str) -> Secret {
    Secret::from(value)
}

impl Env {
    pub async fn create(&self, username: &str, role: Role) -> AccountView {
        self.service
            .create(username, secret(PASSWORD), role, by())
            .await
            .expect("création")
    }

    /// Écrit une session par la fonction d'écriture de production (`SessionTx::insert`).
    pub async fn insert_session(&self, account: &AccountId, id: &str, expires_in: Duration) {
        let now = self.clock.now();
        let mut bytes = [0_u8; 32];
        for (slot, byte) in bytes.iter_mut().zip(id.bytes().cycle()) {
            *slot = byte;
        }
        let session = Session {
            id: SessionId::new(id),
            account: account.clone(),
            token_hash: SessionToken::from_bytes(bytes).hash(),
            client_name: "test".into(),
            client_addr: "127.0.0.1".into(),
            created_at: now,
            last_seen_at: now,
            expires_at: now + expires_in,
        };
        let store = SqliteStore::new(self.db.pool().clone());
        let mut tx = store.begin().await.expect("unité de travail");
        tx.sessions().insert(&session).await.expect("session");
        tx.commit().await.expect("validation");
    }

    /// Inscrit un poste de confiance par la fonction d'écriture de production (`DeviceTx::insert`).
    pub async fn insert_device(&self, account: &AccountId, id: &str, key_id: &str, name: &str) {
        let store = SqliteStore::new(self.db.pool().clone());
        let mut tx = store.begin().await.expect("unité de travail");
        tx.devices()
            .insert(&NewDevice {
                id: DeviceId::new(id),
                account: account.clone(),
                key_id: key_id.to_owned(),
                public_key: [7; 32],
                name: name.to_owned(),
                now: self.clock.now(),
                addr: "10.0.0.7".to_owned(),
            })
            .await
            .expect("poste");
        tx.commit().await.expect("validation");
    }

    pub async fn hash_of(&self, account: &AccountId) -> String {
        sqlx::query_scalar("SELECT password_hash FROM accounts WHERE id = ?")
            .bind(account.as_str())
            .fetch_one(self.db.pool())
            .await
            .expect("haché")
    }

    pub async fn session_ids(&self, account: &AccountId) -> Vec<String> {
        sqlx::query_scalar("SELECT id FROM sessions WHERE account_id = ? ORDER BY id")
            .bind(account.as_str())
            .fetch_all(self.db.pool())
            .await
            .expect("sessions")
    }
}

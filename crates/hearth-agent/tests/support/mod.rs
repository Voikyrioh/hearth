//! Outils communs aux tests d'intégration : base temporaire, horloge et identifiants
//! déterministes, hacheur qui compte ses appels, sessions écrites par la fonction d'écriture de
//! production (`SessionTx::insert`, donc au format de production).
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

pub mod api;
pub mod https;
pub mod probe;
pub mod ws;

use async_trait::async_trait;
use hearth_agent::application::accounts::{AccountService, AccountView};
use hearth_agent::application::audit::AuditService;
use hearth_agent::application::maintenance::MaintenanceService;
use hearth_agent::application::operations::OperationService;
use hearth_agent::application::ports::{
    AuditFeed, Clock, HashError, IdGen, PasswordHasher, Store, TokenGen,
};
use hearth_agent::application::sessions::{ClientInfo, SessionService};
use hearth_agent::domain::accounts::{AccountId, PlainPassword, Role, Username};
use hearth_agent::domain::audit::{Actor, Origin};
use hearth_agent::domain::secret::Secret;
use hearth_agent::domain::session_token::SessionToken;
use hearth_agent::domain::sessions::{Session, SessionId};
use hearth_agent::infrastructure::argon2::Argon2Hasher;
use hearth_agent::infrastructure::audit_feed::BroadcastAuditFeed;
use hearth_agent::infrastructure::random::OsTokenGen;
use hearth_agent::infrastructure::sqlite::{
    Database, SqliteAccountRepo, SqliteAuditRepo, SqliteLoginAttemptRepo, SqliteOperationRepo,
    SqliteSessionRepo, SqliteStore,
};
use tempfile::TempDir;
use time::{Duration, OffsetDateTime};

pub const PASSWORD: &str = "Correct-Horse-9";

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
    pub dir: TempDir,
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
    let dir = tempfile::tempdir().expect("dossier temporaire");
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
    let service = Arc::new(AccountService::new(
        accounts.clone(),
        session_repo.clone(),
        store.clone(),
        hasher.clone(),
        clock.clone(),
        ids.clone(),
        trail.clone(),
    ));
    let sessions = Arc::new(SessionService::new(
        accounts,
        session_repo,
        Arc::new(SqliteLoginAttemptRepo::new(db.pool().clone())),
        store.clone(),
        hasher.clone(),
        clock.clone(),
        ids,
        tokens,
        trail.clone(),
    ));
    let operations = Arc::new(OperationService::new(
        Arc::new(SqliteOperationRepo::new(db.pool().clone())),
        store.clone(),
        clock.clone(),
    ));
    let audit = Arc::new(AuditService::new(
        Arc::new(SqliteAuditRepo::new(db.pool().clone())),
        feed.clone(),
    ));
    let audit_recorder = Arc::new(hearth_agent::application::audit::AuditRecorder::new(
        store,
        clock.clone(),
        trail.clone(),
    ));
    let audit_sink: Arc<dyn hearth_agent::application::ports::AuditSink> = audit_recorder.clone();
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
    }
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

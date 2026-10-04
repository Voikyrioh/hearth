//! Outils communs aux tests d'intégration : base temporaire, horloge et identifiants
//! déterministes, hacheur qui compte ses appels, sessions écrites par la fonction d'écriture de
//! production (`SessionTx::insert`, donc au format de production).
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

pub mod api;
pub mod https;

use async_trait::async_trait;
use hearth_agent::application::accounts::{AccountService, AccountView};
use hearth_agent::application::maintenance::MaintenanceService;
use hearth_agent::application::operations::OperationService;
use hearth_agent::application::ports::{Clock, HashError, IdGen, PasswordHasher, Store, TokenGen};
use hearth_agent::application::sessions::{ClientInfo, SessionService};
use hearth_agent::domain::accounts::{AccountId, PlainPassword, Role};
use hearth_agent::domain::secret::Secret;
use hearth_agent::domain::session_token::SessionToken;
use hearth_agent::domain::sessions::{Session, SessionId};
use hearth_agent::infrastructure::argon2::Argon2Hasher;
use hearth_agent::infrastructure::random::OsTokenGen;
use hearth_agent::infrastructure::sqlite::{
    Database, SqliteAccountRepo, SqliteLoginAttemptRepo, SqliteOperationRepo, SqliteSessionRepo,
    SqliteStore,
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
}

impl CountingHasher {
    pub fn new() -> Self {
        Self {
            inner: Argon2Hasher::with_cost(8, 1, 1).expect("paramètres"),
            verifications: AtomicU64::new(0),
            against_decoy: AtomicU64::new(0),
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
    let service = Arc::new(AccountService::new(
        accounts.clone(),
        session_repo.clone(),
        store.clone(),
        hasher.clone(),
        clock.clone(),
        ids.clone(),
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
    ));
    let operations = Arc::new(OperationService::new(
        Arc::new(SqliteOperationRepo::new(db.pool().clone())),
        store.clone(),
        clock.clone(),
    ));
    let maintenance = Arc::new(MaintenanceService::new(store, clock.clone()));
    Env {
        dir,
        db,
        clock,
        hasher,
        service,
        sessions,
        operations,
        maintenance,
    }
}

pub fn secret(value: &str) -> Secret {
    Secret::from(value)
}

impl Env {
    pub async fn create(&self, username: &str, role: Role) -> AccountView {
        self.service
            .create(username, secret(PASSWORD), role)
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

//! Outils communs aux tests d'intégration des comptes : base temporaire, horloge et
//! identifiants déterministes, sessions insérées à la main (leur création arrive avec HRT-04).
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use hearth_agent::application::accounts::AccountService;
use hearth_agent::application::ports::{Clock, IdGen};
use hearth_agent::domain::accounts::{Account, AccountId, Role};
use hearth_agent::domain::secret::Secret;
use hearth_agent::infrastructure::argon2::Argon2Hasher;
use hearth_agent::infrastructure::sqlite::{Database, SqliteAccountRepo, SqliteSessionRepo};
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

impl IdGen for SequentialIds {
    fn new_id(&self) -> String {
        format!("ID{:08}", self.0.fetch_add(1, Ordering::SeqCst))
    }
}

pub struct Env {
    pub dir: TempDir,
    pub db: Database,
    pub clock: Arc<TestClock>,
    pub service: Arc<AccountService>,
}

pub fn start_time() -> OffsetDateTime {
    OffsetDateTime::UNIX_EPOCH + Duration::seconds(1_790_000_000)
}

pub async fn env() -> Env {
    let dir = tempfile::tempdir().expect("dossier temporaire");
    let db = Database::open(dir.path()).await.expect("base");
    let clock = Arc::new(TestClock(Mutex::new(start_time())));
    let service = Arc::new(AccountService::new(
        Arc::new(SqliteAccountRepo::new(db.pool().clone())),
        Arc::new(SqliteSessionRepo::new(db.pool().clone())),
        Arc::new(Argon2Hasher::with_cost(8, 1, 1).expect("paramètres")),
        clock.clone(),
        Arc::new(SequentialIds(AtomicU64::new(1))),
    ));
    Env {
        dir,
        db,
        clock,
        service,
    }
}

pub fn secret(value: &str) -> Secret {
    Secret::from(value)
}

impl Env {
    pub async fn create(&self, username: &str, role: Role) -> Account {
        self.service
            .create(username, secret(PASSWORD), role)
            .await
            .expect("création")
    }

    pub async fn insert_session(&self, account: &AccountId, id: &str, expires_in: Duration) {
        let now = self.clock.now();
        let format = &time::format_description::well_known::Rfc3339;
        let created = now.format(format).expect("date");
        let expires = (now + expires_in).format(format).expect("date");
        sqlx::query(
            "INSERT INTO sessions (id, account_id, token_hash, client_name, client_addr, \
             created_at, last_seen_at, expires_at) VALUES (?, ?, ?, 'test', '127.0.0.1', ?, ?, ?)",
        )
        .bind(id)
        .bind(account.as_str())
        .bind(format!("hash-{id}"))
        .bind(&created)
        .bind(&created)
        .bind(&expires)
        .execute(self.db.pool())
        .await
        .expect("session");
    }

    pub async fn session_ids(&self, account: &AccountId) -> Vec<String> {
        sqlx::query_scalar("SELECT id FROM sessions WHERE account_id = ? ORDER BY id")
            .bind(account.as_str())
            .fetch_all(self.db.pool())
            .await
            .expect("sessions")
    }
}

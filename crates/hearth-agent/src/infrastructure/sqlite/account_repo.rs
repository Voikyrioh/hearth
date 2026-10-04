//! Comptes en SQLite : les lectures sur le pool (`AccountRepo`) et les requêtes de lecture
//! partagées avec l'unité de travail. Les écritures vivent dans `store.rs` (transaction
//! `BEGIN IMMEDIATE`), pas ici.

use async_trait::async_trait;
use sqlx::{SqliteConnection, SqlitePool};

use super::convert::{parse_date, storage};
use crate::application::ports::{AccountRepo, StoreError};
use crate::domain::accounts::{Account, AccountId, Role, Username};
use crate::domain::secret::Secret;

pub(super) const RESOURCE: &str = "accounts";

pub struct SqliteAccountRepo {
    pool: SqlitePool,
}

impl SqliteAccountRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

struct AccountRow {
    id: String,
    username: String,
    password_hash: String,
    role: String,
    created_at: String,
    password_changed_at: String,
    last_login_at: Option<String>,
}

fn into_account(row: AccountRow) -> Result<Account, StoreError> {
    let corrupt = |what: &str| StoreError::Unavailable {
        resource: RESOURCE,
        message: format!("{what} illisible pour le compte {}", row.id),
    };
    Ok(Account {
        username: Username::parse(&row.username).map_err(|_| corrupt("identifiant"))?,
        role: row.role.parse::<Role>().map_err(|_| corrupt("rôle"))?,
        created_at: parse_date(RESOURCE, &row.created_at)?,
        password_changed_at: parse_date(RESOURCE, &row.password_changed_at)?,
        last_login_at: row
            .last_login_at
            .as_deref()
            .map(|value| parse_date(RESOURCE, value))
            .transpose()?,
        password_hash: Secret::new(row.password_hash),
        id: AccountId::new(row.id),
    })
}

pub(super) async fn find_by_id(
    conn: &mut SqliteConnection,
    id: &AccountId,
) -> Result<Option<Account>, StoreError> {
    sqlx::query_as!(
        AccountRow,
        "SELECT id, username, password_hash, role, created_at, password_changed_at, last_login_at
         FROM accounts WHERE id = ?",
        id.as_str()
    )
    .fetch_optional(&mut *conn)
    .await
    .map_err(storage(RESOURCE))?
    .map(into_account)
    .transpose()
}

pub(super) async fn find_by_username(
    conn: &mut SqliteConnection,
    username: &Username,
) -> Result<Option<Account>, StoreError> {
    sqlx::query_as!(
        AccountRow,
        "SELECT id, username, password_hash, role, created_at, password_changed_at, last_login_at
         FROM accounts WHERE username = ?",
        username.as_str()
    )
    .fetch_optional(&mut *conn)
    .await
    .map_err(storage(RESOURCE))?
    .map(into_account)
    .transpose()
}

#[async_trait]
impl AccountRepo for SqliteAccountRepo {
    async fn find_by_id(&self, id: &AccountId) -> Result<Option<Account>, StoreError> {
        let mut conn = self.pool.acquire().await.map_err(storage(RESOURCE))?;
        find_by_id(&mut conn, id).await
    }

    async fn find_by_username(&self, username: &Username) -> Result<Option<Account>, StoreError> {
        let mut conn = self.pool.acquire().await.map_err(storage(RESOURCE))?;
        find_by_username(&mut conn, username).await
    }

    async fn list(&self) -> Result<Vec<Account>, StoreError> {
        let mut conn = self.pool.acquire().await.map_err(storage(RESOURCE))?;
        sqlx::query_as!(
            AccountRow,
            "SELECT id, username, password_hash, role, created_at, password_changed_at, last_login_at
             FROM accounts ORDER BY created_at, id"
        )
        .fetch_all(&mut *conn)
        .await
        .map_err(storage(RESOURCE))?
        .into_iter()
        .map(into_account)
        .collect()
    }
}

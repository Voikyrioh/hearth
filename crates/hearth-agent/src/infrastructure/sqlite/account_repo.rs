//! Comptes en SQLite : lectures sur le pool, écritures dans une transaction `BEGIN IMMEDIATE`
//! (un seul écrivain à la fois : ce que la transaction a observé ne change plus avant sa
//! validation, ce qui rend atomique la garde du dernier administrateur).

use async_trait::async_trait;
use sqlx::{Sqlite, SqliteConnection, SqlitePool, Transaction};
use time::OffsetDateTime;

use super::convert::{format_date, is_unique_violation, parse_date, storage};
use super::session_repo::close_sessions;
use crate::application::ports::{AccountRepo, AccountTransaction, StoreError};
use crate::domain::accounts::{Account, AccountId, Role, Username};
use crate::domain::secret::Secret;
use crate::domain::sessions::SessionClosure;

const RESOURCE: &str = "accounts";

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

async fn find_by_id(
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

async fn find_by_username(
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

    async fn begin(&self) -> Result<Box<dyn AccountTransaction>, StoreError> {
        let tx = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(storage(RESOURCE))?;
        Ok(Box::new(SqliteAccountTransaction { tx }))
    }
}

/// Transaction d'écriture. Abandonnée sans `commit`, SQLx l'annule.
struct SqliteAccountTransaction {
    tx: Transaction<'static, Sqlite>,
}

#[async_trait]
impl AccountTransaction for SqliteAccountTransaction {
    async fn find(&mut self, id: &AccountId) -> Result<Option<Account>, StoreError> {
        find_by_id(&mut self.tx, id).await
    }

    async fn find_by_username(
        &mut self,
        username: &Username,
    ) -> Result<Option<Account>, StoreError> {
        find_by_username(&mut self.tx, username).await
    }

    async fn count_admins(&mut self) -> Result<u64, StoreError> {
        let count = sqlx::query_scalar!(
            r#"SELECT COUNT(*) AS "count!: i64" FROM accounts WHERE role = ?"#,
            "admin"
        )
        .fetch_one(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(u64::try_from(count).unwrap_or(0))
    }

    async fn insert(&mut self, account: &Account) -> Result<(), StoreError> {
        let created_at = format_date(RESOURCE, account.created_at)?;
        let changed_at = format_date(RESOURCE, account.password_changed_at)?;
        let last_login_at = account
            .last_login_at
            .map(|date| format_date(RESOURCE, date))
            .transpose()?;
        sqlx::query!(
            "INSERT INTO accounts (id, username, password_hash, role, created_at, password_changed_at, last_login_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
            account.id.as_str(),
            account.username.as_str(),
            account.password_hash.expose(),
            account.role.as_str(),
            created_at,
            changed_at,
            last_login_at
        )
        .execute(&mut *self.tx)
        .await
        .map_err(|error| {
            if is_unique_violation(&error) {
                StoreError::Duplicate { resource: RESOURCE }
            } else {
                storage(RESOURCE)(error)
            }
        })?;
        Ok(())
    }

    async fn set_role(&mut self, id: &AccountId, role: Role) -> Result<(), StoreError> {
        sqlx::query!(
            "UPDATE accounts SET role = ? WHERE id = ?",
            role.as_str(),
            id.as_str()
        )
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(())
    }

    async fn set_password(
        &mut self,
        id: &AccountId,
        hash: &Secret,
        changed_at: OffsetDateTime,
    ) -> Result<(), StoreError> {
        let changed_at = format_date(RESOURCE, changed_at)?;
        sqlx::query!(
            "UPDATE accounts SET password_hash = ?, password_changed_at = ? WHERE id = ?",
            hash.expose(),
            changed_at,
            id.as_str()
        )
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(())
    }

    async fn delete(&mut self, id: &AccountId) -> Result<(), StoreError> {
        sqlx::query!("DELETE FROM accounts WHERE id = ?", id.as_str())
            .execute(&mut *self.tx)
            .await
            .map_err(storage(RESOURCE))?;
        Ok(())
    }

    async fn close_sessions(
        &mut self,
        account: &AccountId,
        closure: &SessionClosure,
    ) -> Result<u64, StoreError> {
        close_sessions(&mut self.tx, account, closure).await
    }

    async fn commit(self: Box<Self>) -> Result<(), StoreError> {
        self.tx.commit().await.map_err(storage(RESOURCE))
    }
}

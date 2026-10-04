//! Magasin SQLite : unités de travail (écritures) sur comptes et sessions, dans une seule
//! transaction `BEGIN IMMEDIATE` (un seul écrivain à la fois : ce que l'unité a observé ne
//! change plus avant sa validation, ce qui rend atomique la garde du dernier administrateur).

use async_trait::async_trait;
use sqlx::{Sqlite, SqlitePool, Transaction};
use time::OffsetDateTime;

use super::account_repo::{RESOURCE, find_by_id, find_by_username};

use super::convert::{format_date, is_unique_violation, storage};
use super::session_repo::close_sessions;
use crate::application::ports::{Store, StoreError, UnitOfWork};
use crate::domain::accounts::{Account, AccountId, Role, Username};
use crate::domain::secret::Secret;
use crate::domain::sessions::SessionClosure;

/// Ressource nommée par les erreurs d'ouverture et de validation d'une unité de travail : le
/// magasin lui-même, pas un sujet (comptes, sessions…).
const STORE: &str = "store";

pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl Store for SqliteStore {
    async fn begin(&self) -> Result<Box<dyn UnitOfWork>, StoreError> {
        let tx = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(storage(STORE))?;
        Ok(Box::new(SqliteUnitOfWork { tx }))
    }
}

/// Unité de travail SQLite. Abandonnée sans `commit`, SQLx l'annule.
pub struct SqliteUnitOfWork {
    tx: Transaction<'static, Sqlite>,
}

#[async_trait]
impl UnitOfWork for SqliteUnitOfWork {
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
            Role::Admin.as_str()
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
        self.tx.commit().await.map_err(storage(STORE))
    }
}

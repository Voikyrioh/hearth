//! Compteurs de tentatives de connexion en SQLite (verrouillage progressif, BR-CONN-006).

use async_trait::async_trait;
use sqlx::{SqliteConnection, SqlitePool};
use time::OffsetDateTime;

use super::convert::{format_date, parse_date, storage};
use super::store::SqliteUnitOfWork;
use crate::application::ports::{LoginAttemptRepo, LoginAttemptTx, StoreError};
use crate::domain::lockout::{AttemptKey, LockoutState};

const RESOURCE: &str = "login_attempts";

pub struct SqliteLoginAttemptRepo {
    pool: SqlitePool,
}

impl SqliteLoginAttemptRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

struct AttemptRow {
    failures: i64,
    locked_until: Option<String>,
}

async fn get(conn: &mut SqliteConnection, key: &AttemptKey) -> Result<LockoutState, StoreError> {
    let row = sqlx::query_as!(
        AttemptRow,
        "SELECT failures, locked_until FROM login_attempts WHERE key = ?",
        key.as_str()
    )
    .fetch_optional(&mut *conn)
    .await
    .map_err(storage(RESOURCE))?;
    let Some(row) = row else {
        return Ok(LockoutState::default());
    };
    Ok(LockoutState {
        failures: u32::try_from(row.failures).unwrap_or(u32::MAX),
        locked_until: row
            .locked_until
            .as_deref()
            .map(|value| parse_date(RESOURCE, value))
            .transpose()?,
    })
}

#[async_trait]
impl LoginAttemptRepo for SqliteLoginAttemptRepo {
    async fn get(&self, key: &AttemptKey) -> Result<LockoutState, StoreError> {
        let mut conn = self.pool.acquire().await.map_err(storage(RESOURCE))?;
        get(&mut conn, key).await
    }
}

#[async_trait]
impl LoginAttemptTx for SqliteUnitOfWork {
    async fn get(&mut self, key: &AttemptKey) -> Result<LockoutState, StoreError> {
        get(&mut self.tx, key).await
    }

    async fn save(
        &mut self,
        key: &AttemptKey,
        state: &LockoutState,
        at: OffsetDateTime,
    ) -> Result<(), StoreError> {
        let failures = i64::from(state.failures);
        let locked_until = state
            .locked_until
            .map(|date| format_date(RESOURCE, date))
            .transpose()?;
        let updated_at = format_date(RESOURCE, at)?;
        sqlx::query!(
            "INSERT INTO login_attempts (key, failures, locked_until, updated_at) VALUES (?, ?, ?, ?)
             ON CONFLICT (key) DO UPDATE SET failures = excluded.failures,
                 locked_until = excluded.locked_until, updated_at = excluded.updated_at",
            key.as_str(),
            failures,
            locked_until,
            updated_at
        )
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(())
    }

    async fn purge_inactive(
        &mut self,
        before: OffsetDateTime,
        now: OffsetDateTime,
    ) -> Result<u64, StoreError> {
        let before = format_date(RESOURCE, before)?;
        let now = format_date(RESOURCE, now)?;
        let result = sqlx::query!(
            "DELETE FROM login_attempts WHERE updated_at < ? AND (locked_until IS NULL OR locked_until <= ?)",
            before,
            now
        )
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(result.rows_affected())
    }
}

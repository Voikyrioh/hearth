//! Compteurs de tentatives de connexion en SQLite (verrouillage progressif, BR-CONN-006),
//! et ralentissement par identifiant (ADR-0022).

use async_trait::async_trait;
use sqlx::{SqliteConnection, SqlitePool};
use time::OffsetDateTime;

use super::convert::{format_date, parse_date, storage};
use super::store::SqliteUnitOfWork;
use crate::application::ports::{LoginAttemptRepo, LoginAttemptTx, StoreError};
use crate::domain::identifier_slowdown::{self, Slowdown};
use crate::domain::lockout::{AttemptKey, LockoutState, MAX_TRACKED_ATTEMPTS, attempts_excess};

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
    window_started_at: Option<String>,
}

async fn get(conn: &mut SqliteConnection, key: &AttemptKey) -> Result<LockoutState, StoreError> {
    let row = sqlx::query_as!(
        AttemptRow,
        "SELECT failures, locked_until, window_started_at FROM login_attempts WHERE key = ?",
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
        window_started_at: row
            .window_started_at
            .as_deref()
            .map(|value| parse_date(RESOURCE, value))
            .transpose()?,
    })
}

struct SlowdownRow {
    failures: i64,
    wait_until: Option<String>,
    last_failure_at: String,
    alerted_at: Option<String>,
}

fn slowdown_of(row: SlowdownRow) -> Result<Slowdown, StoreError> {
    Ok(Slowdown {
        failures: u32::try_from(row.failures).unwrap_or(u32::MAX),
        wait_until: row
            .wait_until
            .as_deref()
            .map(|value| parse_date(RESOURCE, value))
            .transpose()?,
        last_failure_at: Some(parse_date(RESOURCE, &row.last_failure_at)?),
        alerted_at: row
            .alerted_at
            .as_deref()
            .map(|value| parse_date(RESOURCE, value))
            .transpose()?,
    })
}

/// Les identifiants à regarder pour l'alerte : plus de `FREE_FAILURES` échecs, ou un épisode noté.
async fn alerting(conn: &mut SqliteConnection) -> Result<Vec<(String, Slowdown)>, StoreError> {
    let free = i64::from(identifier_slowdown::FREE_FAILURES);
    let rows = sqlx::query!(
        "SELECT key, failures, wait_until, last_failure_at, alerted_at
         FROM identifier_slowdowns WHERE failures > ? OR alerted_at IS NOT NULL",
        free
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(storage(RESOURCE))?;
    rows.into_iter()
        .map(|row| {
            let slowdown = slowdown_of(SlowdownRow {
                failures: row.failures,
                wait_until: row.wait_until,
                last_failure_at: row.last_failure_at,
                alerted_at: row.alerted_at,
            })?;
            Ok((row.key, slowdown))
        })
        .collect()
}

async fn identifier(conn: &mut SqliteConnection, key: &AttemptKey) -> Result<Slowdown, StoreError> {
    let row = sqlx::query_as!(
        SlowdownRow,
        "SELECT failures, wait_until, last_failure_at, alerted_at FROM identifier_slowdowns WHERE key = ?",
        key.as_str()
    )
    .fetch_optional(&mut *conn)
    .await
    .map_err(storage(RESOURCE))?;
    let Some(row) = row else {
        return Ok(Slowdown::default());
    };
    slowdown_of(row)
}

#[async_trait]
impl LoginAttemptRepo for SqliteLoginAttemptRepo {
    async fn alerting(&self) -> Result<Vec<(String, Slowdown)>, StoreError> {
        let mut conn = self.pool.acquire().await.map_err(storage(RESOURCE))?;
        alerting(&mut conn).await
    }

    async fn get(&self, key: &AttemptKey) -> Result<LockoutState, StoreError> {
        let mut conn = self.pool.acquire().await.map_err(storage(RESOURCE))?;
        get(&mut conn, key).await
    }

    async fn identifier(&self, key: &AttemptKey) -> Result<Slowdown, StoreError> {
        let mut conn = self.pool.acquire().await.map_err(storage(RESOURCE))?;
        identifier(&mut conn, key).await
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
        let window_started_at = state
            .window_started_at
            .map(|date| format_date(RESOURCE, date))
            .transpose()?;
        let updated_at = format_date(RESOURCE, at)?;
        sqlx::query!(
            "INSERT INTO login_attempts (key, failures, locked_until, updated_at, window_started_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT (key) DO UPDATE SET failures = excluded.failures,
                 locked_until = excluded.locked_until, updated_at = excluded.updated_at,
                 window_started_at = excluded.window_started_at",
            key.as_str(),
            failures,
            locked_until,
            updated_at,
            window_started_at
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

    async fn trim(&mut self, now: OffsetDateTime) -> Result<u64, StoreError> {
        let count = sqlx::query_scalar!("SELECT COUNT(*) FROM login_attempts")
            .fetch_one(&mut *self.tx)
            .await
            .map_err(storage(RESOURCE))?;
        let excess = attempts_excess(usize::try_from(count).unwrap_or(MAX_TRACKED_ATTEMPTS));
        if excess == 0 {
            return Ok(0);
        }
        let limit = i64::try_from(excess).unwrap_or(i64::MAX);
        let now = format_date(RESOURCE, now)?;
        // Même ordre que `domain::eviction::rank` : sans attente en cours d'abord, puis moins
        // d'échecs, puis les plus anciennes.
        let result = sqlx::query!(
            "DELETE FROM login_attempts WHERE key IN (
                 SELECT key FROM login_attempts
                 ORDER BY (locked_until IS NOT NULL AND locked_until > ?) ASC,
                          failures ASC, updated_at ASC
                 LIMIT ?)",
            now,
            limit
        )
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(result.rows_affected())
    }

    async fn identifier(&mut self, key: &AttemptKey) -> Result<Slowdown, StoreError> {
        identifier(&mut self.tx, key).await
    }

    async fn save_identifier(
        &mut self,
        key: &AttemptKey,
        state: &Slowdown,
    ) -> Result<(), StoreError> {
        let failures = i64::from(state.failures);
        let wait_until = state
            .wait_until
            .map(|date| format_date(RESOURCE, date))
            .transpose()?;
        let last_failure_at = format_date(
            RESOURCE,
            state.last_failure_at.unwrap_or(OffsetDateTime::UNIX_EPOCH),
        )?;
        let alerted_at = state
            .alerted_at
            .map(|date| format_date(RESOURCE, date))
            .transpose()?;
        sqlx::query!(
            "INSERT INTO identifier_slowdowns (key, failures, wait_until, last_failure_at, alerted_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT (key) DO UPDATE SET failures = excluded.failures,
                 wait_until = excluded.wait_until, last_failure_at = excluded.last_failure_at,
                 alerted_at = excluded.alerted_at",
            key.as_str(),
            failures,
            wait_until,
            last_failure_at,
            alerted_at
        )
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(())
    }

    async fn alerting(&mut self) -> Result<Vec<(String, Slowdown)>, StoreError> {
        alerting(&mut self.tx).await
    }

    async fn end_alert(
        &mut self,
        key: &str,
        alerted_at: OffsetDateTime,
    ) -> Result<bool, StoreError> {
        let alerted_at = format_date(RESOURCE, alerted_at)?;
        let result = sqlx::query!(
            "UPDATE identifier_slowdowns SET alerted_at = NULL WHERE key = ? AND alerted_at = ?",
            key,
            alerted_at
        )
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(result.rows_affected() > 0)
    }

    async fn trim_identifiers(&mut self, now: OffsetDateTime) -> Result<u64, StoreError> {
        let count = sqlx::query_scalar!("SELECT COUNT(*) FROM identifier_slowdowns")
            .fetch_one(&mut *self.tx)
            .await
            .map_err(storage(RESOURCE))?;
        let excess = identifier_slowdown::excess(
            usize::try_from(count).unwrap_or(identifier_slowdown::MAX_TRACKED),
        );
        if excess == 0 {
            return Ok(0);
        }
        let limit = i64::try_from(excess).unwrap_or(i64::MAX);
        let now = format_date(RESOURCE, now)?;
        // Même ordre que `domain::eviction::rank` : sans attente en cours d'abord, puis moins
        // d'échecs, puis les plus anciens.
        let result = sqlx::query!(
            "DELETE FROM identifier_slowdowns WHERE key IN (
                 SELECT key FROM identifier_slowdowns
                 ORDER BY (wait_until IS NOT NULL AND wait_until > ?) ASC,
                          failures ASC, last_failure_at ASC
                 LIMIT ?)",
            now,
            limit
        )
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(result.rows_affected())
    }

    async fn purge_identifiers(
        &mut self,
        before: OffsetDateTime,
        now: OffsetDateTime,
    ) -> Result<u64, StoreError> {
        let before = format_date(RESOURCE, before)?;
        let now = format_date(RESOURCE, now)?;
        let result = sqlx::query!(
            "DELETE FROM identifier_slowdowns
             WHERE last_failure_at < ? AND (wait_until IS NULL OR wait_until <= ?)",
            before,
            now
        )
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(result.rows_affected())
    }
}

//! Compteurs de tentatives de connexion en SQLite (verrouillage progressif, BR-CONN-006),
//! ralentissement par identifiant et adresses connues des comptes (ADR-0022).

use async_trait::async_trait;
use sqlx::{SqliteConnection, SqlitePool};
use time::OffsetDateTime;

use super::convert::{format_date, parse_date, storage};
use super::store::SqliteUnitOfWork;
use crate::application::ports::{LoginAttemptRepo, LoginAttemptTx, StoreError};
use crate::domain::accounts::AccountId;
use crate::domain::identifier_slowdown::{self, Slowdown};
use crate::domain::known_address::KnownAddress;
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
}

async fn identifier(conn: &mut SqliteConnection, key: &AttemptKey) -> Result<Slowdown, StoreError> {
    let row = sqlx::query_as!(
        SlowdownRow,
        "SELECT failures, wait_until, last_failure_at FROM identifier_slowdowns WHERE key = ?",
        key.as_str()
    )
    .fetch_optional(&mut *conn)
    .await
    .map_err(storage(RESOURCE))?;
    let Some(row) = row else {
        return Ok(Slowdown::default());
    };
    Ok(Slowdown {
        failures: u32::try_from(row.failures).unwrap_or(u32::MAX),
        wait_until: row
            .wait_until
            .as_deref()
            .map(|value| parse_date(RESOURCE, value))
            .transpose()?,
        last_failure_at: Some(parse_date(RESOURCE, &row.last_failure_at)?),
    })
}

struct KnownRow {
    address: String,
    last_success_at: String,
}

fn known_list(rows: Vec<KnownRow>) -> Result<Vec<KnownAddress>, StoreError> {
    rows.into_iter()
        .map(|row| {
            Ok(KnownAddress {
                address: row.address,
                last_success_at: parse_date(RESOURCE, &row.last_success_at)?,
            })
        })
        .collect()
}

/// Jointure sur l'identifiant : la même requête, que le compte existe ou non (liste vide).
async fn known_addresses_of(
    conn: &mut SqliteConnection,
    username: &str,
) -> Result<Vec<KnownAddress>, StoreError> {
    let rows = sqlx::query_as!(
        KnownRow,
        "SELECT k.address, k.last_success_at FROM known_addresses k
         JOIN accounts a ON a.id = k.account_id WHERE a.username = ?",
        username
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(storage(RESOURCE))?;
    known_list(rows)
}

#[async_trait]
impl LoginAttemptRepo for SqliteLoginAttemptRepo {
    async fn get(&self, key: &AttemptKey) -> Result<LockoutState, StoreError> {
        let mut conn = self.pool.acquire().await.map_err(storage(RESOURCE))?;
        get(&mut conn, key).await
    }

    async fn identifier(&self, key: &AttemptKey) -> Result<Slowdown, StoreError> {
        let mut conn = self.pool.acquire().await.map_err(storage(RESOURCE))?;
        identifier(&mut conn, key).await
    }

    async fn known_addresses_of(&self, username: &str) -> Result<Vec<KnownAddress>, StoreError> {
        let mut conn = self.pool.acquire().await.map_err(storage(RESOURCE))?;
        known_addresses_of(&mut conn, username).await
    }

    async fn address_is_known(
        &self,
        address: &str,
        since: OffsetDateTime,
    ) -> Result<bool, StoreError> {
        let since = format_date(RESOURCE, since)?;
        let found = sqlx::query_scalar!(
            r#"SELECT 1 AS "found!: i64" FROM known_addresses WHERE address = ? AND last_success_at > ? LIMIT 1"#,
            address,
            since
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(found.is_some())
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
        // Une attente en cours est gardée en dernier : un flot de clés neuves ne l'efface pas.
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
        sqlx::query!(
            "INSERT INTO identifier_slowdowns (key, failures, wait_until, last_failure_at)
             VALUES (?, ?, ?, ?)
             ON CONFLICT (key) DO UPDATE SET failures = excluded.failures,
                 wait_until = excluded.wait_until, last_failure_at = excluded.last_failure_at",
            key.as_str(),
            failures,
            wait_until,
            last_failure_at
        )
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        // Borne : les identifiants les moins attaqués, les plus anciens, partent d'abord. Un flot
        // d'identifiants inventés (un échec chacun) évince ses propres lignes, pas celle de
        // l'identifiant réellement attaqué.
        let count = sqlx::query_scalar!("SELECT COUNT(*) FROM identifier_slowdowns")
            .fetch_one(&mut *self.tx)
            .await
            .map_err(storage(RESOURCE))?;
        let excess = identifier_slowdown::excess(
            usize::try_from(count).unwrap_or(identifier_slowdown::MAX_TRACKED),
        );
        if excess > 0 {
            let limit = i64::try_from(excess).unwrap_or(i64::MAX);
            sqlx::query!(
                "DELETE FROM identifier_slowdowns WHERE key IN (
                     SELECT key FROM identifier_slowdowns
                     ORDER BY failures ASC, last_failure_at ASC LIMIT ?)",
                limit
            )
            .execute(&mut *self.tx)
            .await
            .map_err(storage(RESOURCE))?;
        }
        Ok(())
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

    async fn known_addresses_of(
        &mut self,
        username: &str,
    ) -> Result<Vec<KnownAddress>, StoreError> {
        known_addresses_of(&mut self.tx, username).await
    }

    async fn known_addresses(
        &mut self,
        account: &AccountId,
    ) -> Result<Vec<KnownAddress>, StoreError> {
        let rows = sqlx::query_as!(
            KnownRow,
            "SELECT address, last_success_at FROM known_addresses WHERE account_id = ?",
            account.as_str()
        )
        .fetch_all(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        known_list(rows)
    }

    async fn replace_known(
        &mut self,
        account: &AccountId,
        list: &[KnownAddress],
    ) -> Result<(), StoreError> {
        sqlx::query!(
            "DELETE FROM known_addresses WHERE account_id = ?",
            account.as_str()
        )
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        for entry in list {
            let last_success_at = format_date(RESOURCE, entry.last_success_at)?;
            sqlx::query!(
                "INSERT INTO known_addresses (account_id, address, last_success_at)
                 VALUES (?, ?, ?)",
                account.as_str(),
                entry.address,
                last_success_at
            )
            .execute(&mut *self.tx)
            .await
            .map_err(storage(RESOURCE))?;
        }
        Ok(())
    }

    async fn forget_known(&mut self, account: &AccountId) -> Result<(), StoreError> {
        sqlx::query!(
            "DELETE FROM known_addresses WHERE account_id = ?",
            account.as_str()
        )
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(())
    }

    async fn purge_known(&mut self, before: OffsetDateTime) -> Result<u64, StoreError> {
        let before = format_date(RESOURCE, before)?;
        let result = sqlx::query!(
            "DELETE FROM known_addresses WHERE last_success_at < ?",
            before
        )
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(result.rows_affected())
    }
}

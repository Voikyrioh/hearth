//! Adresses connues des comptes en SQLite (ADR-0022, BR-CONN-019).

use async_trait::async_trait;
use sqlx::{SqliteConnection, SqlitePool};
use time::OffsetDateTime;

use super::convert::{format_date, parse_date, storage};
use super::store::SqliteUnitOfWork;
use crate::application::ports::{KnownAddressRepo, KnownAddressTx, StoreError};
use crate::domain::accounts::AccountId;
use crate::domain::known_address::KnownAddress;

const RESOURCE: &str = "known_addresses";

pub struct SqliteKnownAddressRepo {
    pool: SqlitePool,
}

impl SqliteKnownAddressRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

struct KnownRow {
    address: String,
    last_success_at: String,
}

fn list(rows: Vec<KnownRow>) -> Result<Vec<KnownAddress>, StoreError> {
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
async fn of_username(
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
    list(rows)
}

async fn of_account(
    conn: &mut SqliteConnection,
    account: &AccountId,
) -> Result<Vec<KnownAddress>, StoreError> {
    let rows = sqlx::query_as!(
        KnownRow,
        "SELECT address, last_success_at FROM known_addresses WHERE account_id = ?",
        account.as_str()
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(storage(RESOURCE))?;
    list(rows)
}

async fn address_is_known(
    conn: &mut SqliteConnection,
    address: &str,
    since: OffsetDateTime,
) -> Result<bool, StoreError> {
    let since = format_date(RESOURCE, since)?;
    let found = sqlx::query_scalar!(
        r#"SELECT 1 AS "found!: i64" FROM known_addresses WHERE address = ? AND last_success_at > ? LIMIT 1"#,
        address,
        since
    )
    .fetch_optional(&mut *conn)
    .await
    .map_err(storage(RESOURCE))?;
    Ok(found.is_some())
}

#[async_trait]
impl KnownAddressRepo for SqliteKnownAddressRepo {
    async fn of_username(&self, username: &str) -> Result<Vec<KnownAddress>, StoreError> {
        let mut conn = self.pool.acquire().await.map_err(storage(RESOURCE))?;
        of_username(&mut conn, username).await
    }

    async fn of_account(&self, account: &AccountId) -> Result<Vec<KnownAddress>, StoreError> {
        let mut conn = self.pool.acquire().await.map_err(storage(RESOURCE))?;
        of_account(&mut conn, account).await
    }

    async fn address_is_known(
        &self,
        address: &str,
        since: OffsetDateTime,
    ) -> Result<bool, StoreError> {
        let mut conn = self.pool.acquire().await.map_err(storage(RESOURCE))?;
        address_is_known(&mut conn, address, since).await
    }
}

#[async_trait]
impl KnownAddressTx for SqliteUnitOfWork {
    async fn of_username(&mut self, username: &str) -> Result<Vec<KnownAddress>, StoreError> {
        of_username(&mut self.tx, username).await
    }

    async fn of_account(&mut self, account: &AccountId) -> Result<Vec<KnownAddress>, StoreError> {
        of_account(&mut self.tx, account).await
    }

    async fn address_is_known(
        &mut self,
        address: &str,
        since: OffsetDateTime,
    ) -> Result<bool, StoreError> {
        address_is_known(&mut self.tx, address, since).await
    }

    async fn replace(
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

    async fn forget(&mut self, account: &AccountId) -> Result<(), StoreError> {
        sqlx::query!(
            "DELETE FROM known_addresses WHERE account_id = ?",
            account.as_str()
        )
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(())
    }

    async fn purge(&mut self, before: OffsetDateTime) -> Result<u64, StoreError> {
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

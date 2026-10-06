//! Adresses connues des comptes en SQLite (ADR-0022, BR-CONN-019).

use async_trait::async_trait;
use sqlx::{SqliteConnection, SqlitePool};
use time::OffsetDateTime;

use super::convert::{format_date, parse_date, storage};
use super::store::SqliteUnitOfWork;
use crate::application::ports::{KnownAddressRepo, KnownAddressTx, StoreError};
use crate::domain::accounts::AccountId;
use crate::domain::known_address::KnownAddress;
use crate::domain::trust::DeviceId;

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
        r#"SELECT k.address,
                  MAX(k.last_success_at, COALESCE(k.last_used_at, k.last_success_at)) AS "last_success_at!: String"
           FROM known_addresses k
           JOIN accounts a ON a.id = k.account_id WHERE a.username = ?"#,
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
        r#"SELECT address,
                  MAX(last_success_at, COALESCE(last_used_at, last_success_at)) AS "last_success_at!: String"
           FROM known_addresses WHERE account_id = ?"#,
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
        r#"SELECT 1 AS "found!: i64" FROM known_addresses WHERE address = ?
           AND MAX(last_success_at, COALESCE(last_used_at, last_success_at)) > ? LIMIT 1"#,
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
        // Les lignes qui restent gardent leur poste et leur dernier usage : on ne réécrit que ce
        // qui change (une date plus récente), on n'efface que ce qui sort de la liste.
        let existing = sqlx::query_scalar!(
            "SELECT address FROM known_addresses WHERE account_id = ?",
            account.as_str()
        )
        .fetch_all(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        for address in existing
            .iter()
            .filter(|address| !list.iter().any(|entry| &entry.address == *address))
        {
            sqlx::query!(
                "DELETE FROM known_addresses WHERE account_id = ? AND address = ?",
                account.as_str(),
                address
            )
            .execute(&mut *self.tx)
            .await
            .map_err(storage(RESOURCE))?;
        }
        for entry in list {
            let last_success_at = format_date(RESOURCE, entry.last_success_at)?;
            sqlx::query!(
                "INSERT INTO known_addresses (account_id, address, last_success_at)
                 VALUES (?, ?, ?)
                 ON CONFLICT (account_id, address) DO UPDATE
                 SET last_success_at = excluded.last_success_at
                 WHERE excluded.last_success_at >
                       MAX(known_addresses.last_success_at,
                           COALESCE(known_addresses.last_used_at, known_addresses.last_success_at))",
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
            "DELETE FROM known_addresses
             WHERE MAX(last_success_at, COALESCE(last_used_at, last_success_at)) < ?",
            before
        )
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(result.rows_affected())
    }

    async fn bind_device(
        &mut self,
        account: &AccountId,
        address: &str,
        device: &DeviceId,
        at: OffsetDateTime,
    ) -> Result<(), StoreError> {
        let at = format_date(RESOURCE, at)?;
        // Un poste n'a qu'une adresse à la fois : la ligne qu'il avait ailleurs est oubliée.
        sqlx::query!(
            "DELETE FROM known_addresses
             WHERE device_id = ? AND NOT (account_id = ? AND address = ?)",
            device.as_str(),
            account.as_str(),
            address
        )
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        sqlx::query!(
            "UPDATE known_addresses SET device_id = ?, last_used_at = ?
             WHERE account_id = ? AND address = ?",
            device.as_str(),
            at,
            account.as_str(),
            address
        )
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(())
    }

    async fn touch(
        &mut self,
        account: &AccountId,
        address: &str,
        at: OffsetDateTime,
    ) -> Result<bool, StoreError> {
        let at = format_date(RESOURCE, at)?;
        let result = sqlx::query!(
            "UPDATE known_addresses SET last_used_at = ? WHERE account_id = ? AND address = ?",
            at,
            account.as_str(),
            address
        )
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(result.rows_affected() > 0)
    }

    async fn forget_without_device(&mut self, account: &AccountId) -> Result<(), StoreError> {
        sqlx::query!(
            "DELETE FROM known_addresses WHERE account_id = ? AND device_id IS NULL",
            account.as_str()
        )
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(())
    }
}

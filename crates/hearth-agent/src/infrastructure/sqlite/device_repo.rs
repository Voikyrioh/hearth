//! Postes de confiance en SQLite (HRT-22, BR-TRUST-004, 022).

use async_trait::async_trait;
use hearth_proto::device_proof::ALGORITHM_ED25519;
use sqlx::{SqliteConnection, SqlitePool};
use time::OffsetDateTime;

use super::convert::{format_date, is_unique_violation, parse_date, storage};
use super::store::SqliteUnitOfWork;
use crate::application::ports::{DeviceRepo, DeviceTx, StoreError};
use crate::domain::accounts::AccountId;
use crate::domain::sessions::SessionId;
use crate::domain::trust::{DeviceId, NewDevice, TrustedDevice};

const RESOURCE: &str = "trusted_devices";

pub struct SqliteDeviceRepo {
    pool: SqlitePool,
}

impl SqliteDeviceRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

struct DeviceRow {
    id: String,
    account_id: String,
    key_id: String,
    name: String,
    created_at: String,
    last_proved_at: String,
    last_addr: String,
}

fn into_device(row: DeviceRow) -> Result<TrustedDevice, StoreError> {
    Ok(TrustedDevice {
        created_at: parse_date(RESOURCE, &row.created_at)?,
        last_proved_at: parse_date(RESOURCE, &row.last_proved_at)?,
        id: DeviceId::new(row.id),
        account: AccountId::new(row.account_id),
        key_id: row.key_id,
        name: row.name,
        last_addr: row.last_addr,
    })
}

async fn find_by_key(
    conn: &mut SqliteConnection,
    key_id: &str,
) -> Result<Option<TrustedDevice>, StoreError> {
    sqlx::query_as!(
        DeviceRow,
        "SELECT id, account_id, key_id, name, created_at, last_proved_at, last_addr
         FROM trusted_devices WHERE key_id = ? ORDER BY created_at, id LIMIT 1",
        key_id
    )
    .fetch_optional(&mut *conn)
    .await
    .map_err(storage(RESOURCE))?
    .map(into_device)
    .transpose()
}

#[async_trait]
impl DeviceRepo for SqliteDeviceRepo {
    async fn of_account(&self, account: &AccountId) -> Result<Vec<TrustedDevice>, StoreError> {
        let mut conn = self.pool.acquire().await.map_err(storage(RESOURCE))?;
        sqlx::query_as!(
            DeviceRow,
            "SELECT id, account_id, key_id, name, created_at, last_proved_at, last_addr
             FROM trusted_devices WHERE account_id = ? ORDER BY created_at, id",
            account.as_str()
        )
        .fetch_all(&mut *conn)
        .await
        .map_err(storage(RESOURCE))?
        .into_iter()
        .map(into_device)
        .collect()
    }

    async fn of_session(&self, session: &SessionId) -> Result<Option<DeviceId>, StoreError> {
        let mut conn = self.pool.acquire().await.map_err(storage(RESOURCE))?;
        let found = sqlx::query_scalar!(
            "SELECT device_id FROM sessions WHERE id = ?",
            session.as_str()
        )
        .fetch_optional(&mut *conn)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(found.flatten().map(DeviceId::new))
    }

    async fn find_by_key(&self, key_id: &str) -> Result<Option<TrustedDevice>, StoreError> {
        let mut conn = self.pool.acquire().await.map_err(storage(RESOURCE))?;
        find_by_key(&mut conn, key_id).await
    }
}

#[async_trait]
impl DeviceTx for SqliteUnitOfWork {
    async fn find_by_key(&mut self, key_id: &str) -> Result<Option<TrustedDevice>, StoreError> {
        find_by_key(&mut self.tx, key_id).await
    }

    async fn get(
        &mut self,
        account: &AccountId,
        id: &DeviceId,
    ) -> Result<Option<TrustedDevice>, StoreError> {
        sqlx::query_as!(
            DeviceRow,
            "SELECT id, account_id, key_id, name, created_at, last_proved_at, last_addr
             FROM trusted_devices WHERE account_id = ? AND id = ?",
            account.as_str(),
            id.as_str()
        )
        .fetch_optional(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?
        .map(into_device)
        .transpose()
    }

    async fn count(&mut self, account: &AccountId) -> Result<usize, StoreError> {
        let count = sqlx::query_scalar!(
            r#"SELECT COUNT(*) AS "count!: i64" FROM trusted_devices WHERE account_id = ?"#,
            account.as_str()
        )
        .fetch_one(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(usize::try_from(count).unwrap_or(usize::MAX))
    }

    async fn insert(&mut self, device: &NewDevice) -> Result<(), StoreError> {
        let now = format_date(RESOURCE, device.now)?;
        let public_key = device.public_key.as_slice();
        sqlx::query!(
            "INSERT INTO trusted_devices
                 (id, account_id, key_id, algorithm, public_key, name, created_at, last_proved_at, last_addr)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            device.id.as_str(),
            device.account.as_str(),
            device.key_id,
            ALGORITHM_ED25519,
            public_key,
            device.name,
            now,
            now,
            device.addr
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

    async fn prove(
        &mut self,
        id: &DeviceId,
        at: OffsetDateTime,
        addr: &str,
    ) -> Result<(), StoreError> {
        let at = format_date(RESOURCE, at)?;
        sqlx::query!(
            "UPDATE trusted_devices SET last_proved_at = ?, last_addr = ? WHERE id = ?",
            at,
            addr,
            id.as_str()
        )
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(())
    }

    async fn delete(&mut self, id: &DeviceId) -> Result<(), StoreError> {
        sqlx::query!("DELETE FROM trusted_devices WHERE id = ?", id.as_str())
            .execute(&mut *self.tx)
            .await
            .map_err(storage(RESOURCE))?;
        Ok(())
    }

    async fn forget_all(&mut self, account: &AccountId) -> Result<u64, StoreError> {
        let result = sqlx::query!(
            "DELETE FROM trusted_devices WHERE account_id = ?",
            account.as_str()
        )
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(result.rows_affected())
    }

    async fn purge(&mut self, before: OffsetDateTime) -> Result<u64, StoreError> {
        let before = format_date(RESOURCE, before)?;
        let result = sqlx::query!(
            "DELETE FROM trusted_devices WHERE last_proved_at < ?",
            before
        )
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(result.rows_affected())
    }

    async fn enrollment_frozen(&mut self) -> Result<bool, StoreError> {
        let active = sqlx::query_scalar!("SELECT active FROM attack_mode WHERE id = 1")
            .fetch_optional(&mut *self.tx)
            .await
            .map_err(storage(RESOURCE))?;
        Ok(active == Some(1))
    }
}

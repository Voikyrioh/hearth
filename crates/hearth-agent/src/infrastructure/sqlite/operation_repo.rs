//! Opérations suivies par clé, en SQLite (BR-RESIL-010). La clé primaire est `(account_id, id)` :
//! la clé d'un compte n'est jamais celle d'un autre.

use async_trait::async_trait;
use sqlx::{SqliteConnection, SqlitePool};
use time::OffsetDateTime;

use super::convert::{format_date, is_unique_violation, parse_date, storage};
use super::store::SqliteUnitOfWork;
use crate::application::ports::{OperationRepo, OperationTx, StoreError};
use crate::domain::accounts::AccountId;
use crate::domain::operations::{Operation, OperationKey, OperationStatus, RequestFingerprint};

const RESOURCE: &str = "operations";

pub struct SqliteOperationRepo {
    pool: SqlitePool,
}

impl SqliteOperationRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

struct OperationRow {
    id: String,
    account_id: String,
    kind: String,
    request_hash: String,
    status: String,
    result_json: Option<String>,
    created_at: String,
    finished_at: Option<String>,
}

fn into_operation(row: OperationRow) -> Result<Operation, StoreError> {
    let key = OperationKey::parse(&row.id).map_err(|_| StoreError::Unavailable {
        resource: RESOURCE,
        message: "clé d'opération illisible".to_owned(),
    })?;
    let status =
        OperationStatus::from_stored(&row.status).map_err(|_| StoreError::Unavailable {
            resource: RESOURCE,
            message: format!("statut illisible pour l'opération {}", row.id),
        })?;
    Ok(Operation {
        key,
        account: AccountId::new(row.account_id),
        kind: row.kind,
        request: RequestFingerprint::from_stored(&row.request_hash),
        status,
        result_json: row.result_json,
        created_at: parse_date(RESOURCE, &row.created_at)?,
        finished_at: row
            .finished_at
            .as_deref()
            .map(|value| parse_date(RESOURCE, value))
            .transpose()?,
    })
}

async fn find(
    conn: &mut SqliteConnection,
    account: &AccountId,
    key: &OperationKey,
) -> Result<Option<Operation>, StoreError> {
    sqlx::query_as!(
        OperationRow,
        "SELECT id, account_id, kind, request_hash, status, result_json, created_at, finished_at
         FROM operations WHERE account_id = ? AND id = ?",
        account.as_str(),
        key.as_str()
    )
    .fetch_optional(&mut *conn)
    .await
    .map_err(storage(RESOURCE))?
    .map(into_operation)
    .transpose()
}

#[async_trait]
impl OperationRepo for SqliteOperationRepo {
    async fn find(
        &self,
        account: &AccountId,
        key: &OperationKey,
    ) -> Result<Option<Operation>, StoreError> {
        let mut conn = self.pool.acquire().await.map_err(storage(RESOURCE))?;
        find(&mut conn, account, key).await
    }
}

#[async_trait]
impl OperationTx for SqliteUnitOfWork {
    async fn find(
        &mut self,
        account: &AccountId,
        key: &OperationKey,
    ) -> Result<Option<Operation>, StoreError> {
        find(&mut self.tx, account, key).await
    }

    async fn insert(&mut self, operation: &Operation) -> Result<(), StoreError> {
        let created_at = format_date(RESOURCE, operation.created_at)?;
        let request_hash = operation.request.to_stored();
        let finished_at = operation
            .finished_at
            .map(|date| format_date(RESOURCE, date))
            .transpose()?;
        sqlx::query!(
            "INSERT INTO operations (id, account_id, kind, request_hash, status, result_json, created_at, finished_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            operation.key.as_str(),
            operation.account.as_str(),
            operation.kind,
            request_hash,
            operation.status.as_str(),
            operation.result_json,
            created_at,
            finished_at
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

    async fn finish(
        &mut self,
        account: &AccountId,
        key: &OperationKey,
        status: OperationStatus,
        result_json: &str,
        at: OffsetDateTime,
    ) -> Result<(), StoreError> {
        let at = format_date(RESOURCE, at)?;
        sqlx::query!(
            "UPDATE operations SET status = ?, result_json = ?, finished_at = ?
             WHERE account_id = ? AND id = ?",
            status.as_str(),
            result_json,
            at,
            account.as_str(),
            key.as_str()
        )
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(())
    }

    async fn delete(&mut self, account: &AccountId, key: &OperationKey) -> Result<(), StoreError> {
        sqlx::query!(
            "DELETE FROM operations WHERE account_id = ? AND id = ?",
            account.as_str(),
            key.as_str()
        )
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(())
    }

    async fn interrupt_running(&mut self, at: OffsetDateTime) -> Result<u64, StoreError> {
        let at = format_date(RESOURCE, at)?;
        let result = sqlx::query!(
            "UPDATE operations SET status = 'interrupted', finished_at = ? WHERE status = 'running'",
            at
        )
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(result.rows_affected())
    }

    async fn purge(&mut self, before: OffsetDateTime) -> Result<u64, StoreError> {
        let before = format_date(RESOURCE, before)?;
        let result = sqlx::query!("DELETE FROM operations WHERE created_at < ?", before)
            .execute(&mut *self.tx)
            .await
            .map_err(storage(RESOURCE))?;
        Ok(result.rows_affected())
    }
}

//! Le registre des transferts, en SQLite (`transfers`, HRT-95, ADR-0048). Ce dépôt ne connaît aucun nom de fichier du
//! client ni le contenu d'un fichier : des identifiants, des tailles, des empreintes, des dates.

use async_trait::async_trait;
use hearth_proto::api::transfers::{DIGEST_LEN, digest_from_hex, hex_of};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use time::OffsetDateTime;

use super::convert::{format_date, parse_date, storage};
use crate::application::ports::{
    NewTransfer, StoreError, TransferRepo, TransferRow, TransferState,
};
use crate::domain::transfers::TransferId;

const RESOURCE: &str = "transfers";

pub struct SqliteTransferRepo {
    pool: SqlitePool,
}

impl SqliteTransferRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn unreadable(what: &str) -> StoreError {
    StoreError::Unavailable {
        resource: RESOURCE,
        message: format!("{what} illisible"),
    }
}

fn unsigned(row: &SqliteRow, column: &str) -> Result<u64, StoreError> {
    let value: i64 = row.try_get(column).map_err(storage(RESOURCE))?;
    u64::try_from(value).map_err(|_| unreadable(column))
}

fn count_of(value: u64, what: &str) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| unreadable(what))
}

fn row_of(row: &SqliteRow) -> Result<TransferRow, StoreError> {
    let id: String = row.try_get("id").map_err(storage(RESOURCE))?;
    let state: String = row.try_get("state").map_err(storage(RESOURCE))?;
    let tree: String = row.try_get("tree_digest").map_err(storage(RESOURCE))?;
    let created: String = row.try_get("created_at").map_err(storage(RESOURCE))?;
    let updated: String = row.try_get("updated_at").map_err(storage(RESOURCE))?;
    let dev: i64 = row.try_get("dev").map_err(storage(RESOURCE))?;
    let ino: i64 = row.try_get("ino").map_err(storage(RESOURCE))?;
    Ok(TransferRow {
        id: TransferId::parse(&id).ok_or_else(|| unreadable("id"))?,
        account: row.try_get("account_id").map_err(storage(RESOURCE))?,
        state: match state.as_str() {
            "receiving" => TransferState::Receiving,
            "complete" => TransferState::Complete,
            _ => return Err(unreadable("state")),
        },
        size: unsigned(row, "size")?,
        received_chunks: unsigned(row, "received_chunks")?,
        dev: dev.cast_unsigned(),
        ino: ino.cast_unsigned(),
        tree_digest: if tree.is_empty() {
            None
        } else {
            Some(digest_from_hex(&tree).ok_or_else(|| unreadable("tree_digest"))?)
        },
        complete_mtime_ns: row
            .try_get("complete_mtime_ns")
            .map_err(storage(RESOURCE))?,
        created_at: parse_date(RESOURCE, &created)?,
        updated_at: parse_date(RESOURCE, &updated)?,
    })
}

/// Les colonnes d'une ligne, en littéral (jamais une requête bâtie à l'exécution).
macro_rules! columns {
    () => {
        "id, account_id, state, size, received_chunks, dev, ino, tree_digest, complete_mtime_ns, \
         created_at, updated_at"
    };
}

#[async_trait]
impl TransferRepo for SqliteTransferRepo {
    async fn insert(&self, new: &NewTransfer) -> Result<(), StoreError> {
        let now = format_date(RESOURCE, new.now)?;
        sqlx::query(
            "INSERT INTO transfers (id, account_id, state, size, dev, ino, created_at, updated_at)
             VALUES (?, ?, 'receiving', ?, ?, ?, ?, ?)",
        )
        .bind(new.id.as_str())
        .bind(&new.account)
        .bind(count_of(new.size, "size")?)
        .bind(new.dev.cast_signed())
        .bind(new.ino.cast_signed())
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(())
    }

    async fn get(&self, id: &TransferId) -> Result<Option<TransferRow>, StoreError> {
        let row = sqlx::query(concat!(
            "SELECT ",
            columns!(),
            " FROM transfers WHERE id = ?"
        ))
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage(RESOURCE))?;
        row.as_ref().map(row_of).transpose()
    }

    async fn note_progress(
        &self,
        id: &TransferId,
        received_chunks: u64,
        now: OffsetDateTime,
    ) -> Result<bool, StoreError> {
        let result = sqlx::query(
            "UPDATE transfers SET received_chunks = ?, updated_at = ?
             WHERE id = ? AND state = 'receiving'",
        )
        .bind(count_of(received_chunks, "received_chunks")?)
        .bind(format_date(RESOURCE, now)?)
        .bind(id.as_str())
        .execute(&self.pool)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(result.rows_affected() > 0)
    }

    async fn complete(
        &self,
        id: &TransferId,
        tree_digest: &[u8; DIGEST_LEN],
        mtime_ns: i64,
        now: OffsetDateTime,
    ) -> Result<bool, StoreError> {
        let result = sqlx::query(
            "UPDATE transfers
             SET state = 'complete', tree_digest = ?, complete_mtime_ns = ?, updated_at = ?
             WHERE id = ? AND state = 'receiving'",
        )
        .bind(hex_of(tree_digest))
        .bind(mtime_ns)
        .bind(format_date(RESOURCE, now)?)
        .bind(id.as_str())
        .execute(&self.pool)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(result.rows_affected() > 0)
    }

    async fn delete(&self, id: &TransferId) -> Result<bool, StoreError> {
        let result = sqlx::query("DELETE FROM transfers WHERE id = ?")
            .bind(id.as_str())
            .execute(&self.pool)
            .await
            .map_err(storage(RESOURCE))?;
        Ok(result.rows_affected() > 0)
    }

    async fn count_open(&self, account: &str) -> Result<usize, StoreError> {
        let row = sqlx::query("SELECT COUNT(*) AS n FROM transfers WHERE account_id = ?")
            .bind(account)
            .fetch_one(&self.pool)
            .await
            .map_err(storage(RESOURCE))?;
        let n: i64 = row.try_get("n").map_err(storage(RESOURCE))?;
        usize::try_from(n).map_err(|_| unreadable("n"))
    }

    async fn expired(
        &self,
        before: OffsetDateTime,
        limit: usize,
    ) -> Result<Vec<TransferRow>, StoreError> {
        let rows = sqlx::query(concat!(
            "SELECT ",
            columns!(),
            " FROM transfers WHERE updated_at < ? ORDER BY updated_at LIMIT ?"
        ))
        .bind(format_date(RESOURCE, before)?)
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(storage(RESOURCE))?;
        rows.iter().map(row_of).collect()
    }
}

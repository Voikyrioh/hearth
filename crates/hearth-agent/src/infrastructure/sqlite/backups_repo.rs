//! Le registre des sauvegardes, en SQLite (`backups`, `backup_settings`, HRT-97, ADR-0047). Ce dépôt ne connaît ni
//! la clé ni une note en clair : il range la note scellée (nonce + texte chiffré) que le cas d'usage lui donne.

use async_trait::async_trait;
use hearth_proto::api::backups::{BackupFlag, BackupIntegrity, BackupMode, BackupSettings};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use time::OffsetDateTime;

use super::convert::{format_date, parse_date, storage};
use crate::application::ports::{BackupRepo, BackupRow, NewBackup, Published, StoreError};
use crate::domain::backups::BackupId;
use crate::domain::sealed::{NONCE_LEN, SealedBlob};
use crate::domain::services::key::ServiceKey;

const RESOURCE: &str = "backups";

pub struct SqliteBackupRepo {
    pool: SqlitePool,
}

impl SqliteBackupRepo {
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

fn mode_text(mode: BackupMode) -> Result<&'static str, StoreError> {
    match mode {
        BackupMode::Unknown => Err(unreadable("mode")),
        other => Ok(other.as_str()),
    }
}

fn mode_of(text: &str) -> Result<BackupMode, StoreError> {
    match text {
        "stopped" => Ok(BackupMode::Stopped),
        "hot" => Ok(BackupMode::Hot),
        "stop_copy_start" => Ok(BackupMode::StopCopyStart),
        _ => Err(unreadable("mode")),
    }
}

fn integrity_text(integrity: BackupIntegrity) -> Result<&'static str, StoreError> {
    match integrity {
        BackupIntegrity::Verified => Ok("verified"),
        BackupIntegrity::Corrupt => Ok("corrupt"),
        BackupIntegrity::Unchecked => Ok("unchecked"),
        BackupIntegrity::Missing => Err(unreadable("intégrité")),
        BackupIntegrity::Unknown => Err(unreadable("intégrité")),
    }
}

fn integrity_of(text: &str) -> Result<BackupIntegrity, StoreError> {
    match text {
        "verified" => Ok(BackupIntegrity::Verified),
        "corrupt" => Ok(BackupIntegrity::Corrupt),
        "unchecked" => Ok(BackupIntegrity::Unchecked),
        _ => Err(unreadable("intégrité")),
    }
}

fn flags_text(flags: &[BackupFlag]) -> Result<String, StoreError> {
    serde_json::to_string(flags).map_err(|_| unreadable("repères"))
}

fn blob_of(
    nonce: Option<Vec<u8>>,
    sealed: Option<Vec<u8>>,
) -> Result<Option<SealedBlob>, StoreError> {
    match (nonce, sealed) {
        (Some(nonce), Some(bytes)) => {
            let nonce: [u8; NONCE_LEN] = nonce.try_into().map_err(|_| unreadable("nonce"))?;
            Ok(Some(SealedBlob { nonce, bytes }))
        }
        _ => Ok(None),
    }
}

fn unsigned(row: &SqliteRow, column: &str) -> Result<u64, StoreError> {
    let value: i64 = row.try_get(column).map_err(storage(RESOURCE))?;
    u64::try_from(value).map_err(|_| unreadable(column))
}

/// Un identifiant de périphérique ou d'inode : 64 bits non signés rangés dans un entier signé (mêmes bits).
fn bits(row: &SqliteRow, column: &str) -> Result<u64, StoreError> {
    let value: i64 = row.try_get(column).map_err(storage(RESOURCE))?;
    Ok(value.cast_unsigned())
}

fn row_of(row: &SqliteRow) -> Result<BackupRow, StoreError> {
    let id: String = row.try_get("id").map_err(storage(RESOURCE))?;
    let key: String = row.try_get("service_key").map_err(storage(RESOURCE))?;
    let state: String = row.try_get("state").map_err(storage(RESOURCE))?;
    let created_at: String = row.try_get("created_at").map_err(storage(RESOURCE))?;
    let mode: String = row.try_get("mode").map_err(storage(RESOURCE))?;
    let flags: String = row.try_get("flags_json").map_err(storage(RESOURCE))?;
    let integrity: String = row.try_get("integrity").map_err(storage(RESOURCE))?;
    let verified_at: Option<String> = row.try_get("verified_at").map_err(storage(RESOURCE))?;
    let changed: i64 = row.try_get("changed_files").map_err(storage(RESOURCE))?;
    Ok(BackupRow {
        id: BackupId::parse(&id).ok_or_else(|| unreadable("id"))?,
        service_key: ServiceKey::parse(&key).ok_or_else(|| unreadable("service_key"))?,
        service_name: row.try_get("service_name").map_err(storage(RESOURCE))?,
        ready: state == "ready",
        created_at: parse_date(RESOURCE, &created_at)?,
        created_by: row.try_get("created_by").map_err(storage(RESOURCE))?,
        dev: bits(row, "dev")?,
        ino: bits(row, "ino")?,
        archive_bytes: unsigned(row, "archive_bytes")?,
        data_bytes: unsigned(row, "data_bytes")?,
        files: unsigned(row, "files")?,
        mode: mode_of(&mode)?,
        flags: serde_json::from_str(&flags).map_err(|_| unreadable("repères"))?,
        changed_files: u32::try_from(changed).map_err(|_| unreadable("changed_files"))?,
        image: row.try_get("image").map_err(storage(RESOURCE))?,
        integrity: integrity_of(&integrity)?,
        verified_at: verified_at
            .as_deref()
            .map(|text| parse_date(RESOURCE, text))
            .transpose()?,
        manifest_sha256: row.try_get("manifest_sha256").map_err(storage(RESOURCE))?,
        archive_sha256: row.try_get("archive_sha256").map_err(storage(RESOURCE))?,
        note: blob_of(
            row.try_get("note_nonce").map_err(storage(RESOURCE))?,
            row.try_get("note_sealed").map_err(storage(RESOURCE))?,
        )?,
    })
}

/// Les colonnes d'une ligne, en littéral (jamais une requête bâtie à l'exécution).
macro_rules! columns {
    () => {
        "id, service_key, service_name, state, created_at, created_by, dev, ino, archive_bytes, data_bytes, \
         files, mode, flags_json, changed_files, image, integrity, verified_at, manifest_sha256, \
         archive_sha256, note_nonce, note_sealed"
    };
}

#[async_trait]
impl BackupRepo for SqliteBackupRepo {
    async fn insert_writing(&self, new: &NewBackup) -> Result<(), StoreError> {
        let (nonce, sealed) = match &new.note {
            Some(blob) => (Some(blob.nonce.to_vec()), Some(blob.bytes.clone())),
            None => (None, None),
        };
        sqlx::query(
            "INSERT INTO backups
                 (id, service_key, service_name, state, created_at, created_by, dev, ino, mode, image,
                  note_nonce, note_sealed)
             VALUES (?, ?, ?, 'writing', ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(new.id.as_str())
        .bind(new.service_key.as_str())
        .bind(&new.service_name)
        .bind(format_date(RESOURCE, new.created_at)?)
        .bind(&new.created_by)
        .bind(new.dev.cast_signed())
        .bind(new.ino.cast_signed())
        .bind(mode_text(new.mode)?)
        .bind(&new.image)
        .bind(nonce)
        .bind(sealed)
        .execute(&self.pool)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(())
    }

    async fn mark_ready(&self, id: &BackupId, published: &Published) -> Result<bool, StoreError> {
        let result = sqlx::query(
            "UPDATE backups
             SET state = 'ready', archive_bytes = ?, data_bytes = ?, files = ?, flags_json = ?,
                 changed_files = ?, manifest_sha256 = ?, archive_sha256 = ?
             WHERE id = ? AND state = 'writing'",
        )
        .bind(i64::try_from(published.archive_bytes).map_err(|_| unreadable("archive_bytes"))?)
        .bind(i64::try_from(published.data_bytes).map_err(|_| unreadable("data_bytes"))?)
        .bind(i64::try_from(published.files).map_err(|_| unreadable("files"))?)
        .bind(flags_text(&published.flags)?)
        .bind(i64::from(published.changed_files))
        .bind(&published.manifest_sha256)
        .bind(&published.archive_sha256)
        .bind(id.as_str())
        .execute(&self.pool)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(result.rows_affected() > 0)
    }

    async fn set_integrity(
        &self,
        id: &BackupId,
        integrity: BackupIntegrity,
        verified_at: Option<OffsetDateTime>,
    ) -> Result<(), StoreError> {
        let verified = verified_at
            .map(|at| format_date(RESOURCE, at))
            .transpose()?;
        sqlx::query("UPDATE backups SET integrity = ?, verified_at = ? WHERE id = ?")
            .bind(integrity_text(integrity)?)
            .bind(verified)
            .bind(id.as_str())
            .execute(&self.pool)
            .await
            .map_err(storage(RESOURCE))?;
        Ok(())
    }

    async fn get(&self, id: &BackupId) -> Result<Option<BackupRow>, StoreError> {
        sqlx::query(concat!("SELECT ", columns!(), " FROM backups WHERE id = ?"))
            .bind(id.as_str())
            .fetch_optional(&self.pool)
            .await
            .map_err(storage(RESOURCE))?
            .map(|row| row_of(&row))
            .transpose()
    }

    async fn list(&self, key: &ServiceKey) -> Result<Vec<BackupRow>, StoreError> {
        sqlx::query(concat!(
            "SELECT ",
            columns!(),
            " FROM backups WHERE service_key = ? AND state = 'ready' ORDER BY created_at DESC, id DESC"
        ))
        .bind(key.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(storage(RESOURCE))?
        .iter()
        .map(row_of)
        .collect()
    }

    async fn delete(&self, id: &BackupId) -> Result<bool, StoreError> {
        let result = sqlx::query("DELETE FROM backups WHERE id = ?")
            .bind(id.as_str())
            .execute(&self.pool)
            .await
            .map_err(storage(RESOURCE))?;
        Ok(result.rows_affected() > 0)
    }

    async fn drop_writing(&self) -> Result<u64, StoreError> {
        let result = sqlx::query("DELETE FROM backups WHERE state = 'writing'")
            .execute(&self.pool)
            .await
            .map_err(storage(RESOURCE))?;
        Ok(result.rows_affected())
    }

    async fn settings(&self, key: &ServiceKey) -> Result<BackupSettings, StoreError> {
        let Some(row) = sqlx::query(
            "SELECT ignored_json, included_json FROM backup_settings WHERE service_key = ?",
        )
        .bind(key.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage(RESOURCE))?
        else {
            return Ok(BackupSettings::default());
        };
        let ignored: String = row.try_get("ignored_json").map_err(storage(RESOURCE))?;
        let included: String = row.try_get("included_json").map_err(storage(RESOURCE))?;
        Ok(BackupSettings {
            ignored: serde_json::from_str(&ignored).map_err(|_| unreadable("ignored_json"))?,
            included: serde_json::from_str(&included).map_err(|_| unreadable("included_json"))?,
        })
    }

    async fn save_settings(
        &self,
        key: &ServiceKey,
        settings: &BackupSettings,
        by: &str,
        at: OffsetDateTime,
    ) -> Result<(), StoreError> {
        let ignored =
            serde_json::to_string(&settings.ignored).map_err(|_| unreadable("ignored_json"))?;
        let included =
            serde_json::to_string(&settings.included).map_err(|_| unreadable("included_json"))?;
        sqlx::query(
            "INSERT INTO backup_settings (service_key, ignored_json, included_json, updated_by, updated_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT (service_key) DO UPDATE
             SET ignored_json = excluded.ignored_json, included_json = excluded.included_json,
                 updated_by = excluded.updated_by, updated_at = excluded.updated_at",
        )
        .bind(key.as_str())
        .bind(ignored)
        .bind(included)
        .bind(by)
        .bind(format_date(RESOURCE, at)?)
        .execute(&self.pool)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(())
    }
}

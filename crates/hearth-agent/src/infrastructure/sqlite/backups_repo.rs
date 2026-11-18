//! Le registre des sauvegardes, en SQLite (`backups`, `backup_settings`, HRT-97, ADR-0047). Ce dépôt ne connaît ni
//! la clé ni une note en clair : il range la note scellée (nonce + texte chiffré) que le cas d'usage lui donne.

use async_trait::async_trait;
use hearth_proto::api::backups::{BackupFlag, BackupIntegrity, BackupMode, BackupSettings};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use time::OffsetDateTime;

use super::convert::{format_date, parse_date, storage};
use crate::application::ports::{
    BackupRepo, BackupRow, ModsRecord, ModsSummaryRow, NewBackup, Published, Publishing, StoreError,
};
use crate::domain::backups::BackupId;
use crate::domain::mods::backup_list::{Entry, IdentitySummary, Place, RecordedSet, SetState};
use crate::domain::mods::game_info::Origin;
use crate::domain::mods::jar::{Environment, JarLoader};
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
    let skipped: i64 = row.try_get("skipped").map_err(storage(RESOURCE))?;
    Ok(BackupRow {
        id: BackupId::parse(&id).ok_or_else(|| unreadable("id"))?,
        service_key: ServiceKey::parse(&key).ok_or_else(|| unreadable("service_key"))?,
        service_name: row.try_get("service_name").map_err(storage(RESOURCE))?,
        ready: state == "ready",
        created_at: parse_date(RESOURCE, &created_at)?,
        created_by: row.try_get("created_by").map_err(storage(RESOURCE))?,
        dev: bits(row, "dev")?,
        ino: bits(row, "ino")?,
        archive_mtime_ns: row.try_get("archive_mtime_ns").map_err(storage(RESOURCE))?,
        archive_bytes: unsigned(row, "archive_bytes")?,
        data_bytes: unsigned(row, "data_bytes")?,
        files: unsigned(row, "files")?,
        mode: mode_of(&mode)?,
        flags: serde_json::from_str(&flags).map_err(|_| unreadable("repères"))?,
        changed_files: u32::try_from(changed).map_err(|_| unreadable("changed_files"))?,
        skipped: u32::try_from(skipped).map_err(|_| unreadable("skipped"))?,
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
        "id, service_key, service_name, state, created_at, created_by, dev, ino, archive_mtime_ns, \
         archive_bytes, data_bytes, files, mode, flags_json, changed_files, skipped, image, integrity, \
         verified_at, manifest_sha256, archive_sha256, note_nonce, note_sealed"
    };
}

fn set_state_text(state: SetState) -> &'static str {
    match state {
        SetState::Recorded => "recorded",
        SetState::Partial => "partial",
        SetState::Unreadable => "unreadable",
        SetState::NotInBackup => "not_in_backup",
    }
}

fn set_state_of(text: &str) -> Option<SetState> {
    match text {
        "recorded" => Some(SetState::Recorded),
        "partial" => Some(SetState::Partial),
        "unreadable" => Some(SetState::Unreadable),
        "not_in_backup" => Some(SetState::NotInBackup),
        _ => None,
    }
}

fn place_text(place: Place) -> &'static str {
    match place {
        Place::Active => "active",
        Place::Disabled => "disabled",
        Place::Retired => "retired",
    }
}

fn place_of(text: &str) -> Option<Place> {
    match text {
        "active" => Some(Place::Active),
        "disabled" => Some(Place::Disabled),
        "retired" => Some(Place::Retired),
        _ => None,
    }
}

fn origin_text(origin: Origin) -> &'static str {
    match origin {
        Origin::ModrinthPack => "modrinth_pack",
        Origin::Hearth => "hearth",
        Origin::ServerPack => "server_pack",
        Origin::Manual => "manual",
    }
}

fn origin_of(text: &str) -> Option<Origin> {
    match text {
        "modrinth_pack" => Some(Origin::ModrinthPack),
        "hearth" => Some(Origin::Hearth),
        "server_pack" => Some(Origin::ServerPack),
        "manual" => Some(Origin::Manual),
        _ => None,
    }
}

fn loader_of(text: &str) -> Option<JarLoader> {
    match text {
        "fabric" => Some(JarLoader::Fabric),
        "quilt" => Some(JarLoader::Quilt),
        "forge" => Some(JarLoader::Forge),
        "neoforge" => Some(JarLoader::NeoForge),
        _ => None,
    }
}

fn environment_text(environment: Environment) -> &'static str {
    match environment {
        Environment::Both => "both",
        Environment::ServerOnly => "server",
        Environment::ClientOnly => "client",
        Environment::Unknown => "unknown",
    }
}

fn environment_of(text: Option<&str>) -> Environment {
    match text {
        Some("both") => Environment::Both,
        Some("server") => Environment::ServerOnly,
        Some("client") => Environment::ClientOnly,
        _ => Environment::Unknown,
    }
}

/// Écrit la liste des mods d'une sauvegarde dans la transaction qui la publie.
async fn write_mods(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: &BackupId,
    record: &ModsRecord,
) -> Result<(), StoreError> {
    let set = &record.set;
    let service: String = sqlx::query_scalar("SELECT service_key FROM backups WHERE id = ?")
        .bind(id.as_str())
        .fetch_one(&mut **tx)
        .await
        .map_err(storage(RESOURCE))?;
    sqlx::query(
        "INSERT INTO backup_mod_sets
         (backup_id, service_key, state, loader, minecraft, active, disabled, retired, not_listed, recorded_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id.as_str())
    .bind(&service)
    .bind(set_state_text(set.state))
    .bind(set.loader.map(JarLoader::variant_id))
    .bind(&set.minecraft)
    .bind(i64::from(set.active))
    .bind(i64::from(set.disabled))
    .bind(i64::from(set.retired))
    .bind(i64::from(set.not_listed))
    .bind(format_date(RESOURCE, record.at)?)
    .execute(&mut **tx)
    .await
    .map_err(storage(RESOURCE))?;
    for entry in &set.entries {
        let identity = entry.identity.as_ref();
        sqlx::query(
            "INSERT INTO backup_mod_entries
             (backup_id, rank, file_name, place, mod_id, name, version, loader, minecraft_req, environment,
              origin, size_bytes, sha256)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?, ?, ?)",
        )
        .bind(id.as_str())
        .bind(i64::from(entry.rank))
        .bind(&entry.file_name)
        .bind(place_text(entry.place))
        .bind(identity.map(|i| i.mod_id.as_str()))
        .bind(identity.and_then(|i| i.name.as_deref()))
        .bind(identity.and_then(|i| i.version.as_deref()))
        .bind(identity.map(|i| i.loader.variant_id()))
        .bind(identity.map(|i| environment_text(i.environment)))
        .bind(origin_text(entry.origin))
        .bind(i64::try_from(entry.size).map_err(|_| unreadable("size_bytes"))?)
        .bind(&entry.sha256)
        .execute(&mut **tx)
        .await
        .map_err(storage(RESOURCE))?;
    }
    Ok(())
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

    async fn note_publishing(
        &self,
        id: &BackupId,
        publishing: &Publishing,
    ) -> Result<bool, StoreError> {
        let result = sqlx::query(
            "UPDATE backups
             SET archive_bytes = ?, manifest_sha256 = ?, archive_sha256 = ?
             WHERE id = ? AND state = 'writing'",
        )
        .bind(i64::try_from(publishing.archive_bytes).map_err(|_| unreadable("archive_bytes"))?)
        .bind(&publishing.manifest_sha256)
        .bind(&publishing.archive_sha256)
        .bind(id.as_str())
        .execute(&self.pool)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(result.rows_affected() > 0)
    }

    async fn mark_ready(&self, id: &BackupId, published: &Published) -> Result<bool, StoreError> {
        let mut tx = self.pool.begin().await.map_err(storage(RESOURCE))?;
        let result = sqlx::query(
            "UPDATE backups
             SET state = 'ready', archive_bytes = ?, archive_mtime_ns = ?, data_bytes = ?, files = ?,
                 flags_json = ?, changed_files = ?, skipped = ?, manifest_sha256 = ?, archive_sha256 = ?
             WHERE id = ? AND state = 'writing'",
        )
        .bind(i64::try_from(published.archive_bytes).map_err(|_| unreadable("archive_bytes"))?)
        .bind(published.archive_mtime_ns)
        .bind(i64::try_from(published.data_bytes).map_err(|_| unreadable("data_bytes"))?)
        .bind(i64::try_from(published.files).map_err(|_| unreadable("files"))?)
        .bind(flags_text(&published.flags)?)
        .bind(i64::from(published.changed_files))
        .bind(i64::from(published.skipped))
        .bind(&published.manifest_sha256)
        .bind(&published.archive_sha256)
        .bind(id.as_str())
        .execute(&mut *tx)
        .await
        .map_err(storage(RESOURCE))?;
        let ready = result.rows_affected() > 0;
        // La liste des mods entre avec la ligne `ready` (BR-MOD-034) ; une ligne qui n'est plus `writing` n'en reçoit pas.
        if let (true, Some(record)) = (ready, &published.mods) {
            write_mods(&mut tx, id, record).await?;
        }
        tx.commit().await.map_err(storage(RESOURCE))?;
        Ok(ready)
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
        // La liste des mods part avec sa sauvegarde, dans la même transaction (BR-MOD-034).
        let mut tx = self.pool.begin().await.map_err(storage(RESOURCE))?;
        let result = sqlx::query("DELETE FROM backups WHERE id = ?")
            .bind(id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(storage(RESOURCE))?;
        sqlx::query("DELETE FROM backup_mod_entries WHERE backup_id = ?")
            .bind(id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(storage(RESOURCE))?;
        sqlx::query("DELETE FROM backup_mod_sets WHERE backup_id = ?")
            .bind(id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(storage(RESOURCE))?;
        tx.commit().await.map_err(storage(RESOURCE))?;
        Ok(result.rows_affected() > 0)
    }

    async fn mods_of(&self, id: &BackupId) -> Result<Option<ModsRecord>, StoreError> {
        let Some(head) = sqlx::query(
            "SELECT state, loader, minecraft, active, disabled, retired, not_listed, recorded_at
             FROM backup_mod_sets WHERE backup_id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage(RESOURCE))?
        else {
            return Ok(None);
        };
        let rows = sqlx::query(
            "SELECT rank, file_name, place, mod_id, name, version, loader, environment, origin, size_bytes, sha256
             FROM backup_mod_entries WHERE backup_id = ? ORDER BY rank",
        )
        .bind(id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(storage(RESOURCE))?;
        let count = |column: &str| -> Result<u32, StoreError> {
            let value: i64 = head.try_get(column).map_err(storage(RESOURCE))?;
            u32::try_from(value).map_err(|_| unreadable(column))
        };
        let state: String = head.try_get("state").map_err(storage(RESOURCE))?;
        let loader: Option<String> = head.try_get("loader").map_err(storage(RESOURCE))?;
        let at: String = head.try_get("recorded_at").map_err(storage(RESOURCE))?;
        let mut entries = Vec::with_capacity(rows.len());
        for row in &rows {
            let rank: i64 = row.try_get("rank").map_err(storage(RESOURCE))?;
            let place: String = row.try_get("place").map_err(storage(RESOURCE))?;
            let origin: String = row.try_get("origin").map_err(storage(RESOURCE))?;
            let size: i64 = row.try_get("size_bytes").map_err(storage(RESOURCE))?;
            let mod_id: Option<String> = row.try_get("mod_id").map_err(storage(RESOURCE))?;
            let entry_loader: Option<String> = row.try_get("loader").map_err(storage(RESOURCE))?;
            let environment: Option<String> =
                row.try_get("environment").map_err(storage(RESOURCE))?;
            let identity = match (mod_id, entry_loader.as_deref().and_then(loader_of)) {
                (Some(mod_id), Some(loader)) => Some(IdentitySummary {
                    mod_id,
                    name: row.try_get("name").map_err(storage(RESOURCE))?,
                    version: row.try_get("version").map_err(storage(RESOURCE))?,
                    loader,
                    environment: environment_of(environment.as_deref()),
                }),
                _ => None,
            };
            entries.push(Entry {
                rank: u32::try_from(rank).map_err(|_| unreadable("rank"))?,
                file_name: row.try_get("file_name").map_err(storage(RESOURCE))?,
                place: place_of(&place).ok_or_else(|| unreadable("place"))?,
                identity,
                origin: origin_of(&origin).ok_or_else(|| unreadable("origin"))?,
                size: u64::try_from(size).map_err(|_| unreadable("size_bytes"))?,
                sha256: row.try_get("sha256").map_err(storage(RESOURCE))?,
            });
        }
        Ok(Some(ModsRecord {
            set: RecordedSet {
                state: set_state_of(&state).ok_or_else(|| unreadable("state"))?,
                loader: loader.as_deref().and_then(loader_of),
                minecraft: head.try_get("minecraft").map_err(storage(RESOURCE))?,
                active: count("active")?,
                disabled: count("disabled")?,
                retired: count("retired")?,
                not_listed: count("not_listed")?,
                entries,
            },
            at: parse_date(RESOURCE, &at)?,
        }))
    }

    async fn mods_summaries(
        &self,
        key: &ServiceKey,
    ) -> Result<std::collections::HashMap<String, ModsSummaryRow>, StoreError> {
        let rows = sqlx::query(
            "SELECT backup_id, state, active, disabled, retired FROM backup_mod_sets WHERE service_key = ?",
        )
        .bind(key.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(storage(RESOURCE))?;
        let mut out = std::collections::HashMap::with_capacity(rows.len());
        for row in &rows {
            let id: String = row.try_get("backup_id").map_err(storage(RESOURCE))?;
            let state: String = row.try_get("state").map_err(storage(RESOURCE))?;
            let count = |column: &str| -> Result<u32, StoreError> {
                let value: i64 = row.try_get(column).map_err(storage(RESOURCE))?;
                u32::try_from(value).map_err(|_| unreadable(column))
            };
            out.insert(
                id,
                ModsSummaryRow {
                    state: set_state_of(&state).ok_or_else(|| unreadable("state"))?,
                    active: count("active")?,
                    disabled: count("disabled")?,
                    retired: count("retired")?,
                },
            );
        }
        Ok(out)
    }

    async fn writing(&self) -> Result<Vec<BackupRow>, StoreError> {
        sqlx::query(concat!(
            "SELECT ",
            columns!(),
            " FROM backups WHERE state = 'writing' ORDER BY created_at, id"
        ))
        .fetch_all(&self.pool)
        .await
        .map_err(storage(RESOURCE))?
        .iter()
        .map(row_of)
        .collect()
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

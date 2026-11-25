//! Le registre d'origine des mods, en SQLite (`service_mod_origins`, HRT-256, ADR-0062).

use async_trait::async_trait;
use sqlx::{Row, SqlitePool};
use time::OffsetDateTime;

use super::convert::{format_date, storage};
use super::store::SqliteUnitOfWork;
use crate::application::ports::{
    HearthFile, MAX_ORIGIN_ROWS, ModOrigins, ModOriginsTx, StoreError,
};
use crate::domain::mods::game_info::OriginRow;
use crate::domain::services::ServiceKey;

const RESOURCE: &str = "service_mod_origins";

pub struct SqliteModOrigins {
    pool: SqlitePool,
}

impl SqliteModOrigins {
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

#[async_trait]
impl ModOriginsTx for SqliteUnitOfWork {
    async fn record_hearth(
        &mut self,
        key: &ServiceKey,
        files: &[HearthFile],
        at: OffsetDateTime,
    ) -> Result<(), StoreError> {
        let at = format_date(RESOURCE, at)?;
        for file in files {
            let Ok(size) = i64::try_from(file.size) else {
                continue;
            };
            if size == 0 || file.sha256.len() != 64 {
                continue;
            }
            sqlx::query(
                "INSERT OR REPLACE INTO service_mod_origins
                 (service_key, file_name, size_bytes, crc32, sha256, origin, recorded_at)
                 VALUES (?, ?, ?, NULL, ?, 'hearth', ?)",
            )
            .bind(key.as_str())
            .bind(&file.file_name)
            .bind(size)
            .bind(&file.sha256)
            .bind(&at)
            .execute(&mut *self.tx)
            .await
            .map_err(storage(RESOURCE))?;
        }
        // Au plus `MAX_ORIGIN_ROWS` lignes par service : les plus anciennes lignes `hearth` sont oubliées.
        sqlx::query(
            "DELETE FROM service_mod_origins
             WHERE service_key = ?1 AND origin = 'hearth' AND rowid IN (
                 SELECT rowid FROM service_mod_origins WHERE service_key = ?1
                 ORDER BY recorded_at DESC, rowid DESC LIMIT -1 OFFSET ?2)",
        )
        .bind(key.as_str())
        .bind(i64::try_from(MAX_ORIGIN_ROWS).unwrap_or(i64::MAX))
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(())
    }
}

#[async_trait]
impl ModOrigins for SqliteModOrigins {
    async fn rows(&self, key: &ServiceKey) -> Result<Vec<OriginRow>, StoreError> {
        let rows = sqlx::query(
            "SELECT file_name, size_bytes, crc32, sha256, origin FROM service_mod_origins
             WHERE service_key = ? ORDER BY recorded_at DESC LIMIT ?",
        )
        .bind(key.as_str())
        .bind(i64::try_from(MAX_ORIGIN_ROWS).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(storage(RESOURCE))?;
        rows.iter()
            .map(|row| {
                let origin: String = row.try_get("origin").map_err(storage(RESOURCE))?;
                let size: i64 = row.try_get("size_bytes").map_err(storage(RESOURCE))?;
                let crc: Option<i64> = row.try_get("crc32").map_err(storage(RESOURCE))?;
                Ok(OriginRow {
                    file_name: row.try_get("file_name").map_err(storage(RESOURCE))?,
                    size: u64::try_from(size).map_err(|_| unreadable("size_bytes"))?,
                    crc32: crc
                        .map(|value| u32::try_from(value).map_err(|_| unreadable("crc32")))
                        .transpose()?,
                    sha256: row.try_get("sha256").map_err(storage(RESOURCE))?,
                    from_hearth: match origin.as_str() {
                        "hearth" => true,
                        "server_pack" => false,
                        _ => return Err(unreadable("origin")),
                    },
                })
            })
            .collect()
    }

    async fn record_server_pack(
        &self,
        key: &ServiceKey,
        rows: &[OriginRow],
        at: OffsetDateTime,
    ) -> Result<(), StoreError> {
        let at = format_date(RESOURCE, at)?;
        let mut tx = self.pool.begin().await.map_err(storage(RESOURCE))?;
        for row in rows.iter().take(MAX_ORIGIN_ROWS) {
            let (Some(crc), Ok(size)) = (row.crc32, i64::try_from(row.size)) else {
                continue;
            };
            sqlx::query(
                "INSERT OR IGNORE INTO service_mod_origins
                 (service_key, file_name, size_bytes, crc32, sha256, origin, recorded_at)
                 VALUES (?, ?, ?, ?, NULL, 'server_pack', ?)",
            )
            .bind(key.as_str())
            .bind(&row.file_name)
            .bind(size)
            .bind(i64::from(crc))
            .bind(&at)
            .execute(&mut *tx)
            .await
            .map_err(storage(RESOURCE))?;
        }
        tx.commit().await.map_err(storage(RESOURCE))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::sqlite::Database;

    fn key() -> ServiceKey {
        ServiceKey::parse("h_01JZ0000000000000000000101").expect("clé")
    }

    #[tokio::test]
    async fn pack_rows_are_recorded_once_and_read_back() {
        let dir = crate::infrastructure::data_dir::private_tempdir();
        let db = Database::open(dir.path()).await.unwrap();
        let repo = SqliteModOrigins::new(db.pool().clone());
        let row = OriginRow {
            file_name: "a.jar".into(),
            size: 10,
            crc32: Some(u32::MAX),
            sha256: None,
            from_hearth: false,
        };
        let now = OffsetDateTime::now_utc();
        repo.record_server_pack(&key(), std::slice::from_ref(&row), now)
            .await
            .unwrap();
        repo.record_server_pack(&key(), std::slice::from_ref(&row), now)
            .await
            .unwrap();
        let rows = repo.rows(&key()).await.unwrap();
        assert_eq!(rows, vec![row]);
    }

    #[tokio::test]
    async fn the_migration_accepts_the_mods_kind_and_the_backup_tables_refuse_bad_rows() {
        let dir = crate::infrastructure::data_dir::private_tempdir();
        let db = Database::open(dir.path()).await.unwrap();
        let pool = db.pool();
        let insert = |kind: &'static str| {
            sqlx::query(
                "INSERT INTO service_ops (id, op_key, service_key, service_name, kind, account, plan_json, status, started_at, updated_at)
                 VALUES ('op1', 'k', 'h_x', 'n', ?, 'a', '{}', 'succeeded', 't', 't')",
            )
            .bind(kind)
        };
        insert("mods").execute(pool).await.unwrap();
        assert!(insert("nope").execute(pool).await.is_err());
        let refused = sqlx::query(
            "INSERT INTO backup_mod_sets (backup_id, service_key, state, recorded_at) VALUES ('short', 'h_x', 'recorded', 't')",
        )
        .execute(pool)
        .await;
        assert!(refused.is_err(), "identifiant de sauvegarde trop court");
        let refused = sqlx::query(
            "INSERT INTO backup_mod_entries (backup_id, rank, place, origin, size_bytes, sha256)
             VALUES ('01J00000000000000000000000', 0, 'active', 'manual', 1, 'x')",
        )
        .execute(pool)
        .await;
        assert!(refused.is_err(), "empreinte trop courte");
    }
}

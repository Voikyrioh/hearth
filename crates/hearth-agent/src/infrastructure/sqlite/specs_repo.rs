//! Les descriptions durables des services, en SQLite (`service_specs`, `service_spec_secrets`, HRT-83,
//! ADR-0044), et la vérification de la clé de scellement (`meta.services_key_check`).
//!
//! Ce dépôt ne connaît NI la clé NI un secret en clair : il reçoit des scellés (nonce + texte chiffré)
//! et les range. Même le nom d'un champ scellé (`env:3`) ne dit rien du contenu. Le nom d'un service, son
//! image, ses ports, ses montages sont en clair (ils ne sont pas secrets). Une révision et ses scellés
//! s'écrivent dans UNE transaction avec la mise hors-courant de la précédente.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use super::convert::{format_date, is_unique_violation, parse_date, storage};
use crate::application::ports::{
    KeyCheckRepo, NewRevision, SealedField, ServiceSpecRepo, SpecStoreError, StoreError,
    StoredRevision,
};
use crate::domain::sealed::{DatabaseFacts, NONCE_LEN, SealedBlob, SecretField};
use crate::domain::services::key::ServiceKey;
use crate::domain::services::spec::{
    SpecBody, SpecFiche, SpecImage, SpecMount, SpecPort, SpecRestart, SpecSettings, fold_name,
};

const RESOURCE: &str = "service_specs";
/// La clé de `meta` qui garde la vérification de la clé de scellement.
const KEY_CHECK_META: &str = "services_key_check";

pub struct SqliteSpecRepo {
    pool: SqlitePool,
}

impl SqliteSpecRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

/// Ce qui va dans `spec_json` : la description sans ses réglages ni sa fiche (qui ont leurs colonnes).
#[derive(Serialize, Deserialize)]
struct StoredSpec {
    image: SpecImage,
    #[serde(default)]
    ports: Vec<SpecPort>,
    #[serde(default)]
    mounts: Vec<SpecMount>,
    restart: SpecRestart,
    #[serde(default)]
    stop_grace_s: Option<u32>,
    #[serde(default)]
    stdin_open: bool,
    #[serde(default)]
    cpu_millis: Option<u32>,
    #[serde(default)]
    memory_bytes: Option<u64>,
}

struct SpecRow {
    service_key: String,
    rev: i64,
    name: String,
    spec_json: String,
    settings_json: String,
    fiche_id: Option<String>,
    fiche_version: Option<i64>,
    fiche_digest: Option<String>,
    updated_by: String,
    created_at: String,
    current: i64,
}

fn unreadable(what: &str) -> StoreError {
    StoreError::Unavailable {
        resource: RESOURCE,
        message: format!("{what} illisible"),
    }
}

fn into_revision(row: SpecRow, sealed: Vec<SealedField>) -> Result<StoredRevision, StoreError> {
    let stored: StoredSpec =
        serde_json::from_str(&row.spec_json).map_err(|_| unreadable("spec_json"))?;
    let settings: SpecSettings =
        serde_json::from_str(&row.settings_json).map_err(|_| unreadable("settings_json"))?;
    let fiche = match (row.fiche_id, row.fiche_version, row.fiche_digest) {
        (Some(id), Some(version), Some(digest)) => Some(SpecFiche {
            id,
            version: u32::try_from(version).map_err(|_| unreadable("fiche_version"))?,
            digest,
        }),
        (None, None, None) => None,
        _ => return Err(unreadable("fiche")),
    };
    Ok(StoredRevision {
        key: ServiceKey::parse(&row.service_key).ok_or_else(|| unreadable("service_key"))?,
        rev: u32::try_from(row.rev).map_err(|_| unreadable("rev"))?,
        body: SpecBody {
            name: row.name,
            image: stored.image,
            ports: stored.ports,
            mounts: stored.mounts,
            restart: stored.restart,
            stop_grace_s: stored.stop_grace_s,
            stdin_open: stored.stdin_open,
            cpu_millis: stored.cpu_millis,
            memory_bytes: stored.memory_bytes,
            settings,
            fiche,
        },
        updated_by: row.updated_by,
        created_at: parse_date(RESOURCE, &row.created_at)?,
        current: row.current == 1,
        sealed,
    })
}

async fn sealed_of(pool: &SqlitePool, key: &str, rev: i64) -> Result<Vec<SealedField>, StoreError> {
    let rows = sqlx::query!(
        "SELECT field, nonce, sealed FROM service_spec_secrets
         WHERE service_key = ? AND rev = ? ORDER BY field",
        key,
        rev
    )
    .fetch_all(pool)
    .await
    .map_err(storage(RESOURCE))?;
    rows.into_iter()
        .map(|row| {
            let nonce: [u8; NONCE_LEN] = row
                .nonce
                .as_slice()
                .try_into()
                .map_err(|_| unreadable("nonce"))?;
            Ok(SealedField {
                field: SecretField::parse(&row.field).ok_or_else(|| unreadable("field"))?,
                blob: SealedBlob {
                    nonce,
                    bytes: row.sealed,
                },
            })
        })
        .collect()
}

#[async_trait]
impl ServiceSpecRepo for SqliteSpecRepo {
    async fn append_revision(&self, new: NewRevision) -> Result<u32, SpecStoreError> {
        let key = new.key.as_str();
        let body = &new.body;
        let stored = StoredSpec {
            image: body.image.clone(),
            ports: body.ports.clone(),
            mounts: body.mounts.clone(),
            restart: body.restart,
            stop_grace_s: body.stop_grace_s,
            stdin_open: body.stdin_open,
            cpu_millis: body.cpu_millis,
            memory_bytes: body.memory_bytes,
        };
        let spec_json = serde_json::to_string(&stored).map_err(|_| unreadable("spec_json"))?;
        let settings_json =
            serde_json::to_string(&body.settings).map_err(|_| unreadable("settings_json"))?;
        let (fiche_id, fiche_version, fiche_digest) = match &body.fiche {
            Some(fiche) => (
                Some(fiche.id.as_str()),
                Some(i64::from(fiche.version)),
                Some(fiche.digest.as_str()),
            ),
            None => (None, None, None),
        };
        let created_at = format_date(RESOURCE, new.at)?;
        let name_folded = fold_name(&body.name);

        let mut tx = self.pool.begin().await.map_err(storage(RESOURCE))?;
        let current: Option<i64> = sqlx::query_scalar!(
            "SELECT rev FROM service_specs WHERE service_key = ? AND current = 1",
            key
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage(RESOURCE))?;
        let current = current.and_then(|rev| u32::try_from(rev).ok());
        if current != new.expected_rev {
            return Err(SpecStoreError::Conflict { current });
        }
        let rev = current.unwrap_or(0).saturating_add(1);
        let rev_db = i64::from(rev);
        sqlx::query!(
            "UPDATE service_specs SET current = 0 WHERE service_key = ? AND current = 1",
            key
        )
        .execute(&mut *tx)
        .await
        .map_err(storage(RESOURCE))?;
        let inserted = sqlx::query!(
            "INSERT INTO service_specs
                 (service_key, rev, name, name_folded, spec_json, settings_json, fiche_id,
                  fiche_version, fiche_digest, updated_by, created_at, current)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1)",
            key,
            rev_db,
            body.name,
            name_folded,
            spec_json,
            settings_json,
            fiche_id,
            fiche_version,
            fiche_digest,
            new.by,
            created_at
        )
        .execute(&mut *tx)
        .await;
        match inserted {
            Ok(_) => {}
            Err(error) if is_unique_violation(&error) => return Err(SpecStoreError::NameTaken),
            Err(error) => return Err(storage(RESOURCE)(error).into()),
        }
        for SealedField { field, blob } in &new.sealed {
            let field = field.as_text();
            let nonce = blob.nonce.as_slice();
            let bytes = blob.bytes.as_slice();
            sqlx::query!(
                "INSERT INTO service_spec_secrets (service_key, rev, field, nonce, sealed)
                 VALUES (?, ?, ?, ?, ?)",
                key,
                rev_db,
                field,
                nonce,
                bytes
            )
            .execute(&mut *tx)
            .await
            .map_err(storage(RESOURCE))?;
        }
        tx.commit().await.map_err(storage(RESOURCE))?;
        Ok(rev)
    }

    async fn current(&self, key: &ServiceKey) -> Result<Option<StoredRevision>, StoreError> {
        let key = key.as_str();
        let row = sqlx::query_as!(
            SpecRow,
            "SELECT service_key, rev, name, spec_json, settings_json, fiche_id, fiche_version,
                    fiche_digest, updated_by, created_at, current
             FROM service_specs WHERE service_key = ? AND current = 1",
            key
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(storage(RESOURCE))?;
        match row {
            Some(row) => {
                let sealed = sealed_of(&self.pool, &row.service_key, row.rev).await?;
                into_revision(row, sealed).map(Some)
            }
            None => Ok(None),
        }
    }

    async fn revision(
        &self,
        key: &ServiceKey,
        rev: u32,
    ) -> Result<Option<StoredRevision>, StoreError> {
        let key = key.as_str();
        let rev = i64::from(rev);
        let row = sqlx::query_as!(
            SpecRow,
            "SELECT service_key, rev, name, spec_json, settings_json, fiche_id, fiche_version,
                    fiche_digest, updated_by, created_at, current
             FROM service_specs WHERE service_key = ? AND rev = ?",
            key,
            rev
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(storage(RESOURCE))?;
        match row {
            Some(row) => {
                let sealed = sealed_of(&self.pool, &row.service_key, row.rev).await?;
                into_revision(row, sealed).map(Some)
            }
            None => Ok(None),
        }
    }

    async fn revisions(&self, key: &ServiceKey) -> Result<Vec<u32>, StoreError> {
        let key = key.as_str();
        let revs = sqlx::query_scalar!(
            "SELECT rev FROM service_specs WHERE service_key = ? ORDER BY rev",
            key
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage(RESOURCE))?;
        revs.into_iter()
            .map(|rev| u32::try_from(rev).map_err(|_| unreadable("rev")))
            .collect()
    }

    async fn holder_of_name(&self, folded: &str) -> Result<Option<ServiceKey>, StoreError> {
        let key = sqlx::query_scalar!(
            "SELECT service_key FROM service_specs WHERE name_folded = ? AND current = 1",
            folded
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(storage(RESOURCE))?;
        key.map(|key| ServiceKey::parse(&key).ok_or_else(|| unreadable("service_key")))
            .transpose()
    }

    async fn discard_before(&self, key: &ServiceKey, rev: u32) -> Result<u64, StoreError> {
        let key = key.as_str();
        let rev = i64::from(rev);
        let mut tx = self.pool.begin().await.map_err(storage(RESOURCE))?;
        sqlx::query!(
            "DELETE FROM service_spec_secrets
             WHERE service_key = ? AND rev < ?
               AND rev NOT IN (SELECT rev FROM service_specs WHERE service_key = ? AND current = 1)",
            key,
            rev,
            key
        )
        .execute(&mut *tx)
        .await
        .map_err(storage(RESOURCE))?;
        let done = sqlx::query!(
            "DELETE FROM service_specs WHERE service_key = ? AND rev < ? AND current = 0",
            key,
            rev
        )
        .execute(&mut *tx)
        .await
        .map_err(storage(RESOURCE))?;
        tx.commit().await.map_err(storage(RESOURCE))?;
        Ok(done.rows_affected())
    }

    async fn forget(&self, key: &ServiceKey) -> Result<u64, StoreError> {
        let key = key.as_str();
        let mut tx = self.pool.begin().await.map_err(storage(RESOURCE))?;
        sqlx::query!(
            "DELETE FROM service_spec_secrets WHERE service_key = ?",
            key
        )
        .execute(&mut *tx)
        .await
        .map_err(storage(RESOURCE))?;
        let done = sqlx::query!("DELETE FROM service_specs WHERE service_key = ?", key)
            .execute(&mut *tx)
            .await
            .map_err(storage(RESOURCE))?;
        tx.commit().await.map_err(storage(RESOURCE))?;
        Ok(done.rows_affected())
    }
}

#[async_trait]
impl KeyCheckRepo for SqliteSpecRepo {
    async fn database_facts(&self) -> Result<DatabaseFacts, StoreError> {
        let sealed_rows: i64 = sqlx::query_scalar!(
            r#"SELECT COUNT(*) AS "n!: i64" FROM (SELECT 1 FROM service_spec_secrets LIMIT 1)"#
        )
        .fetch_one(&self.pool)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(DatabaseFacts {
            sealed_rows: sealed_rows > 0,
            check_stored: self.stored_key_check().await?.is_some(),
        })
    }

    async fn stored_key_check(&self) -> Result<Option<String>, StoreError> {
        sqlx::query_scalar!("SELECT value FROM meta WHERE key = ?", KEY_CHECK_META)
            .fetch_optional(&self.pool)
            .await
            .map_err(storage(RESOURCE))
    }

    async fn store_key_check(&self, check: &str) -> Result<String, StoreError> {
        sqlx::query!(
            "INSERT OR IGNORE INTO meta (key, value) VALUES (?, ?)",
            KEY_CHECK_META,
            check
        )
        .execute(&self.pool)
        .await
        .map_err(storage(RESOURCE))?;
        self.stored_key_check()
            .await?
            .ok_or_else(|| unreadable("services_key_check"))
    }
}

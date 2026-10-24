//! Les réglages voulus d'un serveur de jeu, en SQLite (`game_settings`, `game_setting_secrets`, HRT-108,
//! ADR-0051). Ce dépôt ne connaît NI la clé NI un secret en clair : il range des scellés (nonce + texte chiffré).
//! `answers_json` ne porte jamais qu'une valeur non secrète ou le marqueur « secret défini ».

use std::collections::BTreeMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use super::convert::{format_date, is_unique_violation, parse_date, storage};
use crate::application::ports::{
    GameSettingsRepo, NewSettings, SettingsStoreError, StoreError, StoredSettings,
};
use crate::domain::catalog::settings_state::{SettingsState, Slot};
use crate::domain::sealed::{NONCE_LEN, SealedBlob};
use crate::domain::services::key::ServiceKey;

const RESOURCE: &str = "game_settings";

pub struct SqliteGameSettingsRepo {
    pool: SqlitePool,
}

impl SqliteGameSettingsRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

/// Une ligne de `answers_json` : `{"id":"motd","v":"Bonjour"}` ou `{"id":"password","s":true}`. Un réglage
/// « non écrit » n'a pas de ligne.
#[derive(Serialize, Deserialize)]
struct StoredAnswer {
    id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    v: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    s: bool,
}

fn unreadable(what: &str) -> StoreError {
    StoreError::Unavailable {
        resource: RESOURCE,
        message: format!("{what} illisible"),
    }
}

fn encode(state: &SettingsState) -> Result<String, StoreError> {
    let answers: Vec<StoredAnswer> = state
        .iter()
        .filter_map(|(id, slot)| match slot {
            Slot::Value(v) => Some(StoredAnswer {
                id: id.to_owned(),
                v: Some(v.clone()),
                s: false,
            }),
            Slot::Secret => Some(StoredAnswer {
                id: id.to_owned(),
                v: None,
                s: true,
            }),
            Slot::Unwritten => None,
        })
        .collect();
    serde_json::to_string(&answers).map_err(|_| unreadable("answers_json"))
}

fn decode(text: &str) -> Result<SettingsState, StoreError> {
    let answers: Vec<StoredAnswer> =
        serde_json::from_str(text).map_err(|_| unreadable("answers_json"))?;
    let mut slots = BTreeMap::new();
    for answer in answers {
        let slot = match (answer.s, answer.v) {
            (true, _) => Slot::Secret,
            (false, Some(v)) => Slot::Value(v),
            (false, None) => Slot::Unwritten,
        };
        slots.insert(answer.id, slot);
    }
    Ok(SettingsState::from_slots(slots))
}

#[async_trait]
impl GameSettingsRepo for SqliteGameSettingsRepo {
    async fn load(&self, key: &ServiceKey) -> Result<Option<StoredSettings>, StoreError> {
        let Some(row) = sqlx::query(
            "SELECT settings_rev, answers_json, file_fingerprint, updated_by, updated_at
             FROM game_settings WHERE service_key = ?",
        )
        .bind(key.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage(RESOURCE))?
        else {
            return Ok(None);
        };
        let rev: i64 = row.try_get("settings_rev").map_err(storage(RESOURCE))?;
        let answers: String = row.try_get("answers_json").map_err(storage(RESOURCE))?;
        let updated_at: String = row.try_get("updated_at").map_err(storage(RESOURCE))?;
        let secrets = sqlx::query(
            "SELECT setting_id, nonce, sealed FROM game_setting_secrets
             WHERE service_key = ? ORDER BY setting_id",
        )
        .bind(key.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(storage(RESOURCE))?;
        let mut sealed = Vec::with_capacity(secrets.len());
        for secret in secrets {
            let id: String = secret.try_get("setting_id").map_err(storage(RESOURCE))?;
            let nonce: Vec<u8> = secret.try_get("nonce").map_err(storage(RESOURCE))?;
            let bytes: Vec<u8> = secret.try_get("sealed").map_err(storage(RESOURCE))?;
            let nonce: [u8; NONCE_LEN] = nonce.try_into().map_err(|_| unreadable("nonce"))?;
            sealed.push((id, SealedBlob { nonce, bytes }));
        }
        Ok(Some(StoredSettings {
            rev: u32::try_from(rev).map_err(|_| unreadable("settings_rev"))?,
            state: decode(&answers)?,
            file_fingerprint: row.try_get("file_fingerprint").map_err(storage(RESOURCE))?,
            sealed,
            updated_by: row.try_get("updated_by").map_err(storage(RESOURCE))?,
            updated_at: parse_date(RESOURCE, &updated_at)?,
        }))
    }

    async fn save(&self, new: NewSettings) -> Result<u32, SettingsStoreError> {
        let key = new.key.as_str();
        let answers = encode(&new.state)?;
        let at = format_date(RESOURCE, new.at)?;
        let mut tx = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(storage(RESOURCE))?;
        let current = sqlx::query(
            "SELECT settings_rev, updated_by, updated_at FROM game_settings WHERE service_key = ?",
        )
        .bind(key)
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage(RESOURCE))?;
        let current_rev = match &current {
            Some(row) => {
                let rev: i64 = row.try_get("settings_rev").map_err(storage(RESOURCE))?;
                Some(u32::try_from(rev).map_err(|_| unreadable("settings_rev"))?)
            }
            None => None,
        };
        if current_rev != new.expected_rev {
            let (by, when) = match &current {
                Some(row) => {
                    let by: String = row.try_get("updated_by").map_err(storage(RESOURCE))?;
                    let text: String = row.try_get("updated_at").map_err(storage(RESOURCE))?;
                    (Some(by), parse_date(RESOURCE, &text).ok())
                }
                None => (None, None),
            };
            return Err(SettingsStoreError::Conflict {
                current: current_rev,
                by,
                at: when,
            });
        }
        let rev = current_rev.unwrap_or(0).saturating_add(1);
        if current_rev.is_some() {
            sqlx::query(
                "UPDATE game_settings
                 SET settings_rev = ?, answers_json = ?, file_fingerprint = ?, updated_by = ?, updated_at = ?
                 WHERE service_key = ?",
            )
            .bind(i64::from(rev))
            .bind(&answers)
            .bind(&new.file_fingerprint)
            .bind(&new.by)
            .bind(&at)
            .bind(key)
            .execute(&mut *tx)
            .await
            .map_err(storage(RESOURCE))?;
        } else {
            sqlx::query(
                "INSERT INTO game_settings
                     (service_key, settings_rev, answers_json, file_fingerprint, updated_by, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(key)
            .bind(i64::from(rev))
            .bind(&answers)
            .bind(&new.file_fingerprint)
            .bind(&new.by)
            .bind(&at)
            .execute(&mut *tx)
            .await
            .map_err(storage(RESOURCE))?;
        }
        // Un réglage qui n'est plus un secret défini perd son scellé ; un secret changé reçoit le sien.
        let kept: Vec<&str> = new
            .state
            .iter()
            .filter(|(_, slot)| **slot == Slot::Secret)
            .map(|(id, _)| id)
            .collect();
        let existing =
            sqlx::query("SELECT setting_id FROM game_setting_secrets WHERE service_key = ?")
                .bind(key)
                .fetch_all(&mut *tx)
                .await
                .map_err(storage(RESOURCE))?;
        for row in existing {
            let id: String = row.try_get("setting_id").map_err(storage(RESOURCE))?;
            if !kept.contains(&id.as_str()) {
                sqlx::query(
                    "DELETE FROM game_setting_secrets WHERE service_key = ? AND setting_id = ?",
                )
                .bind(key)
                .bind(&id)
                .execute(&mut *tx)
                .await
                .map_err(storage(RESOURCE))?;
            }
        }
        for (id, blob) in &new.sealed {
            sqlx::query(
                "INSERT INTO game_setting_secrets (service_key, setting_id, format, nonce, sealed)
                 VALUES (?, ?, 1, ?, ?)
                 ON CONFLICT (service_key, setting_id)
                 DO UPDATE SET format = 1, nonce = excluded.nonce, sealed = excluded.sealed",
            )
            .bind(key)
            .bind(id)
            .bind(blob.nonce.as_slice())
            .bind(&blob.bytes)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                if is_unique_violation(&error) {
                    StoreError::Duplicate { resource: RESOURCE }
                } else {
                    storage(RESOURCE)(error)
                }
            })?;
        }
        tx.commit().await.map_err(storage(RESOURCE))?;
        Ok(rev)
    }

    async fn forget(&self, key: &ServiceKey) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM game_settings WHERE service_key = ?")
            .bind(key.as_str())
            .execute(&self.pool)
            .await
            .map_err(storage(RESOURCE))?;
        Ok(())
    }
}

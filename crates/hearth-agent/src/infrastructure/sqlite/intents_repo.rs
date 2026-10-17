//! Les arrêts voulus depuis Hearth (`service_intents`, HRT-64, BR-LIFE-010, BR-SVC-005). Une ligne par
//! service, écrite AVANT l'ordre d'arrêt, effacée quand Hearth démarre le service ou qu'il est revu en
//! marche. Les requêtes sont dynamiques (`sqlx::query`) : elles ne portent que des valeurs liées.

use async_trait::async_trait;
use sqlx::Row;
use time::OffsetDateTime;

use super::convert::{format_date, parse_date, storage};
use super::services_repo::SqliteServiceRepo;
use crate::application::ports::{ServiceIntents, StopIntent, StoreError};
use crate::domain::services::ServiceKey;

const RESOURCE: &str = "service_intents";

#[async_trait]
impl ServiceIntents for SqliteServiceRepo {
    async fn load(&self) -> Result<Vec<StopIntent>, StoreError> {
        let rows = sqlx::query(
            "SELECT service_key, container_id, account, at FROM service_intents ORDER BY service_key",
        )
        .fetch_all(self.pool())
        .await
        .map_err(storage(RESOURCE))?;
        let mut intents = Vec::with_capacity(rows.len());
        for row in rows {
            let key: String = row.try_get("service_key").map_err(storage(RESOURCE))?;
            let at: String = row.try_get("at").map_err(storage(RESOURCE))?;
            // Une ligne illisible est omise et tracée, jamais fatale : elle fait seulement afficher « En
            // erreur » au lieu d'« Arrêté » (la trace ne dit ni la clé ni la date).
            let (Some(key), Ok(at)) = (ServiceKey::parse(&key), parse_date(RESOURCE, &at)) else {
                tracing::warn!("arrêt voulu illisible : ligne ignorée");
                continue;
            };
            intents.push(StopIntent {
                key,
                container_id: row.try_get("container_id").map_err(storage(RESOURCE))?,
                account: row.try_get("account").map_err(storage(RESOURCE))?,
                at,
            });
        }
        Ok(intents)
    }

    async fn save(&self, intent: &StopIntent) -> Result<(), StoreError> {
        let at = format_date(RESOURCE, intent.at)?;
        sqlx::query(
            "INSERT INTO service_intents (service_key, container_id, intent, account, at)
             VALUES (?, ?, 'stopped', ?, ?)
             ON CONFLICT (service_key) DO UPDATE SET
                 container_id = excluded.container_id,
                 account = excluded.account,
                 at = excluded.at",
        )
        .bind(intent.key.as_str())
        .bind(&intent.container_id)
        .bind(&intent.account)
        .bind(at)
        .execute(self.pool())
        .await
        .map_err(storage(RESOURCE))?;
        Ok(())
    }

    async fn clear(&self, key: &ServiceKey) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM service_intents WHERE service_key = ?")
            .bind(key.as_str())
            .execute(self.pool())
            .await
            .map_err(storage(RESOURCE))?;
        Ok(())
    }

    async fn clear_if(&self, key: &ServiceKey, at: OffsetDateTime) -> Result<(), StoreError> {
        // La date est écrite et relue par le même format : l'égalité de texte est celle des instants.
        let at = format_date(RESOURCE, at)?;
        sqlx::query("DELETE FROM service_intents WHERE service_key = ? AND at = ?")
            .bind(key.as_str())
            .bind(at)
            .execute(self.pool())
            .await
            .map_err(storage(RESOURCE))?;
        Ok(())
    }
}

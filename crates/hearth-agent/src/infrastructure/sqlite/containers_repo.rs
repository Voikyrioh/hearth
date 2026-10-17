//! Les conteneurs créés par l'agent pour ses services (`service_containers`, HRT-68, ADR-0037). Une ligne
//! par conteneur : son identifiant (rendu par le moteur) et la clé `h_` du service. Aucun secret.

use async_trait::async_trait;
use sqlx::Row;
use time::OffsetDateTime;

use super::convert::{format_date, storage};
use super::services_repo::SqliteServiceRepo;
use crate::application::ports::{ServiceContainers, StoreError};
use crate::domain::services::{KeyKind, ServiceKey};

const RESOURCE: &str = "service_containers";

#[async_trait]
impl ServiceContainers for SqliteServiceRepo {
    async fn record(
        &self,
        key: &ServiceKey,
        container_id: &str,
        at: OffsetDateTime,
    ) -> Result<(), StoreError> {
        let at = format_date(RESOURCE, at)?;
        sqlx::query(
            "INSERT INTO service_containers (container_id, service_key, recorded_at)
             VALUES (?, ?, ?)
             ON CONFLICT (container_id) DO NOTHING",
        )
        .bind(container_id)
        .bind(key.as_str())
        .bind(at)
        .execute(self.pool())
        .await
        .map_err(storage(RESOURCE))?;
        Ok(())
    }

    async fn all(&self) -> Result<Vec<(String, ServiceKey)>, StoreError> {
        let rows = sqlx::query("SELECT container_id, service_key FROM service_containers")
            .fetch_all(self.pool())
            .await
            .map_err(storage(RESOURCE))?;
        let mut found = Vec::with_capacity(rows.len());
        for row in rows {
            let id: String = row.try_get("container_id").map_err(storage(RESOURCE))?;
            let key: String = row.try_get("service_key").map_err(storage(RESOURCE))?;
            // Une ligne illisible est omise : le conteneur redevient un conteneur hors Hearth (tracé sans clé).
            match ServiceKey::parse(&key).filter(|key| key.kind() == KeyKind::Hearth) {
                Some(key) => found.push((id, key)),
                None => tracing::warn!("conteneur consigné illisible : ligne ignorée"),
            }
        }
        Ok(found)
    }

    async fn of(&self, key: &ServiceKey) -> Result<Vec<String>, StoreError> {
        let rows = sqlx::query("SELECT container_id FROM service_containers WHERE service_key = ?")
            .bind(key.as_str())
            .fetch_all(self.pool())
            .await
            .map_err(storage(RESOURCE))?;
        rows.into_iter()
            .map(|row| row.try_get("container_id").map_err(storage(RESOURCE)))
            .collect()
    }

    async fn forget_container(&self, container_id: &str) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM service_containers WHERE container_id = ?")
            .bind(container_id)
            .execute(self.pool())
            .await
            .map_err(storage(RESOURCE))?;
        Ok(())
    }

    async fn forget_service(&self, key: &ServiceKey) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM service_containers WHERE service_key = ?")
            .bind(key.as_str())
            .execute(self.pool())
            .await
            .map_err(storage(RESOURCE))?;
        Ok(())
    }
}

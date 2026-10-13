//! Écriture de l'adoption d'un service : l'UNIQUE endroit qui écrit `adopted_services` pour un acte
//! (HRT-80, ADR-0043). La ligne est écrite dans l'unité de travail de l'acte, avec l'entrée du journal
//! d'activité : l'une n'existe pas sans l'autre. Les requêtes sont dynamiques (`sqlx::query`) : elles ne
//! portent que des valeurs liées, jamais un texte composé.
//!
//! L'oubli d'une adoption dont le conteneur n'existe plus (`forget_adopted`, `services_repo.rs`) est
//! l'autre écriture : elle n'est pas un acte, aucun compte n'agit (BR-ADOPT-019).

use async_trait::async_trait;
use sqlx::Row;

use super::convert::{format_date, storage};
use super::store::SqliteUnitOfWork;
use crate::application::ports::{
    AdoptionTx, AdoptionWrite, NewAdoption, ReleasedAdoption, StoreError,
};
use crate::domain::services::ServiceKey;

const RESOURCE: &str = "adopted_services";

#[async_trait]
impl AdoptionTx for SqliteUnitOfWork {
    async fn adopt(&mut self, adoption: &NewAdoption) -> Result<AdoptionWrite, StoreError> {
        let at = format_date(RESOURCE, adoption.at)?;
        let written = sqlx::query(
            "INSERT INTO adopted_services (service_key, name, unit, adopted_by, adopted_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT (service_key) DO NOTHING",
        )
        .bind(adoption.key.as_str())
        .bind(&adoption.name)
        .bind(adoption.unit.as_deref())
        .bind(&adoption.by)
        .bind(at)
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?
        .rows_affected();
        if written == 1 {
            return Ok(AdoptionWrite::Adopted);
        }
        let row = sqlx::query("SELECT adopted_by FROM adopted_services WHERE service_key = ?")
            .bind(adoption.key.as_str())
            .fetch_one(&mut *self.tx)
            .await
            .map_err(storage(RESOURCE))?;
        Ok(AdoptionWrite::AlreadyAdopted {
            by: row.try_get("adopted_by").map_err(storage(RESOURCE))?,
        })
    }

    async fn release(&mut self, key: &ServiceKey) -> Result<Option<ReleasedAdoption>, StoreError> {
        let row = sqlx::query("SELECT name, unit FROM adopted_services WHERE service_key = ?")
            .bind(key.as_str())
            .fetch_optional(&mut *self.tx)
            .await
            .map_err(storage(RESOURCE))?;
        let Some(row) = row else {
            return Ok(None);
        };
        let released = ReleasedAdoption {
            name: row.try_get("name").map_err(storage(RESOURCE))?,
            unit: row.try_get("unit").map_err(storage(RESOURCE))?,
        };
        sqlx::query("DELETE FROM adopted_services WHERE service_key = ?")
            .bind(key.as_str())
            .execute(&mut *self.tx)
            .await
            .map_err(storage(RESOURCE))?;
        Ok(Some(released))
    }
}

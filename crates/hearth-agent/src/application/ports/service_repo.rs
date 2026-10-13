use async_trait::async_trait;
use time::OffsetDateTime;

use super::StoreError;
use crate::domain::services::{Observation, UnitName};

/// Ce que l'agent retient d'un service supervisé pour le garder listé quand son conteneur n'existe plus
/// (`supervised_services`, ADR-0037) : le service est recréé par le système à chaque démarrage, avec
/// les mêmes réglages. L'observation retenue ne porte NI nom de variable, NI commande, NI valeur :
/// elle est nettoyée avant d'être confiée au dépôt.
#[derive(Debug, Clone, PartialEq)]
pub struct SupervisedRecord {
    pub unit: UnitName,
    pub observation: Observation,
    pub last_seen_at: OffsetDateTime,
}

/// Les services que l'agent retient d'un démarrage à l'autre.
#[async_trait]
pub trait ServiceRepo: Send + Sync {
    /// Tous les services supervisés retenus. Une ligne illisible est omise (tracée), pas fatale.
    async fn load_supervised(&self) -> Result<Vec<SupervisedRecord>, StoreError>;

    /// Retient (ou met à jour) un service supervisé, par son unité.
    async fn save_supervised(&self, record: &SupervisedRecord) -> Result<(), StoreError>;

    /// Oublie le service de cette unité.
    async fn forget_supervised(&self, unit: &UnitName) -> Result<(), StoreError>;
}

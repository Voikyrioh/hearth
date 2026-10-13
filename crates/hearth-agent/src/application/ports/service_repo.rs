use async_trait::async_trait;
use time::OffsetDateTime;

use super::StoreError;
use crate::domain::services::{Observation, ServiceKey, UnitName};

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

/// Un service adopté (`adopted_services`, HRT-79, ADR-0043) : l'identité stable choisie par le
/// propriétaire de faire gérer, le compte qui l'a adoptée et la date. Le registre ne lit que cela ;
/// le nom et l'unité de la ligne servent le journal d'activité de l'acte d'adoption (HRT-80).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdoptedRecord {
    pub key: ServiceKey,
    pub by: String,
    pub at: OffsetDateTime,
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

    /// Tous les services adoptés (HRT-79). Une ligne illisible est omise (tracée) : un service dont
    /// l'adoption est illisible n'est pas publié, jamais l'inverse.
    async fn load_adopted(&self) -> Result<Vec<AdoptedRecord>, StoreError>;

    /// Oublie l'adoption d'un service : le conteneur d'un service adopté sans unité n'existe plus
    /// (`BR-ADOPT-017`). La base oublie aussi ce qu'elle retenait du service.
    async fn forget_adopted(&self, key: &ServiceKey) -> Result<(), StoreError>;
}

use async_trait::async_trait;
use time::OffsetDateTime;

use super::StoreError;
use crate::domain::operations::{Operation, OperationKey, OperationStatus};

/// Lecture des opérations suivies par clé. Écritures : `UnitOfWork::operations`.
#[async_trait]
pub trait OperationRepo: Send + Sync {
    async fn find(&self, key: &OperationKey) -> Result<Option<Operation>, StoreError>;
}

#[async_trait]
pub trait OperationTx: Send {
    async fn find(&mut self, key: &OperationKey) -> Result<Option<Operation>, StoreError>;

    /// Enregistre une opération ; `StoreError::Duplicate` si la clé existe déjà.
    async fn insert(&mut self, operation: &Operation) -> Result<(), StoreError>;

    /// Termine l'opération : statut final et résultat (JSON de la réponse).
    async fn finish(
        &mut self,
        key: &OperationKey,
        status: OperationStatus,
        result_json: &str,
        at: OffsetDateTime,
    ) -> Result<(), StoreError>;

    /// Oublie l'opération (un résultat `5xx` ne se rejoue pas).
    async fn delete(&mut self, key: &OperationKey) -> Result<(), StoreError>;

    /// Oublie les opérations créées avant `before` ; rend leur nombre.
    async fn purge(&mut self, before: OffsetDateTime) -> Result<u64, StoreError>;
}

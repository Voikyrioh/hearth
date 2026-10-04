use async_trait::async_trait;
use time::OffsetDateTime;

use super::StoreError;
use crate::domain::accounts::AccountId;
use crate::domain::operations::{Operation, OperationKey, OperationStatus};

/// Lecture des opérations suivies par clé. Une clé est celle d'un compte. Écritures :
/// `UnitOfWork::operations`.
#[async_trait]
pub trait OperationRepo: Send + Sync {
    async fn find(
        &self,
        account: &AccountId,
        key: &OperationKey,
    ) -> Result<Option<Operation>, StoreError>;
}

#[async_trait]
pub trait OperationTx: Send {
    async fn find(
        &mut self,
        account: &AccountId,
        key: &OperationKey,
    ) -> Result<Option<Operation>, StoreError>;

    /// Enregistre une opération ; `StoreError::Duplicate` si ce compte a déjà cette clé.
    async fn insert(&mut self, operation: &Operation) -> Result<(), StoreError>;

    /// Termine l'opération : statut final et résultat (JSON de la réponse).
    async fn finish(
        &mut self,
        account: &AccountId,
        key: &OperationKey,
        status: OperationStatus,
        result_json: &str,
        at: OffsetDateTime,
    ) -> Result<(), StoreError>;

    /// Oublie l'opération (un résultat `5xx` ne se rejoue pas).
    async fn delete(&mut self, account: &AccountId, key: &OperationKey) -> Result<(), StoreError>;

    /// Passe toute opération « en cours » à « interrompue » (démarrage de l'agent : aucune
    /// exécution ne peut avoir survécu à l'arrêt) ; rend leur nombre.
    async fn interrupt_running(&mut self, at: OffsetDateTime) -> Result<u64, StoreError>;

    /// Oublie les opérations créées avant `before` ; rend leur nombre.
    async fn purge(&mut self, before: OffsetDateTime) -> Result<u64, StoreError>;
}

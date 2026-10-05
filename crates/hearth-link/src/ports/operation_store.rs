//! Opérations en suspens de chaque serveur, sur disque : elles survivent à un redémarrage de
//! l'application (une action coupée avant sa réponse reste « à annoncer »).

use async_trait::async_trait;

use super::server_store::StoreError;
use crate::domain::pending_ops::PendingOp;
use crate::domain::server::ServerId;

#[async_trait]
pub trait OperationStore: Send + Sync {
    /// Vide si rien n'est mémorisé ou si le fichier est illisible (avertissement).
    async fn load(&self, id: &ServerId) -> Result<Vec<PendingOp>, StoreError>;
    /// Remplace ; une liste vide efface.
    async fn save(&self, id: &ServerId, operations: &[PendingOp]) -> Result<(), StoreError>;
    async fn remove(&self, id: &ServerId) -> Result<(), StoreError>;
}

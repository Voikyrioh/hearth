//! Opérations en suspens de chaque serveur, sur disque : elles survivent à un redémarrage de
//! l'application (une action coupée avant sa réponse reste « à annoncer »).

use async_trait::async_trait;

use super::server_store::StoreError;
use crate::domain::pending_ops::PendingOp;
use crate::domain::server::ServerId;

/// Ce qu'on a pu relire.
#[derive(Debug, Default, PartialEq)]
pub struct LoadedOperations {
    pub operations: Vec<PendingOp>,
    /// Le fichier était illisible, ou des entrées l'étaient : des suivis ont pu être perdus.
    pub damaged: bool,
}

#[async_trait]
pub trait OperationStore: Send + Sync {
    /// Rien de mémorisé : vide. Fichier illisible : mis de côté en `.corrupt`, avertissement,
    /// `damaged`. Une entrée invalide n'emporte pas les autres.
    async fn load(&self, id: &ServerId) -> Result<LoadedOperations, StoreError>;
    /// Remplace ; une liste vide efface.
    async fn save(&self, id: &ServerId, operations: &[PendingOp]) -> Result<(), StoreError>;
    async fn remove(&self, id: &ServerId) -> Result<(), StoreError>;
}

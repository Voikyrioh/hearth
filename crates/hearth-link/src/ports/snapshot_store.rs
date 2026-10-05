//! Dernière vue connue de chaque serveur (affichée périmée hors ligne, BR-RESIL-007).

use async_trait::async_trait;

use super::server_store::StoreError;
use crate::domain::server::{LastKnown, ServerId};

#[async_trait]
pub trait SnapshotStore: Send + Sync {
    /// `None` si rien n'est mémorisé ou si le fichier est illisible (avertissement).
    async fn load(&self, id: &ServerId) -> Result<Option<LastKnown>, StoreError>;
    async fn save(&self, id: &ServerId, view: &LastKnown) -> Result<(), StoreError>;
    async fn remove(&self, id: &ServerId) -> Result<(), StoreError>;
}

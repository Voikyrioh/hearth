//! Carnet des serveurs enregistrés.

use async_trait::async_trait;
use thiserror::Error;

use crate::domain::server::{ServerId, ServerRecord};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("stockage indisponible : {0}")]
pub struct StoreError(pub String);

#[async_trait]
pub trait ServerStore: Send + Sync {
    /// Tous les serveurs. Un stockage illisible rend une liste vide (avec un avertissement) :
    /// jamais de panique.
    async fn list(&self) -> Result<Vec<ServerRecord>, StoreError>;
    /// Ajoute ou remplace.
    async fn save(&self, record: &ServerRecord) -> Result<(), StoreError>;
    async fn remove(&self, id: &ServerId) -> Result<(), StoreError>;
}

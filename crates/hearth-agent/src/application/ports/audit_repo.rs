use async_trait::async_trait;
use time::OffsetDateTime;

use super::StoreError;
use crate::domain::audit::{AuditEvent, AuditFilter, AuditRecord};

/// Lecture du journal d'activité. Écritures : `UnitOfWork::audit`. Aucune modification d'une
/// entrée écrite (BR-AUDIT-009) : ni ici, ni dans `AuditTx`.
#[async_trait]
pub trait AuditRepo: Send + Sync {
    /// Les entrées qui correspondent au filtre, de la plus récente à la plus ancienne, au plus
    /// `limit` (le filtre porte déjà sa propre taille de page ; l'export en demande plus).
    async fn search(
        &self,
        filter: &AuditFilter,
        limit: usize,
    ) -> Result<Vec<AuditRecord>, StoreError>;
}

#[async_trait]
pub trait AuditTx: Send {
    /// Écrit une entrée dans la transaction et rend l'entrée telle qu'écrite (avec son
    /// identifiant) : à diffuser une fois la transaction validée.
    async fn record(&mut self, event: &AuditEvent) -> Result<AuditRecord, StoreError>;

    /// Nombre d'entrées.
    async fn count(&mut self) -> Result<u64, StoreError>;

    /// Supprime les entrées écrites avant `before` ; rend leur nombre.
    async fn purge_before(&mut self, before: OffsetDateTime) -> Result<u64, StoreError>;

    /// Supprime les `count` entrées les plus anciennes ; rend leur nombre.
    async fn purge_oldest(&mut self, count: u64) -> Result<u64, StoreError>;
}

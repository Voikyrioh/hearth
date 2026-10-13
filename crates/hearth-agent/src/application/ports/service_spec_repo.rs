//! Le dépôt des descriptions durables des services (`service_specs`, `service_spec_secrets`, HRT-83,
//! ADR-0044). Il garde ce qu'on lui confie, déjà scellé : il ne connaît pas la clé et ne voit jamais un
//! secret en clair.

use async_trait::async_trait;
use thiserror::Error;
use time::OffsetDateTime;

use super::StoreError;
use crate::domain::sealed::{SealedBlob, SecretField};
use crate::domain::services::key::ServiceKey;
use crate::domain::services::spec::SpecBody;

/// Un champ scellé d'une révision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealedField {
    pub field: SecretField,
    pub blob: SealedBlob,
}

/// Une révision à écrire. Les champs sont scellés avec `rev` dans leurs données associées : le dépôt
/// refuse la révision si son numéro n'est pas celui qu'il aurait donné (`expected_rev + 1`).
#[derive(Debug)]
pub struct NewRevision {
    pub key: ServiceKey,
    /// La révision courante que l'appelant a lue (`None` = première révision).
    pub expected_rev: Option<u32>,
    pub body: SpecBody,
    pub sealed: Vec<SealedField>,
    /// Identifiant du compte.
    pub by: String,
    pub at: OffsetDateTime,
}

/// Une révision relue.
#[derive(Debug)]
pub struct StoredRevision {
    pub key: ServiceKey,
    pub rev: u32,
    pub body: SpecBody,
    pub updated_by: String,
    pub created_at: OffsetDateTime,
    pub current: bool,
    pub sealed: Vec<SealedField>,
}

#[derive(Debug, Error)]
pub enum SpecStoreError {
    /// La révision courante n'est plus celle que l'appelant avait lue (modification concurrente).
    #[error("la description a changé depuis sa lecture")]
    Conflict { current: Option<u32> },
    /// Un autre service porte ce nom.
    #[error("un service porte déjà ce nom")]
    NameTaken,
    #[error(transparent)]
    Store(#[from] StoreError),
}

#[async_trait]
pub trait ServiceSpecRepo: Send + Sync {
    /// Écrit une révision, dans UNE transaction avec la mise hors-courant de la précédente. Rend son numéro.
    async fn append_revision(&self, new: NewRevision) -> Result<u32, SpecStoreError>;

    /// La révision courante, ou `None`.
    async fn current(&self, key: &ServiceKey) -> Result<Option<StoredRevision>, StoreError>;

    /// Une révision précise, ou `None`.
    async fn revision(
        &self,
        key: &ServiceKey,
        rev: u32,
    ) -> Result<Option<StoredRevision>, StoreError>;

    /// Les numéros de révision gardés, du plus ancien au plus récent.
    async fn revisions(&self, key: &ServiceKey) -> Result<Vec<u32>, StoreError>;

    /// Le service qui porte ce nom (replié) dans sa révision courante.
    async fn holder_of_name(&self, folded: &str) -> Result<Option<ServiceKey>, StoreError>;

    /// Efface les révisions strictement antérieures à `rev`, secrets compris (l'ancien conteneur est
    /// supprimé). Ne touche jamais la révision courante. Rend le nombre de révisions effacées.
    async fn discard_before(&self, key: &ServiceKey, rev: u32) -> Result<u64, StoreError>;

    /// Efface toutes les révisions du service et leurs secrets (suppression du service). Rend leur nombre.
    async fn forget(&self, key: &ServiceKey) -> Result<u64, StoreError>;
}

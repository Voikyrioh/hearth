use async_trait::async_trait;
use time::OffsetDateTime;

use super::StoreError;
use crate::domain::accounts::AccountId;

/// Lecture des sessions des comptes. Toute écriture (fermer, plus tard créer) passe par
/// `UnitOfWork`. HRT-04 y ajoute la recherche par jeton.
#[async_trait]
pub trait SessionRepo: Send + Sync {
    /// Dates d'expiration des sessions du compte. L'application décide lesquelles sont
    /// encore ouvertes (`domain::sessions::is_open`) : le stockage ne juge pas.
    async fn expiries_of(&self, account: &AccountId) -> Result<Vec<OffsetDateTime>, StoreError>;
}

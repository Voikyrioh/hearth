use async_trait::async_trait;
use time::OffsetDateTime;

use super::StoreError;
use crate::domain::accounts::AccountId;
use crate::domain::sessions::SessionClosure;

/// Sessions ouvertes par les comptes. HRT-04 y ajoute la création, la recherche par jeton et
/// l'expiration glissante.
#[async_trait]
pub trait SessionRepo: Send + Sync {
    /// Dates d'expiration des sessions du compte. L'application décide lesquelles sont
    /// encore ouvertes (`domain::sessions::is_open`) : le stockage ne juge pas.
    async fn expiries_of(&self, account: &AccountId) -> Result<Vec<OffsetDateTime>, StoreError>;

    /// Ferme des sessions du compte ; rend le nombre de sessions supprimées.
    async fn close(&self, account: &AccountId, closure: &SessionClosure)
    -> Result<u64, StoreError>;
}

use async_trait::async_trait;

use super::StoreError;
use crate::domain::accounts::{Account, AccountId, Username};

/// Lecture des comptes. Les écritures passent par l'unité de travail du magasin (`Store`).
#[async_trait]
pub trait AccountRepo: Send + Sync {
    async fn find_by_id(&self, id: &AccountId) -> Result<Option<Account>, StoreError>;

    async fn find_by_username(&self, username: &Username) -> Result<Option<Account>, StoreError>;

    /// Tous les comptes, du plus ancien au plus récent.
    async fn list(&self) -> Result<Vec<Account>, StoreError>;
}

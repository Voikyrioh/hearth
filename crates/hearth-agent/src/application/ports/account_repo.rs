use async_trait::async_trait;
use time::OffsetDateTime;

use super::StoreError;
use crate::domain::accounts::{Account, AccountId, Role, Username};
use crate::domain::secret::Secret;

/// Lecture des comptes. Les écritures passent par l'unité de travail du magasin
/// (`UnitOfWork::accounts`).
#[async_trait]
pub trait AccountRepo: Send + Sync {
    async fn find_by_id(&self, id: &AccountId) -> Result<Option<Account>, StoreError>;

    async fn find_by_username(&self, username: &Username) -> Result<Option<Account>, StoreError>;

    /// Tous les comptes, du plus ancien au plus récent.
    async fn list(&self) -> Result<Vec<Account>, StoreError>;
}

/// Les comptes vus de l'intérieur d'une unité de travail : lectures qui servent à décider et
/// écritures, dans la même transaction.
#[async_trait]
pub trait AccountTx: Send {
    async fn find(&mut self, id: &AccountId) -> Result<Option<Account>, StoreError>;

    async fn find_by_username(
        &mut self,
        username: &Username,
    ) -> Result<Option<Account>, StoreError>;

    /// Nombre d'administrateurs actuels.
    async fn count_admins(&mut self) -> Result<u64, StoreError>;

    /// Insère un compte ; `StoreError::Duplicate` si l'identifiant existe déjà.
    async fn insert(&mut self, account: &Account) -> Result<(), StoreError>;

    async fn set_role(&mut self, id: &AccountId, role: Role) -> Result<(), StoreError>;

    async fn set_password(
        &mut self,
        id: &AccountId,
        hash: &Secret,
        changed_at: OffsetDateTime,
    ) -> Result<(), StoreError>;

    /// Date de la dernière connexion réussie.
    async fn record_login(&mut self, id: &AccountId, at: OffsetDateTime) -> Result<(), StoreError>;

    /// Supprime le compte (ses sessions suivent).
    async fn delete(&mut self, id: &AccountId) -> Result<(), StoreError>;
}

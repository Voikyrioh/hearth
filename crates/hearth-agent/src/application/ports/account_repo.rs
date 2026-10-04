use async_trait::async_trait;
use time::OffsetDateTime;

use super::StoreError;
use crate::domain::accounts::{Account, AccountId, Role, Username};
use crate::domain::secret::Secret;
use crate::domain::sessions::SessionClosure;

/// Lecture des comptes, et ouverture des transactions d'écriture.
#[async_trait]
pub trait AccountRepo: Send + Sync {
    async fn find_by_id(&self, id: &AccountId) -> Result<Option<Account>, StoreError>;

    async fn find_by_username(&self, username: &Username) -> Result<Option<Account>, StoreError>;

    /// Tous les comptes, du plus ancien au plus récent.
    async fn list(&self) -> Result<Vec<Account>, StoreError>;

    /// Ouvre une transaction d'écriture. Les écritures sont sérialisées : ce que la transaction
    /// observe ne peut plus changer avant sa validation. Abandonnée sans `commit`, elle annule
    /// tout.
    async fn begin(&self) -> Result<Box<dyn AccountTransaction>, StoreError>;
}

/// Opérations d'une transaction d'écriture sur les comptes et leurs sessions. Le stockage
/// exécute ; les règles (dernier administrateur, sessions à fermer) sont décidées par le domaine.
#[async_trait]
pub trait AccountTransaction: Send {
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

    /// Supprime le compte (ses sessions suivent).
    async fn delete(&mut self, id: &AccountId) -> Result<(), StoreError>;

    /// Ferme des sessions du compte ; rend le nombre de sessions supprimées.
    async fn close_sessions(
        &mut self,
        account: &AccountId,
        closure: &SessionClosure,
    ) -> Result<u64, StoreError>;

    async fn commit(self: Box<Self>) -> Result<(), StoreError>;
}

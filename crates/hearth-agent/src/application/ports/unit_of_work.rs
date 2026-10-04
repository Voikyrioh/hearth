use async_trait::async_trait;
use time::OffsetDateTime;

use super::StoreError;
use crate::domain::accounts::{Account, AccountId, Role, Username};
use crate::domain::secret::Secret;
use crate::domain::sessions::SessionClosure;

/// Point d'entrée des écritures. Le magasin est neutre : il ne porte aucun sujet (comptes,
/// sessions, plus tard journal et opérations), l'unité de travail en porte les opérations.
#[async_trait]
pub trait Store: Send + Sync {
    /// Ouvre une unité de travail. Les écritures sont sérialisées : ce qu'elle observe ne peut
    /// plus changer avant sa validation. Abandonnée sans `commit`, elle annule tout.
    async fn begin(&self) -> Result<Box<dyn UnitOfWork>, StoreError>;
}

/// Une transaction qui couvre tous les sujets : comptes et sessions changent ensemble ou pas du
/// tout. Le stockage exécute ; les règles (dernier administrateur, sessions à fermer) sont
/// décidées par le domaine. Un seul chemin existe pour fermer des sessions : `close_sessions`.
/// HRT-04 y ajoutera la création d'une session et la mise à jour de `last_login_at`, dans la
/// même transaction.
#[async_trait]
pub trait UnitOfWork: Send {
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

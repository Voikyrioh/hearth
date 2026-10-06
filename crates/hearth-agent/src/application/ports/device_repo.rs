use async_trait::async_trait;
use time::OffsetDateTime;

use super::StoreError;
use crate::domain::accounts::AccountId;
use crate::domain::sessions::SessionId;
use crate::domain::trust::{DeviceId, NewDevice, TrustedDevice};

/// Lecture des postes de confiance (HRT-22). Écritures : `UnitOfWork::devices`.
#[async_trait]
pub trait DeviceRepo: Send + Sync {
    /// Les postes d'un compte, du plus ancien au plus récent.
    async fn of_account(&self, account: &AccountId) -> Result<Vec<TrustedDevice>, StoreError>;

    /// Le poste dont la session a été ouverte (ou prouvée) par sa clé, s'il y en a un.
    async fn of_session(&self, session: &SessionId) -> Result<Option<DeviceId>, StoreError>;

    /// Le poste de cette empreinte de clé, **tous comptes confondus** : une même clé n'est jamais
    /// confiée à deux comptes.
    async fn find_by_key(&self, key_id: &str) -> Result<Option<TrustedDevice>, StoreError>;
}

#[async_trait]
pub trait DeviceTx: Send {
    /// Voir `DeviceRepo::find_by_key`, dans la transaction. **Même requête** que le compte existe
    /// ou non (BR-CONN-013).
    async fn find_by_key(&mut self, key_id: &str) -> Result<Option<TrustedDevice>, StoreError>;

    /// Le poste `id` s'il est à ce compte.
    async fn get(
        &mut self,
        account: &AccountId,
        id: &DeviceId,
    ) -> Result<Option<TrustedDevice>, StoreError>;

    /// Nombre de postes du compte.
    async fn count(&mut self, account: &AccountId) -> Result<usize, StoreError>;

    /// Inscrit un poste. `StoreError::Duplicate` si la clé est déjà inscrite pour ce compte.
    async fn insert(&mut self, device: &NewDevice) -> Result<(), StoreError>;

    /// Date la dernière preuve du poste et retient d'où elle vient.
    async fn prove(
        &mut self,
        id: &DeviceId,
        at: OffsetDateTime,
        addr: &str,
    ) -> Result<(), StoreError>;

    /// Retire un poste ; son adresse retenue part avec lui (cascade), ses sessions ne sont pas
    /// touchées ici (`SessionTx::close_device`).
    async fn delete(&mut self, id: &DeviceId) -> Result<(), StoreError>;

    /// Oublie tous les postes du compte (et, par cascade, leurs adresses) ; rend leur nombre.
    async fn forget_all(&mut self, account: &AccountId) -> Result<u64, StoreError>;

    /// Oublie les postes dont la dernière preuve date d'avant `before` ; rend leur nombre.
    async fn purge(&mut self, before: OffsetDateTime) -> Result<u64, StoreError>;

    /// L'inscription est-elle gelée (mode attaque actif) ? Lu dans la ligne unique `attack_mode`,
    /// que seule la fonction « mode attaque » (HRT-25) écrira.
    async fn enrollment_frozen(&mut self) -> Result<bool, StoreError>;
}

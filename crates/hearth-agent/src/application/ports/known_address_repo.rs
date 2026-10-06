use async_trait::async_trait;
use time::OffsetDateTime;

use super::StoreError;
use crate::domain::accounts::AccountId;
use crate::domain::known_address::KnownAddress;

/// Lecture des adresses connues des comptes (ADR-0022, BR-CONN-019). Écritures :
/// `UnitOfWork::known_addresses`.
#[async_trait]
pub trait KnownAddressRepo: Send + Sync {
    /// Adresses connues du compte dont c'est l'identifiant. **Même requête** que l'identifiant
    /// existe ou non (liste vide pour un identifiant inconnu) : le chemin ne dit rien sur
    /// l'existence du compte (BR-CONN-013).
    async fn of_username(&self, username: &str) -> Result<Vec<KnownAddress>, StoreError>;

    /// Adresses connues d'un compte.
    async fn of_account(&self, account: &AccountId) -> Result<Vec<KnownAddress>, StoreError>;

    /// Cette adresse (exacte, canonique) a-t-elle une connexion ou un usage authentifié depuis
    /// `since`, pour n'importe quel compte ? Sert aux places réservées, qui n'ont pas encore de
    /// compte.
    async fn address_is_known(
        &self,
        address: &str,
        since: OffsetDateTime,
    ) -> Result<bool, StoreError>;
}

#[async_trait]
pub trait KnownAddressTx: Send {
    /// Voir `KnownAddressRepo::of_username`, dans la transaction.
    async fn of_username(&mut self, username: &str) -> Result<Vec<KnownAddress>, StoreError>;

    async fn of_account(&mut self, account: &AccountId) -> Result<Vec<KnownAddress>, StoreError>;

    /// Voir `KnownAddressRepo::address_is_known`, dans la transaction.
    async fn address_is_known(
        &mut self,
        address: &str,
        since: OffsetDateTime,
    ) -> Result<bool, StoreError>;

    /// Remplace la liste des adresses connues du compte (déjà bornée par le domaine).
    async fn replace(
        &mut self,
        account: &AccountId,
        list: &[KnownAddress],
    ) -> Result<(), StoreError>;

    /// Oublie toutes les adresses connues du compte (mot de passe changé par un administrateur,
    /// sessions fermées par l'administration).
    async fn forget(&mut self, account: &AccountId) -> Result<(), StoreError>;

    /// Oublie les adresses dont la dernière authentification date d'avant `before` ; rend leur
    /// nombre.
    async fn purge(&mut self, before: OffsetDateTime) -> Result<u64, StoreError>;
}

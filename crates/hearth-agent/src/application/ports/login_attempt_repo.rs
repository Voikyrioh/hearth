use async_trait::async_trait;
use time::OffsetDateTime;

use super::StoreError;
use crate::domain::accounts::AccountId;
use crate::domain::identifier_slowdown::Slowdown;
use crate::domain::known_address::KnownAddress;
use crate::domain::lockout::{AttemptKey, LockoutState};

/// Lecture de ce que l'on retient des tentatives de connexion : compteurs du couple et de
/// l'origine, ralentissement par identifiant, adresses connues des comptes (ADR-0022). Écritures :
/// `UnitOfWork::login_attempts`.
#[async_trait]
pub trait LoginAttemptRepo: Send + Sync {
    /// État du couple identifiant + adresse (ou de l'origine) ; l'état vierge s'il n'a jamais
    /// échoué.
    async fn get(&self, key: &AttemptKey) -> Result<LockoutState, StoreError>;

    /// Ralentissement de l'identifiant (clé `AttemptKey::identifier`) ; vierge s'il n'a jamais
    /// échoué.
    async fn identifier(&self, key: &AttemptKey) -> Result<Slowdown, StoreError>;

    /// Adresses connues du compte dont c'est l'identifiant. **Même requête** que l'identifiant
    /// existe ou non (liste vide pour un identifiant inconnu) : le chemin ne dit rien sur
    /// l'existence du compte (BR-CONN-013).
    async fn known_addresses_of(&self, username: &str) -> Result<Vec<KnownAddress>, StoreError>;

    /// Cette adresse (exacte, canonique) a-t-elle une connexion réussie depuis `since`, pour
    /// n'importe quel compte ? Sert aux places d'attente du flux, qui n'a pas encore de compte.
    async fn address_is_known(
        &self,
        address: &str,
        since: OffsetDateTime,
    ) -> Result<bool, StoreError>;
}

#[async_trait]
pub trait LoginAttemptTx: Send {
    async fn get(&mut self, key: &AttemptKey) -> Result<LockoutState, StoreError>;

    /// Enregistre l'état du couple, daté de `at`.
    async fn save(
        &mut self,
        key: &AttemptKey,
        state: &LockoutState,
        at: OffsetDateTime,
    ) -> Result<(), StoreError>;

    /// Oublie les compteurs sans activité depuis `before` et sans attente en cours à `now` ;
    /// rend leur nombre.
    async fn purge_inactive(
        &mut self,
        before: OffsetDateTime,
        now: OffsetDateTime,
    ) -> Result<u64, StoreError>;

    /// Ramène `login_attempts` sous `domain::lockout::MAX_TRACKED_ATTEMPTS` lignes : oublie
    /// d'abord les lignes sans attente en cours à `now`, de moins d'échecs, les plus anciennes ;
    /// rend leur nombre.
    async fn trim(&mut self, now: OffsetDateTime) -> Result<u64, StoreError>;

    async fn identifier(&mut self, key: &AttemptKey) -> Result<Slowdown, StoreError>;

    /// Enregistre le ralentissement de l'identifiant, puis ramène la table sous
    /// `domain::identifier_slowdown::MAX_TRACKED` lignes (les moins attaquées, les plus
    /// anciennes d'abord).
    async fn save_identifier(
        &mut self,
        key: &AttemptKey,
        state: &Slowdown,
    ) -> Result<(), StoreError>;

    /// Oublie les ralentissements dont le dernier échec date d'avant `before` et dont l'attente
    /// est finie à `now` ; rend leur nombre.
    async fn purge_identifiers(
        &mut self,
        before: OffsetDateTime,
        now: OffsetDateTime,
    ) -> Result<u64, StoreError>;

    /// Voir `LoginAttemptRepo::known_addresses_of`, dans la transaction.
    async fn known_addresses_of(&mut self, username: &str)
    -> Result<Vec<KnownAddress>, StoreError>;

    /// Adresses connues d'un compte.
    async fn known_addresses(
        &mut self,
        account: &AccountId,
    ) -> Result<Vec<KnownAddress>, StoreError>;

    /// Remplace la liste des adresses connues du compte (déjà bornée par le domaine).
    async fn replace_known(
        &mut self,
        account: &AccountId,
        list: &[KnownAddress],
    ) -> Result<(), StoreError>;

    /// Oublie toutes les adresses connues du compte (mot de passe changé, sessions fermées par
    /// l'administration).
    async fn forget_known(&mut self, account: &AccountId) -> Result<(), StoreError>;

    /// Oublie les adresses dont la dernière connexion réussie date d'avant `before` ; rend leur
    /// nombre.
    async fn purge_known(&mut self, before: OffsetDateTime) -> Result<u64, StoreError>;
}

use async_trait::async_trait;
use time::OffsetDateTime;

use super::StoreError;
use crate::domain::accounts::AccountId;
use crate::domain::session_token::TokenHash;
use crate::domain::sessions::{Session, SessionClosure, SessionId};

/// Lecture des sessions. Toute écriture passe par `UnitOfWork::sessions`.
#[async_trait]
pub trait SessionRepo: Send + Sync {
    /// Dates d'expiration des sessions du compte. L'application décide lesquelles sont
    /// encore ouvertes (`domain::sessions::is_open`) : le stockage ne juge pas.
    async fn expiries_of(&self, account: &AccountId) -> Result<Vec<OffsetDateTime>, StoreError>;

    /// La session de ce jeton (par son empreinte), expirée ou non.
    async fn find_by_token_hash(&self, hash: &TokenHash) -> Result<Option<Session>, StoreError>;

    /// Ce jeton a-t-il appartenu à une session fermée par l'administration (BR-RESIL-014) ?
    async fn is_revoked(&self, hash: &TokenHash) -> Result<bool, StoreError>;

    /// Une session encore ouverte à `now` a-t-elle été ouverte depuis cette adresse (exacte,
    /// canonique), pour n'importe quel compte ? Sert aux places d'attente du flux (ADR-0022).
    async fn has_open_session_from(
        &self,
        address: &str,
        now: OffsetDateTime,
    ) -> Result<bool, StoreError>;
}

/// Les sessions vues de l'intérieur d'une unité de travail. Un seul chemin pour fermer des
/// sessions : `close`.
#[async_trait]
pub trait SessionTx: Send {
    async fn insert(&mut self, session: &Session) -> Result<(), StoreError>;

    /// Expiration glissante : nouvelle dernière activité et nouvelle expiration.
    async fn renew(
        &mut self,
        id: &SessionId,
        last_seen_at: OffsetDateTime,
        expires_at: OffsetDateTime,
    ) -> Result<(), StoreError>;

    /// Déconnexion explicite : supprime la session, sans trace de révocation.
    async fn delete(&mut self, id: &SessionId) -> Result<(), StoreError>;

    /// Ferme des sessions du compte (fermeture par l'administration) et retient leurs jetons
    /// comme révoqués à la date `at` ; rend le nombre de sessions fermées.
    async fn close(
        &mut self,
        account: &AccountId,
        closure: &SessionClosure,
        at: OffsetDateTime,
    ) -> Result<u64, StoreError>;

    /// Supprime les sessions expirées avant `now` ; rend leur nombre.
    async fn purge_expired(&mut self, now: OffsetDateTime) -> Result<u64, StoreError>;

    /// Oublie les révocations antérieures à `before` ; rend leur nombre.
    async fn purge_revocations(&mut self, before: OffsetDateTime) -> Result<u64, StoreError>;
}

use async_trait::async_trait;
use time::OffsetDateTime;

use super::StoreError;
use crate::domain::lockout::{AttemptKey, LockoutState};

/// Lecture des compteurs de tentatives de connexion. Écritures : `UnitOfWork::login_attempts`.
#[async_trait]
pub trait LoginAttemptRepo: Send + Sync {
    /// État du couple identifiant + adresse ; l'état vierge s'il n'a jamais échoué.
    async fn get(&self, key: &AttemptKey) -> Result<LockoutState, StoreError>;
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
}

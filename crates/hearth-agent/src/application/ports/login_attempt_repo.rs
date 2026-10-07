use async_trait::async_trait;
use time::OffsetDateTime;

use super::StoreError;
use crate::domain::identifier_slowdown::Slowdown;
use crate::domain::lockout::{AttemptKey, LockoutState};

/// Lecture de ce que l'on retient des tentatives de connexion : compteurs du couple et de
/// l'adresse, ralentissement par identifiant (ADR-0022). Écritures : `UnitOfWork::login_attempts`.
#[async_trait]
pub trait LoginAttemptRepo: Send + Sync {
    /// État du couple identifiant + adresse (ou de l'adresse seule) ; l'état vierge s'il n'a jamais
    /// échoué.
    async fn get(&self, key: &AttemptKey) -> Result<LockoutState, StoreError>;

    /// Ralentissement de l'identifiant (clé `AttemptKey::identifier`) ; vierge s'il n'a jamais
    /// échoué.
    async fn identifier(&self, key: &AttemptKey) -> Result<Slowdown, StoreError>;

    /// Les identifiants qui ont plus de `FREE_FAILURES` échecs ou un épisode d'alerte noté, avec la
    /// clé de leur ligne (`AttemptKey::as_str`) : de quoi dire lesquels sont visés (le domaine
    /// tranche, `identifier_slowdown::is_alert`). Borné par `MAX_TRACKED`.
    async fn alerting(&self) -> Result<Vec<(String, Slowdown)>, StoreError>;
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

    /// Ramène `login_attempts` sous `domain::lockout::MAX_TRACKED_ATTEMPTS` lignes (voir
    /// `domain::eviction` pour l'ordre) ; rend leur nombre. À appeler quand une ligne a été créée.
    async fn trim(&mut self, now: OffsetDateTime) -> Result<u64, StoreError>;

    async fn identifier(&mut self, key: &AttemptKey) -> Result<Slowdown, StoreError>;

    /// Voir `LoginAttemptRepo::alerting`, dans la transaction.
    async fn alerting(&mut self) -> Result<Vec<(String, Slowdown)>, StoreError>;

    /// Efface l'épisode d'alerte noté sur cette ligne, **si c'est bien celui-là** (`alerted_at`
    /// inchangé) : rend `false` quand un autre passage l'a déjà fini ou qu'un nouvel épisode a
    /// commencé. Une fin d'alerte ne se consigne donc qu'une fois.
    async fn end_alert(
        &mut self,
        key: &str,
        alerted_at: OffsetDateTime,
    ) -> Result<bool, StoreError>;

    /// Enregistre le ralentissement de l'identifiant.
    async fn save_identifier(
        &mut self,
        key: &AttemptKey,
        state: &Slowdown,
    ) -> Result<(), StoreError>;

    /// Ramène `identifier_slowdowns` sous `domain::identifier_slowdown::MAX_TRACKED` lignes (voir
    /// `domain::eviction` pour l'ordre) ; rend leur nombre. À appeler quand une ligne a été
    /// créée.
    async fn trim_identifiers(&mut self, now: OffsetDateTime) -> Result<u64, StoreError>;

    /// Oublie les ralentissements dont le dernier échec date d'avant `before` et dont l'attente
    /// est finie à `now` ; rend leur nombre.
    async fn purge_identifiers(
        &mut self,
        before: OffsetDateTime,
        now: OffsetDateTime,
    ) -> Result<u64, StoreError>;
}

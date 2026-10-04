use time::OffsetDateTime;

/// Source de l'heure, injectable pour les tests.
pub trait Clock: Send + Sync {
    fn now(&self) -> OffsetDateTime;
}

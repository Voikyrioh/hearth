//! Source d'aléa (délais de reconnexion).

pub trait Rng: Send + Sync {
    fn next_u32(&self) -> u32;
}

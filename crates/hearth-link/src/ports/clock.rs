//! Horloge : deux lectures, jamais mélangées.

use crate::domain::time::{Mono, WallTime};

pub trait Clock: Send + Sync {
    /// Instant monotone (durées, seuils, attentes).
    fn mono(&self) -> Mono;
    /// Date murale (affichage, âge des données).
    fn wall(&self) -> WallTime;
}

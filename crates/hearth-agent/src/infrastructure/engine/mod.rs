//! Adaptateurs du port `ContainerEngine` (HRT-61, ADR-0036).
//!
//! - `bollard` : le vrai moteur par le socket Unix (seul endroit où `bollard` apparaît) ;
//! - `logs` : la lecture des journaux d'un conteneur (HRT-66), écrite à la main sur le socket (bornée) ;
//! - `translate` : réponse d'inspection du moteur vers `Observation` (pur, sans variables ni commande) ;
//! - `errors` : erreur du moteur vers `EngineError` (le texte du moteur n'en sort jamais) ;
//! - `installed` : un moteur est-il installé sur ce serveur ? (séparer « arrêté » d'« absent ») ;
//! - `fake` : le faux moteur des tests, qui vit dans `tests/support/fake_engine.rs` (partagé avec les
//!   tests d'intégration) et n'est compilé que sous `cfg(test)` : il n'est pas dans le binaire livré.

pub mod bollard;
pub mod errors;
#[cfg(test)]
#[path = "../../../tests/support/fake_engine.rs"]
pub mod fake;
pub mod installed;
mod logs;
pub mod translate;

pub use self::bollard::{BollardEngine, Target};
pub use self::installed::InstallMarkers;

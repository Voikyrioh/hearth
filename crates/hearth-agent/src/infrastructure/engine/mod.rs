//! Adaptateurs du port `ContainerEngine` (HRT-61, ADR-0036).
//!
//! - `bollard` : le vrai moteur par le socket Unix (seul endroit où `bollard` apparaît) ;
//! - `translate` : réponse d'inspection du moteur vers `Observation` (pur, sans variables ni commande) ;
//! - `errors` : erreur du moteur vers `EngineError` (le texte du moteur n'en sort jamais) ;
//! - `installed` : un moteur est-il installé sur ce serveur ? (séparer « arrêté » d'« absent ») ;
//! - `fake` : le faux moteur de tous les tests.

pub mod bollard;
pub mod errors;
pub mod fake;
pub mod installed;
pub mod translate;

pub use self::bollard::{BollardEngine, Target};
pub use self::fake::FakeEngine;
pub use self::installed::InstallMarkers;

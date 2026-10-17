//! Chargement d'une fiche : analyse fermée, bornes, invariants, empreinte.
//!
//! Ce dossier est AUTONOME : il n'utilise que `super::` et `self::` (jamais `crate::`), parce que
//! `build.rs` et `xtask` l'incluent tel quel par `#[path]` pour appliquer le chargeur RÉEL (une fiche
//! que le chargeur refuse ne compile pas). Aucune E/S ici non plus.

pub mod error;
pub mod ids_lock;
pub mod invariants;
pub mod limits;
pub mod loader;
pub mod memory;
pub mod model;
pub mod schema;
pub mod value;

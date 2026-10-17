//! Fichiers d'un service : ce que le domaine sait d'un chemin relatif sous une racine (HRT-145, ADR-0046).
//! Aucune E/S : `infrastructure::fs` ouvre et écrit.

mod rel_path;

pub use rel_path::{MAX_COMPONENT_BYTES, MAX_DEPTH, MAX_PATH_BYTES, RelPath, RelPathError};

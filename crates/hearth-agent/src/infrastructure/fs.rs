//! Adaptateur `SafeFs` : accès aux fichiers d'un service sous `<données de l'agent>/services/<clé>/`,
//! sans jamais suivre un lien (HRT-145, ADR-0046, `BR-DEPLOY-010`, `BR-FILES-003`,
//! `BR-FILES-004`, `BR-DEL-007`, `BR-DEL-009`).
//!
//! # Comment
//!
//! Le dossier de données de l'agent est ouvert UNE fois par son chemin (il est de confiance : à l'agent,
//! non inscriptible par un autre compte). Ensuite tout se fait par descripteur : chaque composant est
//! ouvert par `openat(parent, nom, O_DIRECTORY | O_NOFOLLOW)` depuis le descripteur du composant
//! précédent, et son périphérique est comparé à celui de la racine (un montage est refusé). Un lien
//! planté par le service à la place d'un dossier fait échouer le geste (`FsError::Blocked`), sa cible
//! n'est pas touchée. Aucun appel ne prend un chemin composé après l'ouverture de la racine.
//!
//! # Limites dites
//!
//! - Le noyau n'est pas chargé de la résolution (`openat2` avec `RESOLVE_BENEATH` n'est pas utilisé) :
//!   la marche composant par composant est le repli de l'ADR-0046, retenu seul parce qu'il tient sans
//!   dépendre du filtre d'appels système de l'unité. Un montage « bind » du même système de fichiers
//!   garde le même périphérique et n'est donc pas vu.
//! - Hors Linux (poste de développement Windows), rien ne s'écrit : `FsError::Unsupported`.
//! - La faible priorité d'un effacement (`erase_tree`, HRT-85) est un « nice » de 19 posé sur un fil dédié, pas
//!   une classe d'entrée-sortie `idle` (`ioprio_set` n'est pas exposé par `rustix`, et `libc` est interdit
//!   avec `unsafe`) : le noyau en déduit la priorité d'entrée-sortie des ordonnanceurs qui la lisent.

use std::path::PathBuf;
#[cfg(unix)]
use std::time::Duration;

#[cfg(unix)]
use crate::application::ports::{Erased, Measure, Perms};
use crate::application::ports::{FsError, SafeFs, ServiceRoot};
#[cfg(unix)]
use crate::domain::files::RelPath;
use crate::domain::services::ServiceKey;

#[cfg(unix)]
mod unix;

#[cfg(test)]
#[cfg(unix)]
mod tests_unix;

/// Nom du dossier des racines de service, sous le dossier de données de l'agent.
pub const SERVICES_DIR: &str = "services";

/// `SafeFs` réel : un dossier de données de l'agent, de confiance.
#[derive(Debug, Clone)]
pub struct SystemSafeFs {
    data_dir: PathBuf,
}

impl SystemSafeFs {
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }
}

#[cfg(unix)]
impl SafeFs for SystemSafeFs {
    fn open_service_root(&self, key: &ServiceKey) -> Result<Box<dyn ServiceRoot>, FsError> {
        let root = unix::open_service_root(&self.data_dir, key, false)?;
        Ok(Box::new(root))
    }

    fn ensure_service_root(&self, key: &ServiceKey) -> Result<Box<dyn ServiceRoot>, FsError> {
        let root = unix::open_service_root(&self.data_dir, key, true)?;
        Ok(Box::new(root))
    }

    fn remove_service_root(&self, key: &ServiceKey) -> Result<(), FsError> {
        unix::remove_service_root(&self.data_dir, key)
    }

    fn remove_empty_service_root(&self, key: &ServiceKey) -> Result<bool, FsError> {
        unix::remove_empty_service_root(&self.data_dir, key)
    }
}

#[cfg(not(unix))]
impl SafeFs for SystemSafeFs {
    fn open_service_root(&self, _key: &ServiceKey) -> Result<Box<dyn ServiceRoot>, FsError> {
        let _ = &self.data_dir;
        Err(FsError::Unsupported)
    }

    fn ensure_service_root(&self, _key: &ServiceKey) -> Result<Box<dyn ServiceRoot>, FsError> {
        Err(FsError::Unsupported)
    }

    fn remove_service_root(&self, _key: &ServiceKey) -> Result<(), FsError> {
        Err(FsError::Unsupported)
    }

    fn remove_empty_service_root(&self, _key: &ServiceKey) -> Result<bool, FsError> {
        Err(FsError::Unsupported)
    }
}

#[cfg(unix)]
impl ServiceRoot for unix::UnixRoot {
    fn mkdir(&self, rel: &RelPath, perms: Perms) -> Result<bool, FsError> {
        self.make_dirs(rel, perms)
    }

    fn write_atomic(&self, rel: &RelPath, bytes: &[u8], perms: Perms) -> Result<(), FsError> {
        self.write_file(rel, bytes, perms)
    }

    fn read_bounded(&self, rel: &RelPath, max: u64) -> Result<Vec<u8>, FsError> {
        self.read_file(rel, max)
    }

    fn measure(&self, rel: &RelPath, budget: Duration) -> Result<Measure, FsError> {
        self.measure_tree(rel, budget)
    }

    fn remove_tree(&self, rel: &RelPath) -> Result<(), FsError> {
        self.remove(rel)
    }

    fn erase_tree(
        &self,
        rel: &RelPath,
        progress: &mut (dyn FnMut(u64) + Send),
    ) -> Result<Erased, FsError> {
        self.erase(rel, progress, true)
    }
}

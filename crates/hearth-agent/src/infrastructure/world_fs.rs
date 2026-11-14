//! Adaptateur `WorldFs` : lire un monde dans un dossier ouvert aux services SANS Y ÉCRIRE, écrire sa copie dans
//! un dossier d'import neuf, le mettre en place (HRT-105, ADR-0059, `BR-MONDE-004`, `BR-MONDE-005`).
//!
//! # Comment
//!
//! Comme `fs` (ADR-0046) : le dossier ouvert est ouvert UNE fois par son chemin (celui de la configuration de
//! l'agent, écrit par l'administrateur du serveur : de confiance), puis tout se fait par descripteur, chaque
//! composant ouvert par `openat(O_DIRECTORY | O_NOFOLLOW)` depuis le descripteur du précédent, sur le périphérique
//! de la racine. Un lien planté dans le monde fait échouer le geste ou est listé comme lien ; sa cible n'est
//! jamais touchée. La source n'est ouverte qu'en lecture : le type [`WorldSource`] n'a aucune méthode
//! d'écriture, et aucun appel de ce module n'écrit, ne renomme ni ne retire sous la source.
//!
//! # Limites dites
//!
//! - Le dossier d'import est créé sous `<données>/staging/` et mis en place par `renameat` vers
//!   `<données>/services/<clé>/<dossier>` : les deux doivent être sur le même système de fichiers (le dossier de
//!   données de l'agent), sinon la mise en place échoue et la reprise ne crée rien.
//! - Le retrait du dossier d'import vérifie l'inode puis supprime par chemin : même fenêtre assumée que
//!   `created_objects` (le dossier d'attente est à l'agent seul, `0700`).
//! - Un nom qui n'est pas de l'UTF-8 n'est pas listé (il ne peut pas être désigné par un [`RelPath`]).
//! - Hors Unix (poste de développement Windows), rien ne se lit ni ne s'écrit : `WorldFsError::Unsupported`.

use std::path::{Path, PathBuf};

use crate::application::ports::{
    ImportDir, Ownership, WorldFs, WorldFsError, WorldSink, WorldSource,
};
use crate::domain::services::ServiceKey;

#[cfg(unix)]
mod unix;

/// Nom du dossier d'attente, sous le dossier de données de l'agent (le même que `ObjectRoot::Staging`).
pub const STAGING_DIR: &str = "staging";

/// `WorldFs` réel : le dossier de données de l'agent, de confiance.
#[derive(Debug, Clone)]
pub struct SystemWorldFs {
    data_dir: PathBuf,
}

impl SystemWorldFs {
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }
}

#[cfg(unix)]
impl WorldFs for SystemWorldFs {
    fn open_source(&self, root: &Path) -> Result<Box<dyn WorldSource>, WorldFsError> {
        Ok(Box::new(unix::UnixSource::open(root)?))
    }

    fn create_import(
        &self,
        op: &str,
        owner: Ownership,
    ) -> Result<(Box<dyn WorldSink>, ImportDir), WorldFsError> {
        let (sink, import) = unix::UnixSink::create(&self.data_dir, op, owner)?;
        Ok((Box::new(sink), import))
    }

    fn place(
        &self,
        import: &ImportDir,
        key: &ServiceKey,
        folder: &str,
    ) -> Result<(), WorldFsError> {
        unix::place(&self.data_dir, import, key, folder)
    }

    fn remove_import(&self, import: &ImportDir) -> Result<(), WorldFsError> {
        unix::remove_import(&self.data_dir, import)
    }

    fn lower_priority(&self) {
        unix::lower_priority();
    }
}

#[cfg(not(unix))]
impl WorldFs for SystemWorldFs {
    fn open_source(&self, _root: &Path) -> Result<Box<dyn WorldSource>, WorldFsError> {
        let _ = &self.data_dir;
        Err(WorldFsError::Unsupported)
    }

    fn create_import(
        &self,
        _op: &str,
        _owner: Ownership,
    ) -> Result<(Box<dyn WorldSink>, ImportDir), WorldFsError> {
        Err(WorldFsError::Unsupported)
    }

    fn place(
        &self,
        _import: &ImportDir,
        _key: &ServiceKey,
        _folder: &str,
    ) -> Result<(), WorldFsError> {
        Err(WorldFsError::Unsupported)
    }

    fn remove_import(&self, _import: &ImportDir) -> Result<(), WorldFsError> {
        Err(WorldFsError::Unsupported)
    }
}

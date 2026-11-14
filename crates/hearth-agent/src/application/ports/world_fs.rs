//! Port `WorldFs` : lire un monde dans un dossier ouvert aux services, SANS JAMAIS Y ÉCRIRE, et en écrire la
//! copie dans un dossier d'import neuf (HRT-105, `BR-MONDE-004`, `BR-MONDE-005`, `BR-MONDE-010`).
//!
//! La **source** ([`WorldSource`]) n'a que des méthodes de lecture : le refus d'écrire est une propriété du type,
//! pas une promesse de l'adaptateur. La **destination** ([`WorldSink`]) est un dossier créé par l'opération
//! elle-même, dans le dossier d'attente de l'agent ; elle n'existe que pour la copie. Aucun chemin absolu ne
//! passe ici hors de `open_source` (un dossier écrit dans la configuration de l'agent) : tout le reste est un
//! [`RelPath`] déjà jugé. Aucune erreur ne porte de chemin.

use std::io::{self, Read, Write};
use std::path::Path;

use super::Ownership;
use crate::domain::files::RelPath;
use crate::domain::services::ServiceKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum WorldFsError {
    #[error("not found")]
    NotFound,
    /// Un composant est un lien ou n'est pas un dossier : jamais suivi.
    #[error("blocked: link or not a directory")]
    Blocked,
    /// Autre chose qu'un fichier ordinaire (lien, tube, socket, périphérique).
    #[error("not a regular file")]
    NotRegular,
    #[error("crosses a mount point")]
    CrossesDevice,
    /// La racine n'est pas un dossier absolu, ou le dossier d'attente n'est pas à l'agent seul.
    #[error("unsafe root")]
    UnsafeRoot,
    #[error("already exists")]
    Exists,
    #[error("not available on this platform")]
    Unsupported,
    #[error("i/o error: {0:?}")]
    Io(io::ErrorKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldEntryKind {
    File,
    Dir,
    /// Lien symbolique.
    Link,
    /// Tube, socket, périphérique.
    Other,
}

/// Un élément d'un dossier : son nom (un composant), son genre SANS suivre de lien, sa taille et sa date.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldDirEntry {
    pub name: String,
    pub kind: WorldEntryKind,
    pub size: u64,
    /// Date de dernière modification en nanosecondes depuis l'époque Unix.
    pub mtime_ns: i128,
}

/// Ce que `list` a lu : au plus `limit` éléments ; `truncated` quand il y en avait davantage.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorldDirListing {
    pub entries: Vec<WorldDirEntry>,
    pub truncated: bool,
}

/// Taille et date d'un fichier ordinaire, relevées sur le fichier ouvert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldFileStat {
    pub size: u64,
    pub mtime_ns: i128,
}

/// Une racine ouverte en LECTURE SEULE. Les noms de dossiers sont des composants d'un [`RelPath`] ; la racine
/// elle-même est `RelPath::root()`.
pub trait WorldSource: Send + Sync {
    /// Lit au plus `limit` éléments du dossier, sans ouvrir aucun d'eux.
    fn list(&self, dir: &RelPath, limit: usize) -> Result<WorldDirListing, WorldFsError>;

    /// Taille et date d'un fichier ordinaire ; `NotRegular` pour un lien ou un fichier spécial.
    fn stat(&self, file: &RelPath) -> Result<WorldFileStat, WorldFsError>;

    /// Ouvre un fichier ordinaire en lecture (jamais un lien, jamais un fichier spécial : le genre est jugé
    /// AVANT l'ouverture).
    fn open(&self, file: &RelPath) -> Result<Box<dyn Read + Send>, WorldFsError>;
}

/// Un fichier de la copie en cours d'écriture ; `finish` le rend durable.
pub trait SinkFile: Write + Send {
    fn finish(self: Box<Self>) -> io::Result<()>;
}

/// Le dossier d'import, neuf, créé par l'opération. Propriétaire et droits sont ceux de la fiche.
pub trait WorldSink: Send + Sync {
    /// Crée un dossier dont le parent existe.
    fn mkdir(&self, dir: &RelPath) -> Result<(), WorldFsError>;

    /// Crée un fichier NEUF (jamais un remplacement), dont le parent existe.
    fn create(&self, file: &RelPath) -> Result<Box<dyn SinkFile>, WorldFsError>;

    /// Relit un fichier de la copie.
    fn open(&self, file: &RelPath) -> Result<Box<dyn Read + Send>, WorldFsError>;
}

/// Le dossier d'import tel que l'opération le nomme au journal : un nom sous le dossier d'attente de l'agent,
/// et l'identité (périphérique, inode) qui permet de ne retirer QUE lui (`BR-MONDE-026`, ADR-0045).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportDir {
    pub rel: String,
    pub dev: u64,
    pub ino: u64,
}

pub trait WorldFs: Send + Sync {
    /// Ouvre un dossier écrit dans la configuration de l'agent, en lecture seule.
    fn open_source(&self, root: &Path) -> Result<Box<dyn WorldSource>, WorldFsError>;

    /// Crée `import-<op>` sous le dossier d'attente (`0700` pour l'agent seul jusqu'à la mise en place) et le
    /// rend écrivable ; les fichiers et dossiers créés dedans ont `owner`.
    fn create_import(
        &self,
        op: &str,
        owner: Ownership,
    ) -> Result<(Box<dyn WorldSink>, ImportDir), WorldFsError>;

    /// Met en place le dossier d'import : renommage vers `services/<clé>/<dossier>`, qui doit exister et
    /// être VIDE (le dossier géré créé pour le service). Ne remplace jamais un contenu.
    fn place(&self, import: &ImportDir, key: &ServiceKey, folder: &str)
    -> Result<(), WorldFsError>;

    /// Retire le dossier d'import SI c'est toujours lui (périphérique et inode du journal). Rien d'autre.
    fn remove_import(&self, import: &ImportDir) -> Result<(), WorldFsError>;

    /// Abaisse la priorité du fil appelant (la copie tourne sur un fil dédié). Sans effet par défaut.
    fn lower_priority(&self) {}
}

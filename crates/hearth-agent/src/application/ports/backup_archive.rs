//! Port `BackupArchive` : le format de l'archive d'une sauvegarde (HRT-97, ADR-0047, BR-BACKUP-013,
//! BR-BACKUP-014).
//!
//! Le cas d'usage ne sait pas que l'archive est un `.tar.gz` : il demande un constructeur qui reçoit des dossiers,
//! des liens et des fichiers lus en flux, puis un manifeste, et un vérificateur qui relit une archive comme
//! hostile et la juge contre ce manifeste. Le format (tar GNU, gzip niveau 1, SHA-256 au fil de l'écriture) est
//! dans `infrastructure/backup_archive.rs`.

use std::io::{self, Read};

use super::{BackupSink, EntryMeta};
use crate::domain::backups::manifest::Manifest;

/// Ce qu'un fichier archivé a donné.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDigest {
    pub sha256: String,
    /// Le fichier s'est arrêté avant sa taille annoncée (il a rétréci) : complété de zéros.
    pub short: bool,
}

/// Le fichier de fin d'écriture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveEnd {
    pub bytes: u64,
    pub sha256: String,
}

/// Pourquoi une vérification n'a pas conclu « vérifiée ».
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum VerifyError {
    /// L'archive n'est pas ce que le manifeste dit : format, entrée, taille, empreinte (« Corrompue »).
    #[error("archive corrupt")]
    Corrupt,
    /// La lecture a échoué pour une autre raison (disque) : rien n'est conclu sur l'archive.
    #[error("read error: {0:?}")]
    Io(io::ErrorKind),
    /// La vérification a été interrompue (arrêt de l'agent).
    #[error("stopped")]
    Stopped,
}

/// Ce que la lecture a établi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerifyReport {
    pub files_checked: u64,
}

/// Une archive en cours de construction.
pub trait ArchiveBuilder: Send {
    /// Un dossier.
    fn dir(&mut self, tar_path: &str, meta: &EntryMeta) -> io::Result<()>;

    /// Un lien symbolique, jamais suivi.
    fn symlink(&mut self, tar_path: &str, meta: &EntryMeta, target: &str) -> io::Result<()>;

    /// Un fichier : exactement `meta.size` octets, lus en flux, hachés au passage.
    fn file(
        &mut self,
        tar_path: &str,
        meta: &EntryMeta,
        source: &mut dyn Read,
    ) -> io::Result<FileDigest>;

    /// Ajoute le manifeste en dernière entrée, ferme l'archive et rend le fichier durable.
    fn finish(self: Box<Self>, manifest_json: &[u8]) -> io::Result<ArchiveEnd>;
}

pub trait BackupArchive: Send + Sync {
    /// Commence une archive dans `sink` (le fichier partiel).
    fn builder(&self, sink: Box<dyn BackupSink>) -> Box<dyn ArchiveBuilder>;

    /// Relit l'archive et la juge contre `manifest` (qui a passé `Manifest::check`) : chaque entrée a sa ligne, du
    /// bon type, de la bonne taille, du bon SHA-256 ; aucune entrée de plus ; le manifeste de l'archive est celui
    /// d'à côté ; l'empreinte de l'archive entière est `archive_sha256`. `stop` : demande d'arrêt, relue entre deux
    /// entrées.
    fn verify(
        &self,
        archive: Box<dyn Read + Send>,
        manifest: &Manifest,
        archive_sha256: &str,
        stop: &dyn Fn() -> bool,
    ) -> Result<VerifyReport, VerifyError>;
}

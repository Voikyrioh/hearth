//! Port `BackupFiles` : la racine des sauvegardes (HRT-97, ADR-0047, BR-BACKUP-004, BR-BACKUP-005).
//!
//! **Hearth n'y touche qu'un fichier inscrit à son registre.** Le port ne liste donc JAMAIS la racine : il n'a ni
//! `read_dir` ni recherche par nom. Chaque geste vise un fichier dont le cas d'usage a déjà le droit de parler
//! (un nom dérivé d'un identifiant de sauvegarde, dans le sous-dossier d'un service), et les gestes qui
//! détruisent (`remove`) revérifient le numéro d'inode donné par le registre.
//!
//! La racine est `0700` à l'utilisateur de l'agent ; les archives sont `0600`. Aucune erreur ne porte un chemin.

use std::io::{self, Read, Write};

use thiserror::Error;

use crate::domain::backups::ownership::Seen;

/// Périphérique et inode d'un fichier.
pub type FileIdent = (u64, u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum BackupFsError {
    /// Plate-forme sans inode (l'agent ne tourne que sous Linux) : pas d'inode, pas de sauvegarde.
    #[error("backups are not available on this platform")]
    Unsupported,
    /// La racine ou le dossier d'un service n'est pas un vrai dossier de l'agent (lien, autre propriétaire).
    #[error("unsafe backups root")]
    Unsafe,
    #[error("not found")]
    NotFound,
    /// Le fichier existe déjà (création exclusive).
    #[error("already exists")]
    Exists,
    #[error("i/o error: {0:?}")]
    Io(io::ErrorKind),
}

/// L'espace d'un disque.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskSpace {
    /// Octets disponibles pour l'agent.
    pub free: u64,
    pub total: u64,
}

/// L'identité complète d'un fichier publié : de quoi reconnaître qu'il est encore CELUI que Hearth a écrit (un
/// numéro d'inode libéré peut être réattribué ; la taille et la date de modification, à la nanoseconde, ne
/// coïncident pas par hasard).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileStamp {
    pub dev: u64,
    pub ino: u64,
    pub size: u64,
    pub mtime_ns: i64,
}

/// Ce que `remove` exige du fichier avant de le retirer. La taille et la date ne sont connues que d'une archive
/// publiée ; une partie en cours d'écriture ou un manifeste n'a que son inode (le manifeste a, en plus, son
/// empreinte, vérifiée par l'appelant).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Expected {
    pub dev: u64,
    pub ino: u64,
    pub size: Option<u64>,
    pub mtime_ns: Option<i64>,
}

impl Expected {
    /// L'inode seulement.
    pub const fn inode(ident: FileIdent) -> Self {
        Self {
            dev: ident.0,
            ino: ident.1,
            size: None,
            mtime_ns: None,
        }
    }

    /// Le fichier publié tel que Hearth l'a vu.
    pub const fn stamp(stamp: FileStamp) -> Self {
        Self {
            dev: stamp.dev,
            ino: stamp.ino,
            size: Some(stamp.size),
            mtime_ns: Some(stamp.mtime_ns),
        }
    }
}

/// Ce que `remove` a fait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Removed {
    /// Le fichier de l'inode attendu est retiré.
    Done,
    /// Il n'y avait plus rien.
    Gone,
    /// Il y a un fichier, mais pas celui du registre : RIEN n'est touché.
    NotOurs,
}

/// Un fichier en cours d'écriture : des octets, puis `finish` le rend durable.
pub trait BackupSink: Write + Send {
    /// Vide les tampons et rend le fichier durable (`fsync`).
    fn finish(&mut self) -> io::Result<()>;
}

pub trait BackupFiles: Send + Sync {
    /// Crée la racine (`0700`) si elle manque et la juge : un vrai dossier de l'agent, ni lien ni autre propriétaire.
    fn ensure_root(&self) -> Result<(), BackupFsError>;

    /// L'espace du disque de la racine.
    fn space(&self) -> Result<DiskSpace, BackupFsError>;

    /// Crée `<service>/<nom>` NEUF (création exclusive, jamais par un lien), `0600`. Crée le sous-dossier du
    /// service (`0700`) s'il manque. Rend son identité.
    fn create(
        &self,
        service_dir: &str,
        name: &str,
    ) -> Result<(Box<dyn BackupSink>, FileIdent), BackupFsError>;

    /// Rend `part` définitif sous `name` (renommage dans le même dossier qui ne remplace JAMAIS un fichier
    /// existant, atomiquement : `RENAME_NOREPLACE`), après avoir revérifié que `part` est toujours le fichier
    /// `expect`. Rend l'identité complète du fichier publié.
    fn publish(
        &self,
        service_dir: &str,
        part: &str,
        name: &str,
        expect: FileIdent,
    ) -> Result<FileStamp, BackupFsError>;

    /// Écrit un petit fichier neuf d'un coup (le manifeste voisin) : création exclusive, `fsync`.
    fn write_new(
        &self,
        service_dir: &str,
        name: &str,
        bytes: &[u8],
    ) -> Result<FileIdent, BackupFsError>;

    /// Ce qui se trouve à cet endroit : rien, autre chose qu'un fichier ordinaire, ou un fichier et son identité.
    fn seen(&self, service_dir: &str, name: &str) -> Result<Seen, BackupFsError>;

    /// Ouvre un fichier ordinaire en lecture (jamais par un lien) et rend son identité.
    fn open(
        &self,
        service_dir: &str,
        name: &str,
    ) -> Result<(Box<dyn Read + Send>, FileIdent), BackupFsError>;

    /// Retire le fichier SI c'est celui de l'identité donnée (revérifiée sur le fichier ouvert, juste avant).
    /// Le dossier du service reste : un dossier vide ne gêne personne, et en retirer un serait une course avec la
    /// création d'une sauvegarde.
    fn remove(
        &self,
        service_dir: &str,
        name: &str,
        expect: Expected,
    ) -> Result<Removed, BackupFsError>;
}

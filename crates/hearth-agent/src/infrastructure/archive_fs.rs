//! Adaptateur fichiers de la lecture d'archive (HRT-130, ADR-0055, `BR-ARCH-007`, `BR-ARCH-008`,
//! `BR-ARCH-009`).
//!
//! Le domaine (`domain::archive`) juge et décrit ; ce module ouvre le fichier d'archive et écrit, dans
//! un dossier d'attente NEUF, ce que le domaine lui remet, SANS JAMAIS SUIVRE UN LIEN et SANS AUCUN
//! APPEL PAR CHEMIN une fois le dossier parent ouvert (`unix.rs`).
//!
//! # La source
//!
//! L'archive est ouverte sans suivre de lien ni bloquer sur un tube nommé (aide de `services_key.rs`),
//! puis `fstat` sur le descripteur : un fichier ordinaire, sinon `SOURCE_UNREADABLE`.
//!
//! # La destination (Unix, seule plate-forme prise en charge pour écrire)
//!
//! L'appelant donne une [`Destination`] : une racine de CONFIANCE (chemin absolu, dont elle répond) et un
//! chemin relatif (`parents/.../nom`). Le dossier parent est ouvert UNE fois, composant par composant
//! depuis la racine (`O_DIRECTORY | O_NOFOLLOW`), puis tout se fait relativement à ce descripteur :
//! création du dossier d'attente (`mkdirat`, nom imprévisible, `0700`), son ouverture, les écritures,
//! la bascule (`renameat2` avec `RENAME_NOREPLACE`, les deux noms relatifs au MÊME descripteur) et le
//! nettoyage (`unlinkat`, jamais de lien suivi, seulement le dossier dont le périphérique et l'inode sont
//! ceux de celui que l'opération a créé).
//!
//! **Précondition, vérifiée sur le descripteur du parent (`fstat`) et refusée sinon
//! (`UNSAFE_DESTINATION`)** : le parent appartient à l'utilisateur EFFECTIF de l'agent et n'est
//! inscriptible ni par le groupe ni par les autres (`mode & 0o022 == 0`). Un autre compte qui pourrait y
//! écrire pourrait substituer ou renommer le dossier d'attente entre deux appels : on ne l'accepte pas.
//!
//! **Bascule** : si le noyau ou le système de fichiers ne prend pas en charge `RENAME_NOREPLACE`
//! (`EINVAL`, `ENOSYS`, `ENOTSUP`), le dossier d'attente est retiré et l'extraction est refusée
//! (`ATOMIC_SWITCH_UNAVAILABLE`) : jamais de repli par un `rename` qui remplacerait en silence.
//!
//! **Propriétaire et droits** : posés par descripteur à la création (`fchown`, `fchmod`) d'après la
//! [`WriteSpec`] de l'appelant (identifiants numériques et modes fixes), jamais d'après l'archive ni le
//! masque ambiant. **Durabilité** : `fsync` de chaque fichier, de chaque dossier puis du dossier
//! d'attente avant la bascule, et du parent après.
//!
//! # Hors Unix
//!
//! L'agent ne tourne que sous Linux. Hors Unix, lire (`inspect_file`, `read_file_entry`) reste possible
//! (poste de développement), mais ÉCRIRE est refusé (`ATOMIC_SWITCH_UNAVAILABLE`) : il n'existe aucun
//! repli par chemin, donc aucun repli « non sûr face aux courses » que la production pourrait atteindre.

use std::fs::File;
use std::path::{Component, Path};

use crate::domain::archive::{
    ArchiveLimits, ArchiveRefusal, Cancel, ExtractReport, Inspection, inspect, read_one,
};
use crate::infrastructure::services_key::open_read_no_follow;

#[cfg(test)]
#[cfg(unix)]
mod tests_unix;
#[cfg(unix)]
mod unix;

#[cfg(unix)]
pub use unix::DirectorySink;

/// Propriétaire demandé pour tout ce que l'extraction crée : identifiants numériques donnés par
/// l'appelant, jamais repris de l'archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileOwner {
    uid: u32,
    gid: u32,
}

impl FileOwner {
    /// `None` si un identifiant vaut `u32::MAX` (« ne pas changer » pour `chown`, jamais un compte).
    pub fn new(uid: u32, gid: u32) -> Option<Self> {
        (uid != u32::MAX && gid != u32::MAX).then_some(Self { uid, gid })
    }

    pub fn uid(&self) -> u32 {
        self.uid
    }

    pub fn gid(&self) -> u32 {
        self.gid
    }
}

/// Propriétaire et droits FIXES de ce que l'extraction crée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteSpec {
    owner: FileOwner,
    file_mode: u32,
    dir_mode: u32,
}

impl WriteSpec {
    /// `file_mode` : lecture/écriture seulement (aucun bit d'exécution, `setuid`, `setgid` ni
    /// `sticky`), lecture par le propriétaire au moins. `dir_mode` : `rwx` pour le propriétaire au
    /// moins, aucun bit spécial. Sinon `None`.
    pub fn new(owner: FileOwner, file_mode: u32, dir_mode: u32) -> Option<Self> {
        let file_ok = file_mode & !0o666 == 0 && file_mode & 0o400 != 0;
        let dir_ok = dir_mode & !0o777 == 0 && dir_mode & 0o700 == 0o700;
        (file_ok && dir_ok).then_some(Self {
            owner,
            file_mode,
            dir_mode,
        })
    }

    pub fn owner(&self) -> FileOwner {
        self.owner
    }

    pub fn file_mode(&self) -> u32 {
        self.file_mode
    }

    pub fn dir_mode(&self) -> u32 {
        self.dir_mode
    }
}

/// Où poser le dossier extrait : une racine de CONFIANCE absolue et un chemin relatif sous elle, dont
/// le dernier composant est le nom (qui ne doit pas exister) et les autres des dossiers existants.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(not(unix), allow(dead_code))] // hors Unix, rien n'est écrit : les champs ne sont pas lus
pub struct Destination<'a> {
    trusted_root: &'a Path,
    relative: &'a Path,
}

/// Profondeur maximale du chemin relatif d'une destination.
const MAX_DESTINATION_DEPTH: usize = 64;

impl<'a> Destination<'a> {
    /// `None` si la racine n'est pas absolue ou si le chemin relatif est vide, absolu, ou contient
    /// autre chose que des noms simples (`.`, `..`, préfixe).
    pub fn new(trusted_root: &'a Path, relative: &'a Path) -> Option<Self> {
        if !trusted_root.is_absolute() {
            return None;
        }
        let mut count = 0usize;
        for component in relative.components() {
            if !matches!(component, Component::Normal(_)) {
                return None;
            }
            count += 1;
        }
        (1..=MAX_DESTINATION_DEPTH)
            .contains(&count)
            .then_some(Self {
                trusted_root,
                relative,
            })
    }

    #[cfg(unix)]
    fn trusted_root(&self) -> &Path {
        self.trusted_root
    }

    /// Les noms simples du chemin relatif, le dernier compris.
    #[cfg(unix)]
    fn names(&self) -> Vec<&std::ffi::OsStr> {
        self.relative
            .components()
            .filter_map(|component| match component {
                Component::Normal(name) => Some(name),
                _ => None,
            })
            .collect()
    }
}

/// Ouvre le fichier d'archive : sans suivre de lien, sans bloquer sur un tube nommé, et seulement un
/// fichier ordinaire (vérifié sur le descripteur ouvert).
fn open_archive(path: &Path) -> Result<File, ArchiveRefusal> {
    let file = open_read_no_follow(path).map_err(|_| ArchiveRefusal::SourceUnreadable)?;
    match file.metadata() {
        Ok(metadata) if metadata.is_file() => Ok(file),
        _ => Err(ArchiveRefusal::SourceUnreadable),
    }
}

/// Juge le fichier d'archive `path` : rien n'est écrit.
pub fn inspect_file(
    path: &Path,
    limits: &ArchiveLimits,
    cancel: Cancel<'_>,
) -> Result<Inspection, ArchiveRefusal> {
    let mut file = open_archive(path)?;
    inspect(&mut file, limits, cancel)
}

/// Lit UN fichier de l'archive déjà inspectée (1 Mio au plus).
pub fn read_file_entry(
    path: &Path,
    inspection: &Inspection,
    wanted: &str,
    limits: &ArchiveLimits,
    cancel: Cancel<'_>,
) -> Result<Vec<u8>, ArchiveRefusal> {
    let mut file = open_archive(path)?;
    read_one(&mut file, inspection, wanted, limits, cancel)
}

/// Extrait l'archive inspectée sous `destination` (dont le dernier composant ne doit pas exister) :
/// dossier d'attente voisin, extraction, `fsync`, bascule sans remplacement, `fsync` du parent.
/// L'empreinte du fichier est recalculée et doit être celle de l'inspection. Refus, panne ou
/// annulation : le dossier d'attente est retiré et la destination n'existe pas.
pub fn extract_file(
    path: &Path,
    inspection: &Inspection,
    destination: &Destination<'_>,
    spec: &WriteSpec,
    limits: &ArchiveLimits,
    cancel: Cancel<'_>,
) -> Result<ExtractReport, ArchiveRefusal> {
    #[cfg(unix)]
    {
        let mut file = open_archive(path)?;
        unix::extract_into(&mut file, inspection, destination, spec, limits, cancel)
    }
    #[cfg(not(unix))]
    {
        let _ = (path, inspection, destination, spec, limits, cancel);
        Err(ArchiveRefusal::AtomicSwitchUnavailable)
    }
}

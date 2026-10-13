//! Adaptateur fichiers de la lecture d'archive (HRT-130, ADR-0055, `BR-ARCH-007`, `BR-ARCH-009`).
//!
//! Le domaine (`domain::archive`) juge et décrit ; ce module ouvre le fichier d'archive et écrit, dans
//! un dossier d'attente NEUF, ce que le domaine lui remet, SANS JAMAIS SUIVRE UN LIEN :
//!
//! - Unix : chaque dossier traversé est ouvert par `openat(parent, composant, O_NOFOLLOW | O_DIRECTORY)`
//!   à partir du descripteur du dossier d'attente, jamais par un chemin complet ; un fichier est créé par
//!   `openat(..., O_CREAT | O_EXCL | O_NOFOLLOW)`, qui échoue sur tout objet déjà présent, lien compris.
//!   Un lien trouvé en route (créé par un tiers, ou déjà présent dans une destination reprise) donne
//!   `SinkError::Blocked` : on ne le suit pas, on ne l'ouvre pas, on n'écrit pas au travers.
//! - Hors Unix (poste de développement Windows) : repli par `symlink_metadata` à chaque composant puis
//!   `create_new` ; sans la garantie sans course de la voie Unix (aucun root ni service ici).
//!
//! Fichiers créés `0640`, dossiers `0750` (moins le `umask`), aucun bit d'exécution, aucun
//! setuid/setgid, propriétaire du processus : rien n'est repris de l'archive.
//!
//! Extraction : dossier d'attente voisin de la destination, puis bascule par `rename`. Un refus, une
//! panne ou une annulation retire le dossier d'attente (`remove_dir_all` ne suit pas les liens) et ne
//! laisse rien à la destination.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::domain::archive::{
    ArchiveLimits, ArchivePath, ArchiveRefusal, Cancel, ExtractReport, ExtractSink, Inspection,
    SinkError, extract, inspect, read_one,
};

/// Écrit sous un dossier racine donné, sans suivre de lien.
pub struct DirectorySink {
    root: imp::Root,
}

impl DirectorySink {
    /// Ouvre un dossier existant comme racine d'écriture. La racine elle-même n'est pas suivie si c'est
    /// un lien (Unix).
    pub fn open(root: &Path) -> std::io::Result<Self> {
        Ok(Self {
            root: imp::Root::open(root)?,
        })
    }
}

impl ExtractSink for DirectorySink {
    fn create_dir(&mut self, path: &ArchivePath) -> Result<(), SinkError> {
        let components: Vec<&str> = path.components().collect();
        self.root.make_dirs(&components)
    }

    fn create_file(&mut self, path: &ArchivePath) -> Result<Box<dyn Write + '_>, SinkError> {
        let components: Vec<&str> = path.components().collect();
        let Some((name, parents)) = components.split_last() else {
            return Err(SinkError::Blocked);
        };
        let file = self.root.create_file(parents, name)?;
        Ok(Box::new(file))
    }
}

#[cfg(unix)]
mod imp {
    use std::fs::File;
    use std::os::fd::OwnedFd;
    use std::path::Path;

    use rustix::fs::{CWD, Mode, OFlags, mkdirat, openat};
    use rustix::io::Errno;

    use crate::domain::archive::SinkError;

    const DIR_MODE: Mode = Mode::RWXU.union(Mode::RGRP).union(Mode::XGRP);
    const FILE_MODE: Mode = Mode::RUSR.union(Mode::WUSR).union(Mode::RGRP);

    fn blocked_or_io(errno: Errno) -> SinkError {
        match errno {
            Errno::EXIST => SinkError::AlreadyExists,
            // Un lien (O_NOFOLLOW), ou un fichier là où un dossier était attendu.
            Errno::LOOP | Errno::NOTDIR => SinkError::Blocked,
            other => SinkError::Io(std::io::Error::from(other).kind()),
        }
    }

    pub(super) struct Root(OwnedFd);

    impl Root {
        pub(super) fn open(path: &Path) -> std::io::Result<Self> {
            let fd = openat(
                CWD,
                path,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )?;
            Ok(Self(fd))
        }

        /// Crée (si besoin) puis ouvre chaque dossier de la chaîne, un composant à la fois, depuis le
        /// descripteur du précédent. Rend le descripteur du dernier.
        fn descend(&self, components: &[&str]) -> Result<OwnedFd, SinkError> {
            let mut current = self
                .0
                .try_clone()
                .map_err(|error| SinkError::Io(error.kind()))?;
            for component in components {
                match mkdirat(&current, *component, DIR_MODE) {
                    Ok(()) | Err(Errno::EXIST) => {}
                    Err(errno) => return Err(blocked_or_io(errno)),
                }
                current = openat(
                    &current,
                    *component,
                    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                    Mode::empty(),
                )
                .map_err(blocked_or_io)?;
            }
            Ok(current)
        }

        pub(super) fn make_dirs(&self, components: &[&str]) -> Result<(), SinkError> {
            self.descend(components).map(drop)
        }

        pub(super) fn create_file(&self, parents: &[&str], name: &str) -> Result<File, SinkError> {
            let parent = self.descend(parents)?;
            let fd = openat(
                &parent,
                name,
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                FILE_MODE,
            )
            .map_err(blocked_or_io)?;
            Ok(File::from(fd))
        }
    }
}

#[cfg(not(unix))]
mod imp {
    use std::fs::{File, OpenOptions};
    use std::path::{Path, PathBuf};

    use crate::domain::archive::SinkError;

    pub(super) struct Root(PathBuf);

    fn io(error: std::io::Error) -> SinkError {
        match error.kind() {
            std::io::ErrorKind::AlreadyExists => SinkError::AlreadyExists,
            kind => SinkError::Io(kind),
        }
    }

    impl Root {
        pub(super) fn open(path: &Path) -> std::io::Result<Self> {
            let metadata = std::fs::symlink_metadata(path)?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(std::io::ErrorKind::InvalidInput.into());
            }
            Ok(Self(path.to_path_buf()))
        }

        fn descend(&self, components: &[&str]) -> Result<PathBuf, SinkError> {
            let mut current = self.0.clone();
            for component in components {
                current.push(component);
                match std::fs::symlink_metadata(&current) {
                    Ok(metadata) if metadata.file_type().is_symlink() => {
                        return Err(SinkError::Blocked);
                    }
                    Ok(metadata) if metadata.is_dir() => {}
                    Ok(_) => return Err(SinkError::Blocked),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        std::fs::create_dir(&current).map_err(io)?;
                    }
                    Err(error) => return Err(io(error)),
                }
            }
            Ok(current)
        }

        pub(super) fn make_dirs(&self, components: &[&str]) -> Result<(), SinkError> {
            self.descend(components).map(drop)
        }

        pub(super) fn create_file(&self, parents: &[&str], name: &str) -> Result<File, SinkError> {
            let parent = self.descend(parents)?;
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(parent.join(name))
                .map_err(io)
        }
    }
}

/// Juge le fichier d'archive `path` : rien n'est écrit.
pub fn inspect_file(
    path: &Path,
    limits: &ArchiveLimits,
    cancel: Cancel<'_>,
) -> Result<Inspection, ArchiveRefusal> {
    let mut file = File::open(path).map_err(|_| ArchiveRefusal::SourceUnreadable)?;
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
    let mut file = File::open(path).map_err(|_| ArchiveRefusal::SourceUnreadable)?;
    read_one(&mut file, inspection, wanted, limits, cancel)
}

fn staging_beside(destination: &Path) -> Option<PathBuf> {
    let name = destination.file_name()?.to_str()?;
    let parent = destination.parent().filter(|p| !p.as_os_str().is_empty())?;
    Some(parent.join(format!(".{name}.staging-{}", ulid::Ulid::generate())))
}

fn create_private_dir(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new().mode(0o750).create(path)
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir(path)
    }
}

fn absent(path: &Path) -> bool {
    matches!(
        std::fs::symlink_metadata(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    )
}

/// Extrait l'archive inspectée dans `destination`, qui ne doit pas exister : dossier d'attente voisin,
/// extraction, puis bascule. L'empreinte du fichier est recalculée et doit être celle de l'inspection.
/// Refus, panne ou annulation : le dossier d'attente est retiré, `destination` n'existe pas.
pub fn extract_file(
    path: &Path,
    inspection: &Inspection,
    destination: &Path,
    limits: &ArchiveLimits,
    cancel: Cancel<'_>,
) -> Result<ExtractReport, ArchiveRefusal> {
    let Some(staging) = staging_beside(destination) else {
        return Err(ArchiveRefusal::WriteFailed);
    };
    if !absent(destination) {
        return Err(ArchiveRefusal::WriteFailed);
    }
    let mut file = File::open(path).map_err(|_| ArchiveRefusal::SourceUnreadable)?;
    create_private_dir(&staging).map_err(|_| ArchiveRefusal::WriteFailed)?;
    let outcome = DirectorySink::open(&staging)
        .map_err(|_| ArchiveRefusal::WriteFailed)
        .and_then(|mut sink| extract(&mut file, inspection, &mut sink, limits, cancel));
    let report = match outcome {
        Ok(report) => report,
        Err(refusal) => {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(refusal);
        }
    };
    if !absent(destination) || std::fs::rename(&staging, destination).is_err() {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(ArchiveRefusal::WriteFailed);
    }
    Ok(report)
}

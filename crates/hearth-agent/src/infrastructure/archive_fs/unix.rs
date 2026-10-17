//! Écriture de l'extraction sous Unix (HRT-130, ADR-0055). Voir `../archive_fs.rs` pour le contrat.
//!
//! Règle de ce fichier : une fois le dossier PARENT de la destination ouvert, aucun appel ne prend un
//! chemin. Tout est `openat`, `mkdirat`, `unlinkat`, `renameat2`, `fstat`, `fchown`, `fchmod`, `fsync`
//! relatifs à un descripteur, par NOMS SIMPLES (un composant).

use std::ffi::{CStr, CString, OsStr, OsString};
use std::fs::File;
use std::io::{self, Write};
use std::os::fd::OwnedFd;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::Path;

use rustix::fs::{
    AtFlags, CWD, Dir, FileType, Mode, RenameFlags, Stat, fchmod, fstat, fsync, mkdirat, openat,
    renameat_with, statat, unlinkat,
};
use rustix::io::Errno;
use rustix::process::geteuid;

use super::{Destination, FileOwner, WriteSpec};
use crate::domain::archive::{
    ArchiveLimits, ArchivePath, ArchiveRefusal, Cancel, ExtractReport, ExtractSink, Inspection,
    SinkError, extract as extract_archive,
};
use crate::infrastructure::dir_fd::{self, CREATE_FILE, OPEN_DIR, is_trusted};

/// Plus long nom de fichier que les systèmes de fichiers courants acceptent.
const NAME_MAX: usize = 255;

/// Profondeur au-delà de laquelle un parcours par descripteur s'arrête (l'archive est jugée à 32).
const MAX_TREE_DEPTH: usize = 64;

fn io_kind(errno: Errno) -> io::ErrorKind {
    io::Error::from(errno).kind()
}

fn blocked_or_io(errno: Errno) -> SinkError {
    match errno {
        Errno::EXIST => SinkError::AlreadyExists,
        // Un lien (O_NOFOLLOW), ou un fichier là où un dossier était attendu.
        Errno::LOOP | Errno::NOTDIR => SinkError::Blocked,
        other => SinkError::Io(io_kind(other)),
    }
}

/// Le propriétaire et les droits demandés par l'appelant, jamais ce que dit l'archive.
fn apply(fd: &OwnedFd, owner: FileOwner, bits: u32) -> Result<(), Errno> {
    dir_fd::apply(fd, owner.uid(), owner.gid(), bits)
}

/// Un objet du système de fichiers : périphérique et inode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Identity {
    dev: u128,
    ino: u128,
}

pub(super) fn identity_of(stat: &Stat) -> Identity {
    Identity {
        dev: u128::from(stat.st_dev),
        ino: u128::from(stat.st_ino),
    }
}

pub(super) fn identity_of_fd(fd: &OwnedFd) -> Result<Identity, Errno> {
    fstat(fd).map(|stat| identity_of(&stat))
}

/// Un fichier dont `flush` le rend durable (`fsync`). L'extraction appelle `flush` après les derniers
/// octets de chaque fichier.
struct DurableFile(File);

impl Write for DurableFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.sync_all()
    }
}

/// Écrit sous un dossier racine déjà ouvert, sans suivre de lien : chaque dossier traversé est ouvert
/// par `openat(parent, composant, O_NOFOLLOW | O_DIRECTORY)` depuis le descripteur du précédent ; un
/// fichier est créé par `openat(..., O_CREAT | O_EXCL | O_NOFOLLOW)`, qui échoue sur tout objet déjà
/// présent, lien compris. Un lien trouvé en route donne `SinkError::Blocked`.
pub struct DirectorySink {
    root: OwnedFd,
    spec: WriteSpec,
}

impl DirectorySink {
    /// `root` est un dossier déjà ouvert (le descripteur de l'appelant, qui répond de ce qu'il est).
    pub fn new(root: File, spec: WriteSpec) -> Self {
        Self {
            root: OwnedFd::from(root),
            spec,
        }
    }

    fn from_fd(root: OwnedFd, spec: WriteSpec) -> Self {
        Self { root, spec }
    }

    /// Crée (si besoin) puis ouvre chaque dossier de la chaîne, un composant à la fois, depuis le
    /// descripteur du précédent. Un dossier créé ici reçoit le propriétaire et les droits demandés.
    fn descend(&self, components: &[&str]) -> Result<OwnedFd, SinkError> {
        let mut current = self
            .root
            .try_clone()
            .map_err(|error| SinkError::Io(error.kind()))?;
        for component in components {
            let created = match mkdirat(&current, *component, Mode::RWXU) {
                Ok(()) => true,
                Err(Errno::EXIST) => false,
                Err(errno) => return Err(blocked_or_io(errno)),
            };
            let next =
                openat(&current, *component, OPEN_DIR, Mode::empty()).map_err(blocked_or_io)?;
            if created {
                apply(&next, self.spec.owner(), self.spec.dir_mode())
                    .map_err(|errno| SinkError::Io(io_kind(errno)))?;
            }
            current = next;
        }
        Ok(current)
    }
}

impl ExtractSink for DirectorySink {
    fn create_dir(&mut self, path: &ArchivePath) -> Result<(), SinkError> {
        let components: Vec<&str> = path.components().collect();
        self.descend(&components).map(drop)
    }

    fn create_file(&mut self, path: &ArchivePath) -> Result<Box<dyn Write + '_>, SinkError> {
        let components: Vec<&str> = path.components().collect();
        let Some((name, parents)) = components.split_last() else {
            return Err(SinkError::Blocked);
        };
        let parent = self.descend(parents)?;
        // Créé privé (`0600`, au propriétaire du processus) ; propriétaire et droits demandés posés par
        // le descripteur AVANT le premier octet.
        let fd = openat(&parent, *name, CREATE_FILE, Mode::RUSR.union(Mode::WUSR))
            .map_err(blocked_or_io)?;
        apply(&fd, self.spec.owner(), self.spec.file_mode())
            .map_err(|errno| SinkError::Io(io_kind(errno)))?;
        Ok(Box::new(DurableFile(File::from(fd))))
    }
}

/// Les entrées d'un dossier (hors `.` et `..`), avec « est-ce un dossier ? » (un lien n'en est pas un).
fn list(dir: &OwnedFd) -> Result<Vec<(CString, bool)>, Errno> {
    let mut out = Vec::new();
    for entry in Dir::read_from(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        if name.to_bytes() == b"." || name.to_bytes() == b".." {
            continue;
        }
        let is_dir = match entry.file_type() {
            FileType::Directory => true,
            FileType::Unknown => {
                let stat = statat(dir, name, AtFlags::SYMLINK_NOFOLLOW)?;
                FileType::from_raw_mode(stat.st_mode) == FileType::Directory
            }
            _ => false,
        };
        out.push((name.to_owned(), is_dir));
    }
    Ok(out)
}

fn open_child_dir(dir: &OwnedFd, name: &CStr) -> Result<OwnedFd, Errno> {
    openat(dir, name, OPEN_DIR, Mode::empty())
}

/// Vide un dossier par descripteur : un lien est retiré, jamais suivi. Continue après une erreur et
/// rend la première.
pub(super) fn empty_dir(dir: &OwnedFd, depth: usize) -> Result<(), Errno> {
    if depth > MAX_TREE_DEPTH {
        return Err(Errno::LOOP);
    }
    let mut first_error = None;
    for (name, is_dir) in list(dir)? {
        let result = if is_dir {
            open_child_dir(dir, &name).and_then(|child| {
                empty_dir(&child, depth + 1)?;
                unlinkat(dir, name.as_c_str(), AtFlags::REMOVEDIR)
            })
        } else {
            unlinkat(dir, name.as_c_str(), AtFlags::empty())
        };
        if let Err(errno) = result {
            first_error.get_or_insert(errno);
        }
    }
    first_error.map_or(Ok(()), Err)
}

/// `fsync` de chaque dossier de l'arbre, les plus profonds d'abord, puis de `dir`.
pub(super) fn sync_tree(dir: &OwnedFd, depth: usize) -> Result<(), Errno> {
    if depth > MAX_TREE_DEPTH {
        return Err(Errno::LOOP);
    }
    for (name, is_dir) in list(dir)? {
        if is_dir {
            sync_tree(&open_child_dir(dir, &name)?, depth + 1)?;
        }
    }
    fsync(dir)
}

/// Retire `parent/name` SI ET SEULEMENT SI c'est le dossier (périphérique, inode) que cette opération a
/// créé. Rien n'est suivi, rien n'est retiré hors de lui. Au mieux : une erreur est ignorée.
pub(super) fn remove_created_dir(parent: &OwnedFd, name: &OsStr, created: Identity) {
    let Ok(dir) = openat(parent, name, OPEN_DIR, Mode::empty()) else {
        return;
    };
    if identity_of_fd(&dir) != Ok(created) {
        return;
    }
    let _ = empty_dir(&dir, 0);
    let _ = unlinkat(parent, name, AtFlags::REMOVEDIR);
}

/// Ouvre le dossier parent : la racine de confiance (dernier composant sans suivre de lien), puis
/// chaque composant `O_DIRECTORY | O_NOFOLLOW` depuis le précédent.
pub(super) fn open_parent(root: &Path, parents: &[&OsStr]) -> Result<OwnedFd, ArchiveRefusal> {
    let unsafe_destination = |_| ArchiveRefusal::UnsafeDestination;
    let mut current = openat(CWD, root, OPEN_DIR, Mode::empty()).map_err(unsafe_destination)?;
    for name in parents {
        current = openat(&current, *name, OPEN_DIR, Mode::empty()).map_err(unsafe_destination)?;
    }
    Ok(current)
}

/// Précondition du parent, sur son descripteur : propriétaire = utilisateur EFFECTIF de l'agent, ni
/// groupe ni autres n'y écrivent.
pub(super) fn check_parent(parent: &OwnedFd) -> Result<(), ArchiveRefusal> {
    let stat = fstat(parent).map_err(|_| ArchiveRefusal::UnsafeDestination)?;
    if !is_trusted(&stat) {
        return Err(ArchiveRefusal::UnsafeDestination);
    }
    Ok(())
}

fn absent(parent: &OwnedFd, name: &OsStr) -> bool {
    matches!(
        statat(parent, name, AtFlags::SYMLINK_NOFOLLOW),
        Err(Errno::NOENT)
    )
}

/// La bascule : `renameat2(parent/from, parent/to, RENAME_NOREPLACE)`. Noyau ou système de fichiers
/// sans cette option : refus (`AtomicSwitchUnavailable`), jamais un remplacement silencieux.
pub(super) fn switch(parent: &OwnedFd, from: &OsStr, to: &OsStr) -> Result<(), ArchiveRefusal> {
    renameat_with(parent, from, parent, to, RenameFlags::NOREPLACE).map_err(switch_failure)
}

pub(super) fn switch_failure(errno: Errno) -> ArchiveRefusal {
    match errno {
        Errno::INVAL | Errno::NOSYS | Errno::NOTSUP => ArchiveRefusal::AtomicSwitchUnavailable,
        _ => ArchiveRefusal::WriteFailed,
    }
}

/// Le dossier d'attente, créé et tenu par descripteur.
struct Staging {
    fd: OwnedFd,
    /// Nom actuel sous le parent : celui de l'attente, puis celui de la destination après la bascule.
    name: OsString,
    created: Identity,
}

fn staging_name(leaf: &OsStr) -> Option<OsString> {
    let mut bytes = Vec::with_capacity(leaf.len() + 40);
    bytes.push(b'.');
    bytes.extend_from_slice(leaf.as_bytes());
    bytes.extend_from_slice(b".staging-");
    bytes.extend_from_slice(ulid::Ulid::generate().to_string().as_bytes());
    (bytes.len() <= NAME_MAX).then(|| OsString::from_vec(bytes))
}

impl Staging {
    /// `mkdirat` (nom imprévisible, `0700`), puis ouverture, relevé de l'identité, `fchmod 0700`.
    fn create(parent: &OwnedFd, leaf: &OsStr) -> Result<Self, ArchiveRefusal> {
        let name = staging_name(leaf).ok_or(ArchiveRefusal::WriteFailed)?;
        mkdirat(parent, name.as_os_str(), Mode::RWXU).map_err(|_| ArchiveRefusal::WriteFailed)?;
        let opened = openat(parent, name.as_os_str(), OPEN_DIR, Mode::empty()).and_then(|fd| {
            let stat = fstat(&fd)?;
            if stat.st_uid != geteuid().as_raw() {
                return Err(Errno::PERM);
            }
            fchmod(&fd, Mode::RWXU)?;
            Ok((fd, identity_of(&stat)))
        });
        match opened {
            Ok((fd, created)) => Ok(Self { fd, name, created }),
            Err(_) => {
                // Dossier vide que nous venons de créer : `rmdir` échoue de lui-même s'il ne l'est pas.
                let _ = unlinkat(parent, name.as_os_str(), AtFlags::REMOVEDIR);
                Err(ArchiveRefusal::WriteFailed)
            }
        }
    }

    fn discard(&self, parent: &OwnedFd) {
        remove_created_dir(parent, &self.name, self.created);
    }
}

fn write_failed(_: Errno) -> ArchiveRefusal {
    ArchiveRefusal::WriteFailed
}

/// Ce que l'extraction lit et respecte : la source, l'inspection, les droits demandés, les bornes.
struct Job<'a> {
    source: &'a mut File,
    inspection: &'a Inspection,
    spec: &'a WriteSpec,
    limits: &'a ArchiveLimits,
    cancel: Cancel<'a>,
}

/// Écrit, rend durable, bascule. Tout échec ici est suivi du retrait de ce que `staging` désigne.
fn fill_and_switch(
    job: Job<'_>,
    parent: &OwnedFd,
    staging: &mut Staging,
    leaf: &OsStr,
) -> Result<ExtractReport, ArchiveRefusal> {
    let root = staging
        .fd
        .try_clone()
        .map_err(|_| ArchiveRefusal::WriteFailed)?;
    let mut sink = DirectorySink::from_fd(root, *job.spec);
    let report = extract_archive(
        job.source,
        job.inspection,
        &mut sink,
        job.limits,
        job.cancel,
    )?;
    drop(sink);
    if (job.cancel)() {
        return Err(ArchiveRefusal::Cancelled);
    }
    // Le dossier d'attente prend ses droits définitifs en dernier : jusque-là, privé (`0700`).
    apply(&staging.fd, job.spec.owner(), job.spec.dir_mode()).map_err(write_failed)?;
    sync_tree(&staging.fd, 0).map_err(write_failed)?;
    switch(parent, &staging.name, leaf)?;
    staging.name = leaf.to_owned();
    fsync(parent).map_err(write_failed)?;
    Ok(report)
}

/// Extraction complète : parent ouvert et vérifié, dossier d'attente, écritures, bascule, nettoyage.
pub(super) fn extract_into(
    source: &mut File,
    inspection: &Inspection,
    destination: &Destination<'_>,
    spec: &WriteSpec,
    limits: &ArchiveLimits,
    cancel: Cancel<'_>,
) -> Result<ExtractReport, ArchiveRefusal> {
    let names = destination.names();
    let Some((leaf, parents)) = names.split_last() else {
        return Err(ArchiveRefusal::UnsafeDestination);
    };
    let parent = open_parent(destination.trusted_root(), parents)?;
    check_parent(&parent)?;
    if !absent(&parent, leaf) {
        return Err(ArchiveRefusal::WriteFailed);
    }
    let mut staging = Staging::create(&parent, leaf)?;
    let job = Job {
        source,
        inspection,
        spec,
        limits,
        cancel,
    };
    let outcome = fill_and_switch(job, &parent, &mut staging, leaf);
    if outcome.is_err() {
        staging.discard(&parent);
    }
    outcome
}

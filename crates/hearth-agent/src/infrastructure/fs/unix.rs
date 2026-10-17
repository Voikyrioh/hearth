//! `SafeFs` sous Unix (HRT-145, ADR-0046). Voir `../fs.rs` pour le contrat.
//!
//! Règle de ce fichier : une fois la racine ouverte, aucun appel ne prend un chemin. Tout est `openat`,
//! `mkdirat`, `unlinkat`, `renameat`, `statat`, `fstat`, `fchown`, `fchmod`, `fsync` relatif à un
//! descripteur, par NOMS SIMPLES (un composant). Chaque dossier traversé est ouvert sans suivre de lien
//! et sur le périphérique de la racine.

use std::collections::HashSet;
use std::ffi::{CStr, CString};
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::OwnedFd;
use std::path::Path;
use std::time::{Duration, Instant};

use rustix::fs::{
    AtFlags, CWD, Dir, FileType, Gid, Mode, OFlags, Stat, Uid, fchmod, fchown, fstat, fsync,
    mkdirat, openat, renameat, statat, unlinkat,
};
use rustix::io::Errno;
use rustix::process::geteuid;

use super::SERVICES_DIR;
use crate::application::ports::{FsError, Measure, Perms};
use crate::domain::files::{MAX_DEPTH, RelPath};
use crate::domain::services::ServiceKey;

/// Ouvrir un dossier sans suivre un lien à son dernier composant.
const OPEN_DIR: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);

/// Créer un fichier NEUF, sans suivre un lien.
const CREATE_FILE: OFlags = OFlags::WRONLY
    .union(OFlags::CREATE)
    .union(OFlags::EXCL)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);

/// Ouvrir un fichier en lecture sans suivre de lien ni bloquer sur un tube nommé.
const OPEN_READ: OFlags = OFlags::RDONLY
    .union(OFlags::NOFOLLOW)
    .union(OFlags::NONBLOCK)
    .union(OFlags::CLOEXEC);

/// Fichiers à plusieurs noms retenus pour ne les compter qu'une fois ; au-delà, la mesure est partielle.
const MAX_TRACKED_HARDLINKS: usize = 100_000;

/// Le budget de temps est relu toutes les 1 024 entrées.
const CLOCK_EVERY: u64 = 1024;

fn mode(bits: u32) -> Mode {
    Mode::from_bits_truncate(bits)
}

fn fs_error(errno: Errno) -> FsError {
    match errno {
        Errno::NOENT => FsError::NotFound,
        // Un lien (`O_NOFOLLOW`) ou un fichier là où un dossier était attendu (`O_DIRECTORY`).
        Errno::LOOP | Errno::NOTDIR => FsError::Blocked,
        other => FsError::Io(io::Error::from(other).kind()),
    }
}

fn dev_of(stat: &Stat) -> u128 {
    u128::from(stat.st_dev)
}

fn nlink_of(stat: &Stat) -> u128 {
    u128::from(stat.st_nlink)
}

fn file_type(stat: &Stat) -> FileType {
    FileType::from_raw_mode(stat.st_mode)
}

/// Taille sur le disque : `st_blocks` est compté en secteurs de 512 octets.
fn disk_bytes(stat: &Stat) -> u64 {
    u64::try_from(i128::from(stat.st_blocks) * 512).unwrap_or(0)
}

/// Propriétaire puis droits, sur un descripteur : jamais le masque ambiant, jamais un bit spécial.
fn apply(fd: &OwnedFd, perms: Perms) -> Result<(), FsError> {
    fchown(
        fd,
        Some(Uid::from_raw(perms.owner().uid())),
        Some(Gid::from_raw(perms.owner().gid())),
    )
    .map_err(fs_error)?;
    fchmod(fd, mode(perms.mode())).map_err(fs_error)
}

/// Un dossier de confiance : à l'utilisateur EFFECTIF de l'agent, ni le groupe ni les autres n'y
/// écrivent (sinon un autre compte pourrait y substituer un lien entre deux appels).
fn check_trusted(dir: &OwnedFd) -> Result<Stat, FsError> {
    let stat = fstat(dir).map_err(fs_error)?;
    if stat.st_uid != geteuid().as_raw() || stat.st_mode & 0o022 != 0 {
        return Err(FsError::UnsafeRoot);
    }
    Ok(stat)
}

/// Ouvre `parent/name` comme dossier, sans suivre de lien, sur le périphérique `dev`.
fn open_dir_on(parent: &OwnedFd, name: &str, dev: u128) -> Result<OwnedFd, FsError> {
    let fd = openat(parent, name, OPEN_DIR, Mode::empty()).map_err(fs_error)?;
    if dev_of(&fstat(&fd).map_err(fs_error)?) != dev {
        return Err(FsError::CrossesDevice);
    }
    Ok(fd)
}

/// Crée `parent/name` s'il manque (propriétaire du processus, `0700`) puis l'ouvre. `true` si le
/// dossier vient d'être créé.
fn create_then_open(parent: &OwnedFd, name: &str, dev: u128) -> Result<(OwnedFd, bool), FsError> {
    let created = match mkdirat(parent, name, Mode::RWXU) {
        Ok(()) => true,
        Err(Errno::EXIST) => false,
        Err(errno) => return Err(fs_error(errno)),
    };
    let fd = open_dir_on(parent, name, dev)?;
    Ok((fd, created))
}

/// Une racine de service ouverte : le descripteur du dossier `services/<clé>` et son périphérique.
#[derive(Debug)]
pub struct UnixRoot {
    fd: OwnedFd,
    dev: u128,
}

fn check_key(key: &ServiceKey) -> Result<&str, FsError> {
    RelPath::check_component(key.as_str()).map_err(|_| FsError::Refused)?;
    Ok(key.as_str())
}

/// Le dossier `services/` : ouvert (ou créé `0700`) depuis le dossier de données de confiance.
fn open_services_dir(data_dir: &Path, create: bool) -> Result<(OwnedFd, u128), FsError> {
    let data = openat(CWD, data_dir, OPEN_DIR, Mode::empty()).map_err(fs_error)?;
    let dev = dev_of(&check_trusted(&data)?);
    let services = if create {
        let (fd, created) = create_then_open(&data, SERVICES_DIR, dev)?;
        if created {
            fchmod(&fd, Mode::RWXU).map_err(fs_error)?;
        }
        fd
    } else {
        open_dir_on(&data, SERVICES_DIR, dev)?
    };
    check_trusted(&services)?;
    Ok((services, dev))
}

pub(super) fn open_service_root(
    data_dir: &Path,
    key: &ServiceKey,
    create: bool,
) -> Result<UnixRoot, FsError> {
    let name = check_key(key)?;
    let (services, dev) = open_services_dir(data_dir, create)?;
    let fd = if create {
        let (fd, created) = create_then_open(&services, name, dev)?;
        if created {
            fchmod(&fd, Mode::RWXU).map_err(fs_error)?;
        }
        fd
    } else {
        open_dir_on(&services, name, dev)?
    };
    check_trusted(&fd)?;
    Ok(UnixRoot { fd, dev })
}

/// `BR-DEL-007` : vide la racine du service PUIS la retire, sans sortir de `services/<clé>`.
pub(super) fn remove_service_root(data_dir: &Path, key: &ServiceKey) -> Result<(), FsError> {
    let name = check_key(key)?;
    let (services, dev) = open_services_dir(data_dir, false)?;
    let root = open_dir_on(&services, name, dev)?;
    check_trusted(&root)?;
    empty_dir(&root, dev, 0)?;
    drop(root);
    unlinkat(&services, name, AtFlags::REMOVEDIR).map_err(fs_error)?;
    fsync(&services).map_err(fs_error)
}

/// Les noms d'un dossier (hors `.` et `..`), lus en entier avant d'agir : retirer pendant qu'on lit
/// fait sauter des entrées.
fn names_of(dir: &OwnedFd) -> Result<Vec<CString>, FsError> {
    let mut out = Vec::new();
    for entry in Dir::read_from(dir).map_err(fs_error)? {
        let entry = entry.map_err(fs_error)?;
        let name = entry.file_name();
        if name.to_bytes() != b"." && name.to_bytes() != b".." {
            out.push(name.to_owned());
        }
    }
    Ok(out)
}

/// Vide un dossier par descripteur : un lien est retiré comme lien, jamais suivi ; un montage n'est pas
/// franchi. Continue après une erreur et rend la première.
fn empty_dir(dir: &OwnedFd, dev: u128, depth: usize) -> Result<(), FsError> {
    let mut first_error = None;
    for name in names_of(dir)? {
        if let Err(error) = remove_entry(dir, &name, dev, depth) {
            first_error.get_or_insert(error);
        }
    }
    first_error.map_or(Ok(()), Err)
}

fn remove_entry(dir: &OwnedFd, name: &CStr, dev: u128, depth: usize) -> Result<(), FsError> {
    let stat = match statat(dir, name, AtFlags::SYMLINK_NOFOLLOW) {
        Ok(stat) => stat,
        Err(Errno::NOENT) => return Ok(()),
        Err(errno) => return Err(fs_error(errno)),
    };
    if file_type(&stat) != FileType::Directory {
        return unlink_file(dir, name);
    }
    if depth >= MAX_DEPTH {
        return Err(FsError::Refused);
    }
    match openat(dir, name, OPEN_DIR, Mode::empty()) {
        Ok(child) => {
            if dev_of(&fstat(&child).map_err(fs_error)?) != dev {
                return Err(FsError::CrossesDevice);
            }
            empty_dir(&child, dev, depth + 1)?;
            drop(child);
            unlinkat(dir, name, AtFlags::REMOVEDIR).map_err(fs_error)
        }
        // Remplacé par autre chose qu'un dossier entre-temps : on retire CE nom, sans le suivre.
        Err(Errno::NOTDIR | Errno::LOOP) => unlink_file(dir, name),
        Err(Errno::NOENT) => Ok(()),
        Err(errno) => Err(fs_error(errno)),
    }
}

fn unlink_file(dir: &OwnedFd, name: &CStr) -> Result<(), FsError> {
    match unlinkat(dir, name, AtFlags::empty()) {
        Ok(()) | Err(Errno::NOENT) => Ok(()),
        Err(errno) => Err(fs_error(errno)),
    }
}

impl UnixRoot {
    /// Test seulement : une racine dont le périphérique attendu est faux, donc dont tout descendant
    /// ressemble à un point de montage.
    #[cfg(test)]
    pub(super) fn with_dev(mut self, dev: u128) -> Self {
        self.dev = dev;
        self
    }

    #[cfg(test)]
    pub(super) fn dev(&self) -> u128 {
        self.dev
    }

    /// Ouvre, sans en créer, les dossiers de `names` depuis la racine.
    fn walk<'a>(&self, names: impl IntoIterator<Item = &'a str>) -> Result<OwnedFd, FsError> {
        let mut current = self.fd.try_clone().map_err(|e| FsError::Io(e.kind()))?;
        for name in names {
            current = open_dir_on(&current, name, self.dev)?;
        }
        Ok(current)
    }

    /// Le dossier parent de `rel` et le nom de son dernier composant ; `Refused` pour la racine.
    fn parent_of<'a>(&self, rel: &'a RelPath) -> Result<(OwnedFd, &'a str), FsError> {
        let (leaf, parents) = rel.split_last().ok_or(FsError::Refused)?;
        let parent = self.walk(parents.iter().map(String::as_str))?;
        Ok((parent, leaf))
    }

    pub(super) fn make_dirs(&self, rel: &RelPath, perms: Perms) -> Result<bool, FsError> {
        if rel.is_root() {
            return Err(FsError::Refused);
        }
        let mut current = self.fd.try_clone().map_err(|e| FsError::Io(e.kind()))?;
        let mut created_last = false;
        for name in rel.components() {
            let (next, created) = create_then_open(&current, name, self.dev)?;
            if created {
                apply(&next, perms)?;
            }
            created_last = created;
            current = next;
        }
        Ok(created_last)
    }

    pub(super) fn write_file(
        &self,
        rel: &RelPath,
        bytes: &[u8],
        perms: Perms,
    ) -> Result<(), FsError> {
        let (parent, leaf) = self.parent_of(rel)?;
        match statat(&parent, leaf, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(existing) => match file_type(&existing) {
                FileType::RegularFile if nlink_of(&existing) > 1 => {
                    return Err(FsError::Hardlinked);
                }
                FileType::RegularFile => {}
                FileType::Symlink => return Err(FsError::Blocked),
                _ => return Err(FsError::NotRegular),
            },
            Err(Errno::NOENT) => {}
            Err(errno) => return Err(fs_error(errno)),
        }
        let staging = format!(".hearth-tmp-{}", ulid::Ulid::generate());
        let fd = openat(
            &parent,
            staging.as_str(),
            CREATE_FILE,
            Mode::RUSR | Mode::WUSR,
        )
        .map_err(fs_error)?;
        let outcome = fill_and_place(fd, &parent, &staging, leaf, bytes, perms);
        if outcome.is_err() {
            // Au mieux : c'est CE nom d'attente, créé par nous (`O_EXCL`), jamais suivi.
            let _ = unlinkat(&parent, staging.as_str(), AtFlags::empty());
        }
        outcome
    }

    pub(super) fn read_file(&self, rel: &RelPath, max: u64) -> Result<Vec<u8>, FsError> {
        let (parent, leaf) = self.parent_of(rel)?;
        let fd = openat(&parent, leaf, OPEN_READ, Mode::empty()).map_err(fs_error)?;
        let stat = fstat(&fd).map_err(fs_error)?;
        if file_type(&stat) != FileType::RegularFile {
            return Err(FsError::NotRegular);
        }
        let mut out = Vec::new();
        File::from(fd)
            .take(max.saturating_add(1))
            .read_to_end(&mut out)
            .map_err(|e| FsError::Io(e.kind()))?;
        if u64::try_from(out.len()).unwrap_or(u64::MAX) > max {
            return Err(FsError::TooLarge);
        }
        Ok(out)
    }

    pub(super) fn measure_tree(&self, rel: &RelPath, budget: Duration) -> Result<Measure, FsError> {
        let start = self.walk(rel.components())?;
        let mut walker = Walker {
            deadline: Instant::now().checked_add(budget),
            dev: self.dev,
            visited: 0,
            hardlinks: HashSet::new(),
            total: Measure::default(),
        };
        walker.walk(&start, 0);
        Ok(walker.total)
    }

    pub(super) fn remove(&self, rel: &RelPath) -> Result<(), FsError> {
        let (parent, leaf) = self.parent_of(rel)?;
        let name = CString::new(leaf).map_err(|_| FsError::Refused)?;
        statat(&parent, name.as_c_str(), AtFlags::SYMLINK_NOFOLLOW).map_err(fs_error)?;
        remove_entry(&parent, &name, self.dev, rel.depth())?;
        fsync(&parent).map_err(fs_error)
    }
}

/// Remplit le fichier d'attente, le rend durable puis le met en place par renommage dans le MÊME dossier.
fn fill_and_place(
    fd: OwnedFd,
    parent: &OwnedFd,
    staging: &str,
    leaf: &str,
    bytes: &[u8],
    perms: Perms,
) -> Result<(), FsError> {
    // Propriétaire et droits AVANT la première donnée.
    apply(&fd, perms)?;
    let mut file = File::from(fd);
    file.write_all(bytes).map_err(|e| FsError::Io(e.kind()))?;
    file.sync_all().map_err(|e| FsError::Io(e.kind()))?;
    drop(file);
    renameat(parent, staging, parent, leaf).map_err(fs_error)?;
    fsync(parent).map_err(fs_error)
}

/// Parcours de mesure : somme de `st_blocks`, un fichier à plusieurs noms compté une fois.
struct Walker {
    deadline: Option<Instant>,
    dev: u128,
    visited: u64,
    hardlinks: HashSet<(u128, u128)>,
    total: Measure,
}

impl Walker {
    fn walk(&mut self, dir: &OwnedFd, depth: usize) {
        if depth >= MAX_DEPTH {
            self.total.partial = true;
            return;
        }
        let Ok(entries) = Dir::read_from(dir) else {
            self.total.partial = true;
            return;
        };
        for entry in entries {
            if self.total.partial {
                return;
            }
            let Ok(entry) = entry else {
                self.total.partial = true;
                return;
            };
            let name = entry.file_name();
            if name.to_bytes() == b"." || name.to_bytes() == b".." {
                continue;
            }
            if self.visited.is_multiple_of(CLOCK_EVERY)
                && self
                    .deadline
                    .is_some_and(|deadline| Instant::now() >= deadline)
            {
                self.total.partial = true;
                return;
            }
            self.visited += 1;
            self.visit(dir, name, depth);
        }
    }

    fn visit(&mut self, dir: &OwnedFd, name: &CStr, depth: usize) {
        let stat = match statat(dir, name, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(stat) => stat,
            // Disparu entre la lecture et le relevé : rien à compter.
            Err(Errno::NOENT) => return,
            Err(_) => {
                self.total.partial = true;
                return;
            }
        };
        if file_type(&stat) == FileType::Directory {
            // Jamais par un lien ; jamais dans un autre système de fichiers.
            let Ok(child) = openat(dir, name, OPEN_DIR, Mode::empty()) else {
                return;
            };
            match fstat(&child) {
                Ok(opened) if dev_of(&opened) == self.dev => {}
                _ => return,
            }
            self.count(&stat, false);
            self.walk(&child, depth + 1);
        } else {
            self.count(&stat, nlink_of(&stat) > 1);
        }
    }

    fn count(&mut self, stat: &Stat, multi_named: bool) {
        self.total.items += 1;
        if multi_named {
            let id = (dev_of(stat), u128::from(stat.st_ino));
            if self.hardlinks.contains(&id) {
                return;
            }
            if self.hardlinks.len() >= MAX_TRACKED_HARDLINKS {
                self.total.partial = true;
            } else {
                self.hardlinks.insert(id);
            }
        }
        self.total.bytes_on_disk = self.total.bytes_on_disk.saturating_add(disk_bytes(stat));
    }
}

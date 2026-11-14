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
use std::os::fd::{AsRawFd, OwnedFd};
use std::path::Path;
use std::time::{Duration, Instant};

use rustix::fs::{
    AtFlags, CWD, Dir, FileType, Mode, OFlags, Stat, fchmod, fstat, fsync, mkdirat, openat,
    readlinkat, renameat, statat, unlinkat,
};
use rustix::io::Errno;

use super::SERVICES_DIR;
use crate::application::ports::{
    DirListing, EntryKind, EntryMeta, Erased, FsError, ListedEntry, Measure, Perms, ReadFile,
};
use crate::domain::files::{MAX_DEPTH, RelPath};
use crate::domain::services::ServiceKey;
use crate::infrastructure::dir_fd::{self, CREATE_FILE, OPEN_DIR, is_trusted};

pub(super) mod erase;

use erase::Eraser;

/// Désigner un objet SANS l'ouvrir (`O_PATH`) : aucun effet d'ouverture sur un périphérique ou un tube,
/// un lien est désigné lui-même, jamais suivi. Son type se juge ensuite sur ce descripteur.
const DESIGNATE: OFlags = OFlags::PATH.union(OFlags::NOFOLLOW).union(OFlags::CLOEXEC);

/// Rouvrir en lecture un fichier ordinaire déjà désigné.
const REOPEN_READ: OFlags = OFlags::RDONLY.union(OFlags::NOCTTY).union(OFlags::CLOEXEC);

/// Fichiers à plusieurs noms retenus pour ne les compter qu'une fois ; au-delà, la mesure est partielle.
const MAX_TRACKED_HARDLINKS: usize = 100_000;

/// Le budget de temps est relu toutes les 1 024 entrées.
const CLOCK_EVERY: u64 = 1024;

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

/// Propriétaire puis droits, sur un descripteur : jamais un bit spécial (`Perms` les refuse).
fn apply(fd: &OwnedFd, perms: Perms) -> Result<(), FsError> {
    let owner = perms.owner();
    dir_fd::apply(fd, owner.uid(), owner.gid(), perms.mode()).map_err(fs_error)
}

/// Un dossier de confiance (`dir_fd::is_trusted`), sinon `UnsafeRoot`.
fn check_trusted(dir: &OwnedFd) -> Result<Stat, FsError> {
    let stat = fstat(dir).map_err(fs_error)?;
    if !is_trusted(&stat) {
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

/// `BR-DEL-007` : vide la racine du service PUIS la retire, sans sortir de `services/<clé>`. Continue après
/// une erreur et rend la première (`left > 0`) : rien n'est retiré par-dessus ce qui est resté.
pub(super) fn remove_service_root(data_dir: &Path, key: &ServiceKey) -> Result<(), FsError> {
    let name = check_key(key)?;
    let (services, dev) = open_services_dir(data_dir, false)?;
    let root = open_dir_on(&services, name, dev)?;
    check_trusted(&root)?;
    let mut quiet = |_: u64| true;
    let mut eraser = Eraser::new(dev, erase::mnt_id_of(&root), &mut quiet);
    eraser.contents(&root, 0);
    let report = eraser.finish();
    if report.left > 0 || report.first_error.is_some() {
        return Err(report.first_error.unwrap_or(FsError::Refused));
    }
    drop(root);
    unlinkat(&services, name, AtFlags::REMOVEDIR).map_err(fs_error)?;
    fsync(&services).map_err(fs_error)
}

/// `BR-DEL-007` : retire la racine du service si elle est vide, sans toucher à un contenu.
pub(super) fn remove_empty_service_root(
    data_dir: &Path,
    key: &ServiceKey,
) -> Result<bool, FsError> {
    let name = check_key(key)?;
    let (services, _) = open_services_dir(data_dir, false)?;
    match unlinkat(&services, name, AtFlags::REMOVEDIR) {
        Ok(()) => {
            fsync(&services).map_err(fs_error)?;
            Ok(true)
        }
        // Absente, ou encore pleine : rien n'est retiré.
        Err(Errno::NOENT | Errno::NOTEMPTY | Errno::EXIST) => Ok(false),
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

    /// Désigne puis ouvre en lecture un fichier ordinaire du service : jugé AVANT d'ouvrir (sur un périphérique,
    /// c'est l'ouverture elle-même qui agit), jamais par un lien, sur le périphérique de la racine, et c'est
    /// bien l'objet jugé qui est ouvert (même périphérique et même inode).
    fn open_regular(&self, rel: &RelPath) -> Result<OwnedFd, FsError> {
        let (parent, leaf) = self.parent_of(rel)?;
        // Juger AVANT d'ouvrir : sur un périphérique, c'est l'ouverture elle-même qui agit.
        let handle = openat(&parent, leaf, DESIGNATE, Mode::empty()).map_err(fs_error)?;
        let designated = fstat(&handle).map_err(fs_error)?;
        match file_type(&designated) {
            FileType::RegularFile => {}
            FileType::Symlink => return Err(FsError::Blocked),
            _ => return Err(FsError::NotRegular),
        }
        if dev_of(&designated) != self.dev {
            return Err(FsError::CrossesDevice);
        }
        let fd = reopen(&handle, REOPEN_READ)?;
        let opened = fstat(&fd).map_err(fs_error)?;
        if (opened.st_dev, opened.st_ino) != (designated.st_dev, designated.st_ino) {
            return Err(FsError::Blocked);
        }
        Ok(fd)
    }

    pub(super) fn read_file(&self, rel: &RelPath, max: u64) -> Result<Vec<u8>, FsError> {
        let fd = self.open_regular(rel)?;
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

    /// Ce que `lstat` dit de `rel` (la racine si `rel` est vide), sans suivre de lien.
    pub(super) fn stat_rel(&self, rel: &RelPath) -> Result<EntryMeta, FsError> {
        if rel.is_root() {
            let stat = fstat(&self.fd).map_err(fs_error)?;
            return Ok(meta_of(&stat, self.dev));
        }
        let (parent, leaf) = self.parent_of(rel)?;
        let stat = statat(&parent, leaf, AtFlags::SYMLINK_NOFOLLOW).map_err(fs_error)?;
        Ok(meta_of(&stat, self.dev))
    }

    /// Le contenu d'un dossier, trié par nom (HRT-97). Un nom qui n'est pas de l'UTF-8, ou pas un composant
    /// sûr, est compté, jamais listé.
    pub(super) fn list(&self, rel: &RelPath) -> Result<DirListing, FsError> {
        let dir = self.walk(rel.components())?;
        let reader = Dir::read_from(&dir).map_err(fs_error)?;
        let mut listing = DirListing::default();
        for entry in reader {
            let entry = entry.map_err(fs_error)?;
            let name = entry.file_name();
            if name.to_bytes() == b"." || name.to_bytes() == b".." {
                continue;
            }
            let Some(text) = name
                .to_str()
                .ok()
                .filter(|text| RelPath::check_component(text).is_ok())
            else {
                listing.skipped_names = listing.skipped_names.saturating_add(1);
                continue;
            };
            let stat = match statat(&dir, name, AtFlags::SYMLINK_NOFOLLOW) {
                Ok(stat) => stat,
                // Disparu entre la lecture et le relevé : rien à lister.
                Err(Errno::NOENT) => continue,
                Err(errno) => return Err(fs_error(errno)),
            };
            listing.entries.push(ListedEntry {
                name: text.to_owned(),
                meta: meta_of(&stat, self.dev),
            });
        }
        listing.entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(listing)
    }

    /// Ouvre un fichier ordinaire en lecture.
    pub(super) fn open_for_read(&self, rel: &RelPath) -> Result<Box<dyn ReadFile>, FsError> {
        let fd = self.open_regular(rel)?;
        Ok(Box::new(UnixFile {
            file: File::from(fd),
            dev: self.dev,
        }))
    }

    /// La cible d'un lien symbolique, sans la suivre.
    pub(super) fn link_target(&self, rel: &RelPath) -> Result<String, FsError> {
        let (parent, leaf) = self.parent_of(rel)?;
        let target = match readlinkat(&parent, leaf, Vec::new()) {
            Ok(target) => target,
            // Pas un lien symbolique.
            Err(Errno::INVAL) => return Err(FsError::NotRegular),
            Err(errno) => return Err(fs_error(errno)),
        };
        target
            .into_string()
            .map_err(|_| FsError::Io(io::ErrorKind::InvalidData))
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
        let mut quiet = |_: u64| true;
        let report = self.erase(rel, &mut quiet, false)?;
        if report.left > 0 || report.first_error.is_some() {
            return Err(report.first_error.unwrap_or(FsError::Refused));
        }
        Ok(())
    }

    /// Efface l'arbre `rel` (HRT-85) : voir [`erase::Eraser`]. `low_priority` : sur un fil dédié dont la
    /// priorité est abaissée (la suppression d'un service), sinon sur le fil appelant.
    pub(super) fn erase(
        &self,
        rel: &RelPath,
        progress: &mut (dyn FnMut(u64) -> bool + Send),
        low_priority: bool,
    ) -> Result<Erased, FsError> {
        let (parent, leaf) = self.parent_of(rel)?;
        let name = CString::new(leaf).map_err(|_| FsError::Refused)?;
        statat(&parent, name.as_c_str(), AtFlags::SYMLINK_NOFOLLOW).map_err(fs_error)?;
        let (dev, depth) = (self.dev, rel.depth());
        let mnt = erase::mnt_id_of(&self.fd);
        let work = move |parent: OwnedFd,
                         name: CString,
                         progress: &mut (dyn FnMut(u64) -> bool + Send)| {
            let mut eraser = Eraser::new(dev, mnt, progress);
            eraser.erase_entry(&parent, &name, depth);
            let report = eraser.finish();
            // Le retrait du nom doit survivre à une coupure : `fsync` du dossier parent.
            let _ = fsync(&parent);
            report
        };
        if !low_priority {
            return Ok(work(parent, name, progress));
        }
        std::thread::scope(|scope| {
            scope
                .spawn(move || {
                    erase::lower_priority();
                    work(parent, name, progress)
                })
                .join()
        })
        .map_err(|_| FsError::Io(io::ErrorKind::Other))
    }
}

/// Ce que `stat` dit d'un élément, traduit : un dossier d'un autre périphérique que la racine est un point de
/// montage (`Other`, jamais descendu).
fn meta_of(stat: &Stat, root_dev: u128) -> EntryMeta {
    let kind = match file_type(stat) {
        FileType::RegularFile => EntryKind::File,
        FileType::Directory if dev_of(stat) == root_dev => EntryKind::Dir,
        FileType::Symlink => EntryKind::Symlink,
        _ => EntryKind::Other,
    };
    let nanos = i128::from(stat.st_mtime) * 1_000_000_000 + i128::from(stat.st_mtime_nsec);
    let mode: u32 = stat.st_mode & 0o777;
    EntryMeta {
        kind,
        size: u64::try_from(i128::from(stat.st_size)).unwrap_or(0),
        mode,
        uid: stat.st_uid,
        gid: stat.st_gid,
        mtime_ns: i64::try_from(nanos).unwrap_or(if nanos < 0 { i64::MIN } else { i64::MAX }),
        nlink: u64::try_from(nlink_of(stat)).unwrap_or(u64::MAX),
    }
}

/// Un fichier ordinaire ouvert en lecture sous la racine du service.
struct UnixFile {
    file: File,
    dev: u128,
}

impl Read for UnixFile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.file.read(buf)
    }
}

impl ReadFile for UnixFile {
    fn meta(&self) -> Result<EntryMeta, FsError> {
        let stat = fstat(&self.file).map_err(fs_error)?;
        Ok(meta_of(&stat, self.dev))
    }
}

/// Rouvre l'objet désigné par `handle` (`O_PATH`) par `/proc/self/fd/<n>` : ce lien du noyau mène au
/// MÊME objet que le descripteur, aucun nom du dossier du service n'est relu entre le jugement et
/// l'ouverture.
fn reopen(handle: &OwnedFd, flags: OFlags) -> Result<OwnedFd, FsError> {
    let link = format!("/proc/self/fd/{}", handle.as_raw_fd());
    openat(CWD, link.as_str(), flags, Mode::empty())
        .map_err(|errno| FsError::Io(io::Error::from(errno).kind()))
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
            let child = match openat(dir, name, OPEN_DIR, Mode::empty()) {
                Ok(child) => child,
                // Disparu ou remplacé par un lien entre le relevé et l'ouverture : rien à compter.
                Err(Errno::NOENT | Errno::LOOP | Errno::NOTDIR) => return,
                Err(_) => {
                    self.total.partial = true;
                    return;
                }
            };
            match fstat(&child) {
                Ok(opened) if dev_of(&opened) == self.dev => {}
                // Un point de montage : hors de la mesure, jamais descendu.
                Ok(_) => return,
                Err(_) => {
                    self.total.partial = true;
                    return;
                }
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

//! `WorldFs` sous Unix (HRT-105, ADR-0059). Voir `../world_fs.rs` pour le contrat.
//!
//! Règle de ce fichier : une fois un dossier ouvert, aucun appel ne prend un chemin composé. Tout est `openat`,
//! `statat`, `mkdirat`, `renameat`, `fstat`, `fsync` relatif à un descripteur, par NOMS SIMPLES. Aucune fonction
//! de ce fichier n'écrit dans la source.

use std::ffi::CStr;
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, OwnedFd};
use std::path::Path;

use rustix::fs::{
    AtFlags, CWD, Dir, FileType, Mode, OFlags, Stat, fstat, fsync, mkdirat, openat, renameat,
    statat, unlinkat,
};
use rustix::io::Errno;

use crate::application::ports::{
    ImportDir, Ownership, SinkFile, WorldDirEntry, WorldDirListing, WorldEntryKind, WorldFileStat,
    WorldFsError, WorldSink, WorldSource,
};
use crate::domain::files::RelPath;
use crate::domain::services::ServiceKey;
use crate::infrastructure::dir_fd::{self, CREATE_FILE, OPEN_DIR, is_trusted};
use crate::infrastructure::fs::SERVICES_DIR;

use super::STAGING_DIR;

/// Désigner un objet SANS l'ouvrir (`O_PATH`) : aucun effet d'ouverture sur un tube ou un périphérique.
const DESIGNATE: OFlags = OFlags::PATH.union(OFlags::NOFOLLOW).union(OFlags::CLOEXEC);
/// Rouvrir en lecture un fichier ordinaire déjà désigné.
const REOPEN_READ: OFlags = OFlags::RDONLY.union(OFlags::NOCTTY).union(OFlags::CLOEXEC);
/// Ouvrir en lecture un fichier de la copie, sans suivre de lien.
const OPEN_COPY: OFlags = OFlags::RDONLY
    .union(OFlags::NOFOLLOW)
    .union(OFlags::NOCTTY)
    .union(OFlags::CLOEXEC);

/// Droits des dossiers et des fichiers de la copie : le propriétaire de la fiche écrit, les autres lisent.
const DIR_MODE: u32 = 0o755;
const FILE_MODE: u32 = 0o644;

fn werr(errno: Errno) -> WorldFsError {
    match errno {
        Errno::NOENT => WorldFsError::NotFound,
        // Un lien (`O_NOFOLLOW`) ou un fichier là où un dossier était attendu (`O_DIRECTORY`).
        Errno::LOOP | Errno::NOTDIR => WorldFsError::Blocked,
        Errno::EXIST => WorldFsError::Exists,
        other => WorldFsError::Io(io::Error::from(other).kind()),
    }
}

fn dev_of(stat: &Stat) -> u128 {
    u128::from(stat.st_dev)
}

fn kind_of(stat: &Stat) -> WorldEntryKind {
    match FileType::from_raw_mode(stat.st_mode) {
        FileType::RegularFile => WorldEntryKind::File,
        FileType::Directory => WorldEntryKind::Dir,
        FileType::Symlink => WorldEntryKind::Link,
        _ => WorldEntryKind::Other,
    }
}

fn mtime_ns(stat: &Stat) -> i128 {
    i128::from(stat.st_mtime) * 1_000_000_000 + i128::from(stat.st_mtime_nsec)
}

/// Ouvre `parent/name` comme dossier, sans suivre de lien, sur le périphérique `dev`.
fn open_dir_on(parent: &OwnedFd, name: &str, dev: u128) -> Result<OwnedFd, WorldFsError> {
    let fd = openat(parent, name, OPEN_DIR, Mode::empty()).map_err(werr)?;
    if dev_of(&fstat(&fd).map_err(werr)?) != dev {
        return Err(WorldFsError::CrossesDevice);
    }
    Ok(fd)
}

fn open_data_dir(data_dir: &Path) -> Result<(OwnedFd, u128), WorldFsError> {
    let data = openat(CWD, data_dir, OPEN_DIR, Mode::empty()).map_err(werr)?;
    let stat = fstat(&data).map_err(werr)?;
    if !is_trusted(&stat) {
        return Err(WorldFsError::UnsafeRoot);
    }
    Ok((data, dev_of(&stat)))
}

/// Un dossier de l'agent, à lui seul : sinon un autre compte pourrait y substituer un lien.
fn check_trusted(dir: &OwnedFd) -> Result<(), WorldFsError> {
    let stat = fstat(dir).map_err(werr)?;
    if is_trusted(&stat) {
        Ok(())
    } else {
        Err(WorldFsError::UnsafeRoot)
    }
}

fn identity(stat: &Stat) -> (u64, u64) {
    (
        u64::try_from(dev_of(stat)).unwrap_or(0),
        u64::try_from(u128::from(stat.st_ino)).unwrap_or(0),
    )
}

// ---------------------------------------------------------------------------------------------------
// La source : lecture seule
// ---------------------------------------------------------------------------------------------------

pub(super) struct UnixSource {
    fd: OwnedFd,
    dev: u128,
}

impl UnixSource {
    /// Ouvre le dossier ouvert aux services, tel qu'il est écrit dans la configuration de l'agent : chemin
    /// absolu, ouvert en lecture seule (un lien posé par l'administrateur sur le dossier lui-même est suivi : il
    /// l'a écrit).
    pub(super) fn open(root: &Path) -> Result<Self, WorldFsError> {
        if !root.is_absolute() {
            return Err(WorldFsError::UnsafeRoot);
        }
        let fd = openat(
            CWD,
            root,
            OFlags::RDONLY
                .union(OFlags::DIRECTORY)
                .union(OFlags::CLOEXEC),
            Mode::empty(),
        )
        .map_err(werr)?;
        let dev = dev_of(&fstat(&fd).map_err(werr)?);
        Ok(Self { fd, dev })
    }

    fn walk<'a>(&self, names: impl IntoIterator<Item = &'a str>) -> Result<OwnedFd, WorldFsError> {
        let mut current = self
            .fd
            .try_clone()
            .map_err(|e| WorldFsError::Io(e.kind()))?;
        for name in names {
            current = open_dir_on(&current, name, self.dev)?;
        }
        Ok(current)
    }

    fn parent_of<'a>(&self, rel: &'a RelPath) -> Result<(OwnedFd, &'a str), WorldFsError> {
        let (leaf, parents) = rel.split_last().ok_or(WorldFsError::NotRegular)?;
        let parent = self.walk(parents.iter().map(String::as_str))?;
        Ok((parent, leaf))
    }
}

impl WorldSource for UnixSource {
    fn list(&self, dir: &RelPath, limit: usize) -> Result<WorldDirListing, WorldFsError> {
        let dirfd = self.walk(dir.components())?;
        let entries = Dir::read_from(&dirfd).map_err(werr)?;
        let mut listing = WorldDirListing::default();
        for entry in entries {
            let entry = entry.map_err(werr)?;
            let name: &CStr = entry.file_name();
            if name.to_bytes() == b"." || name.to_bytes() == b".." {
                continue;
            }
            // Un nom qui n'est pas de l'UTF-8 ne peut pas être désigné : il n'est pas listé.
            let Ok(text) = std::str::from_utf8(name.to_bytes()) else {
                continue;
            };
            if listing.entries.len() >= limit {
                listing.truncated = true;
                break;
            }
            let stat = match statat(&dirfd, name, AtFlags::SYMLINK_NOFOLLOW) {
                Ok(stat) => stat,
                // Disparu entre la lecture et le relevé.
                Err(Errno::NOENT) => continue,
                Err(errno) => return Err(werr(errno)),
            };
            listing.entries.push(WorldDirEntry {
                name: text.to_owned(),
                kind: kind_of(&stat),
                size: u64::try_from(stat.st_size).unwrap_or(0),
                mtime_ns: mtime_ns(&stat),
            });
        }
        Ok(listing)
    }

    fn stat(&self, file: &RelPath) -> Result<WorldFileStat, WorldFsError> {
        let (parent, leaf) = self.parent_of(file)?;
        let stat = statat(&parent, leaf, AtFlags::SYMLINK_NOFOLLOW).map_err(werr)?;
        if kind_of(&stat) != WorldEntryKind::File {
            return Err(WorldFsError::NotRegular);
        }
        Ok(WorldFileStat {
            size: u64::try_from(stat.st_size).unwrap_or(0),
            mtime_ns: mtime_ns(&stat),
        })
    }

    fn open(&self, file: &RelPath) -> Result<Box<dyn Read + Send>, WorldFsError> {
        let (parent, leaf) = self.parent_of(file)?;
        // Juger AVANT d'ouvrir : sur un tube ou un périphérique, c'est l'ouverture elle-même qui agit.
        let handle = openat(&parent, leaf, DESIGNATE, Mode::empty()).map_err(werr)?;
        let designated = fstat(&handle).map_err(werr)?;
        if kind_of(&designated) != WorldEntryKind::File {
            return Err(WorldFsError::NotRegular);
        }
        if dev_of(&designated) != self.dev {
            return Err(WorldFsError::CrossesDevice);
        }
        let link = format!("/proc/self/fd/{}", handle.as_raw_fd());
        let fd = openat(CWD, link.as_str(), REOPEN_READ, Mode::empty())
            .map_err(|errno| WorldFsError::Io(io::Error::from(errno).kind()))?;
        let opened = fstat(&fd).map_err(werr)?;
        if (opened.st_dev, opened.st_ino) != (designated.st_dev, designated.st_ino) {
            return Err(WorldFsError::Blocked);
        }
        Ok(Box::new(File::from(fd)))
    }
}

// ---------------------------------------------------------------------------------------------------
// La destination : le dossier d'import
// ---------------------------------------------------------------------------------------------------

pub(super) struct UnixSink {
    fd: OwnedFd,
    dev: u128,
    owner: Ownership,
}

struct CopyFile(File);

impl Write for CopyFile {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.0.write(data)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl SinkFile for CopyFile {
    fn finish(self: Box<Self>) -> io::Result<()> {
        self.0.sync_all()
    }
}

fn check_op(op: &str) -> Result<(), WorldFsError> {
    let plain = !op.is_empty()
        && op.len() <= 64
        && op
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if plain {
        Ok(())
    } else {
        Err(WorldFsError::UnsafeRoot)
    }
}

impl UnixSink {
    /// Crée `<données>/staging/import-<op>` (le dossier d'attente est à l'agent seul, `0700`).
    pub(super) fn create(
        data_dir: &Path,
        op: &str,
        owner: Ownership,
    ) -> Result<(Self, ImportDir), WorldFsError> {
        check_op(op)?;
        let (data, dev) = open_data_dir(data_dir)?;
        match mkdirat(&data, STAGING_DIR, Mode::RWXU) {
            Ok(()) | Err(Errno::EXIST) => {}
            Err(errno) => return Err(werr(errno)),
        }
        let staging = open_dir_on(&data, STAGING_DIR, dev)?;
        check_trusted(&staging)?;
        let name = format!("import-{op}");
        mkdirat(&staging, name.as_str(), Mode::RWXU).map_err(werr)?;
        // Un échec entre la création et le retour laisserait un dossier que le journal ne nomme pas : retiré ici.
        let made = (|| {
            let fd = open_dir_on(&staging, &name, dev)?;
            let (dev_id, ino) = identity(&fstat(&fd).map_err(werr)?);
            dir_fd::apply(&fd, owner.uid(), owner.gid(), DIR_MODE).map_err(werr)?;
            Ok::<_, WorldFsError>((fd, dev_id, ino))
        })();
        let (fd, dev_id, ino) = match made {
            Ok(made) => made,
            Err(error) => {
                let _ = unlinkat(&staging, name.as_str(), AtFlags::REMOVEDIR);
                return Err(error);
            }
        };
        Ok((
            Self { fd, dev, owner },
            ImportDir {
                rel: name,
                dev: dev_id,
                ino,
            },
        ))
    }

    fn parent_of<'a>(&self, rel: &'a RelPath) -> Result<(OwnedFd, &'a str), WorldFsError> {
        let (leaf, parents) = rel.split_last().ok_or(WorldFsError::Blocked)?;
        let mut current = self
            .fd
            .try_clone()
            .map_err(|e| WorldFsError::Io(e.kind()))?;
        for name in parents {
            current = open_dir_on(&current, name, self.dev)?;
        }
        Ok((current, leaf))
    }
}

impl WorldSink for UnixSink {
    fn mkdir(&self, dir: &RelPath) -> Result<(), WorldFsError> {
        let (parent, leaf) = self.parent_of(dir)?;
        mkdirat(&parent, leaf, Mode::RWXU).map_err(werr)?;
        let child = open_dir_on(&parent, leaf, self.dev)?;
        dir_fd::apply(&child, self.owner.uid(), self.owner.gid(), DIR_MODE).map_err(werr)
    }

    fn create(&self, file: &RelPath) -> Result<Box<dyn SinkFile>, WorldFsError> {
        let (parent, leaf) = self.parent_of(file)?;
        let fd = openat(&parent, leaf, CREATE_FILE, Mode::RUSR | Mode::WUSR).map_err(werr)?;
        // Propriétaire et droits de la fiche, posés AVANT la première donnée.
        dir_fd::apply(&fd, self.owner.uid(), self.owner.gid(), FILE_MODE).map_err(werr)?;
        Ok(Box::new(CopyFile(File::from(fd))))
    }

    fn open(&self, file: &RelPath) -> Result<Box<dyn Read + Send>, WorldFsError> {
        let (parent, leaf) = self.parent_of(file)?;
        let fd = openat(&parent, leaf, OPEN_COPY, Mode::empty()).map_err(werr)?;
        if kind_of(&fstat(&fd).map_err(werr)?) != WorldEntryKind::File {
            return Err(WorldFsError::NotRegular);
        }
        Ok(Box::new(File::from(fd)))
    }
}

/// Le dossier d'import est-il toujours celui que l'opération a créé ? (périphérique et inode du journal)
fn is_ours(staging: &OwnedFd, import: &ImportDir) -> Result<bool, WorldFsError> {
    match statat(staging, import.rel.as_str(), AtFlags::SYMLINK_NOFOLLOW) {
        Ok(stat) => Ok(
            kind_of(&stat) == WorldEntryKind::Dir && identity(&stat) == (import.dev, import.ino)
        ),
        Err(Errno::NOENT) => Err(WorldFsError::NotFound),
        Err(errno) => Err(werr(errno)),
    }
}

/// Renomme le dossier d'import en `services/<clé>/<dossier>`, qui doit exister et être vide.
pub(super) fn place(
    data_dir: &Path,
    import: &ImportDir,
    key: &ServiceKey,
    folder: &str,
) -> Result<(), WorldFsError> {
    RelPath::check_component(&import.rel).map_err(|_| WorldFsError::Blocked)?;
    RelPath::check_component(folder).map_err(|_| WorldFsError::Blocked)?;
    let (data, dev) = open_data_dir(data_dir)?;
    let staging = open_dir_on(&data, STAGING_DIR, dev)?;
    if !is_ours(&staging, import)? {
        return Err(WorldFsError::Blocked);
    }
    let services = open_dir_on(&data, SERVICES_DIR, dev)?;
    check_trusted(&services)?;
    let service = open_dir_on(&services, key.as_str(), dev)?;
    check_trusted(&service)?;
    let target = open_dir_on(&service, folder, dev)?;
    for entry in Dir::read_from(&target).map_err(werr)? {
        let entry = entry.map_err(werr)?;
        let name = entry.file_name().to_bytes();
        if name != b"." && name != b".." {
            // Jamais par-dessus un contenu.
            return Err(WorldFsError::Exists);
        }
    }
    drop(target);
    // Les entrées du dossier d'import sont rendues durables avant qu'il ne prenne sa place.
    let imported = open_dir_on(&staging, import.rel.as_str(), dev)?;
    fsync(&imported).map_err(werr)?;
    drop(imported);
    renameat(&staging, import.rel.as_str(), &service, folder).map_err(werr)?;
    fsync(&service).map_err(werr)
}

/// Retire le dossier d'import SI c'est toujours lui. Rien d'autre.
pub(super) fn remove_import(data_dir: &Path, import: &ImportDir) -> Result<(), WorldFsError> {
    RelPath::check_component(&import.rel).map_err(|_| WorldFsError::Blocked)?;
    let (data, dev) = open_data_dir(data_dir)?;
    let staging = match open_dir_on(&data, STAGING_DIR, dev) {
        Ok(staging) => staging,
        // Pas de dossier d'attente : rien à retirer.
        Err(WorldFsError::NotFound) => return Ok(()),
        Err(error) => return Err(error),
    };
    match is_ours(&staging, import) {
        Ok(true) => {}
        // Déjà parti (mis en place ou retiré) : rien à faire.
        Err(WorldFsError::NotFound) => return Ok(()),
        Ok(false) => return Err(WorldFsError::Blocked),
        Err(error) => return Err(error),
    }
    // `remove_dir_all` ne suit pas les liens : un lien dans le dossier est retiré comme lien.
    std::fs::remove_dir_all(data_dir.join(STAGING_DIR).join(&import.rel))
        .map_err(|e| WorldFsError::Io(e.kind()))
}

/// La copie tourne sur un fil dédié : son « nice » est abaissé, pas celui des fils de l'agent.
pub(super) fn lower_priority() {
    let _ = rustix::process::setpriority_process(None, 19);
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
    use std::time::{Duration, Instant};

    use rustix::process::{getegid, geteuid};

    use super::*;
    use crate::application::world_import::copy::{Control, copy_world};
    use crate::application::world_import::search::{find_worlds, plan_world};
    use crate::domain::worlds::{Shape, WorldRules};
    use crate::infrastructure::data_dir::private_tempdir;

    fn me() -> Ownership {
        Ownership::new(geteuid().as_raw(), getegid().as_raw()).expect("identité")
    }

    fn rules() -> WorldRules {
        WorldRules {
            shape: Shape::File {
                suffix: ".wld".into(),
                companions: vec![],
            },
            names_setting: "world_name".into(),
            carried: vec![],
            folder_source: "w".into(),
        }
    }

    fn rel(text: &str) -> RelPath {
        RelPath::parse(text).expect("chemin")
    }

    /// Tout ce qu'un dossier montre : noms, genres, tailles, droits, dates, contenus.
    fn fingerprint(root: &Path) -> Vec<String> {
        fn walk(dir: &Path, base: &Path, out: &mut Vec<String>) {
            let mut names: Vec<_> = std::fs::read_dir(dir)
                .expect("lecture")
                .map(|e| e.expect("entrée").path())
                .collect();
            names.sort();
            for path in names {
                let meta = std::fs::symlink_metadata(&path).expect("méta");
                let shown = path
                    .strip_prefix(base)
                    .expect("préfixe")
                    .display()
                    .to_string();
                let content = if meta.is_file() {
                    std::fs::read(&path).map(|b| b.len()).unwrap_or(0)
                } else {
                    0
                };
                out.push(format!(
                    "{shown} mode={:o} mtime={} nsec={} len={content}",
                    meta.mode(),
                    meta.mtime(),
                    meta.mtime_nsec()
                ));
                if meta.is_dir() {
                    walk(&path, base, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(root, root, &mut out);
        out
    }

    #[test]
    fn the_listing_shows_links_as_links_and_never_follows_them() {
        let dir = private_tempdir();
        let world = dir.path().join("jeux");
        std::fs::create_dir_all(world.join("Worlds")).expect("dossier");
        std::fs::write(world.join("Worlds/A.wld"), b"a").expect("fichier");
        symlink("/etc", world.join("Worlds/dehors")).expect("lien");
        symlink(world.join("Worlds/A.wld"), world.join("Worlds/lien.wld")).expect("lien");
        let source = UnixSource::open(&world).expect("source");
        let listing = source.list(&rel("Worlds"), 100).expect("liste");
        let kinds: Vec<(String, WorldEntryKind)> = {
            let mut v: Vec<_> = listing
                .entries
                .iter()
                .map(|e| (e.name.clone(), e.kind))
                .collect();
            v.sort_by(|a, b| a.0.cmp(&b.0));
            v
        };
        assert_eq!(
            kinds,
            vec![
                ("A.wld".to_owned(), WorldEntryKind::File),
                ("dehors".to_owned(), WorldEntryKind::Link),
                ("lien.wld".to_owned(), WorldEntryKind::Link),
            ]
        );
        assert!(
            source.list(&rel("Worlds/dehors"), 10).is_err(),
            "un lien n'est pas un dossier"
        );
        assert_eq!(
            source.open(&rel("Worlds/lien.wld")).err(),
            Some(WorldFsError::NotRegular),
            "un lien n'est jamais ouvert"
        );
        assert_eq!(
            source.stat(&rel("Worlds/lien.wld")).err(),
            Some(WorldFsError::NotRegular)
        );
        let limited = source.list(&rel("Worlds"), 2).expect("liste");
        assert!(limited.truncated && limited.entries.len() == 2);
    }

    #[test]
    fn a_relative_root_is_refused() {
        assert_eq!(
            UnixSource::open(Path::new("jeux")).err(),
            Some(WorldFsError::UnsafeRoot)
        );
    }

    /// Le cas d'ensemble : source en LECTURE SEULE, copie vérifiée, mise en place ; la source est identique.
    #[test]
    fn a_read_only_source_is_copied_verified_and_placed_without_a_change_to_the_source() {
        let dir = private_tempdir();
        let data = dir.path().join("data");
        std::fs::create_dir_all(data.join("services/h_01ARZ3NDEKTSV4RRFFQ69G5FAV/w"))
            .expect("service");
        std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o700)).expect("droits");
        std::fs::set_permissions(
            data.join("services"),
            std::fs::Permissions::from_mode(0o700),
        )
        .expect("droits");
        std::fs::set_permissions(
            data.join("services/h_01ARZ3NDEKTSV4RRFFQ69G5FAV"),
            std::fs::Permissions::from_mode(0o700),
        )
        .expect("droits");
        let world = dir.path().join("jeux");
        std::fs::create_dir_all(world.join("Worlds")).expect("dossier");
        let payload: Vec<u8> = (0..700_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(world.join("Worlds/Alpha.wld"), &payload).expect("monde");
        std::fs::write(world.join("Worlds/Alpha.wld.bak"), b"vieux").expect("bak");
        symlink("/etc/hostname", world.join("Worlds/Lien.wld")).expect("lien");
        std::fs::set_permissions(world.join("Worlds"), std::fs::Permissions::from_mode(0o555))
            .expect("lecture seule");
        std::fs::set_permissions(&world, std::fs::Permissions::from_mode(0o555))
            .expect("lecture seule");
        let before = fingerprint(&world);

        let fs = super::super::SystemWorldFs::new(data.clone());
        let source = UnixSource::open(&world).expect("source");
        let found = find_worlds(&source, &rules(), 5_000).expect("trouvés");
        assert_eq!(
            found
                .worlds
                .iter()
                .map(|w| w.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Alpha"]
        );
        let plan = plan_world(
            &source,
            &rules(),
            &rel("Worlds/Alpha.wld"),
            Duration::from_secs(60),
        )
        .expect("plan");
        assert_eq!(plan.bytes, 700_000);

        let (sink, import) =
            crate::application::ports::WorldFs::create_import(&fs, "01TEST", me()).expect("import");
        let report = copy_world(
            &source,
            sink.as_ref(),
            &plan,
            &Control::default(),
            Instant::now() + Duration::from_secs(60),
        )
        .expect("copie");
        assert_eq!(
            (report.files, report.bytes, report.changed),
            (1, 700_000, 0)
        );
        crate::application::ports::WorldFs::place(
            &fs,
            &import,
            &ServiceKey::parse("h_01ARZ3NDEKTSV4RRFFQ69G5FAV").expect("clé"),
            "w",
        )
        .expect("mise en place");

        let placed = std::fs::read(data.join("services/h_01ARZ3NDEKTSV4RRFFQ69G5FAV/w/Alpha.wld"))
            .expect("copie");
        assert_eq!(placed, payload);
        assert!(!data.join("staging/import-01TEST").exists());
        assert_eq!(fingerprint(&world), before, "la source est identique");
        // Remettre les droits pour que le dossier temporaire se nettoie.
        std::fs::set_permissions(&world, std::fs::Permissions::from_mode(0o755)).expect("droits");
        std::fs::set_permissions(world.join("Worlds"), std::fs::Permissions::from_mode(0o755))
            .expect("droits");
    }

    #[test]
    fn only_the_import_directory_of_the_operation_is_removed() {
        let dir = private_tempdir();
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).expect("données");
        std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o700)).expect("droits");
        let fs = super::super::SystemWorldFs::new(data.clone());
        let (_sink, import) =
            crate::application::ports::WorldFs::create_import(&fs, "01ABC", me()).expect("import");
        assert!(data.join("staging/import-01ABC").is_dir());
        // Un second import du même nom n'est jamais écrasé.
        assert!(matches!(
            crate::application::ports::WorldFs::create_import(&fs, "01ABC", me()),
            Err(WorldFsError::Exists)
        ));
        // Une identité qui n'est pas la sienne : rien n'est retiré.
        let stranger = ImportDir {
            ino: import.ino + 1,
            ..import.clone()
        };
        assert_eq!(
            crate::application::ports::WorldFs::remove_import(&fs, &stranger),
            Err(WorldFsError::Blocked)
        );
        assert!(data.join("staging/import-01ABC").is_dir());
        crate::application::ports::WorldFs::remove_import(&fs, &import).expect("retrait");
        assert!(!data.join("staging/import-01ABC").exists());
        // Déjà parti : pas une erreur.
        crate::application::ports::WorldFs::remove_import(&fs, &import).expect("déjà parti");
    }

    #[test]
    fn a_non_empty_target_folder_is_never_replaced() {
        let dir = private_tempdir();
        let data = dir.path().join("data");
        std::fs::create_dir_all(data.join("services/h_01ARZ3NDEKTSV4RRFFQ69G5FAV/w"))
            .expect("service");
        for part in ["", "services", "services/h_01ARZ3NDEKTSV4RRFFQ69G5FAV"] {
            std::fs::set_permissions(data.join(part), std::fs::Permissions::from_mode(0o700))
                .expect("droits");
        }
        std::fs::write(
            data.join("services/h_01ARZ3NDEKTSV4RRFFQ69G5FAV/w/deja"),
            b"x",
        )
        .expect("contenu");
        let fs = super::super::SystemWorldFs::new(data.clone());
        let (_sink, import) =
            crate::application::ports::WorldFs::create_import(&fs, "01XYZ", me()).expect("import");
        let outcome = crate::application::ports::WorldFs::place(
            &fs,
            &import,
            &ServiceKey::parse("h_01ARZ3NDEKTSV4RRFFQ69G5FAV").expect("clé"),
            "w",
        );
        assert_eq!(outcome, Err(WorldFsError::Exists));
        assert!(
            data.join("services/h_01ARZ3NDEKTSV4RRFFQ69G5FAV/w/deja")
                .exists()
        );
    }
}

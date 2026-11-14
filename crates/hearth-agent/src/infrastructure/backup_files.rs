//! La racine des sauvegardes sur le disque (HRT-97, ADR-0047, BR-BACKUP-004, BR-BACKUP-005).
//!
//! Écrit avec `std::fs` : cette racine n'a PAS de contenu hostile (c'est celle de Hearth, `0700`, à l'agent), à
//! la différence d'un dossier de données de service (`SafeFs`). Les garde-fous sont ceux de la règle absolue :
//!
//! - **aucun listage** : chaque geste vise un nom dérivé d'un identifiant de sauvegarde, dans le sous-dossier d'un
//!   service ; la racine n'est jamais parcourue ;
//! - la racine et chaque sous-dossier sont jugés AVANT usage : un vrai dossier (jamais un lien), à l'utilisateur
//!   de l'agent, ni le groupe ni les autres n'y écrivent ;
//! - une création est EXCLUSIVE (jamais par-dessus un fichier, jamais par un lien) ; une ouverture ne suit jamais
//!   un lien ni ne bloque sur un tube ; une suppression revérifie l'inode juste avant (`Removed::NotOurs`
//!   sinon : rien n'est touché) ;
//! - rien n'est renommé ni supprimé qui ne soit celui du registre : l'archive publiée se reconnaît à son inode,
//!   sa taille ET sa date de modification (un numéro d'inode libéré est vite réattribué) ;
//! - la publication est un renommage qui ne remplace JAMAIS (`RENAME_NOREPLACE`, atomique) : un fichier posé à la
//!   main sous le nom définitif entre-temps n'est pas écrasé.
//!
//! Sans Unix (poste de développement Windows), il n'y a pas d'inode : tout geste répond `Unsupported`, comme
//! `SafeFs`. Le code d'inode et de droits est Linux ; il compile partout, il ne s'exécute pas sous Windows.

use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use crate::application::ports::{
    BackupFiles, BackupFsError, BackupRemoved, BackupSink, DiskSpace, Expected, FileIdent,
    FileStamp,
};
use crate::domain::backups::ownership::Seen;
use crate::domain::files::RelPath;

pub struct StdBackupFiles {
    root: PathBuf,
}

impl StdBackupFiles {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

fn io_error(error: &io::Error) -> BackupFsError {
    match error.kind() {
        io::ErrorKind::NotFound => BackupFsError::NotFound,
        io::ErrorKind::AlreadyExists => BackupFsError::Exists,
        other => BackupFsError::Io(other),
    }
}

/// Un nom d'un seul composant sûr : jamais `..`, jamais un séparateur.
fn component(name: &str) -> Result<&str, BackupFsError> {
    RelPath::check_component(name).map_err(|_| BackupFsError::Unsafe)?;
    Ok(name)
}

#[cfg(unix)]
mod sys {
    use super::*;
    use std::fs::{self, File, OpenOptions};
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};

    pub fn ident(meta: &fs::Metadata) -> FileIdent {
        (meta.dev(), meta.ino())
    }

    /// L'identité complète : inode, taille, date de modification à la nanoseconde.
    pub fn stamp(meta: &fs::Metadata) -> FileStamp {
        FileStamp {
            dev: meta.dev(),
            ino: meta.ino(),
            size: meta.len(),
            mtime_ns: meta
                .mtime()
                .saturating_mul(1_000_000_000)
                .saturating_add(meta.mtime_nsec()),
        }
    }

    /// Renomme sans jamais remplacer : `renameat2(RENAME_NOREPLACE)`. Un système de fichiers ou un noyau qui ne le
    /// connaît pas (`EINVAL`, `ENOSYS`) : lien dur vers le nom définitif (qui refuse d'exister déjà), puis retrait
    /// de l'ancien nom.
    pub fn rename_noreplace(from: &Path, to: &Path) -> Result<(), BackupFsError> {
        use rustix::fs::{CWD, RenameFlags, renameat_with};
        use rustix::io::Errno;
        match renameat_with(CWD, from, CWD, to, RenameFlags::NOREPLACE) {
            Ok(()) => Ok(()),
            Err(Errno::EXIST) => Err(BackupFsError::Exists),
            Err(Errno::INVAL | Errno::NOSYS) => match fs::hard_link(from, to) {
                Ok(()) => fs::remove_file(from).map_err(|e| io_error(&e)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    Err(BackupFsError::Exists)
                }
                Err(error) => Err(io_error(&error)),
            },
            Err(error) => Err(io_error(&io::Error::from(error))),
        }
    }

    /// Un dossier de confiance : un vrai dossier, à l'utilisateur effectif de l'agent, que ni le groupe ni les
    /// autres ne modifient.
    pub fn trusted_dir(path: &Path) -> Result<(), BackupFsError> {
        let meta = fs::symlink_metadata(path).map_err(|e| io_error(&e))?;
        if !meta.is_dir() || meta.file_type().is_symlink() {
            return Err(BackupFsError::Unsafe);
        }
        if meta.uid() != rustix::process::geteuid().as_raw() || meta.mode() & 0o022 != 0 {
            return Err(BackupFsError::Unsafe);
        }
        Ok(())
    }

    /// Crée un dossier `0700` (et ses parents manquants).
    pub fn make_private_dirs(path: &Path) -> Result<(), BackupFsError> {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)
            .map_err(|e| io_error(&e))
    }

    /// Crée un dossier `0700`, un seul niveau ; un dossier déjà là est gardé.
    pub fn make_private_dir(path: &Path) -> Result<(), BackupFsError> {
        match fs::DirBuilder::new().mode(0o700).create(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
            Err(error) => Err(io_error(&error)),
        }
    }

    /// Crée un fichier neuf `0600` : exclusif, jamais par un lien.
    pub fn create_new(path: &Path) -> Result<File, BackupFsError> {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .map_err(|e| io_error(&e))
    }

    /// Ouvre en lecture un fichier ordinaire : jamais par un lien, sans bloquer sur un tube.
    pub fn open_regular(path: &Path) -> Result<File, BackupFsError> {
        let before = fs::symlink_metadata(path).map_err(|e| io_error(&e))?;
        if !before.is_file() {
            return Err(BackupFsError::Unsafe);
        }
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
            .map_err(|e| io_error(&e))?;
        let meta = file.metadata().map_err(|e| io_error(&e))?;
        if !meta.is_file() {
            return Err(BackupFsError::Unsafe);
        }
        Ok(file)
    }

    pub fn space(path: &Path) -> Result<DiskSpace, BackupFsError> {
        let stat = rustix::fs::statvfs(path).map_err(|e| io_error(&io::Error::from(e)))?;
        let unit = stat.f_frsize.max(1);
        Ok(DiskSpace {
            free: stat.f_bavail.saturating_mul(unit),
            total: stat.f_blocks.saturating_mul(unit),
        })
    }
}

#[cfg(not(unix))]
mod sys {
    use super::*;
    use std::fs::{self, File};

    pub fn ident(_meta: &fs::Metadata) -> FileIdent {
        (0, 0)
    }

    pub fn stamp(_meta: &fs::Metadata) -> FileStamp {
        FileStamp {
            dev: 0,
            ino: 0,
            size: 0,
            mtime_ns: 0,
        }
    }

    pub fn rename_noreplace(_from: &Path, _to: &Path) -> Result<(), BackupFsError> {
        Err(BackupFsError::Unsupported)
    }

    pub fn trusted_dir(_path: &Path) -> Result<(), BackupFsError> {
        Err(BackupFsError::Unsupported)
    }

    pub fn make_private_dirs(_path: &Path) -> Result<(), BackupFsError> {
        Err(BackupFsError::Unsupported)
    }

    pub fn make_private_dir(_path: &Path) -> Result<(), BackupFsError> {
        Err(BackupFsError::Unsupported)
    }

    pub fn create_new(_path: &Path) -> Result<File, BackupFsError> {
        Err(BackupFsError::Unsupported)
    }

    pub fn open_regular(_path: &Path) -> Result<File, BackupFsError> {
        Err(BackupFsError::Unsupported)
    }

    pub fn space(_path: &Path) -> Result<DiskSpace, BackupFsError> {
        Err(BackupFsError::Unsupported)
    }
}

struct FileSink(BufWriter<std::fs::File>);

impl Write for FileSink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl BackupSink for FileSink {
    fn finish(&mut self) -> io::Result<()> {
        self.0.flush()?;
        self.0.get_ref().sync_all()
    }
}

/// Rend durable le nom créé ou retiré dans ce dossier.
fn sync_dir(dir: &Path) -> Result<(), BackupFsError> {
    std::fs::File::open(dir)
        .and_then(|handle| handle.sync_all())
        .map_err(|e| io_error(&e))
}

impl StdBackupFiles {
    /// Le sous-dossier d'un service, jugé de confiance (il existe, créé `0700` si `create`).
    fn service_dir(&self, service_dir: &str, create: bool) -> Result<PathBuf, BackupFsError> {
        let name = component(service_dir)?;
        sys::trusted_dir(&self.root)?;
        let path = self.root.join(name);
        if create {
            sys::make_private_dir(&path)?;
        }
        sys::trusted_dir(&path)?;
        Ok(path)
    }
}

impl BackupFiles for StdBackupFiles {
    fn ensure_root(&self) -> Result<(), BackupFsError> {
        match std::fs::symlink_metadata(&self.root) {
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                sys::make_private_dirs(&self.root)?;
            }
            Err(error) => return Err(io_error(&error)),
        }
        sys::trusted_dir(&self.root)
    }

    fn space(&self) -> Result<DiskSpace, BackupFsError> {
        // Le disque de la racine ; si elle n'existe pas encore, celui de son plus proche parent.
        let existing = self
            .root
            .ancestors()
            .find(|candidate| candidate.exists())
            .unwrap_or(Path::new("/"));
        sys::space(existing)
    }

    fn create(
        &self,
        service_dir: &str,
        name: &str,
    ) -> Result<(Box<dyn BackupSink>, FileIdent), BackupFsError> {
        let dir = self.service_dir(service_dir, true)?;
        let file = sys::create_new(&dir.join(component(name)?))?;
        let ident = sys::ident(&file.metadata().map_err(|e| io_error(&e))?);
        Ok((
            Box::new(FileSink(BufWriter::with_capacity(1 << 20, file))),
            ident,
        ))
    }

    fn publish(
        &self,
        service_dir: &str,
        part: &str,
        name: &str,
        expect: FileIdent,
    ) -> Result<FileStamp, BackupFsError> {
        let dir = self.service_dir(service_dir, false)?;
        let (from, to) = (dir.join(component(part)?), dir.join(component(name)?));
        let file = sys::open_regular(&from)?;
        let meta = file.metadata().map_err(|e| io_error(&e))?;
        if sys::ident(&meta) != expect {
            return Err(BackupFsError::Unsafe);
        }
        drop(file);
        // Jamais par-dessus un fichier, même posé à l'instant : le noyau refuse de remplacer.
        sys::rename_noreplace(&from, &to)?;
        sync_dir(&dir)?;
        Ok(sys::stamp(&meta))
    }

    fn write_new(
        &self,
        service_dir: &str,
        name: &str,
        bytes: &[u8],
    ) -> Result<FileIdent, BackupFsError> {
        let dir = self.service_dir(service_dir, true)?;
        let mut file = sys::create_new(&dir.join(component(name)?))?;
        file.write_all(bytes).map_err(|e| io_error(&e))?;
        file.sync_all().map_err(|e| io_error(&e))?;
        let ident = sys::ident(&file.metadata().map_err(|e| io_error(&e))?);
        sync_dir(&dir)?;
        Ok(ident)
    }

    fn seen(&self, service_dir: &str, name: &str) -> Result<Seen, BackupFsError> {
        let dir = match self.service_dir(service_dir, false) {
            Ok(dir) => dir,
            Err(BackupFsError::NotFound) => return Ok(Seen::Absent),
            Err(error) => return Err(error),
        };
        match std::fs::symlink_metadata(dir.join(component(name)?)) {
            Ok(meta) if meta.is_file() => {
                let stamp = sys::stamp(&meta);
                Ok(Seen::File {
                    dev: stamp.dev,
                    ino: stamp.ino,
                    size: stamp.size,
                    mtime_ns: stamp.mtime_ns,
                })
            }
            Ok(_) => Ok(Seen::NotRegular),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Seen::Absent),
            Err(error) => Err(io_error(&error)),
        }
    }

    fn open(
        &self,
        service_dir: &str,
        name: &str,
    ) -> Result<(Box<dyn Read + Send>, FileIdent), BackupFsError> {
        let dir = self.service_dir(service_dir, false)?;
        let file = sys::open_regular(&dir.join(component(name)?))?;
        let ident = sys::ident(&file.metadata().map_err(|e| io_error(&e))?);
        Ok((Box::new(BufReader::with_capacity(1 << 20, file)), ident))
    }

    fn remove(
        &self,
        service_dir: &str,
        name: &str,
        expect: Expected,
    ) -> Result<BackupRemoved, BackupFsError> {
        let dir = match self.service_dir(service_dir, false) {
            Ok(dir) => dir,
            Err(BackupFsError::NotFound) => return Ok(BackupRemoved::Gone),
            Err(error) => return Err(error),
        };
        let path = dir.join(component(name)?);
        match std::fs::symlink_metadata(&path) {
            Ok(meta) if !meta.is_file() => return Ok(BackupRemoved::NotOurs),
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(BackupRemoved::Gone);
            }
            Err(error) => return Err(io_error(&error)),
        }
        // Revérifié sur le fichier OUVERT, juste avant le retrait.
        let file = match sys::open_regular(&path) {
            Ok(file) => file,
            Err(BackupFsError::NotFound) => return Ok(BackupRemoved::Gone),
            Err(BackupFsError::Unsafe) => return Ok(BackupRemoved::NotOurs),
            Err(error) => return Err(error),
        };
        let found = sys::stamp(&file.metadata().map_err(|e| io_error(&e))?);
        if (found.dev, found.ino) != (expect.dev, expect.ino)
            || expect.size.is_some_and(|size| size != found.size)
            || expect.mtime_ns.is_some_and(|mtime| mtime != found.mtime_ns)
        {
            return Ok(BackupRemoved::NotOurs);
        }
        drop(file);
        match std::fs::remove_file(&path) {
            Ok(()) => {
                sync_dir(&dir)?;
                Ok(BackupRemoved::Done)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(BackupRemoved::Gone),
            Err(error) => Err(io_error(&error)),
        }
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use crate::infrastructure::data_dir::private_tempdir;
    use std::io::Read;
    use std::os::unix::fs::{PermissionsExt, symlink};

    fn files(dir: &crate::test_tmp::TestDir) -> StdBackupFiles {
        let root = dir.path().join("backups");
        let files = StdBackupFiles::new(&root);
        files.ensure_root().expect("racine");
        files
    }

    fn write(sink: &mut Box<dyn BackupSink>, bytes: &[u8]) {
        sink.write_all(bytes).expect("écrit");
        sink.finish().expect("durable");
    }

    #[test]
    fn the_root_is_created_private_and_a_link_is_never_a_root() {
        let dir = private_tempdir();
        let files = files(&dir);
        let mode = std::fs::metadata(dir.path().join("backups"))
            .expect("meta")
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "{mode:o}");
        let other = private_tempdir();
        let target = other.path().join("real");
        std::fs::create_dir(&target).expect("dossier");
        let link = dir.path().join("linked");
        symlink(&target, &link).expect("lien");
        assert_eq!(
            StdBackupFiles::new(&link).ensure_root(),
            Err(BackupFsError::Unsafe)
        );
        drop(files);
    }

    #[test]
    fn a_file_is_created_published_read_and_removed_by_its_identity() {
        let dir = private_tempdir();
        let files = files(&dir);
        let (mut sink, ident) = files.create("h_A", "x.tar.gz.part").expect("créé");
        write(&mut sink, b"archive");
        let stamp = files
            .publish("h_A", "x.tar.gz.part", "x.tar.gz", ident)
            .expect("publié");
        let Seen::File {
            dev,
            ino,
            size,
            mtime_ns,
        } = files.seen("h_A", "x.tar.gz").expect("vu")
        else {
            panic!("un fichier est attendu");
        };
        assert_eq!((dev, ino, size), (ident.0, ident.1, 7));
        assert_eq!((stamp.dev, stamp.ino, stamp.size), (dev, ino, size));
        assert_eq!(stamp.mtime_ns, mtime_ns);
        assert_eq!(
            files.seen("h_A", "x.tar.gz.part").expect("vu"),
            Seen::Absent
        );
        let (mut reader, read_ident) = files.open("h_A", "x.tar.gz").expect("ouvert");
        assert_eq!(read_ident, ident);
        let mut text = String::new();
        reader.read_to_string(&mut text).expect("lu");
        assert_eq!(text, "archive");
        assert_eq!(
            files
                .remove("h_A", "x.tar.gz", Expected::stamp(stamp))
                .expect("retiré"),
            BackupRemoved::Done
        );
        assert_eq!(
            files
                .remove("h_A", "x.tar.gz", Expected::stamp(stamp))
                .expect("retiré"),
            BackupRemoved::Gone
        );
    }

    #[test]
    fn a_file_replaced_by_hand_is_never_removed() {
        let dir = private_tempdir();
        let files = files(&dir);
        let (mut sink, ident) = files.create("h_A", "x.tar.gz").expect("créé");
        write(&mut sink, b"mine");
        let path = dir.path().join("backups/h_A/x.tar.gz");
        std::fs::remove_file(&path).expect("retiré à la main");
        std::fs::write(&path, b"foreign").expect("remplacé à la main");
        assert_eq!(
            files
                .remove("h_A", "x.tar.gz", Expected::inode(ident))
                .expect("jugé"),
            BackupRemoved::NotOurs
        );
        assert_eq!(std::fs::read(&path).expect("lu"), b"foreign");
    }

    #[test]
    fn a_file_with_the_same_inode_but_another_size_or_date_is_never_removed() {
        let dir = private_tempdir();
        let files = files(&dir);
        let (mut sink, ident) = files.create("h_A", "x.part").expect("créé");
        write(&mut sink, b"archive");
        let stamp = files.publish("h_A", "x.part", "x", ident).expect("publié");
        let path = dir.path().join("backups/h_A/x");
        // Le même fichier (même inode) réécrit à la main : plus celui que Hearth a écrit.
        std::fs::write(&path, b"rewritten by hand, longer").expect("réécrit");
        assert_eq!(
            files
                .remove("h_A", "x", Expected::stamp(stamp))
                .expect("jugé"),
            BackupRemoved::NotOurs
        );
        assert!(path.exists());
        // La date seule diffère : même refus.
        let file = std::fs::File::options()
            .write(true)
            .open(&path)
            .expect("ouvert");
        file.set_len(stamp.size).expect("taille remise");
        file.set_modified(std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(42))
            .expect("date changée");
        drop(file);
        assert_eq!(
            files
                .remove("h_A", "x", Expected::stamp(stamp))
                .expect("jugé"),
            BackupRemoved::NotOurs
        );
        assert!(path.exists());
    }

    #[test]
    fn a_link_or_a_folder_with_the_name_is_never_followed_or_removed() {
        let dir = private_tempdir();
        let files = files(&dir);
        let (mut sink, ident) = files.create("h_A", "keep.tar.gz").expect("créé");
        write(&mut sink, b"data");
        let outside = dir.path().join("outside.txt");
        std::fs::write(&outside, b"precious").expect("hors racine");
        symlink(&outside, dir.path().join("backups/h_A/link.tar.gz")).expect("lien");
        std::fs::create_dir(dir.path().join("backups/h_A/folder.tar.gz")).expect("dossier");
        for name in ["link.tar.gz", "folder.tar.gz"] {
            assert_eq!(files.seen("h_A", name).expect("vu"), Seen::NotRegular);
            assert_eq!(
                files
                    .remove("h_A", name, Expected::inode(ident))
                    .expect("jugé"),
                BackupRemoved::NotOurs
            );
            assert!(files.open("h_A", name).is_err());
        }
        assert_eq!(std::fs::read(&outside).expect("lu"), b"precious");
    }

    #[test]
    fn a_creation_never_goes_over_an_existing_file_or_through_a_link() {
        let dir = private_tempdir();
        let files = files(&dir);
        let (mut sink, _) = files.create("h_A", "a").expect("créé");
        write(&mut sink, b"first");
        assert!(matches!(
            files.create("h_A", "a"),
            Err(BackupFsError::Exists)
        ));
        let outside = dir.path().join("target.txt");
        symlink(&outside, dir.path().join("backups/h_A/b")).expect("lien");
        assert!(files.create("h_A", "b").is_err());
        assert!(!outside.exists());
    }

    #[test]
    fn a_publication_never_replaces_a_file() {
        let dir = private_tempdir();
        let files = files(&dir);
        let (mut part, ident) = files.create("h_A", "x.part").expect("créé");
        write(&mut part, b"mine");
        std::fs::write(dir.path().join("backups/h_A/x"), b"foreign").expect("étranger");
        assert_eq!(
            files.publish("h_A", "x.part", "x", ident),
            Err(BackupFsError::Exists)
        );
        assert_eq!(
            std::fs::read(dir.path().join("backups/h_A/x")).expect("lu"),
            b"foreign"
        );
    }

    #[test]
    fn a_service_folder_that_is_a_link_is_refused() {
        let dir = private_tempdir();
        let files = files(&dir);
        let other = private_tempdir();
        symlink(other.path(), dir.path().join("backups/h_L")).expect("lien");
        assert!(matches!(
            files.create("h_L", "x"),
            Err(BackupFsError::Unsafe)
        ));
        assert!(
            std::fs::read_dir(other.path())
                .expect("dossier")
                .next()
                .is_none()
        );
    }

    #[test]
    fn names_that_climb_or_nest_are_refused() {
        let dir = private_tempdir();
        let files = files(&dir);
        for bad in ["..", "a/b", "", "."] {
            assert!(files.create("h_A", bad).is_err(), "{bad}");
            assert!(files.create(bad, "x").is_err(), "{bad}");
        }
    }

    #[test]
    fn the_free_space_is_readable() {
        let dir = private_tempdir();
        let files = files(&dir);
        let space = files.space().expect("espace");
        assert!(space.total > 0 && space.free <= space.total);
    }
}

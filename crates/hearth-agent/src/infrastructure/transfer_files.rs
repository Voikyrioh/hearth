//! La zone d'attente des transferts sur le disque (HRT-95, ADR-0048, BR-FILES-019, BR-MOD-015).
//!
//! Écrit avec `std::fs` : cette racine n'a PAS de contenu hostile à parcourir (c'est celle de Hearth, `0700`, à
//! l'agent ; le contenu reçu n'est jamais lu ici). Les garde-fous sont ceux de la racine des sauvegardes :
//!
//! - **aucun listage** : chaque geste vise un nom dérivé d'un identifiant de transfert ;
//! - la racine est jugée AVANT usage : un vrai dossier (jamais un lien), à l'utilisateur de l'agent, que ni le
//!   groupe ni les autres ne modifient ;
//! - une création est EXCLUSIVE et jamais par un lien ; une écriture rouvre la partie SANS suivre de lien et
//!   revérifie son identité (inode) sur le fichier ouvert ; une suppression revérifie l'inode juste avant ;
//! - le fichier complet est posé à son nom définitif par un renommage qui ne remplace JAMAIS (`RENAME_NOREPLACE`) ;
//! - aucun descripteur n'est gardé entre deux morceaux : un client lent ou arrêté ne retient ni fichier ni mémoire.
//!
//! Sans Unix (poste de développement Windows), il n'y a pas d'inode : tout geste répond `Unsupported`, comme
//! `StdBackupFiles`. Le code d'inode et de droits est Linux ; il compile partout, il ne s'exécute pas sous Windows.

use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};

use hearth_proto::api::transfers::DIGEST_LEN;

use crate::application::ports::{
    BackupFsError, BackupRemoved, DiskSpace, Expected, FileIdent, FileStamp, Loaded, ReadSeek,
    TransferFiles,
};
use crate::domain::transfers::TransferId;
use crate::infrastructure::backup_files::sys;

pub struct StdTransferFiles {
    root: PathBuf,
}

impl StdTransferFiles {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn path(&self, name: &str) -> PathBuf {
        // Les noms viennent de `TransferId` (un ULID jugé) : jamais un séparateur ni `..`.
        self.root.join(name)
    }
}

fn io_error(error: &io::Error) -> BackupFsError {
    match error.kind() {
        io::ErrorKind::NotFound => BackupFsError::NotFound,
        io::ErrorKind::AlreadyExists => BackupFsError::Exists,
        other => BackupFsError::Io(other),
    }
}

/// Rend durable le nom créé ou retiré dans ce dossier.
fn sync_dir(dir: &Path) -> Result<(), BackupFsError> {
    std::fs::File::open(dir)
        .and_then(|handle| handle.sync_all())
        .map_err(|e| io_error(&e))
}

#[cfg(unix)]
mod open {
    use super::*;
    use std::fs::{File, OpenOptions};
    use std::os::unix::fs::OpenOptionsExt;

    /// Rouvre en écriture un fichier qui existe : jamais par un lien, ordinaire, sans en créer un.
    pub fn write_existing(path: &Path) -> Result<File, BackupFsError> {
        let file = OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
            .map_err(|e| io_error(&e))?;
        let meta = file.metadata().map_err(|e| io_error(&e))?;
        if !meta.is_file() {
            return Err(BackupFsError::Unsafe);
        }
        Ok(file)
    }
}

#[cfg(not(unix))]
mod open {
    use super::*;
    use std::fs::File;

    pub fn write_existing(_path: &Path) -> Result<File, BackupFsError> {
        Err(BackupFsError::Unsupported)
    }
}

impl StdTransferFiles {
    /// Les empreintes entières de la liste, au plus `max_digests`. Une empreinte à zéro est un trou laissé par un
    /// système de fichiers qui a étendu la liste sans l'écrire : elle n'est jamais reprise comme une empreinte (le
    /// morceau est renvoyé), et la liste s'arrête là.
    fn read_digests(
        &self,
        id: &TransferId,
        max_digests: u64,
    ) -> Result<Vec<[u8; DIGEST_LEN]>, BackupFsError> {
        let file = sys::open_regular(&self.path(&id.digests_name()))?;
        let limit = max_digests.saturating_mul(DIGEST_LEN as u64);
        let mut reader = BufReader::new(file.take(limit));
        let mut digests = Vec::new();
        loop {
            let mut digest = [0u8; DIGEST_LEN];
            match reader.read_exact(&mut digest) {
                Ok(()) if digest == [0u8; DIGEST_LEN] => break,
                Ok(()) => digests.push(digest),
                // Fin de liste, ou dernière empreinte à moitié écrite : elle ne compte pas.
                Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(error) => return Err(io_error(&error)),
            }
        }
        Ok(digests)
    }

    /// La partie, rouverte et REVÉRIFIÉE : c'est le fichier de l'inode attendu.
    fn part_for_write(
        &self,
        id: &TransferId,
        expect: FileIdent,
    ) -> Result<std::fs::File, BackupFsError> {
        sys::trusted_dir(&self.root)?;
        let file = open::write_existing(&self.path(&id.part_name()))?;
        let meta = file.metadata().map_err(|e| io_error(&e))?;
        if sys::ident(&meta) != expect {
            return Err(BackupFsError::Unsafe);
        }
        Ok(file)
    }

    fn remove_regular(
        &self,
        name: &str,
        check: impl Fn(&std::fs::Metadata) -> bool,
    ) -> Result<BackupRemoved, BackupFsError> {
        sys::trusted_dir(&self.root)?;
        let path = self.path(name);
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
        if !check(&file.metadata().map_err(|e| io_error(&e))?) {
            return Ok(BackupRemoved::NotOurs);
        }
        drop(file);
        match std::fs::remove_file(&path) {
            Ok(()) => {
                sync_dir(&self.root)?;
                Ok(BackupRemoved::Done)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(BackupRemoved::Gone),
            Err(error) => Err(io_error(&error)),
        }
    }
}

impl TransferFiles for StdTransferFiles {
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
        let existing = self
            .root
            .ancestors()
            .find(|candidate| candidate.exists())
            .unwrap_or(Path::new("/"));
        sys::space(existing)
    }

    fn create(&self, id: &TransferId) -> Result<FileIdent, BackupFsError> {
        self.ensure_root()?;
        let part = sys::create_new(&self.path(&id.part_name()))?;
        let ident = sys::ident(&part.metadata().map_err(|e| io_error(&e))?);
        part.sync_all().map_err(|e| io_error(&e))?;
        let digests = sys::create_new(&self.path(&id.digests_name()))?;
        digests.sync_all().map_err(|e| io_error(&e))?;
        sync_dir(&self.root)?;
        Ok(ident)
    }

    fn load(
        &self,
        id: &TransferId,
        expect: FileIdent,
        max_digests: u64,
    ) -> Result<Loaded, BackupFsError> {
        sys::trusted_dir(&self.root)?;
        let part = sys::open_regular(&self.path(&id.part_name()))?;
        let meta = part.metadata().map_err(|e| io_error(&e))?;
        if sys::ident(&meta) != expect {
            return Err(BackupFsError::Unsafe);
        }
        let part_len = meta.len();
        let digests = self.read_digests(id, max_digests)?;
        Ok(Loaded { part_len, digests })
    }

    fn load_complete(
        &self,
        id: &TransferId,
        expect: FileIdent,
        size: u64,
        max_digests: u64,
    ) -> Result<(FileStamp, Vec<[u8; DIGEST_LEN]>), BackupFsError> {
        sys::trusted_dir(&self.root)?;
        let file = sys::open_regular(&self.path(&id.complete_name()))?;
        let meta = file.metadata().map_err(|e| io_error(&e))?;
        if sys::ident(&meta) != expect || meta.len() != size {
            return Err(BackupFsError::Unsafe);
        }
        let digests = self.read_digests(id, max_digests)?;
        Ok((sys::stamp(&meta), digests))
    }

    fn append(
        &self,
        id: &TransferId,
        expect: FileIdent,
        index: u64,
        offset: u64,
        data: &[u8],
        digest: &[u8; DIGEST_LEN],
    ) -> Result<(), BackupFsError> {
        use std::io::{Seek, SeekFrom};
        let mut part = self.part_for_write(id, expect)?;
        // Un morceau à moitié écrit par un arrêt est écrasé : la partie fait EXACTEMENT `offset` octets avant lui.
        // Plus courte, c'est qu'un morceau déjà accusé a disparu : jamais comblé en silence.
        let len = part.metadata().map_err(|e| io_error(&e))?.len();
        if len < offset {
            return Err(BackupFsError::Unsafe);
        }
        if len > offset {
            part.set_len(offset).map_err(|e| io_error(&e))?;
        }
        part.seek(SeekFrom::Start(offset))
            .map_err(|e| io_error(&e))?;
        part.write_all(data).map_err(|e| io_error(&e))?;
        // Le morceau est durable AVANT son empreinte et avant l'accusé de réception.
        part.sync_data().map_err(|e| io_error(&e))?;
        drop(part);
        let mut digests = open::write_existing(&self.path(&id.digests_name()))?;
        let at = index.saturating_mul(DIGEST_LEN as u64);
        digests.set_len(at).map_err(|e| io_error(&e))?;
        digests
            .seek(SeekFrom::Start(at))
            .map_err(|e| io_error(&e))?;
        digests.write_all(digest).map_err(|e| io_error(&e))?;
        // La liste n'a pas besoin d'être durable avant l'accusé : une empreinte perdue fait renvoyer son morceau.
        Ok(())
    }

    fn publish(
        &self,
        id: &TransferId,
        expect: FileIdent,
        size: u64,
    ) -> Result<FileStamp, BackupFsError> {
        let part = self.part_for_write(id, expect)?;
        let meta = part.metadata().map_err(|e| io_error(&e))?;
        if meta.len() != size {
            return Err(BackupFsError::Unsafe);
        }
        part.sync_all().map_err(|e| io_error(&e))?;
        drop(part);
        // La liste des empreintes est durable AVANT le nom définitif : une coupure juste après la publication ne
        // laisse pas un fichier complet dont la liste a perdu sa fin.
        open::write_existing(&self.path(&id.digests_name()))?
            .sync_all()
            .map_err(|e| io_error(&e))?;
        // Jamais par-dessus un fichier, même posé à l'instant : le noyau refuse de remplacer.
        match sys::rename_noreplace(&self.path(&id.part_name()), &self.path(&id.complete_name())) {
            Ok(()) => {}
            // Une coupure entre le lien dur du repli et le retrait de l'ancien nom laisse la partie ET le fichier sur le
            // MÊME inode : c'est la même publication, il ne reste qu'à retirer l'ancien nom.
            Err(BackupFsError::Exists) => {
                let published = sys::open_regular(&self.path(&id.complete_name()))?;
                if sys::ident(&published.metadata().map_err(|e| io_error(&e))?) != expect {
                    return Err(BackupFsError::Exists);
                }
                std::fs::remove_file(self.path(&id.part_name())).map_err(|e| io_error(&e))?;
            }
            Err(error) => return Err(error),
        }
        sync_dir(&self.root)?;
        let published = sys::open_regular(&self.path(&id.complete_name()))?;
        Ok(sys::stamp(&published.metadata().map_err(|e| io_error(&e))?))
    }

    fn open_complete(
        &self,
        id: &TransferId,
        expect: FileStamp,
    ) -> Result<Box<dyn ReadSeek>, BackupFsError> {
        sys::trusted_dir(&self.root)?;
        let file = sys::open_regular(&self.path(&id.complete_name()))?;
        let found = sys::stamp(&file.metadata().map_err(|e| io_error(&e))?);
        if found != expect {
            return Err(BackupFsError::Unsafe);
        }
        Ok(Box::new(file))
    }

    fn remove_part(
        &self,
        id: &TransferId,
        expect: Option<FileIdent>,
    ) -> Result<BackupRemoved, BackupFsError> {
        self.remove_regular(&id.part_name(), |meta| {
            expect.is_none_or(|expected| sys::ident(meta) == expected)
        })
    }

    fn remove_digests(&self, id: &TransferId) -> Result<BackupRemoved, BackupFsError> {
        self.remove_regular(&id.digests_name(), |_| true)
    }

    fn remove_complete(
        &self,
        id: &TransferId,
        expect: Expected,
    ) -> Result<BackupRemoved, BackupFsError> {
        self.remove_regular(&id.complete_name(), |meta| {
            let found = sys::stamp(meta);
            (found.dev, found.ino) == (expect.dev, expect.ino)
                && expect.size.is_none_or(|size| size == found.size)
                && expect.mtime_ns.is_none_or(|mtime| mtime == found.mtime_ns)
        })
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use crate::infrastructure::data_dir::private_tempdir;
    use std::io::Read;
    use std::os::unix::fs::{PermissionsExt, symlink};

    fn files(dir: &crate::test_tmp::TestDir) -> StdTransferFiles {
        let files = StdTransferFiles::new(dir.path().join("transfers"));
        files.ensure_root().expect("racine");
        files
    }

    fn id() -> TransferId {
        TransferId::parse("01J9ZY0G3Q8M2K6W4T7V5N1B9D").expect("ulid")
    }

    #[test]
    fn the_root_is_private_and_a_link_is_never_a_root() {
        let dir = private_tempdir();
        let files = files(&dir);
        let mode = std::fs::metadata(dir.path().join("transfers"))
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
            StdTransferFiles::new(&link).ensure_root(),
            Err(BackupFsError::Unsafe)
        );
        drop(files);
    }

    #[test]
    fn a_transfer_is_written_resumed_published_read_and_removed() {
        let dir = private_tempdir();
        let files = files(&dir);
        let id = id();
        let ident = files.create(&id).expect("créé");
        // Jamais sous son nom définitif tant qu'il est partiel.
        files
            .append(&id, ident, 0, 0, b"abcd", &[1; 32])
            .expect("a");
        assert!(dir.path().join("transfers").join(id.part_name()).exists());
        assert!(
            !dir.path()
                .join("transfers")
                .join(id.complete_name())
                .exists()
        );
        files.append(&id, ident, 1, 4, b"ef", &[2; 32]).expect("b");
        let loaded = files.load(&id, ident, 10).expect("lu");
        assert_eq!(loaded.part_len, 6);
        assert_eq!(loaded.digests, vec![[1; 32], [2; 32]]);
        // La reprise : un morceau à moitié écrit est écrasé, la liste est recoupée à son rang.
        files
            .append(&id, ident, 1, 4, b"EF", &[3; 32])
            .expect("b bis");
        let loaded = files.load(&id, ident, 10).expect("relu");
        assert_eq!(loaded.digests, vec![[1; 32], [3; 32]]);
        // Plus d'empreintes que le plafond : la liste est coupée.
        assert_eq!(files.load(&id, ident, 1).expect("coupé").digests.len(), 1);
        // Une partie qui n'a pas la taille annoncée n'est jamais publiée.
        assert_eq!(files.publish(&id, ident, 7), Err(BackupFsError::Unsafe));
        let stamp = files.publish(&id, ident, 6).expect("publié");
        assert!(!dir.path().join("transfers").join(id.part_name()).exists());
        let mut reader = files.open_complete(&id, stamp).expect("ouvert");
        let mut text = String::new();
        reader.read_to_string(&mut text).expect("lu");
        assert_eq!(text, "abcdEF");
        assert_eq!(
            files.remove_digests(&id).expect("liste"),
            BackupRemoved::Done
        );
        assert_eq!(
            files
                .remove_complete(&id, Expected::stamp(stamp))
                .expect("retiré"),
            BackupRemoved::Done
        );
        assert_eq!(
            files
                .remove_complete(&id, Expected::stamp(stamp))
                .expect("retiré"),
            BackupRemoved::Gone
        );
    }

    #[test]
    fn a_part_that_lost_an_acknowledged_chunk_is_refused_not_filled() {
        let dir = private_tempdir();
        let files = files(&dir);
        let id = id();
        let ident = files.create(&id).expect("créé");
        assert_eq!(
            files.append(&id, ident, 1, 4, b"ef", &[2; 32]),
            Err(BackupFsError::Unsafe)
        );
    }

    #[test]
    fn a_part_replaced_by_hand_is_never_written_nor_removed() {
        let dir = private_tempdir();
        let files = files(&dir);
        let id = id();
        let ident = files.create(&id).expect("créé");
        let path = dir.path().join("transfers").join(id.part_name());
        // Le fichier étranger est créé AVANT le retrait de la partie et posé par un renommage : il a forcément un autre
        // inode (un numéro libéré peut être réattribué tout de suite par certains systèmes de fichiers).
        let foreign = dir.path().join("transfers").join("etranger");
        std::fs::write(&foreign, b"foreign").expect("créé à la main");
        std::fs::rename(&foreign, &path).expect("remplacé à la main");
        assert_eq!(
            files.append(&id, ident, 0, 0, b"abcd", &[1; 32]),
            Err(BackupFsError::Unsafe)
        );
        assert_eq!(
            files.remove_part(&id, Some(ident)).expect("jugé"),
            BackupRemoved::NotOurs
        );
        assert_eq!(std::fs::read(&path).expect("lu"), b"foreign");
    }

    #[test]
    fn a_link_in_place_of_the_part_is_never_followed() {
        let dir = private_tempdir();
        let files = files(&dir);
        let id = id();
        let ident = files.create(&id).expect("créé");
        let path = dir.path().join("transfers").join(id.part_name());
        let outside = dir.path().join("outside");
        std::fs::write(&outside, b"keep").expect("hors zone");
        std::fs::remove_file(&path).expect("retiré");
        symlink(&outside, &path).expect("lien");
        assert!(files.append(&id, ident, 0, 0, b"abcd", &[1; 32]).is_err());
        assert_eq!(
            files.remove_part(&id, None).expect("jugé"),
            BackupRemoved::NotOurs
        );
        assert_eq!(std::fs::read(&outside).expect("lu"), b"keep");
    }

    #[test]
    fn publishing_never_replaces_a_file_that_is_already_there() {
        let dir = private_tempdir();
        let files = files(&dir);
        let id = id();
        let ident = files.create(&id).expect("créé");
        files
            .append(&id, ident, 0, 0, b"abcd", &[1; 32])
            .expect("a");
        let taken = dir.path().join("transfers").join(id.complete_name());
        std::fs::write(&taken, b"mine").expect("posé à la main");
        assert_eq!(files.publish(&id, ident, 4), Err(BackupFsError::Exists));
        assert_eq!(std::fs::read(&taken).expect("lu"), b"mine");
        assert!(dir.path().join("transfers").join(id.part_name()).exists());
    }
    #[test]
    fn a_part_and_a_file_left_on_the_same_inode_by_a_cut_are_one_publication() {
        let dir = private_tempdir();
        let files = files(&dir);
        let id = id();
        let ident = files.create(&id).expect("créé");
        files
            .append(&id, ident, 0, 0, b"abcd", &[1; 32])
            .expect("a");
        let root = dir.path().join("transfers");
        // Coupure entre le lien dur du repli et le retrait de l'ancien nom.
        std::fs::hard_link(root.join(id.part_name()), root.join(id.complete_name())).expect("lien");
        let stamp = files.publish(&id, ident, 4).expect("terminé");
        assert!(!root.join(id.part_name()).exists());
        assert_eq!(stamp.size, 4);
        // Un AUTRE fichier sous le nom définitif n'est jamais pris pour la publication.
        let other = TransferId::parse("01J9ZY0G3Q8M2K6W4T7V5N1B9E").expect("ulid");
        let other_ident = files.create(&other).expect("créé");
        files
            .append(&other, other_ident, 0, 0, b"abcd", &[1; 32])
            .expect("a");
        std::fs::write(root.join(other.complete_name()), b"mine").expect("posé à la main");
        assert_eq!(
            files.publish(&other, other_ident, 4),
            Err(BackupFsError::Exists)
        );
        assert!(root.join(other.part_name()).exists());
    }
}

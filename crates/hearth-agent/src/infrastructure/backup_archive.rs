//! L'archive d'une sauvegarde : écrire et relire un `.tar.gz` (HRT-97, ADR-0047, BR-BACKUP-013, BR-BACKUP-014).
//!
//! **Écrire** : `tar` (format GNU, lisible partout, noms longs compris) compressé en gzip niveau 1 (`flate2`,
//! moteur Rust pur), une entrée par élément, le manifeste en dernière entrée (`.hearth-manifest.json`). Le
//! SHA-256 de l'archive se calcule AU FIL de l'écriture : aucune relecture pour l'avoir. Un fichier est lu en
//! flux, jamais en entier en mémoire ; sa taille annoncée est celle du premier `fstat` : un fichier qui grossit
//! est coupé à cette taille, un fichier qui rétrécit est complété de zéros (et noté changé par l'appelant).
//!
//! **Relire** (vérification, BR-BACKUP-013) : l'archive est lue comme hostile. Aucune fonction d'extraction de
//! la crate (`unpack`) : `entries()` seulement, chaque entrée jugée contre le manifeste (`domain::backups::read`),
//! chaque fichier lu en flux dans un hacheur sans rien écrire sur le disque, l'empreinte de l'archive entière
//! comparée en fin de lecture.

use std::cell::RefCell;
use std::io::{self, Read, Write};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use sha2::{Digest, Sha256};
use tar::{Builder, EntryType as TarType, Header};

use crate::application::ports::{BackupSink, EntryKind, EntryMeta};
use crate::domain::backups::manifest::{IN_ARCHIVE_NAME, MAX_MANIFEST_BYTES, Manifest};
use crate::domain::backups::read::{Accepted, ArchiveIndex, Refusal, TarKind};

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// L'empreinte hexadécimale d'octets en mémoire.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

// ---------------------------------------------------------------------------------------------
// Écrire
// ---------------------------------------------------------------------------------------------

/// Le fichier de l'archive, compté et haché à mesure que gzip y écrit.
struct Counted {
    sink: Box<dyn BackupSink>,
    written: Arc<AtomicU64>,
    hasher: Sha256,
}

impl Write for Counted {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.sink.write(buf)?;
        self.hasher.update(&buf[..n]);
        self.written
            .fetch_add(u64::try_from(n).unwrap_or(u64::MAX), Ordering::Relaxed);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.sink.flush()
    }
}

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

/// Lit exactement `size` octets de `inner`, les hache, et complète de zéros si `inner` s'arrête avant.
struct Exact<'a, R: Read + ?Sized> {
    inner: &'a mut R,
    left: u64,
    hasher: Sha256,
    short: bool,
}

impl<R: Read + ?Sized> Read for Exact<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.left == 0 || buf.is_empty() {
            return Ok(0);
        }
        let room = usize::try_from(self.left)
            .unwrap_or(usize::MAX)
            .min(buf.len());
        let n = if self.short {
            0
        } else {
            self.inner.read(&mut buf[..room])?
        };
        let n = if n == 0 {
            // Fin prématurée : le reste est des zéros, la taille annoncée est tenue.
            self.short = true;
            buf[..room].fill(0);
            room
        } else {
            n
        };
        self.hasher.update(&buf[..n]);
        self.left -= u64::try_from(n).unwrap_or(0);
        Ok(n)
    }
}

pub struct ArchiveWriter {
    builder: Builder<GzEncoder<Counted>>,
}

fn header_for(meta: &EntryMeta, kind: TarType, size: u64) -> Header {
    let mut header = Header::new_gnu();
    header.set_entry_type(kind);
    header.set_size(size);
    // Jamais de bit spécial (setuid, setgid, sticky) dans une archive de Hearth.
    header.set_mode(meta.mode & 0o777);
    header.set_uid(u64::from(meta.uid));
    header.set_gid(u64::from(meta.gid));
    header.set_mtime(u64::try_from(meta.mtime_ns / 1_000_000_000).unwrap_or(0));
    header
}

impl ArchiveWriter {
    /// `sink` : le fichier partiel. Rend aussi le compteur d'octets compressés écrits (lu par l'appelant pour
    /// surveiller l'espace).
    pub fn new(sink: Box<dyn BackupSink>) -> (Self, Arc<AtomicU64>) {
        let written = Arc::new(AtomicU64::new(0));
        let counted = Counted {
            sink,
            written: Arc::clone(&written),
            hasher: Sha256::new(),
        };
        let encoder = GzEncoder::new(counted, Compression::new(1));
        (
            Self {
                builder: Builder::new(encoder),
            },
            written,
        )
    }

    /// Un dossier.
    pub fn dir(&mut self, tar_path: &str, meta: &EntryMeta) -> io::Result<()> {
        let mut header = header_for(meta, TarType::Directory, 0);
        self.builder.append_data(&mut header, tar_path, io::empty())
    }

    /// Un lien symbolique, jamais suivi.
    pub fn symlink(&mut self, tar_path: &str, meta: &EntryMeta, target: &str) -> io::Result<()> {
        let mut header = header_for(meta, TarType::Symlink, 0);
        self.builder.append_link(&mut header, tar_path, target)
    }

    /// Un fichier : exactement `meta.size` octets, lus en flux, hachés au passage.
    pub fn file(
        &mut self,
        tar_path: &str,
        meta: &EntryMeta,
        source: &mut dyn Read,
    ) -> io::Result<FileDigest> {
        let mut header = header_for(meta, TarType::Regular, meta.size);
        let mut exact = Exact {
            inner: source,
            left: meta.size,
            hasher: Sha256::new(),
            short: false,
        };
        self.builder
            .append_data(&mut header, tar_path, &mut exact)?;
        Ok(FileDigest {
            sha256: hex(&exact.hasher.finalize()),
            short: exact.short,
        })
    }

    /// Ajoute le manifeste en dernière entrée, ferme l'archive et rend le fichier durable.
    pub fn finish(mut self, manifest_json: &[u8]) -> io::Result<ArchiveEnd> {
        let meta = EntryMeta {
            kind: EntryKind::File,
            size: u64::try_from(manifest_json.len()).unwrap_or(u64::MAX),
            mode: 0o644,
            uid: 0,
            gid: 0,
            mtime_ns: 0,
            nlink: 1,
        };
        let mut header = header_for(&meta, TarType::Regular, meta.size);
        self.builder
            .append_data(&mut header, IN_ARCHIVE_NAME, manifest_json)?;
        let encoder = self.builder.into_inner()?;
        let mut counted = encoder.finish()?;
        counted.sink.flush()?;
        counted.sink.finish()?;
        Ok(ArchiveEnd {
            bytes: counted.written.load(Ordering::Relaxed),
            sha256: hex(&counted.hasher.finalize()),
        })
    }
}

// ---------------------------------------------------------------------------------------------
// Relire
// ---------------------------------------------------------------------------------------------

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

/// Une erreur de lecture : c'est l'ARCHIVE qui est en cause (gzip, en-tête tar, fin prématurée : `tar` rend ses
/// refus en `Other`), sauf quand c'est le disque ou les droits qui refusent de la lire.
fn corrupt_or_io(error: &io::Error) -> VerifyError {
    match error.kind() {
        io::ErrorKind::PermissionDenied
        | io::ErrorKind::NotFound
        | io::ErrorKind::TimedOut
        | io::ErrorKind::OutOfMemory
        | io::ErrorKind::Interrupted => VerifyError::Io(error.kind()),
        _ => VerifyError::Corrupt,
    }
}

/// Ce que la lecture a établi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerifyReport {
    pub files_checked: u64,
}

struct Shared {
    inner: Box<dyn Read + Send>,
    hasher: Sha256,
}

/// Un lecteur partagé : gzip en garde un exemplaire, l'appelant l'autre pour finir de lire (et hacher) ce que
/// gzip n'a pas consommé.
#[derive(Clone)]
struct SharedReader(Rc<RefCell<Shared>>);

impl Read for SharedReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut shared = self.0.borrow_mut();
        let n = shared.inner.read(buf)?;
        shared.hasher.update(&buf[..n]);
        Ok(n)
    }
}

/// Un écrivain qui hache ce qu'on lui jette (le contenu d'un fichier, jamais écrit nulle part).
struct HashSink(Sha256);

impl Write for HashSink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.update(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn kind_of(kind: TarType) -> TarKind {
    match kind {
        TarType::Regular | TarType::Continuous => TarKind::File,
        TarType::Directory => TarKind::Dir,
        TarType::Symlink => TarKind::Symlink,
        _ => TarKind::Other,
    }
}

/// Relit l'archive et la juge contre `manifest` (qui a passé `Manifest::check`) : chaque entrée a sa ligne, du
/// bon type, de la bonne taille, du bon SHA-256 ; aucune entrée de plus ; le manifeste de l'archive est celui
/// d'à côté ; l'empreinte de l'archive entière est `archive_sha256`. `stop` : demande d'arrêt, relue entre deux
/// entrées.
pub fn verify_archive(
    reader: Box<dyn Read + Send>,
    manifest: &Manifest,
    archive_sha256: &str,
    stop: &dyn Fn() -> bool,
) -> Result<VerifyReport, VerifyError> {
    let shared = SharedReader(Rc::new(RefCell::new(Shared {
        inner: reader,
        hasher: Sha256::new(),
    })));
    let mut index = ArchiveIndex::new(manifest);
    let mut archive = tar::Archive::new(GzDecoder::new(shared.clone()));
    let mut files_checked = 0_u64;
    let entries = archive.entries().map_err(|e| corrupt_or_io(&e))?;
    for entry in entries {
        if stop() {
            return Err(VerifyError::Stopped);
        }
        let mut entry = entry.map_err(|e| corrupt_or_io(&e))?;
        let kind = kind_of(entry.header().entry_type());
        let path = entry
            .path()
            .map_err(|e| corrupt_or_io(&e))?
            .to_string_lossy()
            .into_owned();
        let size = entry.size();
        let accepted = index.accept(&path, kind, size).map_err(|refusal| {
            tracing::warn!(?refusal, "archive de sauvegarde refusée à la lecture");
            VerifyError::Corrupt
        })?;
        match accepted {
            Accepted::Manifest => {
                if size > MAX_MANIFEST_BYTES {
                    return Err(VerifyError::Corrupt);
                }
                let mut bytes = Vec::new();
                (&mut entry)
                    .take(size)
                    .read_to_end(&mut bytes)
                    .map_err(|e| corrupt_or_io(&e))?;
                let inside: Manifest =
                    serde_json::from_slice(&bytes).map_err(|_| VerifyError::Corrupt)?;
                if inside.entries != manifest.entries
                    || inside.totals != manifest.totals
                    || inside.backup_id != manifest.backup_id
                {
                    return Err(VerifyError::Corrupt);
                }
            }
            Accepted::Entry(position) => {
                let expected = &manifest.entries[position];
                match kind {
                    TarKind::File => {
                        let mut sink = HashSink(Sha256::new());
                        io::copy(&mut entry, &mut sink).map_err(|e| corrupt_or_io(&e))?;
                        let sha = hex(&sink.0.finalize());
                        if expected.sha256.as_deref() != Some(sha.as_str()) {
                            return Err(VerifyError::Corrupt);
                        }
                        files_checked += 1;
                    }
                    TarKind::Symlink => {
                        let target = entry
                            .link_name()
                            .map_err(|e| corrupt_or_io(&e))?
                            .map(|target| target.to_string_lossy().into_owned());
                        if target != expected.link_target {
                            return Err(VerifyError::Corrupt);
                        }
                    }
                    TarKind::Dir | TarKind::Other => {}
                }
            }
        }
    }
    drop(archive);
    index.finish().map_err(|refusal: Refusal| {
        tracing::warn!(?refusal, "archive de sauvegarde incomplète");
        VerifyError::Corrupt
    })?;
    if !index.saw_manifest() {
        return Err(VerifyError::Corrupt);
    }
    // Ce que gzip n'a pas consommé (fin de l'archive tar, fin du flux) est lu et haché aussi : l'empreinte est
    // celle du FICHIER entier, jamais d'un préfixe.
    let mut rest = shared.clone();
    io::copy(&mut rest, &mut io::sink()).map_err(|e| corrupt_or_io(&e))?;
    let sha = hex(&shared.0.borrow().hasher.clone().finalize());
    if sha != archive_sha256 {
        return Err(VerifyError::Corrupt);
    }
    Ok(VerifyReport { files_checked })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::backups::manifest::{
        ArchiveFacts, Entry, EntryType, FORMAT, ManifestRoot, tar_path,
    };
    use hearth_proto::api::backups::BackupMode;
    use std::io::Cursor;
    use std::sync::Mutex;

    /// Un fichier d'archive en mémoire.
    #[derive(Clone, Default)]
    struct MemSink(Arc<Mutex<Vec<u8>>>);

    impl Write for MemSink {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().expect("verrou").extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl BackupSink for MemSink {
        fn finish(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn meta(kind: EntryKind, size: u64) -> EntryMeta {
        EntryMeta {
            kind,
            size,
            mode: 0o644,
            uid: 1000,
            gid: 1000,
            mtime_ns: 1_700_000_000_000_000_000,
            nlink: 1,
        }
    }

    struct Built {
        bytes: Vec<u8>,
        manifest: Manifest,
        sha: String,
    }

    fn entry(
        path: &str,
        kind: EntryType,
        size: u64,
        sha: Option<String>,
        link: Option<&str>,
    ) -> Entry {
        Entry {
            root: 0,
            path: path.into(),
            kind,
            size,
            sha256: sha,
            mode: 0o644,
            uid: 1000,
            gid: 1000,
            mtime_ns: 1_700_000_000_000_000_000,
            nlink: 1,
            link_target: link.map(str::to_owned),
            changed: false,
        }
    }

    /// Fabrique une archive de trois entrées avec le vrai écrivain.
    fn build(content: &[u8]) -> Built {
        let sink = MemSink::default();
        let (mut writer, written) = ArchiveWriter::new(Box::new(sink.clone()));
        writer.dir("data", &meta(EntryKind::Dir, 0)).expect("dir");
        let digest = writer
            .file(
                &tar_path("data", "a.txt"),
                &meta(EntryKind::File, content.len() as u64),
                &mut Cursor::new(content.to_vec()),
            )
            .expect("file");
        assert!(!digest.short);
        writer
            .symlink("data/l", &meta(EntryKind::Symlink, 0), "a.txt")
            .expect("link");
        let entries = vec![
            entry("", EntryType::Dir, 0, None, None),
            entry(
                "a.txt",
                EntryType::File,
                content.len() as u64,
                Some(digest.sha256),
                None,
            ),
            entry("l", EntryType::Symlink, 0, None, Some("a.txt")),
        ];
        let mut manifest = Manifest {
            format: FORMAT,
            backup_id: "01J9ZY0G3Q8M2K6W4T7V5N1B9D".into(),
            service_id: "h_x".into(),
            service_name: "s".into(),
            created_at: "2026-11-14T10:00:00Z".into(),
            agent_version: "0".into(),
            image: "i".into(),
            mode: BackupMode::Stopped,
            flags: vec![],
            roots: vec![ManifestRoot {
                id: 0,
                target: "/data".into(),
                dir: "data".into(),
                managed: true,
            }],
            ignored: vec![],
            totals: Manifest::recount(&entries),
            entries,
            archive: None,
        };
        let json = serde_json::to_vec(&manifest).expect("json");
        let end = writer.finish(&json).expect("finish");
        assert_eq!(end.bytes, written.load(Ordering::Relaxed));
        let bytes = sink.0.lock().expect("verrou").clone();
        assert_eq!(end.bytes, bytes.len() as u64);
        assert_eq!(end.sha256, sha256_hex(&bytes));
        manifest.archive = Some(ArchiveFacts {
            name: "x.tar.gz".into(),
            bytes: end.bytes,
            sha256: end.sha256.clone(),
        });
        Built {
            bytes,
            manifest,
            sha: end.sha256,
        }
    }

    fn verify(built: &Built, bytes: Vec<u8>, sha: &str) -> Result<VerifyReport, VerifyError> {
        verify_archive(Box::new(Cursor::new(bytes)), &built.manifest, sha, &|| {
            false
        })
    }

    #[test]
    fn an_archive_written_by_hearth_reads_back_and_matches_its_digest() {
        let built = build(b"hello world");
        assert_eq!(built.manifest.check(), Ok(()));
        let report = verify(&built, built.bytes.clone(), &built.sha).expect("vérifiée");
        assert_eq!(report.files_checked, 1);
    }

    #[test]
    fn the_archive_is_a_standard_tar_gz_with_the_manifest_last() {
        let built = build(b"abc");
        let gz = GzDecoder::new(Cursor::new(built.bytes.clone()));
        let mut archive = tar::Archive::new(gz);
        let names: Vec<String> = archive
            .entries()
            .expect("entrées")
            .map(|entry| {
                entry
                    .expect("entrée")
                    .path()
                    .expect("chemin")
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(
            names,
            ["data", "data/a.txt", "data/l", ".hearth-manifest.json"]
        );
    }

    #[test]
    fn a_changed_byte_in_the_archive_is_corrupt() {
        let built = build(b"hello world hello world hello world");
        let mut bytes = built.bytes.clone();
        let middle = bytes.len() / 2;
        bytes[middle] ^= 0xff;
        assert_eq!(verify(&built, bytes, &built.sha), Err(VerifyError::Corrupt));
    }

    #[test]
    fn a_truncated_archive_is_corrupt() {
        let built = build(b"hello world");
        let mut bytes = built.bytes.clone();
        bytes.truncate(bytes.len() - 8);
        assert_eq!(verify(&built, bytes, &built.sha), Err(VerifyError::Corrupt));
    }

    #[test]
    fn bytes_appended_after_the_archive_change_its_digest() {
        let built = build(b"hello world");
        let mut bytes = built.bytes.clone();
        bytes.extend_from_slice(b"trailing garbage");
        assert_eq!(verify(&built, bytes, &built.sha), Err(VerifyError::Corrupt));
    }

    #[test]
    fn a_manifest_that_disagrees_with_a_file_makes_the_archive_corrupt() {
        let mut built = build(b"hello world");
        built.manifest.entries[1].sha256 = Some("0".repeat(64));
        assert_eq!(
            verify(&built, built.bytes.clone(), &built.sha),
            Err(VerifyError::Corrupt)
        );
    }

    #[test]
    fn an_archive_with_an_entry_the_manifest_does_not_list_is_refused() {
        let built = build(b"hello world");
        let mut shorter = built.manifest.clone();
        shorter.entries.pop();
        shorter.totals = Manifest::recount(&shorter.entries);
        assert_eq!(
            verify(
                &Built {
                    manifest: shorter.clone(),
                    ..built_clone(&built)
                },
                built.bytes.clone(),
                &built.sha
            ),
            Err(VerifyError::Corrupt)
        );
    }

    fn built_clone(built: &Built) -> Built {
        Built {
            bytes: built.bytes.clone(),
            manifest: built.manifest.clone(),
            sha: built.sha.clone(),
        }
    }

    #[test]
    fn a_stop_request_ends_the_reading() {
        let built = build(b"hello");
        let result = verify_archive(
            Box::new(Cursor::new(built.bytes.clone())),
            &built.manifest,
            &built.sha,
            &|| true,
        );
        assert_eq!(result, Err(VerifyError::Stopped));
    }

    #[test]
    fn a_file_that_shrinks_is_padded_to_its_announced_size_and_flagged() {
        let sink = MemSink::default();
        let (mut writer, _) = ArchiveWriter::new(Box::new(sink));
        let digest = writer
            .file(
                "data/a",
                &meta(EntryKind::File, 10),
                &mut Cursor::new(b"abc".to_vec()),
            )
            .expect("file");
        assert!(digest.short);
        let mut expected = b"abc".to_vec();
        expected.resize(10, 0);
        assert_eq!(digest.sha256, sha256_hex(&expected));
    }

    #[test]
    fn a_file_that_grows_is_cut_at_its_announced_size() {
        let sink = MemSink::default();
        let (mut writer, _) = ArchiveWriter::new(Box::new(sink));
        let digest = writer
            .file(
                "data/a",
                &meta(EntryKind::File, 3),
                &mut Cursor::new(b"abcdef".to_vec()),
            )
            .expect("file");
        assert!(!digest.short);
        assert_eq!(digest.sha256, sha256_hex(b"abc"));
    }
}

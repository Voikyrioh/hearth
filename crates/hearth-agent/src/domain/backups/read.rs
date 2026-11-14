//! Lire une archive comme hostile (ADR-0047 point 7, BR-BACKUP-013).
//!
//! L'archive est un fichier de Hearth, mais elle a pu être remplacée par qui est root : on la traite comme
//! venue d'ailleurs. Le manifeste voisin (cru parce que son empreinte est celle du registre) dit ce qu'elle
//! doit contenir, et **rien d'autre** : chaque entrée lue doit y avoir sa ligne, du bon type, de la bonne
//! taille ; le nombre d'entrées et le total des octets sont bornés par lui (bombe de décompression) ; une ligne
//! du manifeste sans entrée est une archive corrompue.
//!
//! Fonction pure : [`ArchiveIndex`] ne lit rien, l'appelant lui dit ce qu'il voit.

use std::collections::HashMap;

use super::manifest::{EntryType, IN_ARCHIVE_NAME, MAX_MANIFEST_BYTES, Manifest, tar_path};

/// Le genre d'une entrée d'archive, tel que l'appelant le lit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TarKind {
    File,
    Dir,
    Symlink,
    /// Lien physique, périphérique, tube, entrée étendue inconnue : refusé.
    Other,
}

/// Pourquoi l'archive est refusée en entier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    #[error("type d'entrée non permis")]
    UnsupportedType,
    #[error("entrée absente du manifeste")]
    UnknownEntry,
    #[error("entrée du manifeste avec un autre type")]
    WrongType,
    #[error("taille différente du manifeste")]
    SizeMismatch,
    #[error("entrée en double")]
    Duplicate,
    #[error("chemin invalide")]
    BadPath,
    #[error("plus d'entrées que le manifeste")]
    TooManyEntries,
    #[error("plus d'octets que le manifeste")]
    TooManyBytes,
    #[error("des entrées du manifeste manquent")]
    Missing,
}

/// Ce que l'entrée lue est pour le manifeste.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accepted {
    /// L'entrée numéro `n` du manifeste.
    Entry(usize),
    /// Le manifeste que l'archive contient (dernière entrée).
    Manifest,
}

pub struct ArchiveIndex<'a> {
    manifest: &'a Manifest,
    by_path: HashMap<String, usize>,
    seen: Vec<bool>,
    seen_count: usize,
    bytes: u64,
    entries_read: usize,
    manifest_seen: bool,
}

/// Un chemin d'archive ramené à sa forme de comparaison : sans `/` final, sans composant `.` ni `..`, sans
/// racine. `None` si le chemin ne désigne pas quelque chose sous l'archive.
fn clean(path: &str) -> Option<&str> {
    let trimmed = path.trim_end_matches('/');
    let ok = !trimmed.is_empty()
        && !trimmed.starts_with('/')
        && !trimmed.contains('\0')
        && !trimmed.contains('\\')
        && trimmed
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..");
    ok.then_some(trimmed)
}

impl<'a> ArchiveIndex<'a> {
    /// `manifest` doit avoir passé [`Manifest::check`] (les dossiers et les chemins sont sûrs et uniques).
    pub fn new(manifest: &'a Manifest) -> Self {
        let dirs: HashMap<u32, &str> = manifest
            .roots
            .iter()
            .map(|root| (root.id, root.dir.as_str()))
            .collect();
        let mut by_path = HashMap::with_capacity(manifest.entries.len());
        for (index, entry) in manifest.entries.iter().enumerate() {
            if let Some(dir) = dirs.get(&entry.root) {
                by_path.insert(tar_path(dir, &entry.path), index);
            }
        }
        Self {
            manifest,
            by_path,
            seen: vec![false; manifest.entries.len()],
            seen_count: 0,
            bytes: 0,
            entries_read: 0,
            manifest_seen: false,
        }
    }

    /// Juge l'entrée suivante de l'archive.
    pub fn accept(&mut self, path: &str, kind: TarKind, size: u64) -> Result<Accepted, Refusal> {
        if kind == TarKind::Other {
            return Err(Refusal::UnsupportedType);
        }
        let path = clean(path).ok_or(Refusal::BadPath)?;
        self.entries_read += 1;
        if self.entries_read > self.manifest.entries.len() + 1 {
            return Err(Refusal::TooManyEntries);
        }
        if path == IN_ARCHIVE_NAME {
            if self.manifest_seen {
                return Err(Refusal::Duplicate);
            }
            if kind != TarKind::File || size > MAX_MANIFEST_BYTES {
                return Err(Refusal::WrongType);
            }
            self.manifest_seen = true;
            return Ok(Accepted::Manifest);
        }
        let index = *self.by_path.get(path).ok_or(Refusal::UnknownEntry)?;
        let entry = &self.manifest.entries[index];
        let expected = match entry.kind {
            EntryType::File => TarKind::File,
            EntryType::Dir => TarKind::Dir,
            EntryType::Symlink => TarKind::Symlink,
        };
        if expected != kind {
            return Err(Refusal::WrongType);
        }
        if self.seen[index] {
            return Err(Refusal::Duplicate);
        }
        // Un fichier annonce sa taille ; un dossier et un lien n'ont aucun contenu.
        let expected_size = if entry.kind == EntryType::File {
            entry.size
        } else {
            0
        };
        if size != expected_size {
            return Err(Refusal::SizeMismatch);
        }
        self.bytes = self.bytes.saturating_add(size);
        if self.bytes > self.manifest.totals.bytes {
            return Err(Refusal::TooManyBytes);
        }
        self.seen[index] = true;
        self.seen_count += 1;
        Ok(Accepted::Entry(index))
    }

    /// Toute ligne du manifeste a-t-elle son entrée ?
    pub fn finish(&self) -> Result<(), Refusal> {
        if self.seen_count == self.manifest.entries.len() {
            Ok(())
        } else {
            Err(Refusal::Missing)
        }
    }

    /// L'archive contenait-elle son manifeste ?
    pub fn saw_manifest(&self) -> bool {
        self.manifest_seen
    }
}

#[cfg(test)]
mod tests {
    use super::super::manifest::{Entry, FORMAT, ManifestRoot, Totals};
    use super::*;
    use hearth_proto::api::backups::BackupMode;

    fn entry(path: &str, kind: EntryType, size: u64) -> Entry {
        Entry {
            root: 0,
            path: path.into(),
            kind,
            size,
            sha256: (kind == EntryType::File).then(|| "a".repeat(64)),
            mode: 0o644,
            uid: 0,
            gid: 0,
            mtime_ns: 0,
            nlink: 1,
            link_target: (kind == EntryType::Symlink).then(|| "x".to_owned()),
            changed: false,
        }
    }

    fn manifest() -> Manifest {
        let entries = vec![
            entry("", EntryType::Dir, 0),
            entry("a.txt", EntryType::File, 5),
            entry("l", EntryType::Symlink, 0),
        ];
        Manifest {
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
            totals: Totals {
                files: 1,
                dirs: 1,
                symlinks: 1,
                bytes: 5,
            },
            entries,
            archive: None,
        }
    }

    #[test]
    fn an_archive_that_matches_its_manifest_is_accepted_to_the_last_line() {
        let manifest = manifest();
        assert_eq!(manifest.check(), Ok(()));
        let mut index = ArchiveIndex::new(&manifest);
        assert_eq!(
            index.accept("data/", TarKind::Dir, 0),
            Ok(Accepted::Entry(0))
        );
        assert_eq!(
            index.accept("data/a.txt", TarKind::File, 5),
            Ok(Accepted::Entry(1))
        );
        assert_eq!(
            index.accept("data/l", TarKind::Symlink, 0),
            Ok(Accepted::Entry(2))
        );
        assert_eq!(
            index.accept(IN_ARCHIVE_NAME, TarKind::File, 1000),
            Ok(Accepted::Manifest)
        );
        assert_eq!(index.finish(), Ok(()));
        assert!(index.saw_manifest());
    }

    #[test]
    fn an_entry_without_a_manifest_line_is_refused() {
        let manifest = manifest();
        let mut index = ArchiveIndex::new(&manifest);
        assert_eq!(
            index.accept("data/extra.txt", TarKind::File, 1),
            Err(Refusal::UnknownEntry)
        );
        // Un chemin qui sort de l'archive n'est même pas comparé.
        for bad in [
            "../data/a.txt",
            "/data/a.txt",
            "data/../a.txt",
            "data\\a.txt",
            "",
        ] {
            let mut index = ArchiveIndex::new(&manifest);
            assert_eq!(
                index.accept(bad, TarKind::File, 5),
                Err(Refusal::BadPath),
                "{bad}"
            );
        }
    }

    #[test]
    fn a_type_that_is_not_a_file_a_folder_or_a_link_is_refused() {
        let manifest = manifest();
        let mut index = ArchiveIndex::new(&manifest);
        assert_eq!(
            index.accept("data/a.txt", TarKind::Other, 5),
            Err(Refusal::UnsupportedType)
        );
    }

    #[test]
    fn a_wrong_type_a_wrong_size_and_a_duplicate_are_refused() {
        let manifest = manifest();
        let mut index = ArchiveIndex::new(&manifest);
        assert_eq!(
            index.accept("data/a.txt", TarKind::Dir, 0),
            Err(Refusal::WrongType)
        );
        let mut index = ArchiveIndex::new(&manifest);
        assert_eq!(
            index.accept("data/a.txt", TarKind::File, 6),
            Err(Refusal::SizeMismatch)
        );
        let mut index = ArchiveIndex::new(&manifest);
        assert!(index.accept("data/a.txt", TarKind::File, 5).is_ok());
        assert_eq!(
            index.accept("data/a.txt", TarKind::File, 5),
            Err(Refusal::Duplicate)
        );
    }

    #[test]
    fn a_missing_line_makes_the_archive_corrupt() {
        let manifest = manifest();
        let mut index = ArchiveIndex::new(&manifest);
        assert!(index.accept("data/a.txt", TarKind::File, 5).is_ok());
        assert_eq!(index.finish(), Err(Refusal::Missing));
    }

    #[test]
    fn more_entries_than_the_manifest_is_a_bomb() {
        let manifest = manifest();
        let mut index = ArchiveIndex::new(&manifest);
        assert!(index.accept("data/", TarKind::Dir, 0).is_ok());
        assert!(index.accept("data/a.txt", TarKind::File, 5).is_ok());
        assert!(index.accept("data/l", TarKind::Symlink, 0).is_ok());
        assert!(index.accept(IN_ARCHIVE_NAME, TarKind::File, 1).is_ok());
        assert_eq!(
            index.accept("data/another", TarKind::File, 1),
            Err(Refusal::TooManyEntries)
        );
    }

    #[test]
    fn the_manifest_entry_comes_once_and_is_a_file() {
        let manifest = manifest();
        let mut index = ArchiveIndex::new(&manifest);
        assert_eq!(
            index.accept(IN_ARCHIVE_NAME, TarKind::Dir, 0),
            Err(Refusal::WrongType)
        );
        let mut index = ArchiveIndex::new(&manifest);
        assert!(index.accept(IN_ARCHIVE_NAME, TarKind::File, 1).is_ok());
        assert_eq!(
            index.accept(IN_ARCHIVE_NAME, TarKind::File, 1),
            Err(Refusal::Duplicate)
        );
    }
}

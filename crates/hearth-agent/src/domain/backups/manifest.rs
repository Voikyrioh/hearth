//! Le manifeste d'une sauvegarde (BR-BACKUP-012, BR-BACKUP-014, ADR-0047).
//!
//! Un fichier JSON lisible hors de Hearth : une ligne par entrée (chemin, type, taille, SHA-256, droits,
//! propriétaire, dates, `changed`), les totaux, l'image et le mode. Il est écrit **à côté** de l'archive
//! (`<id>.manifest.json`, avec l'empreinte et la taille de l'archive) et **en dernière entrée** de l'archive
//! (`.hearth-manifest.json`, sans l'empreinte de l'archive qui ne peut pas se contenir elle-même).
//!
//! **La note du compte n'y est jamais** : elle vit chiffrée dans le registre (BR-BACKUP-012). Le manifeste
//! voyage avec l'archive téléchargée, la note ne doit pas le suivre.
//!
//! Le manifeste d'une archive qu'on relit est cru **seulement** si l'empreinte du fichier voisin est celle du
//! registre (`ownership`) ; sa cohérence interne est jugée par [`Manifest::check`] avant de s'en servir.

use std::collections::HashSet;

use hearth_proto::api::backups::{BackupFlag, BackupMode};
use serde::{Deserialize, Serialize};

use crate::domain::files::{MAX_COMPONENT_BYTES, MAX_DEPTH, RelPath};

/// La version du format du manifeste.
pub const FORMAT: u32 = 1;
/// Le nom du manifeste dans l'archive.
pub const IN_ARCHIVE_NAME: &str = ".hearth-manifest.json";
/// Au plus ce nombre d'entrées : un manifeste plus gros est refusé, jamais lu en entier (bombe).
pub const MAX_ENTRIES: usize = 20_000_000;
/// Taille maximale du fichier de manifeste lu (le manifeste d'un monde de millions de fichiers pèse des
/// centaines de Mio) : 1 Gio. Il est lu en mémoire pour être jugé ; au-delà, la sauvegarde n'est pas vérifiable.
pub const MAX_MANIFEST_BYTES: u64 = 1024 * 1024 * 1024;

/// Pourquoi un manifeste n'est pas cru.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ManifestError {
    #[error("format inconnu")]
    Format,
    #[error("trop d'entrées")]
    TooMany,
    #[error("dossier de l'archive invalide ou en double")]
    Roots,
    #[error("chemin invalide")]
    Path,
    #[error("entrée en double")]
    Duplicate,
    #[error("empreinte absente ou mal formée")]
    Digest,
    #[error("totaux incohérents")]
    Totals,
    #[error("lien invalide")]
    Link,
}

/// Un dossier du service dans l'archive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestRoot {
    pub id: u32,
    /// Où il est monté dans le service.
    pub target: String,
    /// Son nom dans l'archive (un composant).
    pub dir: String,
    /// Un dossier géré par Hearth (sous la racine de données), par opposition à un dossier du serveur.
    pub managed: bool,
}

/// Un élément ignoré, par le rang du dossier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestIgnored {
    pub root: u32,
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryType {
    File,
    Dir,
    Symlink,
}

/// Une entrée de l'archive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub root: u32,
    /// Relatif au dossier, `/` pour séparateur ; vide : le dossier lui-même.
    pub path: String,
    #[serde(rename = "type")]
    pub kind: EntryType,
    /// Taille annoncée dans l'archive (un fichier qui a grossi pendant la copie est coupé à sa taille de départ).
    pub size: u64,
    /// SHA-256 hexadécimal des octets archivés (un fichier).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub mtime_ns: i64,
    pub nlink: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link_target: Option<String>,
    /// La taille ou la date a changé pendant la lecture (copie à chaud), ou le fichier a disparu en route.
    #[serde(default)]
    pub changed: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Totals {
    pub files: u64,
    pub dirs: u64,
    pub symlinks: u64,
    /// Somme des tailles des fichiers.
    pub bytes: u64,
}

/// L'archive vue de dehors : son nom, sa taille, son empreinte. Absent du manifeste que l'archive contient.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveFacts {
    pub name: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: u32,
    pub backup_id: String,
    pub service_id: String,
    pub service_name: String,
    /// RFC 3339 en UTC.
    pub created_at: String,
    pub agent_version: String,
    /// `dépôt:version` de l'image du service à cet instant.
    pub image: String,
    pub mode: BackupMode,
    #[serde(default)]
    pub flags: Vec<BackupFlag>,
    pub roots: Vec<ManifestRoot>,
    #[serde(default)]
    pub ignored: Vec<ManifestIgnored>,
    pub entries: Vec<Entry>,
    pub totals: Totals,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive: Option<ArchiveFacts>,
}

/// Le chemin d'une entrée dans l'archive : `<dir>/<chemin>` (`<dir>` seul pour le dossier lui-même).
pub fn tar_path(dir: &str, path: &str) -> String {
    if path.is_empty() {
        dir.to_owned()
    } else {
        format!("{dir}/{path}")
    }
}

fn is_hex_sha256(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

impl Manifest {
    /// Le nombre de fichiers dont le contenu a changé pendant la copie.
    pub fn changed_count(&self) -> u32 {
        u32::try_from(self.entries.iter().filter(|entry| entry.changed).count()).unwrap_or(u32::MAX)
    }

    /// Les totaux recalculés depuis les entrées.
    pub fn recount(entries: &[Entry]) -> Totals {
        let mut totals = Totals::default();
        for entry in entries {
            match entry.kind {
                EntryType::File => {
                    totals.files += 1;
                    totals.bytes = totals.bytes.saturating_add(entry.size);
                }
                EntryType::Dir => totals.dirs += 1,
                EntryType::Symlink => totals.symlinks += 1,
            }
        }
        totals
    }

    /// Le manifeste est-il cohérent ? Format connu, entrées bornées, dossiers uniques et sûrs, chemins valides
    /// et sans doublon, empreinte sur chaque fichier et sur lui seul, totaux justes. Ne dit rien de l'archive.
    pub fn check(&self) -> Result<(), ManifestError> {
        if self.format != FORMAT {
            return Err(ManifestError::Format);
        }
        if self.entries.len() > MAX_ENTRIES {
            return Err(ManifestError::TooMany);
        }
        let mut dirs = HashSet::new();
        let mut ids = HashSet::new();
        for root in &self.roots {
            if RelPath::check_component(&root.dir).is_err()
                || root.dir.len() > MAX_COMPONENT_BYTES
                || root.dir == IN_ARCHIVE_NAME
                || !dirs.insert(root.dir.as_str())
                || !ids.insert(root.id)
            {
                return Err(ManifestError::Roots);
            }
        }
        let mut seen: HashSet<(u32, &str)> = HashSet::with_capacity(self.entries.len());
        for entry in &self.entries {
            if !ids.contains(&entry.root) {
                return Err(ManifestError::Roots);
            }
            if !entry.path.is_empty() {
                let rel = RelPath::parse(&entry.path).map_err(|_| ManifestError::Path)?;
                if rel.depth() > MAX_DEPTH {
                    return Err(ManifestError::Path);
                }
            } else if entry.kind != EntryType::Dir {
                return Err(ManifestError::Path);
            }
            if !seen.insert((entry.root, entry.path.as_str())) {
                return Err(ManifestError::Duplicate);
            }
            match (entry.kind, entry.sha256.as_deref()) {
                (EntryType::File, Some(sha)) if is_hex_sha256(sha) => {}
                (EntryType::File, _) => return Err(ManifestError::Digest),
                (_, Some(_)) => return Err(ManifestError::Digest),
                (_, None) => {}
            }
            match (entry.kind, entry.link_target.as_deref()) {
                (EntryType::Symlink, Some(target))
                    if !target.is_empty() && target.len() <= 4096 => {}
                (EntryType::Symlink, _) => return Err(ManifestError::Link),
                (_, Some(_)) => return Err(ManifestError::Link),
                (_, None) => {}
            }
        }
        if self.totals != Self::recount(&self.entries) {
            return Err(ManifestError::Totals);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha(byte: char) -> String {
        std::iter::repeat_n(byte, 64).collect()
    }

    fn file(path: &str, size: u64) -> Entry {
        Entry {
            root: 0,
            path: path.into(),
            kind: EntryType::File,
            size,
            sha256: Some(sha('a')),
            mode: 0o644,
            uid: 0,
            gid: 0,
            mtime_ns: 1,
            nlink: 1,
            link_target: None,
            changed: false,
        }
    }

    fn dir(path: &str) -> Entry {
        Entry {
            kind: EntryType::Dir,
            sha256: None,
            size: 0,
            ..file(path, 0)
        }
    }

    fn manifest(entries: Vec<Entry>) -> Manifest {
        Manifest {
            format: FORMAT,
            backup_id: "01J9ZY0G3Q8M2K6W4T7V5N1B9D".into(),
            service_id: "h_01J9ZY0G3Q8M2K6W4T7V5N1B9D".into(),
            service_name: "terraria".into(),
            created_at: "2026-11-14T10:00:00Z".into(),
            agent_version: "0.1.0".into(),
            image: "x:1".into(),
            mode: BackupMode::Hot,
            flags: vec![BackupFlag::Hot],
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
        }
    }

    #[test]
    fn a_consistent_manifest_passes_and_round_trips_through_json() {
        let manifest = manifest(vec![dir(""), dir("world"), file("world/a.wld", 10)]);
        assert_eq!(manifest.check(), Ok(()));
        assert_eq!(manifest.totals.files, 1);
        assert_eq!(manifest.totals.dirs, 2);
        assert_eq!(manifest.totals.bytes, 10);
        let json = serde_json::to_string(&manifest).expect("json");
        assert!(!json.contains("note"), "la note n'est jamais au manifeste");
        let back: Manifest = serde_json::from_str(&json).expect("lu");
        assert_eq!(back, manifest);
    }

    #[test]
    fn a_path_that_climbs_a_duplicate_and_a_missing_digest_are_refused() {
        for (entries, error) in [
            (vec![file("../etc/passwd", 1)], ManifestError::Path),
            (vec![file("/abs", 1)], ManifestError::Path),
            (vec![file("a", 1), file("a", 1)], ManifestError::Duplicate),
            (
                vec![Entry {
                    sha256: None,
                    ..file("a", 1)
                }],
                ManifestError::Digest,
            ),
            (
                vec![Entry {
                    sha256: Some("zz".into()),
                    ..file("a", 1)
                }],
                ManifestError::Digest,
            ),
            // Le dossier lui-même est un dossier, jamais un fichier.
            (vec![file("", 1)], ManifestError::Path),
        ] {
            assert_eq!(manifest(entries).check(), Err(error));
        }
    }

    #[test]
    fn totals_that_do_not_match_the_entries_are_refused() {
        let mut manifest = manifest(vec![file("a", 5)]);
        manifest.totals.bytes = 6;
        assert_eq!(manifest.check(), Err(ManifestError::Totals));
    }

    #[test]
    fn a_link_needs_a_target_and_nothing_else_has_one() {
        let link = Entry {
            kind: EntryType::Symlink,
            sha256: None,
            link_target: Some("a".into()),
            ..file("l", 0)
        };
        assert_eq!(manifest(vec![link.clone()]).check(), Ok(()));
        let bare = Entry {
            link_target: None,
            ..link
        };
        assert_eq!(manifest(vec![bare]).check(), Err(ManifestError::Link));
        let odd = Entry {
            link_target: Some("x".into()),
            ..file("a", 1)
        };
        assert_eq!(manifest(vec![odd]).check(), Err(ManifestError::Link));
    }

    #[test]
    fn two_roots_cannot_share_a_directory_name_in_the_archive() {
        let mut manifest = manifest(vec![]);
        manifest.roots.push(ManifestRoot {
            id: 1,
            target: "/other".into(),
            dir: "data".into(),
            managed: true,
        });
        assert_eq!(manifest.check(), Err(ManifestError::Roots));
    }

    #[test]
    fn a_root_directory_cannot_hide_the_manifest_entry() {
        let mut manifest = manifest(vec![]);
        manifest.roots[0].dir = IN_ARCHIVE_NAME.into();
        assert_eq!(manifest.check(), Err(ManifestError::Roots));
    }

    #[test]
    fn the_tar_path_joins_the_directory_name_and_the_path() {
        assert_eq!(tar_path("data", ""), "data");
        assert_eq!(tar_path("data", "world/a.wld"), "data/world/a.wld");
    }

    #[test]
    fn changed_files_are_counted() {
        let mut changed = file("b", 1);
        changed.changed = true;
        assert_eq!(manifest(vec![file("a", 1), changed]).changed_count(), 1);
    }
}

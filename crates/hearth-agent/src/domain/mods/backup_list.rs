//! La liste des mods d'une sauvegarde et sa comparaison avec les mods actuels (HRT-293, BR-MOD-034, BR-MOD-036) :
//! domaine pur, aucune E/S.
//!
//! La liste décrit EXACTEMENT ce que l'archive contient : le manifeste de la sauvegarde est la vérité de ce qui a été
//! copié. Une lecture des mods faite juste avant la copie n'est gardée pour un fichier que si son chemin ET son
//! empreinte SHA-256 sont ceux du manifeste ; sinon le fichier y figure par son nom seulement, « non reconnu dans
//! cette sauvegarde », jamais avec une version devinée.

use super::game_info::Origin;
use super::jar::{Environment, JarLoader};
use super::listing;

/// Au plus ce nombre de fichiers gardés par dossier (BR-MOD-020) ; le reste est compté (`not_listed`).
pub const MAX_PER_FOLDER: usize = 1_024;

/// Où est un fichier d'une sauvegarde.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Active,
    Disabled,
    Retired,
}

/// Ce qu'un mod dit de lui-même (déjà passé par une forme : `domain::mods::jar`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentitySummary {
    pub mod_id: String,
    pub name: Option<String>,
    pub version: Option<String>,
    pub loader: JarLoader,
    pub environment: Environment,
}

/// Un fichier tel que la lecture des mods l'a vu juste avant la copie.
#[derive(Debug, Clone)]
pub struct SnapshotFile {
    /// Le nom réel (`.jar` ou `.jar.disabled`).
    pub name: String,
    pub retired: bool,
    pub size: u64,
    /// L'empreinte SHA-256 du fichier lu ; `None` : pas lu (budget épuisé, lien, archive refusée).
    pub sha256: Option<String>,
    pub identity: Option<IdentitySummary>,
    pub origin: Origin,
    /// Pas lu faute de temps.
    pub unread: bool,
}

/// La lecture des mods prise avant la copie.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub loader: Option<JarLoader>,
    pub minecraft: Option<String>,
    pub files: Vec<SnapshotFile>,
}

/// Une entrée du manifeste de la sauvegarde sous le dossier géré qui porte les mods.
#[derive(Debug, Clone)]
pub struct ManifestFile {
    /// Le chemin relatif au dossier géré (`mods/x.jar`, `mods-retires/y.jar.disabled`).
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetState {
    Recorded,
    Partial,
    Unreadable,
    NotInBackup,
}

/// Une entrée de la liste d'une sauvegarde.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub rank: u32,
    /// `None` : nom hors de l'alphabet sûr (montré par son rang).
    pub file_name: Option<String>,
    pub place: Place,
    pub identity: Option<IdentitySummary>,
    pub origin: Origin,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedSet {
    pub state: SetState,
    pub loader: Option<JarLoader>,
    pub minecraft: Option<String>,
    pub active: u32,
    pub disabled: u32,
    pub retired: u32,
    pub not_listed: u32,
    pub entries: Vec<Entry>,
}

impl RecordedSet {
    fn bare(state: SetState) -> Self {
        Self {
            state,
            loader: None,
            minecraft: None,
            active: 0,
            disabled: 0,
            retired: 0,
            not_listed: 0,
            entries: Vec::new(),
        }
    }

    /// Une lecture qui a échoué : la sauvegarde réussit, sa liste le dit (BR-MOD-034).
    pub fn unreadable() -> Self {
        Self::bare(SetState::Unreadable)
    }

    /// Le dossier des mods est un élément ignoré : rien n'en est dans la sauvegarde.
    pub fn not_in_backup() -> Self {
        Self::bare(SetState::NotInBackup)
    }
}

/// Le dossier et le fichier d'une entrée de manifeste : `mods/<nom>` ou `mods-retires/<nom>`, un `.jar` ou un
/// `.jar.disabled`. Tout autre chemin n'est pas un mod.
fn classify<'a>(path: &'a str, dir: &str, retired_dir: &str) -> Option<(Place, &'a str)> {
    let (folder, name) = path.split_once('/')?;
    if name.is_empty() || name.contains('/') {
        return None;
    }
    let retired = if folder == dir {
        false
    } else if folder == retired_dir {
        true
    } else {
        return None;
    };
    if !(listing::is_active_jar(name) || listing::is_disabled_jar(name)) {
        return None;
    }
    let place = if retired {
        Place::Retired
    } else if listing::is_disabled_jar(name) {
        Place::Disabled
    } else {
        Place::Active
    };
    Some((place, name))
}

/// Recoupe la lecture avec le manifeste (BR-MOD-034). Une entrée de la lecture absente du manifeste (fichier disparu
/// pendant la copie) n'entre pas.
pub fn reconcile(
    snapshot: &Snapshot,
    manifest: &[ManifestFile],
    dir: &str,
    retired_dir: &str,
) -> RecordedSet {
    let mut set = RecordedSet::bare(SetState::Recorded);
    set.loader = snapshot.loader;
    set.minecraft = snapshot.minecraft.clone();
    let mut per_folder = [0usize; 2];
    let mut partial = false;
    for file in manifest {
        let Some((place, name)) = classify(&file.path, dir, retired_dir) else {
            continue;
        };
        let folder = usize::from(place == Place::Retired);
        if per_folder[folder] >= MAX_PER_FOLDER {
            set.not_listed += 1;
            continue;
        }
        per_folder[folder] += 1;
        let safe = listing::is_safe_file_name(name);
        let seen = safe
            .then(|| {
                snapshot.files.iter().find(|seen| {
                    seen.name == name
                        && seen.retired == (place == Place::Retired)
                        && seen
                            .sha256
                            .as_deref()
                            .is_some_and(|sha| sha.eq_ignore_ascii_case(&file.sha256))
                })
            })
            .flatten();
        // Un fichier de la sauvegarde que la lecture n'a pas pu lire faute de temps rend la liste partielle.
        if safe
            && snapshot.files.iter().any(|seen| {
                seen.name == name && seen.retired == (place == Place::Retired) && seen.unread
            })
        {
            partial = true;
        }
        let (identity, origin) = match seen {
            Some(seen) => (seen.identity.clone(), seen.origin),
            None => (None, Origin::Manual),
        };
        match place {
            Place::Active => set.active += 1,
            Place::Disabled => set.disabled += 1,
            Place::Retired => set.retired += 1,
        }
        set.entries.push(Entry {
            rank: u32::try_from(set.entries.len()).unwrap_or(u32::MAX),
            file_name: safe.then(|| name.to_owned()),
            place,
            identity,
            origin,
            size: file.size,
            sha256: file.sha256.clone(),
        });
    }
    if partial {
        set.state = SetState::Partial;
    }
    set
}

// ---------------------------------------------------------------------------------------------------------------
// La comparaison avec les mods actuels (BR-MOD-036)
// ---------------------------------------------------------------------------------------------------------------

/// Un mod actuel : de quoi le mettre en regard d'une entrée de la sauvegarde.
#[derive(Debug, Clone)]
pub struct CurrentMod {
    pub file_name: Option<String>,
    pub place: Place,
    pub identity: Option<IdentitySummary>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    /// Présent aujourd'hui, absent de la sauvegarde.
    Added,
    /// Dans la sauvegarde, absent aujourd'hui ou mis dans « Mods retirés ».
    Removed,
    VersionChanged,
    /// Désactivé dans la sauvegarde, activé aujourd'hui.
    Enabled,
    /// Activé dans la sauvegarde, désactivé aujourd'hui.
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub kind: ChangeKind,
    pub mod_id: Option<String>,
    pub name: Option<String>,
    pub file_name: Option<String>,
    pub backup_version: Option<String>,
    pub current_version: Option<String>,
}

/// Un mod sans identité se reconnaît par son nom de fichier sans `.disabled` ; un nom illisible ne se compare pas.
fn key_of(identity: Option<&IdentitySummary>, file_name: Option<&str>) -> Option<String> {
    match (identity, file_name) {
        (Some(identity), _) => Some(format!("id:{}", identity.mod_id)),
        (None, Some(name)) => Some(format!(
            "file:{}",
            listing::active_name(name).to_ascii_lowercase()
        )),
        (None, None) => None,
    }
}

struct Side<'a> {
    key: String,
    enabled: bool,
    identity: Option<&'a IdentitySummary>,
    file_name: Option<&'a str>,
}

fn sides<'a>(
    items: impl Iterator<Item = (Place, Option<&'a IdentitySummary>, Option<&'a str>)>,
) -> Vec<Side<'a>> {
    let mut out: Vec<Side<'a>> = Vec::new();
    for (place, identity, file_name) in items {
        // « Mods retirés » compte comme absent.
        if place == Place::Retired {
            continue;
        }
        let Some(key) = key_of(identity, file_name) else {
            continue;
        };
        let enabled = place == Place::Active;
        // Deux fichiers du même mod : celui qui est activé fait foi.
        if let Some(existing) = out.iter_mut().find(|side| side.key == key) {
            if enabled && !existing.enabled {
                *existing = Side {
                    key,
                    enabled,
                    identity,
                    file_name,
                };
            }
            continue;
        }
        out.push(Side {
            key,
            enabled,
            identity,
            file_name,
        });
    }
    out
}

/// Met en regard la liste de la sauvegarde et les mods actuels, mod par mod (BR-MOD-036).
pub fn compare(recorded: &RecordedSet, current: &[CurrentMod]) -> Vec<Change> {
    let before = sides(
        recorded
            .entries
            .iter()
            .map(|e| (e.place, e.identity.as_ref(), e.file_name.as_deref())),
    );
    let after = sides(
        current
            .iter()
            .map(|c| (c.place, c.identity.as_ref(), c.file_name.as_deref())),
    );
    let describe = |side: &Side<'_>| {
        (
            side.identity.map(|i| i.mod_id.clone()),
            side.identity.and_then(|i| i.name.clone()),
            side.file_name.map(str::to_owned),
        )
    };
    let version = |side: &Side<'_>| side.identity.and_then(|i| i.version.clone());
    let mut changes = Vec::new();
    for old in &before {
        match after.iter().find(|now| now.key == old.key) {
            None => {
                let (mod_id, name, file_name) = describe(old);
                changes.push(Change {
                    kind: ChangeKind::Removed,
                    mod_id,
                    name,
                    file_name,
                    backup_version: version(old),
                    current_version: None,
                });
            }
            Some(now) => {
                let kind = if version(old) != version(now) {
                    Some(ChangeKind::VersionChanged)
                } else if old.enabled && !now.enabled {
                    Some(ChangeKind::Disabled)
                } else if !old.enabled && now.enabled {
                    Some(ChangeKind::Enabled)
                } else {
                    None
                };
                if let Some(kind) = kind {
                    let (mod_id, name, file_name) = describe(now);
                    changes.push(Change {
                        kind,
                        mod_id,
                        name,
                        file_name,
                        backup_version: version(old),
                        current_version: version(now),
                    });
                }
            }
        }
    }
    for now in &after {
        if !before.iter().any(|old| old.key == now.key) {
            let (mod_id, name, file_name) = describe(now);
            changes.push(Change {
                kind: ChangeKind::Added,
                mod_id,
                name,
                file_name,
                backup_version: None,
                current_version: version(now),
            });
        }
    }
    changes
}

#[cfg(test)]
mod tests;

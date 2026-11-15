//! Les types que l'interface lit pour reprendre un monde existant (HRT-106, `BR-MONDE-002` à `016`, `039`). Aucune
//! règle ici : la liste et l'inspection viennent de l'agent telles quelles, l'interface les met en phrases. Un nom de
//! monde ou un dossier est un texte du serveur : l'interface le rend inoffensif avant de l'afficher, et ni l'un ni
//! l'autre n'entre dans une trace. Types simples pour `specta` (pas de `u64`).

use hearth_link::{InspectOutcome, WorldsOutcome};
use hearth_proto::api::deploy::WorldPick;
use hearth_proto::api::worlds::{WorldEntry, WorldInspection, WorldRef, WorldRoot, WorldsView};
use serde::{Deserialize, Serialize};
use specta::Type;

/// Un monde trouvé.
#[derive(Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorldEntryDto {
    pub path: String,
    pub name: String,
    pub files: Option<f64>,
    pub bytes: Option<f64>,
    pub modified_at: Option<String>,
    pub usable: bool,
}

impl std::fmt::Debug for WorldEntryDto {
    /// Jamais un nom ni un chemin (BR-MONDE-023).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WorldEntryDto(..)")
    }
}

/// Un dossier ouvert aux services et ce que la recherche y a trouvé.
#[derive(Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorldRootDto {
    pub folder: String,
    pub readable: bool,
    pub truncated: bool,
    pub worlds: Vec<WorldEntryDto>,
}

impl std::fmt::Debug for WorldRootDto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WorldRootDto(..)")
    }
}

/// La liste des mondes de la fiche.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorldsViewDto {
    pub roots: Vec<WorldRootDto>,
    pub max_depth: u32,
    pub max_entries: u32,
}

/// Ce que la lecture a donné : la liste, ou « cette fiche ne dit pas ce qu'est un monde » (`BR-MONDE-039`).
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WorldsReadDto {
    Ready { view: WorldsViewDto },
    NotSupported,
}

/// Le monde désigné : un dossier ouvert et un chemin relatif, jamais un chemin absolu.
#[derive(Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorldRefDto {
    pub folder: String,
    pub path: String,
}

impl std::fmt::Debug for WorldRefDto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WorldRefDto(..)")
    }
}

impl From<WorldRefDto> for WorldRef {
    fn from(dto: WorldRefDto) -> Self {
        Self {
            folder: dto.folder,
            path: dto.path,
        }
    }
}

/// Le monde choisi pour l'installation et les deux gestes qui l'accompagnent (`BR-MONDE-009`, `016`).
#[derive(Clone, PartialEq, Eq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorldPickDto {
    pub folder: String,
    pub path: String,
    pub version_understood: bool,
    pub keep_if_changed: bool,
}

impl std::fmt::Debug for WorldPickDto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WorldPickDto(..)")
    }
}

impl From<WorldPickDto> for WorldPick {
    fn from(dto: WorldPickDto) -> Self {
        Self {
            world: WorldRef {
                folder: dto.folder,
                path: dto.path,
            },
            version_understood: dto.version_understood,
            keep_if_changed: dto.keep_if_changed,
        }
    }
}

/// Ce que l'inspection d'un monde donne.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorldInspectionDto {
    pub files: f64,
    pub bytes: f64,
    pub not_copied: f64,
    pub not_copied_names: Vec<String>,
    pub skipped_links: f64,
    pub skipped_link_names: Vec<String>,
    pub partial: bool,
    pub free_bytes: Option<f64>,
}

/// Pourquoi l'agent refuse un monde (`details.reason`) ; un code que ce client ne connaît pas se lit `Unknown`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum WorldRefusalDto {
    UnknownFolder,
    BadPath,
    NotAWorld,
    NotFound,
    TooBig,
    NameUnusable,
    Unreadable,
    Unknown,
}

impl WorldRefusalDto {
    pub fn from_wire(reason: Option<&str>) -> Self {
        match reason {
            Some("unknown_folder") => Self::UnknownFolder,
            Some("bad_path") => Self::BadPath,
            Some("not_a_world") => Self::NotAWorld,
            Some("not_found") => Self::NotFound,
            Some("too_big") => Self::TooBig,
            Some("name_unusable") => Self::NameUnusable,
            Some("unreadable") => Self::Unreadable,
            _ => Self::Unknown,
        }
    }
}

/// Ce que l'inspection a donné : les chiffres, ou le refus du monde (pas une panne).
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WorldInspectReadDto {
    Ready { inspection: WorldInspectionDto },
    Refused { reason: WorldRefusalDto },
}

fn entry_dto(entry: &WorldEntry) -> WorldEntryDto {
    WorldEntryDto {
        path: entry.path.clone(),
        name: entry.name.clone(),
        files: entry.files.map(|n| n as f64),
        bytes: entry.bytes.map(|n| n as f64),
        modified_at: entry.modified_at.clone(),
        usable: entry.usable,
    }
}

fn root_dto(root: &WorldRoot) -> WorldRootDto {
    WorldRootDto {
        folder: root.folder.clone(),
        readable: root.readable,
        truncated: root.truncated,
        worlds: root.worlds.iter().map(entry_dto).collect(),
    }
}

impl From<&WorldsView> for WorldsViewDto {
    fn from(view: &WorldsView) -> Self {
        Self {
            roots: view.roots.iter().map(root_dto).collect(),
            max_depth: view.max_depth,
            max_entries: view.max_entries,
        }
    }
}

impl From<&WorldInspection> for WorldInspectionDto {
    fn from(view: &WorldInspection) -> Self {
        Self {
            files: view.files as f64,
            bytes: view.bytes as f64,
            not_copied: view.not_copied as f64,
            not_copied_names: view.not_copied_names.clone(),
            skipped_links: view.skipped_links as f64,
            skipped_link_names: view.skipped_link_names.clone(),
            partial: view.partial,
            free_bytes: view.free_bytes.map(|n| n as f64),
        }
    }
}

impl From<&WorldsOutcome> for WorldsReadDto {
    fn from(outcome: &WorldsOutcome) -> Self {
        match outcome {
            WorldsOutcome::Ready(view) => Self::Ready {
                view: WorldsViewDto::from(view),
            },
            WorldsOutcome::NotSupported => Self::NotSupported,
        }
    }
}

impl From<&InspectOutcome> for WorldInspectReadDto {
    fn from(outcome: &InspectOutcome) -> Self {
        match outcome {
            InspectOutcome::Ready(view) => Self::Ready {
                inspection: WorldInspectionDto::from(view),
            },
            InspectOutcome::Refused { reason } => Self::Refused {
                reason: WorldRefusalDto::from_wire(reason.as_deref()),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_world_never_shows_its_name_or_folder_in_debug() {
        let entry = WorldEntryDto {
            path: "SENTINEL/Alpha.wld".into(),
            name: "SENTINEL".into(),
            files: None,
            bytes: None,
            modified_at: None,
            usable: true,
        };
        let root = WorldRootDto {
            folder: "/srv/SENTINEL".into(),
            readable: true,
            truncated: false,
            worlds: vec![entry.clone()],
        };
        let pick = WorldPickDto {
            folder: "/srv/SENTINEL".into(),
            path: "SENTINEL".into(),
            version_understood: true,
            keep_if_changed: false,
        };
        let by_ref = WorldRefDto {
            folder: "SENTINEL".into(),
            path: "SENTINEL".into(),
        };
        for shown in [
            format!("{entry:?}"),
            format!("{root:?}"),
            format!("{pick:?}"),
            format!("{by_ref:?}"),
        ] {
            assert!(!shown.contains("SENTINEL"), "{shown}");
        }
    }

    #[test]
    fn a_refusal_code_this_client_does_not_know_reads_as_unknown() {
        assert_eq!(
            WorldRefusalDto::from_wire(Some("not_a_world")),
            WorldRefusalDto::NotAWorld
        );
        assert_eq!(
            WorldRefusalDto::from_wire(Some("from_the_future")),
            WorldRefusalDto::Unknown
        );
        assert_eq!(WorldRefusalDto::from_wire(None), WorldRefusalDto::Unknown);
    }
}

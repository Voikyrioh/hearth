//! Reprendre un monde existant (HRT-105, BR-MONDE-002 à 026) : `GET /catalog/{id}/worlds` (les mondes que la
//! fiche reconnaît dans les dossiers ouverts aux services) et `POST /catalog/{id}/worlds/inspect` (ce qu'une
//! reprise copierait, ce qu'elle laisserait). Annoncés par la capacité [`CAPABILITY_WORLD_IMPORT`]. Installer
//! avec un monde passe par `POST /deployments` (`DeploySource::Fiche.world`).
//!
//! Ce sont des **consultations** : ni diffusées, ni journalisées (BR-MONDE-003). Elles exigent le droit « gérer
//! les services ». Un monde est désigné par le dossier ouvert tel qu'il est écrit dans la configuration de
//! l'agent et par un chemin relatif à ce dossier : jamais un chemin absolu.
//!
//! Évolution : tout champ ajouté plus tard est facultatif (`serde(default)`).

use serde::{Deserialize, Serialize};

/// Capacité annoncée par `GET /hello` quand l'agent sait reprendre un monde.
pub const CAPABILITY_WORLD_IMPORT: &str = "world-import";

/// Un monde trouvé.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldEntry {
    /// Chemin relatif au dossier ouvert (composants séparés par `/`).
    pub path: String,
    /// Le nom du monde (le nom du fichier sans son suffixe, ou le nom du dossier).
    pub name: String,
    /// Nombre de fichiers, quand la recherche le sait sans rien ouvrir (un monde « fichier ») ; sinon `inspect`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    /// Date de dernière modification, RFC 3339 en UTC.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,
    /// `false` : le nom de ce monde est refusé par le réglage de la fiche qui le reçoit ; il est listé mais ne
    /// s'installe pas (`BR-MONDE-014`).
    #[serde(default = "yes")]
    pub usable: bool,
}

fn yes() -> bool {
    true
}

/// Un dossier ouvert aux services et ce que la recherche y a trouvé.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldRoot {
    /// Le dossier tel qu'il est écrit dans la configuration de l'agent.
    pub folder: String,
    /// `false` : le dossier n'a pas pu être ouvert (absent, droits, dossier protégé).
    pub readable: bool,
    /// La recherche s'est arrêtée à la borne (BR-MONDE-003) : des mondes peuvent manquer.
    #[serde(default)]
    pub truncated: bool,
    #[serde(default)]
    pub worlds: Vec<WorldEntry>,
}

/// `GET /catalog/{id}/worlds`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldsView {
    /// Vide : le serveur n'a ouvert aucun dossier aux services (BR-MONDE-002).
    pub roots: Vec<WorldRoot>,
    /// Profondeur et nombre d'entrées au plus par dossier (BR-MONDE-003).
    pub max_depth: u32,
    pub max_entries: u32,
}

/// `POST /catalog/{id}/worlds/inspect` : le monde choisi.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldRef {
    pub folder: String,
    pub path: String,
}

impl std::fmt::Debug for WorldRef {
    /// Jamais un chemin ni un nom de monde (BR-MONDE-023).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WorldRef(..)")
    }
}

/// Ce que la reprise copierait et ce qu'elle laisserait où il est (BR-MONDE-006, 009, 012).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldInspection {
    /// Fichiers qui seraient copiés, et leur taille totale.
    pub files: u64,
    pub bytes: u64,
    /// Éléments dont le nom évoque une sauvegarde : ni lus ni copiés (leur nombre, puis les premiers noms).
    pub not_copied: u64,
    #[serde(default)]
    pub not_copied_names: Vec<String>,
    /// Liens et fichiers spéciaux : non copiés, listés de la même façon.
    pub skipped_links: u64,
    #[serde(default)]
    pub skipped_link_names: Vec<String>,
    /// La mesure s'est arrêtée à son budget : les chiffres sont « au moins ».
    #[serde(default)]
    pub partial: bool,
    /// L'espace libre là où la copie sera écrite, quand il est connu.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub free_bytes: Option<u64>,
}

/// Combien de noms au plus une inspection rend pour une liste d'éléments non copiés.
pub const MAX_LISTED_NAMES: usize = 50;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_world_ref_never_shows_its_path_in_debug() {
        let shown = format!(
            "{:?}",
            WorldRef {
                folder: "/srv/jeux".into(),
                path: "SENTINEL/Mon monde.wld".into()
            }
        );
        assert!(
            !shown.contains("SENTINEL") && !shown.contains("jeux"),
            "{shown}"
        );
    }

    #[test]
    fn an_old_inspection_without_the_new_fields_still_reads() {
        let view: WorldInspection =
            serde_json::from_str(r#"{"files":1,"bytes":2,"not_copied":0,"skipped_links":0}"#)
                .expect("lu");
        assert!(view.not_copied_names.is_empty() && !view.partial);
    }
}

//! Reprendre un monde existant : ce qu'est un monde, ce qu'on ne copie pas, les bornes (HRT-105,
//! `BR-MONDE-003`, `005`, `006`, `010`, `011`, `039`).
//!
//! Aucune E/S : ce module juge des NOMS. Ce qu'est « un monde » est déclaré par la fiche (`[worlds]`) : un
//! fichier au suffixe donné (avec ses fichiers compagnons de même nom), ou un dossier à une profondeur donnée.
//! Cette définition est la même pour la recherche, l'inspection et la copie.
//!
//! La règle de nom des sauvegardes est celle des sauvegardes de HRT-97 : le nom contient « backup » ou « sauvegarde »
//! (sans tenir compte de la casse) ou finit par « .bak ». Elle est écrite ici une seconde fois à dessein (un
//! domaine ne dépend pas d'un autre pour un test de nom) : la suite de test fixe le comportement.

use std::time::Duration;

use crate::domain::catalog::loading::model::Fiche;
use crate::domain::catalog::loading::schema::fiche::WorldUnit;
use crate::domain::catalog::settings_state::validate_text;
use crate::domain::services::deploy::flatten_target;

/// Profondeur de la recherche sous un dossier ouvert (`BR-MONDE-003`) : les enfants directs sont au niveau 1.
pub const MAX_DEPTH: usize = 3;
/// Entrées lues au plus par dossier ouvert (`BR-MONDE-003`).
pub const MAX_ENTRIES: usize = 5_000;
/// Temps accordé à la mesure d'un monde avant la copie (`BR-MONDE-012`).
pub const MEASURE_BUDGET: Duration = Duration::from_secs(60);
/// Une copie dure au plus six heures (`BR-MONDE-011`).
pub const COPY_MAX: Duration = Duration::from_secs(6 * 3600);
/// Éléments cités au plus quand on dit lesquels ne sont pas copiés (`BR-MONDE-006`).
pub const MAX_LISTED: usize = 50;
/// Éléments au plus dans un monde copié (borne de mémoire de la liste de vérification).
pub const MAX_WORLD_ITEMS: usize = 500_000;

/// La forme d'un monde.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shape {
    /// Un fichier au suffixe donné ; chaque compagnon est un suffixe de plus apposé au nom du fichier.
    File {
        suffix: String,
        companions: Vec<String>,
    },
    /// Un dossier à `depth` niveaux sous le dossier ouvert.
    Dir { depth: usize },
}

/// Ce que la fiche dit d'un monde, prêt à juger des noms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldRules {
    pub shape: Shape,
    /// Le réglage qui reçoit le nom lu sur le monde (`BR-MONDE-014`).
    pub names_setting: String,
    /// Les réglages que le monde porte : ni posés ni écrits par Hearth (`BR-MONDE-014`).
    pub carried: Vec<String>,
    /// Le sous-dossier géré où le monde est mis en place (le chemin de montage aplati).
    pub folder_source: String,
}

/// Pourquoi une fiche ne propose pas la reprise d'un monde.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RulesError {
    /// La fiche ne déclare pas ce qu'est un monde (`BR-MONDE-039`).
    NoWorlds,
    /// La fiche déclare des chemins liés ou une forme que cette version ne sait pas reprendre.
    Unsupported,
}

impl WorldRules {
    pub fn from_fiche(fiche: &Fiche) -> Result<Self, RulesError> {
        let worlds = fiche.file.worlds.as_ref().ok_or(RulesError::NoWorlds)?;
        // Des chemins liés (un monde fait de plusieurs dossiers) : pas dans cette version, aucune fiche n'en déclare.
        if !worlds.also.is_empty() {
            return Err(RulesError::Unsupported);
        }
        let shape = match worlds.unit {
            WorldUnit::File => Shape::File {
                suffix: worlds.suffix.clone().ok_or(RulesError::Unsupported)?,
                companions: worlds.companions.clone(),
            },
            WorldUnit::Dir => {
                let depth = usize::try_from(worlds.depth.ok_or(RulesError::Unsupported)?)
                    .map_err(|_| RulesError::Unsupported)?;
                if depth == 0 || depth > MAX_DEPTH {
                    return Err(RulesError::Unsupported);
                }
                Shape::Dir { depth }
            }
        };
        let folder = fiche
            .file
            .folder
            .iter()
            .find(|folder| folder.id == worlds.folder)
            .ok_or(RulesError::Unsupported)?;
        let folder_source = flatten_target(&folder.target).ok_or(RulesError::Unsupported)?;
        Ok(Self {
            shape,
            names_setting: worlds.names_setting.clone(),
            carried: worlds.carried_by_world.clone(),
            folder_source,
        })
    }

    /// Le nom du monde que désigne un élément de ce nom, s'il en désigne un : le nom du fichier sans son suffixe
    /// (jamais vide), ou le nom du dossier.
    pub fn world_name<'a>(&self, entry_name: &'a str) -> Option<&'a str> {
        match &self.shape {
            Shape::File { suffix, .. } => entry_name
                .strip_suffix(suffix.as_str())
                .filter(|stem| !stem.is_empty()),
            Shape::Dir { .. } => Some(entry_name).filter(|name| !name.is_empty()),
        }
    }

    /// Les noms de fichiers voisins qui font partie du monde (`<nom du fichier><compagnon>`).
    pub fn companion_names(&self, entry_name: &str) -> Vec<String> {
        match &self.shape {
            Shape::File { companions, .. } => companions
                .iter()
                .map(|companion| format!("{entry_name}{companion}"))
                .collect(),
            Shape::Dir { .. } => Vec::new(),
        }
    }

    /// Un élément trouvé à `depth` niveaux (1 = enfant direct) est-il un monde de cette fiche ?
    pub fn is_world(&self, depth: usize, is_dir: bool, name: &str) -> bool {
        if looks_like_backup(name) {
            return false;
        }
        match &self.shape {
            Shape::File { .. } => !is_dir && depth <= MAX_DEPTH && self.world_name(name).is_some(),
            Shape::Dir { depth: wanted } => is_dir && depth == *wanted,
        }
    }

    /// Le nom du monde est-il un nom que le réglage de la fiche accepte ? Un monde dont le nom serait refusé
    /// (caractère que le jeu ne lit pas dans un fichier de réglages) est listé mais ne s'installe pas.
    pub fn name_usable(&self, fiche: &Fiche, name: &str) -> bool {
        !name.is_empty()
            && fiche
                .setting(&self.names_setting)
                .is_some_and(|setting| validate_text(fiche, setting, name).is_ok())
    }
}

/// La règle de nom des sauvegardes (`BR-MONDE-005`, `006`) : « backup », « sauvegarde » ou « .bak ».
pub fn looks_like_backup(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.contains("backup") || lower.contains("sauvegarde") || lower.ends_with(".bak")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::catalog::{Origin, load};

    const FICHE: &str = include_str!("../../../catalog/terraria/fiche.toml");
    const SETTINGS: &str = include_str!("../../../catalog/terraria/settings.toml");

    fn terraria() -> Fiche {
        load(Origin::Embedded, FICHE.as_bytes(), SETTINGS.as_bytes()).expect("fiche")
    }

    fn file_rules(companions: &[&str]) -> WorldRules {
        WorldRules {
            shape: Shape::File {
                suffix: ".wld".into(),
                companions: companions.iter().map(|c| (*c).to_owned()).collect(),
            },
            names_setting: "world_name".into(),
            carried: vec![],
            folder_source: "root-.local-share-Terraria-Worlds".into(),
        }
    }

    #[test]
    fn terraria_declares_a_file_world_in_its_world_folder() {
        let rules = WorldRules::from_fiche(&terraria()).expect("règles");
        assert_eq!(
            rules.shape,
            Shape::File {
                suffix: ".wld".into(),
                companions: vec![]
            }
        );
        assert_eq!(rules.names_setting, "world_name");
        assert!(rules.carried.iter().any(|id| id == "seed"));
        assert!(!rules.folder_source.is_empty() && !rules.folder_source.contains('/'));
    }

    #[test]
    fn a_file_world_is_a_file_with_the_suffix_and_a_non_empty_name() {
        let rules = file_rules(&[]);
        assert!(rules.is_world(1, false, "Monde.wld"));
        assert!(rules.is_world(3, false, "Monde.wld"));
        assert!(
            !rules.is_world(4, false, "Monde.wld"),
            "au-delà de trois niveaux"
        );
        assert!(
            !rules.is_world(1, true, "Monde.wld"),
            "un dossier n'est pas un monde fichier"
        );
        assert!(
            !rules.is_world(1, false, ".wld"),
            "un nom vide n'est pas un monde"
        );
        assert!(!rules.is_world(1, false, "Monde.txt"));
        assert_eq!(rules.world_name("Monde.wld"), Some("Monde"));
    }

    #[test]
    fn a_dir_world_is_a_dir_at_the_declared_depth() {
        let rules = WorldRules {
            shape: Shape::Dir { depth: 2 },
            ..file_rules(&[])
        };
        assert!(rules.is_world(2, true, "Partie1"));
        assert!(!rules.is_world(1, true, "Partie1"));
        assert!(!rules.is_world(2, false, "Partie1"));
        assert_eq!(rules.world_name("Partie1"), Some("Partie1"));
    }

    #[test]
    fn a_name_that_looks_like_a_backup_is_never_a_world() {
        let rules = file_rules(&[]);
        for name in [
            "Backup-Monde.wld",
            "monde_SAUVEGARDE.wld",
            "Ma Sauvegarde.wld",
            "monde.wld.bak",
            "x.BAK",
        ] {
            assert!(looks_like_backup(name), "{name}");
        }
        assert!(!rules.is_world(1, false, "Backup-Monde.wld"));
        assert!(!rules.is_world(1, false, "monde_SAUVEGARDE.wld"));
        assert!(!looks_like_backup("Monde.wld"));
        assert!(
            !looks_like_backup("bak"),
            "« .bak » finit un nom, il ne le résume pas"
        );
    }

    #[test]
    fn companions_are_the_file_name_plus_a_suffix() {
        let rules = file_rules(&[".bak", ".meta"]);
        assert_eq!(
            rules.companion_names("Monde.wld"),
            vec!["Monde.wld.bak".to_owned(), "Monde.wld.meta".to_owned()]
        );
        assert!(file_rules(&[]).companion_names("Monde.wld").is_empty());
    }

    #[test]
    fn a_world_name_the_setting_refuses_is_not_usable() {
        let fiche = terraria();
        let rules = WorldRules::from_fiche(&fiche).expect("règles");
        assert!(rules.name_usable(&fiche, "Mon_monde-2"));
        assert!(
            !rules.name_usable(&fiche, "Mon monde"),
            "l'espace est refusé par le réglage"
        );
        assert!(!rules.name_usable(&fiche, ""));
    }
}

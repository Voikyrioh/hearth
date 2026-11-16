//! La version de Minecraft et le moteur que dit un pack serveur (HRT-225, BR-MOD-019).
//!
//! Le pack est une archive HOSTILE : ce module ne lit que la table des entrées et, au plus, quelques petits fichiers de
//! description (1 Mio chacun, lus par l'appelant). Une valeur n'est retenue que si elle a la forme d'une version ;
//! sinon elle est ignorée, jamais recopiée. Ordre de confiance : le fichier de description d'un lanceur
//! (`manifest.json` CurseForge, `variables.txt` de ServerStarter, `install.properties` de l'installateur Fabric),
//! puis les dossiers de bibliothèques que laissent les installateurs (`libraries/net/fabricmc/...`).

use serde::Deserialize;

use super::pack::{DescriptionReader, PackEntry};

/// Le moteur de serveur d'un pack, tel que la fiche le nomme (identifiant de variante).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Loader {
    Fabric,
    Quilt,
    Forge,
    NeoForge,
}

impl Loader {
    /// L'identifiant de la variante de la fiche qui porte ce moteur.
    pub const fn variant_id(self) -> &'static str {
        match self {
            Self::Fabric => "fabric",
            Self::Quilt => "quilt",
            Self::Forge => "forge",
            Self::NeoForge => "neoforge",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "fabric" | "fabric-loader" => Some(Self::Fabric),
            "quilt" | "quilt-loader" => Some(Self::Quilt),
            "forge" | "minecraftforge" => Some(Self::Forge),
            "neoforge" | "neoforged" => Some(Self::NeoForge),
            _ => None,
        }
    }
}

/// Ce que le pack dit du jeu. Chaque champ est absent quand le pack ne le dit pas.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PackGame {
    pub minecraft: Option<String>,
    pub loader: Option<Loader>,
}

/// Une version de Minecraft : un chiffre en tête, puis chiffres, lettres, points et tirets, 16 caractères au plus.
pub fn is_game_version(text: &str) -> bool {
    text.chars().next().is_some_and(|c| c.is_ascii_digit())
        && text.len() <= 16
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

fn version(text: &str) -> Option<String> {
    let text = text.trim();
    is_game_version(text).then(|| text.to_owned())
}

/// Lit le pack serveur. `read` ne sert qu'aux fichiers de description présents à la racine.
pub fn detect(entries: &[PackEntry<'_>], read: &mut dyn DescriptionReader) -> PackGame {
    let at_root = |name: &str| entries.iter().any(|e| e.is_file && e.path == name);
    let mut found = PackGame::default();
    if at_root("manifest.json")
        && let Ok(bytes) = read.read("manifest.json")
    {
        merge(&mut found, curseforge(&bytes));
    }
    if at_root("variables.txt")
        && let Ok(bytes) = read.read("variables.txt")
    {
        merge(&mut found, server_starter(&bytes));
    }
    for name in ["install.properties", ".fabric-installer/install.properties"] {
        if at_root(name)
            && let Ok(bytes) = read.read(name)
        {
            merge(&mut found, fabric_installer(&bytes));
        }
    }
    merge(&mut found, from_libraries(entries));
    found
}

fn merge(into: &mut PackGame, from: PackGame) {
    if into.minecraft.is_none() {
        into.minecraft = from.minecraft;
    }
    if into.loader.is_none() {
        into.loader = from.loader;
    }
}

#[derive(Deserialize)]
struct CurseManifest {
    minecraft: Option<CurseMinecraft>,
}

#[derive(Deserialize)]
struct CurseMinecraft {
    version: Option<String>,
    #[serde(default, rename = "modLoaders")]
    mod_loaders: Vec<CurseLoader>,
}

#[derive(Deserialize)]
struct CurseLoader {
    id: Option<String>,
}

/// `manifest.json` d'un pack CurseForge : `minecraft.version` et `minecraft.modLoaders[].id` (« fabric-0.14.21 »).
fn curseforge(bytes: &[u8]) -> PackGame {
    let Ok(manifest) = serde_json::from_slice::<CurseManifest>(bytes) else {
        return PackGame::default();
    };
    let Some(minecraft) = manifest.minecraft else {
        return PackGame::default();
    };
    PackGame {
        minecraft: minecraft.version.as_deref().and_then(version),
        loader: minecraft
            .mod_loaders
            .iter()
            .filter_map(|l| l.id.as_deref())
            .find_map(|id| Loader::from_name(id.split('-').next().unwrap_or(""))),
    }
}

fn pairs(bytes: &[u8]) -> Vec<(String, String)> {
    String::from_utf8_lossy(bytes)
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.starts_with('#') {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            Some((
                key.trim().to_owned(),
                value.trim().trim_matches('"').to_owned(),
            ))
        })
        .collect()
}

/// `variables.txt` de ServerStarter : `MINECRAFT_VERSION=1.20.1`, `MODLOADER=Fabric`.
fn server_starter(bytes: &[u8]) -> PackGame {
    let mut game = PackGame::default();
    for (key, value) in pairs(bytes) {
        match key.as_str() {
            "MINECRAFT_VERSION" => game.minecraft = version(&value),
            "MODLOADER" => game.loader = Loader::from_name(&value),
            _ => {}
        }
    }
    game
}

/// `install.properties` de l'installateur Fabric : `game-version=1.20.1`, `loader-version=0.14.21` (la présence du
/// second dit Fabric).
fn fabric_installer(bytes: &[u8]) -> PackGame {
    let mut game = PackGame::default();
    for (key, value) in pairs(bytes) {
        match key.as_str() {
            "game-version" => game.minecraft = version(&value),
            "loader-version" => game.loader = Some(Loader::Fabric),
            _ => {}
        }
    }
    game
}

/// Les dossiers que laissent les installateurs : `libraries/net/fabricmc/intermediary/<jeu>/`, `.../fabric-loader/`,
/// `libraries/org/quiltmc/quilt-loader/`, `libraries/net/minecraftforge/forge/<jeu>-<forge>/`,
/// `libraries/net/neoforged/neoforge/<version>/`.
fn from_libraries(entries: &[PackEntry<'_>]) -> PackGame {
    let mut game = PackGame::default();
    for entry in entries {
        let Some(rest) = entry.path.strip_prefix("libraries/") else {
            continue;
        };
        let parts: Vec<&str> = rest.split('/').collect();
        let is = |prefix: &[&str]| parts.len() > prefix.len() && parts[..prefix.len()] == *prefix;
        let seen = if is(&["net", "fabricmc", "intermediary"]) {
            PackGame {
                minecraft: version(parts[3]),
                loader: Some(Loader::Fabric),
            }
        } else if is(&["net", "fabricmc", "fabric-loader"]) {
            PackGame {
                minecraft: None,
                loader: Some(Loader::Fabric),
            }
        } else if is(&["org", "quiltmc", "quilt-loader"]) {
            PackGame {
                minecraft: None,
                loader: Some(Loader::Quilt),
            }
        } else if is(&["net", "minecraftforge", "forge"]) {
            // `1.20.1-47.2.0` : la version du jeu est avant le premier tiret.
            PackGame {
                minecraft: parts[3].split_once('-').and_then(|(mc, _)| version(mc)),
                loader: Some(Loader::Forge),
            }
        } else if is(&["net", "neoforged", "neoforge"]) {
            PackGame {
                minecraft: neoforge_game(parts[3]),
                loader: Some(Loader::NeoForge),
            }
        } else {
            continue;
        };
        merge(&mut game, seen);
    }
    game
}

/// NeoForge `20.4.237` est pour le jeu `1.20.4`, `21.0.5` pour `1.21` : « 1. » suivi des deux premiers nombres (le
/// second omis quand il vaut 0).
fn neoforge_game(neoforge: &str) -> Option<String> {
    let mut nums = neoforge.split('.');
    let major: u32 = nums.next()?.parse().ok()?;
    let minor: u32 = nums.next()?.parse().ok()?;
    version(&if minor == 0 {
        format!("1.{major}")
    } else {
        format!("1.{major}.{minor}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::mods::pack::DescriptionError;
    use std::collections::HashMap;

    struct Files(HashMap<&'static str, &'static str>);
    impl DescriptionReader for Files {
        fn read(&mut self, name: &str) -> Result<Vec<u8>, DescriptionError> {
            self.0
                .get(name)
                .map(|text| text.as_bytes().to_vec())
                .ok_or(DescriptionError::Unreadable)
        }
    }

    fn game(paths: &[&str], files: &[(&'static str, &'static str)]) -> PackGame {
        let entries: Vec<PackEntry<'_>> = paths
            .iter()
            .map(|path| PackEntry {
                path,
                is_file: true,
                size: 1,
            })
            .collect();
        detect(&entries, &mut Files(files.iter().copied().collect()))
    }

    fn fabric_1_20_1() -> PackGame {
        PackGame {
            minecraft: Some("1.20.1".into()),
            loader: Some(Loader::Fabric),
        }
    }

    #[test]
    fn the_fabric_intermediary_folder_says_both_the_game_and_the_engine() {
        let found = game(
            &[
                "mods/a.jar",
                "libraries/net/fabricmc/intermediary/1.20.1/intermediary-1.20.1.jar",
                "libraries/net/fabricmc/fabric-loader/0.14.21/fabric-loader-0.14.21.jar",
            ],
            &[],
        );
        assert_eq!(found, fabric_1_20_1());
    }

    #[test]
    fn a_curseforge_manifest_a_variables_file_and_an_installer_file_each_say_it() {
        let manifest = r#"{"minecraft":{"version":"1.20.1","modLoaders":[{"id":"fabric-0.14.21","primary":true}]},"files":[]}"#;
        assert_eq!(
            game(
                &["mods/a.jar", "manifest.json"],
                &[("manifest.json", manifest)]
            ),
            fabric_1_20_1()
        );
        let variables = "MODLOADER=Fabric\nMINECRAFT_VERSION=1.20.1\n";
        assert_eq!(
            game(
                &["mods/a.jar", "variables.txt"],
                &[("variables.txt", variables)]
            ),
            fabric_1_20_1()
        );
        let installer = "game-version=1.20.1\nloader-version=0.14.21\n";
        assert_eq!(
            game(
                &["mods/a.jar", "install.properties"],
                &[("install.properties", installer)]
            ),
            fabric_1_20_1()
        );
    }

    #[test]
    fn forge_and_neoforge_libraries_say_the_game_too() {
        let forge = game(
            &["libraries/net/minecraftforge/forge/1.20.1-47.2.0/forge-1.20.1-47.2.0-server.jar"],
            &[],
        );
        assert_eq!(
            (forge.minecraft.as_deref(), forge.loader),
            (Some("1.20.1"), Some(Loader::Forge))
        );
        let neo = game(&["libraries/net/neoforged/neoforge/20.4.237/x.jar"], &[]);
        assert_eq!(
            (neo.minecraft.as_deref(), neo.loader),
            (Some("1.20.4"), Some(Loader::NeoForge))
        );
        assert_eq!(neoforge_game("21.0.5").as_deref(), Some("1.21"));
    }

    #[test]
    fn a_pack_that_says_nothing_or_a_value_that_is_no_version_says_nothing() {
        assert_eq!(
            game(&["mods/a.jar", "config/x.toml"], &[]),
            PackGame::default()
        );
        let hostile =
            r#"{"minecraft":{"version":"1.20.1; rm -rf /","modLoaders":[{"id":"evil-1"}]}}"#;
        assert_eq!(
            game(&["manifest.json"], &[("manifest.json", hostile)]),
            PackGame::default()
        );
        assert!(!is_game_version("LATEST") && !is_game_version("") && is_game_version("24w14a"));
    }

    #[test]
    fn a_description_file_wins_over_the_folders() {
        let variables = "MODLOADER=Quilt\nMINECRAFT_VERSION=1.19.4\n";
        let found = game(
            &[
                "libraries/net/fabricmc/intermediary/1.20.1/i.jar",
                "variables.txt",
            ],
            &[("variables.txt", variables)],
        );
        assert_eq!(
            (found.minecraft.as_deref(), found.loader),
            (Some("1.19.4"), Some(Loader::Quilt))
        );
    }
}

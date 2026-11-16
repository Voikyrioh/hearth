//! Ce que l'interface lit d'un pack de mods déposé (HRT-124, BR-MOD-007, BR-MOD-008, BR-MOD-016) : le genre lu PAR
//! L'AGENT dans le contenu de l'archive (jamais un champ à remplir), les nombres que l'écran montre avant de lancer,
//! et, quand le pack est refusé, un CODE fixe de l'agent plus un nombre ou un nom d'hôte, jamais un nom d'entrée.

use hearth_link::PackRefusal;
use hearth_proto::api::packs::{ModrinthPackView, PackGenreWire, PackInspectView};
use serde::Serialize;
use specta::Type;

/// Le genre d'un pack (BR-MOD-007). `unknown` : un genre plus récent que ce client, dit sans être deviné.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum PackGenreDto {
    ServerPack,
    ModrinthPack,
    Unknown,
}

impl From<PackGenreWire> for PackGenreDto {
    fn from(genre: PackGenreWire) -> Self {
        match genre {
            PackGenreWire::ServerPack => Self::ServerPack,
            PackGenreWire::ModrinthPack => Self::ModrinthPack,
            PackGenreWire::Unknown => Self::Unknown,
        }
    }
}

/// Ce qu'un pack Modrinth dit de lui-même (textes nettoyés et bornés par l'agent).
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthPackDto {
    pub name: String,
    pub version: String,
    pub minecraft: Option<String>,
    /// `fabric-loader`, `quilt-loader`, `forge` ou `neoforge`.
    pub loader: Option<String>,
    pub loader_version: Option<String>,
    /// Le nombre de fichiers que le programme du jeu télécharge.
    pub downloads: u32,
    /// Les hôtes de ces téléchargements.
    pub hosts: Vec<String>,
}

impl From<&ModrinthPackView> for ModrinthPackDto {
    fn from(view: &ModrinthPackView) -> Self {
        Self {
            name: view.name.clone(),
            version: view.version.clone(),
            minecraft: view.minecraft.clone(),
            loader: view.loader.clone(),
            loader_version: view.loader_version.clone(),
            downloads: view.downloads,
            hosts: view.hosts.clone().into_vec(),
        }
    }
}

/// Un nombre d'octets ou de fichiers pour l'écran : un `f64` exact (jusqu'à 2^53, bien au-delà de 4 Gio).
fn exact(value: u64) -> f64 {
    value as f64
}

/// Ce que l'agent a lu dans le pack (`POST /deployments/pack-check`).
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PackInspectDto {
    pub genre: PackGenreDto,
    pub size_bytes: f64,
    pub files: f64,
    /// Pack serveur : le nombre de `.jar` dans `mods` ou `plugins` ; pack Modrinth : 0.
    pub mods: f64,
    pub unpacked_bytes: f64,
    /// Les dossiers de premier niveau.
    pub top_dirs: Vec<String>,
    pub modrinth: Option<ModrinthPackDto>,
}

impl From<&PackInspectView> for PackInspectDto {
    fn from(view: &PackInspectView) -> Self {
        Self {
            genre: view.genre.into(),
            size_bytes: exact(view.size_bytes),
            files: exact(view.files),
            mods: exact(view.mods),
            unpacked_bytes: exact(view.unpacked_bytes),
            top_dirs: view.top_dirs.clone().into_vec(),
            modrinth: view.modrinth.as_ref().map(ModrinthPackDto::from),
        }
    }
}

/// Pourquoi l'agent ne dit pas ce qu'est ce pack : des CODES, jamais un texte de l'agent ni un nom d'entrée.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PackRefusalDto {
    /// Aucun dépôt complet sous cet identifiant : à déposer de nouveau.
    Unknown,
    /// Le pack est refusé : `reason` est un code fixe (`PACK_IS_A_DOWNLOAD_LIST`, `PACK_HOST_FORBIDDEN`,
    /// `ARCHIVE_REFUSED`, ...), `mods` le nombre de mods listés par une liste CurseForge, `host` l'hôte refusé.
    Refused {
        reason: String,
        mods: Option<u32>,
        host: Option<String>,
        pack_version: Option<String>,
        chosen_version: Option<String>,
    },
}

impl From<PackRefusal> for PackRefusalDto {
    fn from(refusal: PackRefusal) -> Self {
        match refusal {
            PackRefusal::Unknown => Self::Unknown,
            PackRefusal::Refused {
                reason,
                mods,
                host,
                pack_version,
                chosen_version,
            } => Self::Refused {
                reason,
                mods,
                host,
                pack_version,
                chosen_version,
            },
        }
    }
}

/// L'issue de la lecture : le pack lu, ou refusé.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PackCheckOutcome {
    Read { pack: PackInspectDto },
    Refused { refusal: PackRefusalDto },
}

#[cfg(test)]
mod tests {
    use super::*;
    use hearth_proto::lenient::Items;

    #[test]
    fn the_screen_gets_the_kind_read_by_the_agent_and_plain_numbers() {
        let view = PackInspectView {
            genre: PackGenreWire::ServerPack,
            size_bytes: 794_497_024,
            files: 2314,
            mods: 652,
            unpacked_bytes: 1_650_000_000,
            top_dirs: Items::from(vec!["config".to_owned(), "mods".to_owned()]),
            modrinth: None,
        };
        let dto = PackInspectDto::from(&view);
        assert_eq!(dto.genre, PackGenreDto::ServerPack);
        assert_eq!((dto.files, dto.mods), (2314.0, 652.0));
        assert_eq!(dto.size_bytes, 794_497_024.0);
        assert_eq!(dto.top_dirs, vec!["config", "mods"]);
        assert!(dto.modrinth.is_none());
    }

    #[test]
    fn a_genre_this_client_does_not_know_is_said_unknown() {
        assert_eq!(
            PackGenreDto::from(PackGenreWire::Unknown),
            PackGenreDto::Unknown
        );
    }

    #[test]
    fn a_refusal_keeps_its_code_and_only_its_number_or_host() {
        let dto = PackRefusalDto::from(PackRefusal::Refused {
            reason: "PACK_IS_A_DOWNLOAD_LIST".into(),
            mods: Some(652),
            host: None,
            pack_version: None,
            chosen_version: None,
        });
        assert_eq!(
            dto,
            PackRefusalDto::Refused {
                reason: "PACK_IS_A_DOWNLOAD_LIST".into(),
                mods: Some(652),
                host: None,
                pack_version: None,
                chosen_version: None,
            }
        );
        assert_eq!(
            PackRefusalDto::from(PackRefusal::Unknown),
            PackRefusalDto::Unknown
        );
    }
}

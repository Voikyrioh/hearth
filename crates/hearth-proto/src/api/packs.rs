//! `POST /deployments/pack-check` : ce que l'agent lit dans un pack de mods déposé, avant l'installation
//! (HRT-124, BR-MOD-007).
//!
//! Le client dépose le fichier par un transfert (zone d'attente de l'agent) puis demande ce que c'est. Le genre
//! est lu par l'agent dans le CONTENU de l'archive, jamais fourni par le client. Une consultation : rien n'est
//! rangé, rien n'est écrit, aucun journal. Un refus est `404 PACK_UNKNOWN` ou `422 PACK_REFUSED`
//! (`details.reason` : un code fixe, jamais un nom d'entrée).

use serde::{Deserialize, Serialize};

use crate::lenient::Items;

/// Corps de la demande : l'identifiant du dépôt (jamais un chemin ni un nom de fichier).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackInspectRequest {
    pub deposit: String,
}

impl std::fmt::Debug for PackInspectRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PackInspectRequest").finish_non_exhaustive()
    }
}

/// Le genre d'un pack, tel que l'agent l'a reconnu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PackGenreWire {
    ServerPack,
    ModrinthPack,
    /// Un genre que cette version ne connaît pas : l'écran le dit sans le deviner.
    #[serde(other)]
    #[default]
    Unknown,
}

/// Ce que dit un pack Modrinth de lui-même (texte nettoyé et borné par l'agent).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModrinthPackView {
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minecraft: Option<String>,
    /// `fabric-loader`, `quilt-loader`, `forge` ou `neoforge`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loader: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loader_version: Option<String>,
    /// Le nombre de fichiers que le programme du jeu télécharge.
    pub downloads: u32,
    /// Les hôtes de ces téléchargements (parmi les quatre du format).
    #[serde(default)]
    pub hosts: Items<String>,
}

/// Réponse : ce que Hearth sait du pack (BR-MOD-007). Aucun nom d'entrée, sauf les dossiers de premier niveau.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackInspectView {
    pub genre: PackGenreWire,
    /// Taille du fichier déposé, en octets.
    pub size_bytes: u64,
    /// Nombre de fichiers de l'archive.
    pub files: u64,
    /// Pack serveur : nombre de `.jar` dans `mods` ou `plugins` ; pack Modrinth : 0.
    pub mods: u64,
    /// Taille une fois déballée par le programme du jeu, en octets.
    pub unpacked_bytes: u64,
    /// Dossiers de premier niveau (douze au plus).
    #[serde(default)]
    pub top_dirs: Items<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modrinth: Option<ModrinthPackView>,
}

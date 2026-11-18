//! Jugement d'un pack Minecraft déposé (HRT-124, BR-MOD-007, BR-MOD-008, BR-MOD-016).
//!
//! Le pack est une archive HOSTILE déjà jugée entière par `domain::archive` (bornes, noms, types, sommes). Ce
//! module n'ouvre aucun fichier : il reçoit la table des entrées et, au plus, UN fichier de description (1 Mio),
//! lu par l'appelant. Le genre se lit dans le CONTENU, jamais dans le nom du fichier déposé.
//!
//! Deux genres seulement : « Pack Modrinth » (`.mrpack`) et « Pack serveur » (jar et dossiers à la racine). Un
//! pack qui n'est qu'une liste de téléchargements CurseForge est refusé (`PACK_IS_A_DOWNLOAD_LIST`) : Hearth n'a
//! aucune voie CurseForge (BR-MOD-017).

use serde::Deserialize;

use super::game::{PackGame, detect};
use crate::domain::catalog::loading::schema::fiche::PackGenre;
use crate::domain::catalog::loading::value::is_risky_char;

/// Les quatre hôtes permis par le format des packs Modrinth (BR-MOD-008).
pub const MODRINTH_HOSTS: [&str; 4] = [
    "cdn.modrinth.com",
    "github.com",
    "raw.githubusercontent.com",
    "gitlab.com",
];

/// Taille maximale du fichier de description lu dans un pack (ADR-0055 point 12).
pub const MAX_DESCRIPTION_BYTES: u64 = 1024 * 1024;
/// Un pack Modrinth qui liste plus de fichiers que cela est refusé (le format n'en demande jamais autant).
pub const MAX_MODRINTH_FILES: usize = 20_000;
/// Nombre de dossiers de premier niveau montrés à l'écran.
const MAX_TOP_DIRS: usize = 12;

pub const MODRINTH_INDEX: &str = "modrinth.index.json";
pub const CURSEFORGE_MANIFEST: &str = "manifest.json";

/// Une entrée de la table de l'archive (nom déjà jugé par la lecture d'archive).
#[derive(Debug, Clone, Copy)]
pub struct PackEntry<'a> {
    pub path: &'a str,
    pub is_file: bool,
    pub size: u64,
    /// CRC-32 de l'entrée, si l'archive en porte un (zip).
    pub crc32: Option<u32>,
}

/// Ce que Hearth sait dire d'un pack accepté (aucun nom d'entrée n'y figure, sauf les dossiers de premier niveau).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackReport {
    pub genre: PackGenre,
    pub files: usize,
    /// Nombre de `.jar` dans `mods` ou `plugins` (pack serveur), 0 pour un pack Modrinth.
    pub mods: usize,
    pub unpacked: u64,
    pub top_dirs: Vec<String>,
    pub modrinth: Option<ModrinthSummary>,
    /// Ce que le pack serveur dit du jeu et de son moteur (HRT-225) ; vide pour un pack Modrinth (l'image lit le pack).
    pub game: PackGame,
    /// Les `.jar` de `mods` d'un pack serveur (nom de l'alphabet sûr, taille, CRC-32), de quoi inscrire leur origine
    /// (HRT-256, BR-MOD-033). Vide pour un pack Modrinth. Au plus `MAX_SERVER_MODS`.
    pub server_mods: Vec<ServerPackMod>,
}

/// Un `.jar` de `mods/` d'un pack serveur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerPackMod {
    pub name: String,
    pub size: u64,
    pub crc32: u32,
}

/// Au plus ce nombre de mods du pack serveur inscrits pour l'origine.
pub const MAX_SERVER_MODS: usize = 4_096;

/// Les mods d'un pack serveur dont l'origine peut s'inscrire : `mods/<nom>.jar` à la racine du dossier `mods`, nom de
/// l'alphabet sûr, avec un CRC-32 connu.
fn server_mods(files: &[&PackEntry<'_>]) -> Vec<ServerPackMod> {
    files
        .iter()
        .filter_map(|entry| {
            let name = entry.path.strip_prefix("mods/")?;
            (!name.contains('/')
                && super::listing::is_active_jar(name)
                && super::listing::is_safe_file_name(name))
            .then_some(())?;
            Some(ServerPackMod {
                name: name.to_owned(),
                size: entry.size,
                crc32: entry.crc32?,
            })
        })
        .take(MAX_SERVER_MODS)
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModrinthSummary {
    pub name: String,
    pub version: String,
    pub minecraft: Option<String>,
    /// Moteur annoncé : `fabric-loader`, `quilt-loader`, `forge` ou `neoforge`, et sa version.
    pub loader: Option<(String, String)>,
    pub downloads: usize,
    pub hosts: Vec<String>,
}

/// Motif de refus d'un pack : un code fixe par cause, jamais un nom d'entrée ni une valeur du pack, sauf un nom
/// d'hôte (déjà réduit à l'alphabet d'un nom d'hôte) et des nombres.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackRefusal {
    /// La liste CurseForge d'un lanceur : `mods` projets, aucun mod dedans.
    DownloadList { mods: usize },
    /// Un téléchargement hors de la liste de quatre hôtes, ou pas en `https`.
    ModrinthHost { host: String },
    /// Un chemin de fichier du pack sort du dossier du jeu.
    ModrinthPathEscapes,
    /// `modrinth.index.json` illisible, d'un autre jeu, d'un autre format ou trop gros.
    ModrinthIndexInvalid,
    /// Le pack serveur est rangé tout entier dans un seul dossier.
    WrappedInFolder,
    /// Ni pack Modrinth, ni pack serveur.
    Unrecognized,
    /// Le pack ne dit pas sa version de Minecraft et la demande est restée « LATEST » (HRT-225).
    VersionUnreadable,
    /// La version choisie contredit celle que dit le pack (HRT-225).
    VersionMismatch { pack: String, chosen: String },
    /// Le moteur choisi n'est pas celui que dit le pack (HRT-225).
    LoaderMismatch,
}

impl PackRefusal {
    pub fn code(&self) -> &'static str {
        match self {
            Self::DownloadList { .. } => "PACK_IS_A_DOWNLOAD_LIST",
            Self::ModrinthHost { .. } => "PACK_HOST_FORBIDDEN",
            Self::ModrinthPathEscapes => "PACK_PATH_ESCAPES",
            Self::ModrinthIndexInvalid => "PACK_INDEX_INVALID",
            Self::WrappedInFolder => "PACK_WRAPPED_IN_FOLDER",
            Self::Unrecognized => "PACK_UNRECOGNIZED",
            Self::VersionUnreadable => "PACK_VERSION_UNREADABLE",
            Self::VersionMismatch { .. } => "PACK_VERSION_MISMATCH",
            Self::LoaderMismatch => "PACK_LOADER_MISMATCH",
        }
    }
}

/// Ce que le jugement demande à l'appelant : lire UN fichier (1 Mio au plus) de l'archive déjà inspectée.
pub trait DescriptionReader {
    fn read(&mut self, name: &str) -> Result<Vec<u8>, DescriptionError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DescriptionError {
    /// Plus gros que 1 Mio, ou illisible : le pack est refusé sans autre détail.
    Unreadable,
}

/// Un `.jar` sous `mods` ou `plugins`. Le nom vient d'une archive HOSTILE et peut être n'importe quel texte UTF-8 :
/// la fin se compare par OCTETS, jamais par une découpe du texte (qui paniquerait au milieu d'un caractère).
fn is_jar_in_mods(path: &str) -> bool {
    let Some((first, rest)) = path.split_once('/') else {
        return false;
    };
    (first == "mods" || first == "plugins")
        && rest.len() >= 4
        && rest.as_bytes()[rest.len() - 4..].eq_ignore_ascii_case(b".jar")
}

fn top_of(path: &str) -> Option<&str> {
    path.split_once('/').map(|(top, _)| top)
}

/// Juge un pack. `read` ne sert qu'à lire `modrinth.index.json` ou `manifest.json`.
pub fn judge_pack(
    entries: &[PackEntry<'_>],
    read: &mut dyn DescriptionReader,
) -> Result<PackReport, PackRefusal> {
    let files: Vec<&PackEntry<'_>> = entries.iter().filter(|e| e.is_file).collect();
    let unpacked: u64 = files.iter().map(|e| e.size).sum();
    let root_has = |name: &str| files.iter().any(|e| e.path == name);

    // 1. Pack Modrinth : reconnu par son fichier de description à la racine.
    if root_has(MODRINTH_INDEX) {
        let bytes = read
            .read(MODRINTH_INDEX)
            .map_err(|_| PackRefusal::ModrinthIndexInvalid)?;
        let modrinth = judge_modrinth(&bytes)?;
        return Ok(PackReport {
            genre: PackGenre::ModrinthPack,
            files: files.len(),
            mods: 0,
            unpacked,
            top_dirs: top_dirs(entries),
            modrinth: Some(modrinth),
            game: PackGame::default(),
            server_mods: Vec::new(),
        });
    }

    // 2. Pack serveur : au moins un `.jar` dans `mods` ou `plugins` à la racine.
    let mods = files.iter().filter(|e| is_jar_in_mods(e.path)).count();
    if mods > 0 {
        return Ok(PackReport {
            genre: PackGenre::ServerPack,
            files: files.len(),
            mods,
            unpacked,
            top_dirs: top_dirs(entries),
            modrinth: None,
            game: detect(entries, read),
            server_mods: server_mods(&files),
        });
    }

    // 3. Une liste de téléchargements CurseForge : un `manifest.json` qui liste des projets, aucun mod dedans.
    if root_has(CURSEFORGE_MANIFEST)
        && let Ok(bytes) = read.read(CURSEFORGE_MANIFEST)
        && let Some(listed) = curseforge_list_len(&bytes)
    {
        return Err(PackRefusal::DownloadList { mods: listed });
    }

    // 4. Un pack serveur rangé tout entier dans un dossier.
    let mut tops: Vec<&str> = files.iter().filter_map(|e| top_of(e.path)).collect();
    tops.sort_unstable();
    tops.dedup();
    let loose = files.iter().any(|e| !e.path.contains('/'));
    if tops.len() == 1 && !loose {
        let inner = format!("{}/", tops[0]);
        let wrapped = files
            .iter()
            .any(|e| e.path.strip_prefix(&inner).is_some_and(is_jar_in_mods));
        if wrapped {
            return Err(PackRefusal::WrappedInFolder);
        }
    }
    Err(PackRefusal::Unrecognized)
}

fn top_dirs(entries: &[PackEntry<'_>]) -> Vec<String> {
    let mut tops: Vec<&str> = entries
        .iter()
        .filter_map(|e| top_of(e.path).or(if e.is_file { None } else { Some(e.path) }))
        .collect();
    tops.sort_unstable();
    tops.dedup();
    tops.into_iter()
        .take(MAX_TOP_DIRS)
        .map(|name| {
            // Un nom venu de l'archive est montré à l'écran : borné, sans caractère qui trompe l'œil.
            if name.chars().count() > 64 || name.chars().any(is_risky_char) {
                "?".to_owned()
            } else {
                name.to_owned()
            }
        })
        .collect()
}

// ---- CurseForge : seulement reconnaître la liste ------------------------------------------------------

#[derive(Deserialize)]
struct CurseForgeManifest {
    #[serde(default)]
    files: Option<Vec<serde_json::Value>>,
}

/// Le nombre de projets listés, si `manifest.json` est bien la liste d'un lanceur (un tableau `files`).
fn curseforge_list_len(bytes: &[u8]) -> Option<usize> {
    let manifest: CurseForgeManifest = serde_json::from_slice(bytes).ok()?;
    manifest.files.map(|f| f.len())
}

// ---- Modrinth ----------------------------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModrinthIndex {
    format_version: u32,
    game: String,
    #[serde(default)]
    version_id: String,
    #[serde(default)]
    name: String,
    files: Vec<ModrinthFile>,
    #[serde(default)]
    dependencies: std::collections::BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct ModrinthFile {
    path: String,
    #[serde(default)]
    downloads: Vec<String>,
}

fn judge_modrinth(bytes: &[u8]) -> Result<ModrinthSummary, PackRefusal> {
    let index: ModrinthIndex =
        serde_json::from_slice(bytes).map_err(|_| PackRefusal::ModrinthIndexInvalid)?;
    if index.format_version != 1
        || index.game != "minecraft"
        || index.files.len() > MAX_MODRINTH_FILES
    {
        return Err(PackRefusal::ModrinthIndexInvalid);
    }
    let mut hosts: Vec<String> = Vec::new();
    let mut downloads = 0usize;
    for file in &index.files {
        if !relative_path_inside(&file.path) {
            return Err(PackRefusal::ModrinthPathEscapes);
        }
        if file.downloads.is_empty() {
            return Err(PackRefusal::ModrinthIndexInvalid);
        }
        for url in &file.downloads {
            let host = https_host(url).ok_or_else(|| PackRefusal::ModrinthHost {
                host: shown_host(url),
            })?;
            if !MODRINTH_HOSTS.contains(&host.as_str()) {
                return Err(PackRefusal::ModrinthHost { host });
            }
            if !hosts.contains(&host) {
                hosts.push(host);
            }
        }
        downloads += 1;
    }
    hosts.sort();
    let loader = ["fabric-loader", "quilt-loader", "neoforge", "forge"]
        .iter()
        .find_map(|key| {
            index
                .dependencies
                .get(*key)
                .map(|v| ((*key).to_owned(), clean(v, 32)))
        });
    Ok(ModrinthSummary {
        name: clean(&index.name, 80),
        version: clean(&index.version_id, 40),
        minecraft: index.dependencies.get("minecraft").map(|v| clean(v, 32)),
        loader,
        downloads,
        hosts,
    })
}

/// Un chemin du format `.mrpack` : relatif, séparé par `/`, sans `.` ni `..`, sans lecteur ni barre inverse.
fn relative_path_inside(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 512
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains(':')
        && !path.chars().any(char::is_control)
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

/// Le nom d'hôte d'une adresse `https://hôte/...` : ni identifiant, ni port, ni caractère hors d'un nom d'hôte.
fn https_host(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://")?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let host = &rest[..end];
    let valid = !host.is_empty()
        && host.len() <= 100
        && host
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-');
    valid.then(|| host.to_owned())
}

/// L'hôte d'une adresse refusée, réduit à un alphabet sûr pour l'afficher (jamais le chemin ni les paramètres).
fn shown_host(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let host: String = rest[..end]
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-')
        .take(100)
        .collect();
    if host.is_empty() {
        "?".to_owned()
    } else {
        host.to_ascii_lowercase()
    }
}

/// Texte d'un pack montré à l'écran : sans caractère de contrôle, borné.
fn clean(text: &str, max: usize) -> String {
    text.chars()
        .filter(|c| !is_risky_char(*c))
        .take(max)
        .collect::<String>()
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed(Option<Vec<u8>>);
    impl DescriptionReader for Fixed {
        fn read(&mut self, _: &str) -> Result<Vec<u8>, DescriptionError> {
            self.0.clone().ok_or(DescriptionError::Unreadable)
        }
    }

    fn file(path: &str) -> PackEntry<'_> {
        PackEntry {
            path,
            is_file: true,
            size: 10,
            crc32: None,
        }
    }

    fn judge(paths: &[&str], description: Option<&str>) -> Result<PackReport, PackRefusal> {
        let entries: Vec<PackEntry<'_>> = paths.iter().map(|p| file(p)).collect();
        judge_pack(
            &entries,
            &mut Fixed(description.map(|d| d.as_bytes().to_vec())),
        )
    }

    fn index(downloads: &str, path: &str) -> String {
        format!(
            r#"{{"formatVersion":1,"game":"minecraft","versionId":"1.2","name":"Mon pack",
            "files":[{{"path":"{path}","hashes":{{"sha1":"a","sha512":"b"}},"downloads":["{downloads}"],"fileSize":1}}],
            "dependencies":{{"minecraft":"1.20.1","fabric-loader":"0.15.0"}}}}"#
        )
    }

    #[test]
    fn a_server_pack_is_known_by_a_jar_in_mods_at_the_root() {
        let report = judge(
            &["mods/a.jar", "mods/b.JAR", "config/x.toml", "start.sh"],
            None,
        )
        .expect("pack serveur");
        assert_eq!(report.genre, PackGenre::ServerPack);
        assert_eq!((report.files, report.mods), (4, 2));
        assert_eq!(
            report.top_dirs,
            vec!["config".to_owned(), "mods".to_owned()]
        );
    }

    #[test]
    fn a_hostile_non_ascii_name_never_panics_the_judge() {
        // Les quatre derniers octets coupent un caractère de trois octets : jamais une découpe du texte.
        for name in [
            "mods/\u{20ac}\u{20ac}",
            "mods/\u{20ac}",
            "plugins/a\u{e9}",
            "mods/\u{1f600}x",
        ] {
            assert_eq!(
                judge(&[name], None),
                Err(PackRefusal::Unrecognized),
                "{name}"
            );
        }
        let report = judge(&["mods/\u{20ac}.jar"], None).expect("un jar au nom non ASCII");
        assert_eq!(report.mods, 1);
    }

    #[test]
    fn plugins_count_too_and_the_file_name_never_matters() {
        let report = judge(&["plugins/p.jar"], None).expect("plugins");
        assert_eq!(report.genre, PackGenre::ServerPack);
    }

    #[test]
    fn a_jar_outside_mods_or_plugins_is_not_a_server_pack() {
        assert_eq!(
            judge(&["server.jar", "libs/x.jar"], None),
            Err(PackRefusal::Unrecognized)
        );
    }

    #[test]
    fn a_pack_wrapped_in_one_folder_is_refused_with_its_own_reason() {
        let refused = judge(&["Pack-1.0/mods/a.jar", "Pack-1.0/config/c.toml"], None);
        assert_eq!(refused, Err(PackRefusal::WrappedInFolder));
        assert_eq!(refused.unwrap_err().code(), "PACK_WRAPPED_IN_FOLDER");
    }

    #[test]
    fn a_curseforge_launcher_list_is_refused_with_its_mod_count() {
        let manifest = r#"{"minecraft":{"version":"1.20.1"},"files":[{"projectID":1,"fileID":2},{"projectID":3,"fileID":4}],"overrides":"overrides"}"#;
        let refused = judge(
            &["manifest.json", "overrides/config/a.toml"],
            Some(manifest),
        );
        assert_eq!(refused, Err(PackRefusal::DownloadList { mods: 2 }));
        assert_eq!(refused.unwrap_err().code(), "PACK_IS_A_DOWNLOAD_LIST");
    }

    #[test]
    fn a_manifest_next_to_real_mods_is_a_server_pack_not_a_list() {
        let manifest = r#"{"files":[{"projectID":1,"fileID":2}]}"#;
        let report = judge(&["manifest.json", "mods/a.jar"], Some(manifest)).expect("pack serveur");
        assert_eq!(report.genre, PackGenre::ServerPack);
    }

    #[test]
    fn an_unreadable_manifest_is_just_an_unrecognized_pack() {
        assert_eq!(
            judge(&["manifest.json", "notes.txt"], Some("pas du json")),
            Err(PackRefusal::Unrecognized)
        );
    }

    #[test]
    fn a_modrinth_pack_is_known_by_its_index_and_summarized() {
        let text = index(
            "https://cdn.modrinth.com/data/a/versions/b/c.jar",
            "mods/c.jar",
        );
        let report = judge(
            &["modrinth.index.json", "overrides/config/c.toml"],
            Some(&text),
        )
        .expect("pack modrinth");
        assert_eq!(report.genre, PackGenre::ModrinthPack);
        let summary = report.modrinth.expect("résumé");
        assert_eq!(summary.name, "Mon pack");
        assert_eq!(summary.minecraft.as_deref(), Some("1.20.1"));
        assert_eq!(
            summary.loader,
            Some(("fabric-loader".into(), "0.15.0".into()))
        );
        assert_eq!(
            (summary.downloads, summary.hosts),
            (1, vec!["cdn.modrinth.com".to_owned()])
        );
    }

    #[test]
    fn a_modrinth_download_outside_the_four_hosts_or_not_https_is_refused() {
        for (url, host) in [
            ("https://evil.example/x.jar", "evil.example"),
            ("http://cdn.modrinth.com/x.jar", "cdn.modrinth.com"),
            (
                "https://cdn.modrinth.com.evil.example/x.jar",
                "cdn.modrinth.com.evil.example",
            ),
            (
                "https://user@cdn.modrinth.com/x.jar",
                "usercdn.modrinth.com",
            ),
            (
                "https://cdn.modrinth.com:8443/x.jar",
                "cdn.modrinth.com8443",
            ),
            ("ftp://cdn.modrinth.com/x.jar", "cdn.modrinth.com"),
        ] {
            let text = index(url, "mods/x.jar");
            let refused = judge(&["modrinth.index.json"], Some(&text));
            assert_eq!(
                refused,
                Err(PackRefusal::ModrinthHost { host: host.into() }),
                "{url}"
            );
        }
    }

    #[test]
    fn a_modrinth_path_leaving_the_game_folder_is_refused() {
        for path in [
            "../x.jar",
            "/etc/x",
            "mods/../../x",
            "C:/x",
            "a\\\\b",
            "",
            "mods//x",
            "./x",
        ] {
            let text = index("https://cdn.modrinth.com/a", path);
            let refused = judge(&["modrinth.index.json"], Some(&text));
            assert_eq!(refused, Err(PackRefusal::ModrinthPathEscapes), "{path}");
        }
    }

    #[test]
    fn a_modrinth_index_of_another_game_or_format_or_unreadable_is_refused() {
        let other_game = index("https://cdn.modrinth.com/a", "mods/a")
            .replace("minecraft\",\"versionId", "terraria\",\"versionId");
        let other_format = index("https://cdn.modrinth.com/a", "mods/a")
            .replace("\"formatVersion\":1", "\"formatVersion\":2");
        for text in [other_game.as_str(), other_format.as_str(), "{", "[]"] {
            assert_eq!(
                judge(&["modrinth.index.json"], Some(text)),
                Err(PackRefusal::ModrinthIndexInvalid)
            );
        }
        assert_eq!(
            judge(&["modrinth.index.json"], None),
            Err(PackRefusal::ModrinthIndexInvalid)
        );
    }

    #[test]
    fn what_a_pack_says_about_itself_is_cleaned_before_it_is_shown() {
        let text = index("https://cdn.modrinth.com/a", "mods/a")
            .replace("Mon pack", "Nom\\u0007\\u202e long");
        let report = judge(&["modrinth.index.json"], Some(&text)).expect("pack");
        let name = report.modrinth.expect("résumé").name;
        assert!(!name.chars().any(is_risky_char), "{name:?}");
    }

    #[test]
    fn a_top_level_name_that_could_trick_the_eye_is_never_shown() {
        let report = judge(&["mods/a.jar", "\u{202e}gnp/x.txt"], None).expect("pack");
        assert_eq!(report.top_dirs, vec!["mods".to_owned(), "?".to_owned()]);
    }

    #[test]
    fn an_empty_or_noise_archive_is_unrecognized() {
        assert_eq!(judge(&[], None), Err(PackRefusal::Unrecognized));
        assert_eq!(
            judge(&["readme.txt", "docs/a.md"], None),
            Err(PackRefusal::Unrecognized)
        );
    }
}

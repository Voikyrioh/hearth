//! Ce que Hearth lit du jeu pour juger des mods (HRT-256, BR-MOD-021, BR-MOD-033) : domaine pur, aucune E/S.
//!
//! Trois textes écrits par le jeu ou par l'image, donc HOSTILES, sont lus ici : la ligne de démarrage de
//! `logs/latest.log`, le manifeste du pack Modrinth et les lignes d'origine de la base. Toute valeur passe une forme
//! avant d'être gardée ; une valeur qui n'a pas la forme n'existe pas, elle n'est jamais devinée ni recopiée.

use std::collections::HashSet;

use serde_json::Value;

use super::game::is_game_version;
use super::jar::JarLoader;

/// La ligne que le code du serveur de Mojang écrit au démarrage (identique sous Fabric, Quilt, Forge, NeoForge).
const LOG_MARK: &str = "Starting minecraft server version ";
/// Seuls les premiers octets du journal sont lus (la ligne est écrite au tout début).
pub const LOG_READ_BYTES: u64 = 64 * 1024;
/// Taille maximale du manifeste du pack Modrinth.
pub const MANIFEST_READ_BYTES: u64 = 1 << 20;
/// Au plus ce nombre de fichiers gardés du manifeste.
const MAX_MANIFEST_FILES: usize = 16_384;

/// La version du jeu écrite par le serveur au démarrage, si elle a la forme d'une version.
pub fn version_from_log(bytes: &[u8]) -> Option<String> {
    // Le texte est lu par octets : un octet invalide ne fait pas échouer la lecture du reste.
    let text = String::from_utf8_lossy(bytes);
    for line in text.lines() {
        let Some(at) = line.find(LOG_MARK) else {
            continue;
        };
        let rest = &line[at + LOG_MARK.len()..];
        let word = rest.split_whitespace().next()?;
        return is_game_version(word).then(|| word.to_owned());
    }
    None
}

/// Ce que le manifeste du pack Modrinth dit : le jeu, le moteur et les fichiers du pack.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModrinthManifest {
    pub minecraft: Option<String>,
    pub loader: Option<JarLoader>,
    /// Les noms des `.jar` de `mods/` que le pack liste (jamais un autre chemin).
    pub mod_files: HashSet<String>,
}

fn loader_of_key(key: &str) -> Option<JarLoader> {
    match key {
        "fabric-loader" => Some(JarLoader::Fabric),
        "quilt-loader" => Some(JarLoader::Quilt),
        "forge" => Some(JarLoader::Forge),
        "neoforge" => Some(JarLoader::NeoForge),
        _ => None,
    }
}

/// Lit le manifeste. `None` si ce n'est pas du JSON ou pas un objet.
pub fn parse_modrinth_manifest(bytes: &[u8]) -> Option<ModrinthManifest> {
    let value: Value = serde_json::from_slice(bytes).ok()?;
    let object = value.as_object()?;
    let mut manifest = ModrinthManifest::default();
    if let Some(dependencies) = object.get("dependencies").and_then(Value::as_object) {
        manifest.minecraft = dependencies
            .get("minecraft")
            .and_then(Value::as_str)
            .filter(|text| is_game_version(text))
            .map(str::to_owned);
        manifest.loader = dependencies.keys().find_map(|key| loader_of_key(key));
    }
    if let Some(files) = object.get("files").and_then(Value::as_array) {
        for file in files.iter().take(MAX_MANIFEST_FILES) {
            let path = file
                .as_str()
                .or_else(|| file.get("path").and_then(Value::as_str));
            if let Some(name) = path.and_then(|path| path.strip_prefix("mods/"))
                && !name.contains('/')
            {
                manifest.mod_files.insert(name.to_owned());
            }
        }
    }
    Some(manifest)
}

/// D'où vient un fichier (BR-MOD-033), dans l'ordre de priorité de la lecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    ModrinthPack,
    Hearth,
    ServerPack,
    Manual,
}

/// Ce que la base sait d'un fichier qu'elle a vu poser : le pack serveur (taille et CRC-32) ou Hearth (taille et
/// SHA-256).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginRow {
    /// Le nom sans `.disabled`.
    pub file_name: String,
    pub size: u64,
    pub crc32: Option<u32>,
    pub sha256: Option<String>,
    pub from_hearth: bool,
}

/// L'identité d'un fichier lu : son nom sans `.disabled`, sa taille et ses deux empreintes (calculées pendant la
/// lecture entière du jar).
#[derive(Debug, Clone, Copy)]
pub struct FileFacts<'a> {
    pub active_name: &'a str,
    pub size: u64,
    pub crc32: Option<u32>,
    pub sha256: Option<&'a str>,
}

/// Le manifeste Modrinth, puis une ligne `hearth` (nom, taille ET SHA-256 égaux), puis une ligne `server_pack` (nom,
/// taille ET CRC-32 égaux), sinon « ajouté à la main » : une restauration qui ramène un fichier sans sa ligne, ou un
/// fichier dont le contenu a changé, n'est jamais attribué par supposition.
pub fn resolve_origin(
    facts: &FileFacts<'_>,
    manifest: Option<&ModrinthManifest>,
    rows: &[OriginRow],
) -> Origin {
    if manifest.is_some_and(|m| m.mod_files.contains(facts.active_name)) {
        return Origin::ModrinthPack;
    }
    let same_file = |row: &&OriginRow| row.file_name == facts.active_name && row.size == facts.size;
    let hearth = rows.iter().filter(same_file).any(|row| {
        row.from_hearth
            && row
                .sha256
                .as_deref()
                .zip(facts.sha256)
                .is_some_and(|(left, right)| left.eq_ignore_ascii_case(right))
    });
    if hearth {
        return Origin::Hearth;
    }
    let pack = rows
        .iter()
        .filter(same_file)
        .any(|row| !row.from_hearth && row.crc32.is_some() && row.crc32 == facts.crc32);
    if pack {
        return Origin::ServerPack;
    }
    Origin::Manual
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_version_is_the_word_after_the_server_line() {
        let log = b"[12:00:00] [main/INFO]: Loading\n[12:00:01] [Server thread/INFO]: Starting minecraft server version 1.20.1\n";
        assert_eq!(version_from_log(log).as_deref(), Some("1.20.1"));
    }

    #[test]
    fn a_word_that_is_not_a_version_or_no_line_gives_nothing() {
        assert_eq!(
            version_from_log(
                b"Starting minecraft server version 24w14a
"
            )
            .as_deref(),
            Some("24w14a")
        );
        assert_eq!(
            version_from_log(b"Starting minecraft server version ../x\n"),
            None
        );
        assert_eq!(version_from_log(b"nothing here\n"), None);
        assert_eq!(version_from_log(b""), None);
        assert_eq!(version_from_log(&[0xff, 0xfe, b'\n']), None);
    }

    #[test]
    fn the_manifest_gives_game_loader_and_the_mod_files_only() {
        let json =
            br#"{"files":["mods/a.jar","config/x.toml","mods/sub/b.jar",{"path":"mods/c.jar"}],
            "dependencies":{"minecraft":"1.20.1","fabric-loader":"0.15.0"}}"#;
        let manifest = parse_modrinth_manifest(json).expect("manifeste");
        assert_eq!(manifest.minecraft.as_deref(), Some("1.20.1"));
        assert_eq!(manifest.loader, Some(JarLoader::Fabric));
        let mut files: Vec<_> = manifest.mod_files.iter().cloned().collect();
        files.sort();
        assert_eq!(files, vec!["a.jar".to_owned(), "c.jar".to_owned()]);
    }

    #[test]
    fn a_hostile_manifest_is_ignored_not_trusted() {
        assert_eq!(parse_modrinth_manifest(b"not json"), None);
        assert_eq!(parse_modrinth_manifest(b"[1,2]"), None);
        let manifest =
            parse_modrinth_manifest(br#"{"dependencies":{"minecraft":"../../etc"}}"#).unwrap();
        assert_eq!(manifest.minecraft, None);
        assert_eq!(manifest.loader, None);
    }

    fn row(
        name: &str,
        size: u64,
        crc: Option<u32>,
        sha: Option<&str>,
        from_hearth: bool,
    ) -> OriginRow {
        OriginRow {
            file_name: name.to_owned(),
            size,
            crc32: crc,
            sha256: sha.map(str::to_owned),
            from_hearth,
        }
    }

    fn facts<'a>(
        name: &'a str,
        size: u64,
        crc: Option<u32>,
        sha: Option<&'a str>,
    ) -> FileFacts<'a> {
        FileFacts {
            active_name: name,
            size,
            crc32: crc,
            sha256: sha,
        }
    }

    #[test]
    fn the_origin_follows_the_priority_and_never_guesses() {
        let sha = "a".repeat(64);
        let rows = vec![
            row("p.jar", 10, Some(7), None, false),
            row("h.jar", 20, None, Some(&sha), true),
        ];
        let mut manifest = ModrinthManifest::default();
        manifest.mod_files.insert("m.jar".to_owned());
        assert_eq!(
            resolve_origin(&facts("p.jar", 10, Some(7), None), None, &rows),
            Origin::ServerPack
        );
        assert_eq!(
            resolve_origin(&facts("h.jar", 20, None, Some(&sha)), None, &rows),
            Origin::Hearth
        );
        assert_eq!(
            resolve_origin(&facts("m.jar", 1, None, None), Some(&manifest), &rows),
            Origin::ModrinthPack
        );
        // Même nom, contenu changé : à la main.
        assert_eq!(
            resolve_origin(&facts("p.jar", 10, Some(8), None), None, &rows),
            Origin::Manual
        );
        assert_eq!(
            resolve_origin(&facts("p.jar", 11, Some(7), None), None, &rows),
            Origin::Manual
        );
        assert_eq!(
            resolve_origin(
                &facts("h.jar", 20, None, Some(&"b".repeat(64))),
                None,
                &rows
            ),
            Origin::Manual
        );
        assert_eq!(
            resolve_origin(&facts("z.jar", 1, None, None), None, &rows),
            Origin::Manual
        );
        // Le manifeste passe avant la base.
        let mut both = ModrinthManifest::default();
        both.mod_files.insert("p.jar".to_owned());
        assert_eq!(
            resolve_origin(&facts("p.jar", 10, Some(7), None), Some(&both), &rows),
            Origin::ModrinthPack
        );
    }
}

//! La carte d'identité d'un mod `.jar` (HRT-254, BR-MOD-021, BR-MOD-022, BR-MOD-029) : domaine pur, aucune E/S.
//!
//! Le jar est une archive HOSTILE. Ce module ne reçoit que quelques petits fichiers de description (1 Mio chacun
//! au plus, lus par l'appelant après un `inspect` complet) : jamais une archive contenue, jamais une classe,
//! rien d'exécuté. Toute valeur lue passe une forme avant d'être gardée ; sinon elle est ignorée, jamais
//! recopiée. Aucune panique : un fichier mal formé ne donne simplement pas d'identité.

use serde_json::Value;

use super::pack::DescriptionReader;
use super::version_req::VersionReq;
use crate::domain::catalog::loading::value::is_risky_char;

/// Les seuls fichiers lus dans un jar, dans cet ordre.
pub const IDENTITY_FILES: [&str; 5] = [
    "fabric.mod.json",
    "quilt.mod.json",
    "META-INF/neoforge.mods.toml",
    "META-INF/mods.toml",
    "META-INF/MANIFEST.MF",
];

/// Taille maximale d'un fichier d'identité (défense en profondeur : l'appelant la borne déjà).
pub const MAX_IDENTITY_FILE: usize = 1 << 20;
/// Au plus ce nombre de dépendances et d'identifiants fournis gardés par mod.
const MAX_LIST: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JarLoader {
    Fabric,
    Quilt,
    Forge,
    NeoForge,
}

impl JarLoader {
    pub const fn variant_id(self) -> &'static str {
        match self {
            Self::Fabric => "fabric",
            Self::Quilt => "quilt",
            Self::Forge => "forge",
            Self::NeoForge => "neoforge",
        }
    }
}

/// Où le mod tourne, d'après ce qu'il dit de lui-même.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Environment {
    Both,
    ServerOnly,
    ClientOnly,
    Unknown,
}

/// Un identifiant de mod : `[a-z][a-z0-9_.-]{0,63}`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModId(String);

impl ModId {
    pub fn parse(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        let ok = !bytes.is_empty()
            && bytes.len() <= 64
            && bytes[0].is_ascii_lowercase()
            && bytes.iter().all(|b| {
                b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'.' | b'-')
            });
        ok.then(|| Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Une version de mod : `[A-Za-z0-9.+_-]{1,64}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModVersion(String);

impl ModVersion {
    pub fn parse(text: &str) -> Option<Self> {
        let ok = !text.is_empty()
            && text.len() <= 64
            && text
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'+' | b'_' | b'-'));
        ok.then(|| Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Une dépendance obligatoire d'un mod (hors Minecraft, moteur et Java).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    pub id: ModId,
    pub range_understood: bool,
}

/// Ce qu'un jar dit de lui-même pour UN moteur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JarIdentity {
    pub loader: JarLoader,
    pub id: ModId,
    pub name: Option<String>,
    /// Absente quand le jar ne la dit pas (jamais inventée).
    pub version: Option<ModVersion>,
    /// La plage de Minecraft visée : `None` si absente OU non comprise ; `minecraft_declared` les distingue.
    pub minecraft: Option<VersionReq>,
    pub minecraft_declared: bool,
    pub depends: Vec<Dependency>,
    pub provides: Vec<ModId>,
    pub environment: Environment,
}

/// Un nom affiché : 1 à 64 caractères, sans contrôle ni caractère bidirectionnel.
fn display_name(text: &str) -> Option<String> {
    let text = text.trim();
    let count = text.chars().count();
    ((1..=64).contains(&count) && !text.chars().any(is_risky_char)).then(|| text.to_owned())
}

/// Toutes les identités que le jar déclare (un jar peut viser Fabric ET Quilt).
pub fn identify(files: &mut dyn DescriptionReader) -> Vec<JarIdentity> {
    let mut read = |name: &str| {
        files
            .read(name)
            .ok()
            .filter(|bytes| bytes.len() <= MAX_IDENTITY_FILE)
    };
    let mut out = Vec::new();
    if let Some(bytes) = read(IDENTITY_FILES[0]) {
        out.extend(fabric(&bytes));
    }
    if let Some(bytes) = read(IDENTITY_FILES[1]) {
        out.extend(quilt(&bytes));
    }
    let manifest = read(IDENTITY_FILES[4]);
    if let Some(bytes) = read(IDENTITY_FILES[2]) {
        out.extend(forge_like(&bytes, JarLoader::NeoForge, manifest.as_deref()));
    }
    if let Some(bytes) = read(IDENTITY_FILES[3]) {
        out.extend(forge_like(&bytes, JarLoader::Forge, manifest.as_deref()));
    }
    out
}

/// Une plage Fabric ou Quilt : une chaîne ou un tableau de chaînes.
fn fabric_range(value: &Value) -> Option<VersionReq> {
    match value {
        Value::String(text) => VersionReq::parse_fabric(&[text.as_str()]),
        Value::Array(items) => {
            let texts: Option<Vec<&str>> = items.iter().map(Value::as_str).collect();
            VersionReq::parse_fabric(&texts?)
        }
        _ => None,
    }
}

fn parse_json(bytes: &[u8]) -> Option<Value> {
    // `serde_json` borne la profondeur (128) : un JSON très profond est une erreur, jamais une panique.
    serde_json::from_slice(bytes).ok()
}

fn fabric(bytes: &[u8]) -> Option<JarIdentity> {
    let root = parse_json(bytes)?;
    let id = ModId::parse(root.get("id")?.as_str()?)?;
    let version = root
        .get("version")
        .and_then(Value::as_str)
        .and_then(ModVersion::parse);
    let name = root
        .get("name")
        .and_then(Value::as_str)
        .and_then(display_name);
    let mut minecraft = None;
    let mut minecraft_declared = false;
    let mut depends = Vec::new();
    if let Some(map) = root.get("depends").and_then(Value::as_object) {
        for (key, range) in map.iter().take(MAX_LIST) {
            match key.as_str() {
                "minecraft" => {
                    minecraft_declared = true;
                    minecraft = fabric_range(range);
                }
                "java" | "fabricloader" | "fabric-loader" => {}
                other => {
                    if let Some(dep) = ModId::parse(other) {
                        depends.push(Dependency {
                            id: dep,
                            range_understood: fabric_range(range).is_some(),
                        });
                    }
                }
            }
        }
    }
    let provides = root
        .get("provides")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .take(MAX_LIST)
                .filter_map(|v| v.as_str().and_then(ModId::parse))
                .collect()
        })
        .unwrap_or_default();
    let environment = match root.get("environment").and_then(Value::as_str) {
        Some("*") => Environment::Both,
        Some("server") => Environment::ServerOnly,
        Some("client") => Environment::ClientOnly,
        _ => Environment::Unknown,
    };
    Some(JarIdentity {
        loader: JarLoader::Fabric,
        id,
        name,
        version,
        minecraft,
        minecraft_declared,
        depends,
        provides,
        environment,
    })
}

fn quilt(bytes: &[u8]) -> Option<JarIdentity> {
    let root = parse_json(bytes)?;
    let loader = root.get("quilt_loader")?;
    let id = ModId::parse(loader.get("id")?.as_str()?)?;
    let version = loader
        .get("version")
        .and_then(Value::as_str)
        .and_then(ModVersion::parse);
    let name = loader
        .get("metadata")
        .and_then(|m| m.get("name"))
        .and_then(Value::as_str)
        .and_then(display_name);
    let mut minecraft = None;
    let mut minecraft_declared = false;
    let mut depends = Vec::new();
    if let Some(items) = loader.get("depends").and_then(Value::as_array) {
        for item in items.iter().take(MAX_LIST) {
            let (key, range, optional) = match item {
                Value::String(text) => (text.as_str(), None, false),
                Value::Object(map) => (
                    map.get("id").and_then(Value::as_str).unwrap_or(""),
                    map.get("versions"),
                    map.get("optional")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                ),
                _ => continue,
            };
            if optional {
                continue;
            }
            match key {
                "minecraft" => {
                    minecraft_declared = true;
                    minecraft = range.and_then(fabric_range);
                }
                "java" | "quilt_loader" => {}
                other => {
                    if let Some(dep) = ModId::parse(other) {
                        depends.push(Dependency {
                            id: dep,
                            range_understood: range.is_none_or(|r| fabric_range(r).is_some()),
                        });
                    }
                }
            }
        }
    }
    let provides = loader
        .get("provides")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .take(MAX_LIST)
                .filter_map(|v| match v {
                    Value::String(text) => ModId::parse(text),
                    Value::Object(map) => {
                        map.get("id").and_then(Value::as_str).and_then(ModId::parse)
                    }
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    let environment = match root
        .get("minecraft")
        .and_then(|m| m.get("environment"))
        .and_then(Value::as_str)
    {
        Some("*") => Environment::Both,
        Some("dedicated_server") => Environment::ServerOnly,
        Some("client") => Environment::ClientOnly,
        _ => Environment::Unknown,
    };
    Some(JarIdentity {
        loader: JarLoader::Quilt,
        id,
        name,
        version,
        minecraft,
        minecraft_declared,
        depends,
        provides,
        environment,
    })
}

/// `Implementation-Version` de `META-INF/MANIFEST.MF` (lignes de continuation ignorées).
fn manifest_version(bytes: &[u8]) -> Option<ModVersion> {
    let text = std::str::from_utf8(bytes).ok()?;
    text.lines()
        .find_map(|line| line.strip_prefix("Implementation-Version:"))
        .and_then(|v| ModVersion::parse(v.trim()))
}

fn forge_like(bytes: &[u8], loader: JarLoader, manifest: Option<&[u8]>) -> Option<JarIdentity> {
    let text = std::str::from_utf8(bytes).ok()?;
    let root: toml::Table = text.parse().ok()?;
    let mods = root.get("mods")?.as_array()?;
    let first = mods.first()?.as_table()?;
    let id = ModId::parse(first.get("modId")?.as_str()?)?;
    let version = match first.get("version").and_then(toml::Value::as_str) {
        Some("${file.jarVersion}") => manifest.and_then(manifest_version),
        Some(text) => ModVersion::parse(text),
        None => None,
    };
    let name = first
        .get("displayName")
        .and_then(toml::Value::as_str)
        .and_then(display_name);
    let provides: Vec<ModId> = mods
        .iter()
        .skip(1)
        .take(MAX_LIST)
        .filter_map(|m| {
            m.as_table()
                .and_then(|t| t.get("modId"))
                .and_then(toml::Value::as_str)
                .and_then(ModId::parse)
        })
        .collect();
    let mut minecraft = None;
    let mut minecraft_declared = false;
    let mut depends = Vec::new();
    if let Some(deps) = root
        .get("dependencies")
        .and_then(toml::Value::as_table)
        .and_then(|t| t.get(id.as_str()))
        .and_then(toml::Value::as_array)
    {
        for dep in deps.iter().take(MAX_LIST) {
            let Some(dep) = dep.as_table() else { continue };
            let required = match (dep.get("mandatory"), dep.get("type")) {
                (Some(toml::Value::Boolean(b)), _) => *b,
                (_, Some(toml::Value::String(t))) => t == "required",
                _ => false,
            };
            if !required {
                continue;
            }
            let Some(key) = dep.get("modId").and_then(toml::Value::as_str) else {
                continue;
            };
            let range = dep.get("versionRange").and_then(toml::Value::as_str);
            match key {
                "minecraft" => {
                    minecraft_declared = true;
                    minecraft = range.and_then(VersionReq::parse_maven);
                }
                "java" | "forge" | "neoforge" => {}
                other => {
                    if let Some(dep_id) = ModId::parse(other) {
                        depends.push(Dependency {
                            id: dep_id,
                            range_understood: range
                                .is_none_or(|r| VersionReq::parse_maven(r).is_some()),
                        });
                    }
                }
            }
        }
    }
    let display_test = root
        .get("displayTest")
        .or_else(|| first.get("displayTest"))
        .and_then(toml::Value::as_str);
    let environment = match display_test {
        Some("IGNORE_SERVER_VERSION" | "IGNORE_ALL_VERSION") => Environment::ServerOnly,
        _ => Environment::Unknown,
    };
    Some(JarIdentity {
        loader,
        id,
        name,
        version,
        minecraft,
        minecraft_declared,
        depends,
        provides,
        environment,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::mods::pack::DescriptionError;
    use crate::domain::mods::version_req::{Containment, GameVersion};
    use std::collections::HashMap;

    struct Files(HashMap<&'static str, Vec<u8>>);

    impl DescriptionReader for Files {
        fn read(&mut self, name: &str) -> Result<Vec<u8>, DescriptionError> {
            self.0
                .get(name)
                .cloned()
                .ok_or(DescriptionError::Unreadable)
        }
    }

    fn files(list: &[(&'static str, &str)]) -> Files {
        Files(
            list.iter()
                .map(|(n, c)| (*n, c.as_bytes().to_vec()))
                .collect(),
        )
    }

    const FABRIC: &str = r#"{"schemaVersion":1,"id":"sodium","version":"0.5.8+mc1.20.1","name":"Sodium",
        "environment":"client","depends":{"fabricloader":">=0.14","minecraft":"~1.20.1","java":">=17","fabric-api":"*"},
        "provides":["rubidium"]}"#;

    const QUILT: &str = r#"{"schema_version":1,"quilt_loader":{"id":"lithium","version":"1.0.0",
        "metadata":{"name":"Lithium"},"provides":["lith",{"id":"lith2","version":"1"}],
        "depends":[{"id":"minecraft","versions":">=1.20 <1.21"},"quilt_loader",{"id":"opt","optional":true},{"id":"needed","versions":"*"}]},
        "minecraft":{"environment":"dedicated_server"}}"#;

    const FORGE: &str = r#"
modLoader = "javafml"
displayTest = "IGNORE_SERVER_VERSION"
[[mods]]
modId = "clumps"
version = "${file.jarVersion}"
displayName = "Clumps"
[[dependencies.clumps]]
modId = "minecraft"
mandatory = true
versionRange = "[1.20.1,1.21)"
[[dependencies.clumps]]
modId = "forge"
mandatory = true
versionRange = "[47,)"
[[dependencies.clumps]]
modId = "curios"
mandatory = true
versionRange = "[1,)"
[[dependencies.clumps]]
modId = "optionalmod"
mandatory = false
"#;

    #[test]
    fn a_fabric_jar_gives_its_identity() {
        let ids = identify(&mut files(&[("fabric.mod.json", FABRIC)]));
        assert_eq!(ids.len(), 1);
        let id = &ids[0];
        assert_eq!(id.loader, JarLoader::Fabric);
        assert_eq!(id.id.as_str(), "sodium");
        assert_eq!(id.version.as_ref().unwrap().as_str(), "0.5.8+mc1.20.1");
        assert_eq!(id.name.as_deref(), Some("Sodium"));
        assert_eq!(id.environment, Environment::ClientOnly);
        assert_eq!(id.provides[0].as_str(), "rubidium");
        assert_eq!(id.depends.len(), 1);
        assert_eq!(id.depends[0].id.as_str(), "fabric-api");
        assert!(id.minecraft_declared);
        let req = id.minecraft.as_ref().unwrap();
        assert_eq!(
            req.contains(GameVersion::parse("1.20.4").as_ref()),
            Containment::Yes
        );
        assert_eq!(
            req.contains(GameVersion::parse("1.21").as_ref()),
            Containment::No
        );
    }

    #[test]
    fn a_quilt_jar_and_a_jar_with_both_files_give_both_identities() {
        let ids = identify(&mut files(&[
            ("quilt.mod.json", QUILT),
            ("fabric.mod.json", FABRIC),
        ]));
        assert_eq!(
            ids.iter().map(|i| i.loader).collect::<Vec<_>>(),
            vec![JarLoader::Fabric, JarLoader::Quilt]
        );
        let quilt = &ids[1];
        assert_eq!(quilt.id.as_str(), "lithium");
        assert_eq!(quilt.environment, Environment::ServerOnly);
        assert_eq!(quilt.provides.len(), 2);
        assert_eq!(
            quilt
                .depends
                .iter()
                .map(|d| d.id.as_str())
                .collect::<Vec<_>>(),
            vec!["needed"]
        );
        assert!(quilt.minecraft.is_some());
    }

    #[test]
    fn a_forge_jar_resolves_its_version_from_the_manifest_or_leaves_it_absent() {
        let with = identify(&mut files(&[
            ("META-INF/mods.toml", FORGE),
            (
                "META-INF/MANIFEST.MF",
                "Manifest-Version: 1.0\nImplementation-Version: 1.20.1-2.0.5\n",
            ),
        ]));
        assert_eq!(with[0].loader, JarLoader::Forge);
        assert_eq!(with[0].version.as_ref().unwrap().as_str(), "1.20.1-2.0.5");
        assert_eq!(with[0].environment, Environment::ServerOnly);
        assert_eq!(
            with[0]
                .depends
                .iter()
                .map(|d| d.id.as_str())
                .collect::<Vec<_>>(),
            vec!["curios"]
        );
        let without = identify(&mut files(&[("META-INF/mods.toml", FORGE)]));
        assert!(without[0].version.is_none());
        let neo = identify(&mut files(&[("META-INF/neoforge.mods.toml", FORGE)]));
        assert_eq!(neo[0].loader, JarLoader::NeoForge);
    }

    #[test]
    fn hostile_files_give_no_identity_and_never_panic() {
        let big = vec![b' '; MAX_IDENTITY_FILE + 1];
        let mut oversized = Files(HashMap::from([("fabric.mod.json", big)]));
        assert!(identify(&mut oversized).is_empty());
        let deep = format!("{}{}", "[".repeat(5000), "]".repeat(5000));
        assert!(identify(&mut files(&[("fabric.mod.json", &deep)])).is_empty());
        let dup = "[[mods]]\nmodId = \"a\"\nmodId = \"b\"\n";
        assert!(identify(&mut files(&[("META-INF/mods.toml", dup)])).is_empty());
        for bad in [
            r#"{"id":"Bad Id","version":"1"}"#,
            r#"{"id":"../x","version":"1"}"#,
            r#"{"id":"","version":"1"}"#,
            "not json",
            "[]",
            "null",
        ] {
            assert!(
                identify(&mut files(&[("fabric.mod.json", bad)])).is_empty(),
                "{bad}"
            );
        }
        // Valeurs hors forme : ignorées, jamais recopiées.
        let odd = r#"{"id":"ok","version":"1 2\u202e","name":"Nom\u202eRenvers\u00e9"}"#;
        let ids = identify(&mut files(&[("fabric.mod.json", odd)]));
        assert_eq!(ids.len(), 1);
        assert!(ids[0].version.is_none());
        assert!(ids[0].name.is_none());
        let ctrl = "{\"id\":\"ok\",\"name\":\"a\\u0007b\"}";
        assert!(
            identify(&mut files(&[("fabric.mod.json", ctrl)]))[0]
                .name
                .is_none()
        );
        let long = format!(r#"{{"id":"ok","name":"{}"}}"#, "x".repeat(65));
        assert!(
            identify(&mut files(&[("fabric.mod.json", &long)]))[0]
                .name
                .is_none()
        );
        assert!(ModVersion::parse(&"1".repeat(65)).is_none());
        assert!(ModId::parse("Upper").is_none() && ModId::parse("1abc").is_none());
    }

    #[test]
    fn a_range_not_understood_is_declared_but_absent() {
        let snap = r#"{"id":"ok","version":"1","depends":{"minecraft":"24w14a"}}"#;
        let id = &identify(&mut files(&[("fabric.mod.json", snap)]))[0];
        assert!(id.minecraft_declared && id.minecraft.is_none());
    }
}

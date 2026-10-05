//! Publication du client Windows (HRT-16, ADR-0017) : les deux contrôles que le flux de publication
//! (`.github/workflows/publish-client.yml`, déclenché à la main) exécute avant de fabriquer quoi que
//! ce soit, et la fabrication du manifeste `latest.json` que le greffon de mise à jour lit.
//!
//! - `client-release-check [--version X.Y.Z]` : refuse la clé publique de DÉVELOPPEMENT du dépôt (sans
//!   clé secrète : rien de ce qu'elle « vérifierait » ne serait jamais installable) et, si une
//!   version est donnée, exige qu'elle soit celle du dépôt.
//! - `client-manifest --version X.Y.Z --signature-file F --url U --notes-file N --date D --out O` :
//!   écrit le manifeste au format statique du greffon de mise à jour de Tauri.
//!
//! Aucun secret ici : la signature de l'installateur se fait dans le flux de publication, avec la
//! clé secrète des secrets du dépôt (runbook `docs/runbooks/publier-une-version-du-client.md`).

use std::path::{Path, PathBuf};

use serde_json::json;

/// Cible du manifeste : doit rester celle du client (`update/feed.rs::TARGET`).
pub const TARGET: &str = "windows-x86_64";
/// Marque du commentaire de la clé de développement (`update/feed.rs::DEV_KEY_MARK`).
pub const DEV_KEY_MARK: &str = "DEV public key";
/// Les installateurs ne sont téléchargés que de ce dépôt (`update/domain.rs`).
pub const DOWNLOAD_PREFIX: &str = "https://github.com/Voikyrioh/hearth/releases/download/";

fn root() -> Result<PathBuf, String> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "racine du dépôt introuvable".to_owned())
}

/// La clé publique du dépôt est-elle utilisable pour une publication ?
pub fn check_key(public_key_file: &str) -> Result<(), String> {
    // Le fichier de minisign, ou celui de `tauri signer generate` (le même texte en base64).
    let decoded = decode_key_file(public_key_file);
    let mut lines = decoded.lines();
    let comment = lines.next().unwrap_or_default();
    let key = lines
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default();
    if comment.contains(DEV_KEY_MARK) {
        return Err(
            "update-key.pub est la clé de DÉVELOPPEMENT : aucune clé secrète n'existe. Génère ta paire et remplace le fichier (docs/runbooks/publier-une-version-du-client.md)"
                .to_owned(),
        );
    }
    if !comment.starts_with("untrusted comment:") || !key.starts_with("RW") {
        return Err("update-key.pub n'est pas une clé publique minisign".to_owned());
    }
    Ok(())
}

fn decode_key_file(file: &str) -> String {
    use base64::Engine as _;
    let file = file.trim();
    if file.starts_with("untrusted comment:") {
        return file.to_owned();
    }
    base64::engine::general_purpose::STANDARD
        .decode(file)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .unwrap_or_default()
}

/// `X.Y.Z` strict (pas de préversion : le flux publié n'en propose jamais).
pub fn check_version(version: &str) -> Result<(), String> {
    let parts: Vec<&str> = version.split('.').collect();
    let numeric = |part: &&str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    if parts.len() == 3 && parts.iter().all(numeric) {
        Ok(())
    } else {
        Err(format!("version « {version} » : attendu X.Y.Z"))
    }
}

/// La version `version = "…"` de `[workspace.package]` du `Cargo.toml` racine.
pub fn workspace_version(cargo_toml: &str) -> Option<String> {
    let mut in_package = false;
    for line in cargo_toml.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_package = line == "[workspace.package]";
        } else if in_package && let Some(rest) = line.strip_prefix("version") {
            let value = rest.trim_start().strip_prefix('=')?.trim();
            return Some(value.trim_matches('"').to_owned());
        }
    }
    None
}

/// Le manifeste statique du greffon : `version`, `notes`, `pub_date`, `platforms`.
pub fn manifest(
    version: &str,
    notes: &str,
    signature: &str,
    url: &str,
    pub_date: &str,
) -> Result<String, String> {
    check_version(version)?;
    let signature = signature.trim();
    if signature.is_empty() {
        return Err("signature vide".to_owned());
    }
    let expected = format!("{DOWNLOAD_PREFIX}v{version}/");
    if !url.starts_with(&expected) {
        return Err(format!("l'adresse doit commencer par {expected}"));
    }
    if time::OffsetDateTime::parse(pub_date, &time::format_description::well_known::Rfc3339)
        .is_err()
    {
        return Err(format!("date « {pub_date} » : attendu RFC 3339"));
    }
    serde_json::to_string_pretty(&json!({
        "version": version,
        "notes": notes.trim(),
        "pub_date": pub_date,
        "platforms": { TARGET: { "signature": signature, "url": url } },
    }))
    .map_err(|error| error.to_string())
}

fn option(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|at| args.get(at + 1))
        .cloned()
}

fn required(args: &[String], name: &str) -> Result<String, String> {
    option(args, name).ok_or_else(|| format!("{name} manquant"))
}

fn read(path: &str) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|error| format!("{path} : {error}"))
}

/// `cargo xtask client-release-check [--version X.Y.Z]`.
pub fn run_check(args: &[String]) -> Result<(), String> {
    let root = root()?;
    let key = read(
        &root
            .join("apps/desktop/src-tauri/update-key.pub")
            .to_string_lossy(),
    )?;
    check_key(&key)?;
    let cargo = read(&root.join("Cargo.toml").to_string_lossy())?;
    let repository = workspace_version(&cargo).ok_or("version du dépôt illisible")?;
    check_version(&repository)?;
    if let Some(version) = option(args, "--version")
        && version != repository
    {
        return Err(format!(
            "la version demandée ({version}) n'est pas celle du dépôt ({repository}) : change `version` de [workspace.package] dans Cargo.toml et celle de apps/desktop/package.json, puis recommence"
        ));
    }
    let package = read(&root.join("apps/desktop/package.json").to_string_lossy())?;
    let package: serde_json::Value = serde_json::from_str(&package).map_err(|e| e.to_string())?;
    if package["version"] != repository.as_str() {
        return Err(format!(
            "apps/desktop/package.json annonce {} et le dépôt {repository}",
            package["version"]
        ));
    }
    println!("publication possible : version {repository}, clé publique de production");
    Ok(())
}

/// `cargo xtask client-manifest …`.
pub fn run_manifest(args: &[String]) -> Result<(), String> {
    let text = manifest(
        &required(args, "--version")?,
        &read(&required(args, "--notes-file")?)?,
        &read(&required(args, "--signature-file")?)?,
        &required(args, "--url")?,
        &required(args, "--date")?,
    )?;
    let out = required(args, "--out")?;
    std::fs::write(&out, format!("{text}\n")).map_err(|error| format!("{out} : {error}"))?;
    println!("manifeste écrit : {out}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str =
        "https://github.com/Voikyrioh/hearth/releases/download/v1.2.0/Hearth_1.2.0_x64-setup.exe";

    #[test]
    fn the_development_key_is_refused() {
        let dev =
            "untrusted comment: Hearth client DEV public key. No secret key exists.\nRWQabc\n";
        assert!(check_key(dev).unwrap_err().contains("DÉVELOPPEMENT"));
    }

    #[test]
    fn a_real_minisign_key_is_accepted_and_garbage_is_not() {
        assert!(
            check_key("untrusted comment: minisign public key: 6FAC17371631EAA5\nRWSl6jEW\n")
                .is_ok()
        );
        assert!(check_key("n'importe quoi").is_err());
        assert!(check_key("untrusted comment: x\nnope\n").is_err());
        assert!(check_key("").is_err());
    }

    #[test]
    fn versions_are_strict() {
        assert!(check_version("1.2.0").is_ok());
        for bad in ["1.2", "1.2.0-beta.1", "v1.2.0", "1..0", "a.b.c", ""] {
            assert!(check_version(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_workspace_version_is_read_from_its_section_only() {
        let toml = "[workspace]\nmembers = []\n\n[workspace.package]\nversion = \"0.3.1\"\nedition = \"2024\"\n\n[workspace.dependencies]\nversion = \"9\"\n";
        assert_eq!(workspace_version(toml).as_deref(), Some("0.3.1"));
        assert_eq!(workspace_version("[package]\nversion = \"1.0.0\"\n"), None);
    }

    #[test]
    fn the_manifest_has_the_shape_the_update_plugin_reads() {
        let text = manifest(
            "1.2.0",
            "  Corrections.\n",
            "c2ln\n",
            URL,
            "2026-10-05T20:00:00Z",
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["version"], "1.2.0");
        assert_eq!(value["notes"], "Corrections.");
        assert_eq!(value["pub_date"], "2026-10-05T20:00:00Z");
        assert_eq!(value["platforms"][TARGET]["signature"], "c2ln");
        assert_eq!(value["platforms"][TARGET]["url"], URL);
    }

    #[test]
    fn the_manifest_refuses_anything_that_the_client_would_refuse() {
        let ok = |url: &str, date: &str, version: &str, signature: &str| {
            manifest(version, "n", signature, url, date)
        };
        assert!(ok(URL, "2026-10-05T20:00:00Z", "1.2.0", "s").is_ok());
        assert!(
            ok(
                "http://github.com/Voikyrioh/hearth/releases/download/v1.2.0/x.exe",
                "2026-10-05T20:00:00Z",
                "1.2.0",
                "s"
            )
            .is_err()
        );
        assert!(
            ok(
                "https://example.com/Voikyrioh/hearth/releases/download/v1.2.0/x.exe",
                "2026-10-05T20:00:00Z",
                "1.2.0",
                "s"
            )
            .is_err()
        );
        assert!(ok(URL, "hier", "1.2.0", "s").is_err());
        assert!(ok(URL, "2026-10-05T20:00:00Z", "1.2.0", "  ").is_err());
        // L'adresse doit être celle de la version annoncée.
        assert!(ok(URL, "2026-10-05T20:00:00Z", "1.3.0", "s").is_err());
    }
}

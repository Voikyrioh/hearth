//! Publication du client Windows (HRT-16, ADR-0017) : les deux contrôles que le flux de publication
//! (`.github/workflows/publish-client.yml`, déclenché à la main) exécute avant de fabriquer quoi que
//! ce soit, et la fabrication du manifeste `latest.json` que le greffon de mise à jour lit.
//!
//! - `client-release-check [--version X.Y.Z]` : refuse la clé publique de DÉVELOPPEMENT du dépôt (sans
//!   clé secrète : rien de ce qu'elle « vérifierait » ne serait jamais installable) et, si une
//!   version est donnée, exige qu'elle soit celle du dépôt et que l'étiquette `vX.Y.Z` n'existe pas
//!   déjà sur `origin` (`git ls-remote --exit-code --tags`, lecture seule : un numéro déjà étiqueté
//!   n'est jamais resigné). L'étiquette n'est posée qu'à la fin, à la main : la fenêtre entre la
//!   signature et l'étiquette n'est pas couverte (le refus de `release-upload` sur une version déjà
//!   déposée la fermera, HRT-51).
//! - `client-version` : la version du dépôt (`[workspace.package]` de `Cargo.toml`), seule source du
//!   numéro publié : le flux de publication n'en reçoit aucun à la main.
//! - `client-sign --installer F --version X.Y.Z` : signe l'installateur (minisign, Ed25519) avec la clé
//!   secrète des variables d'environnement `HEARTH_CLIENT_SIGNING_KEY` et
//!   `HEARTH_CLIENT_SIGNING_KEY_PASSWORD` (mot de passe obligatoire), commentaire de confiance
//!   `timestamp:…\tfile:…\tversion:X.Y.Z`, puis RELIT la signature contre `update-key.pub` du dépôt :
//!   une clé secrète qui n'est pas la paire de la clé publique embarquée est refusée avant toute
//!   publication. Écrit `F.sig` (le contenu du `.sig` en base64, comme Tauri).
//! - `client-manifest --version X.Y.Z --installer-file I --signature-file F --url U --notes-file N
//!   --date D --out O` : vérifie la signature (clé du dépôt, version signée), puis écrit le manifeste
//!   au format statique du greffon de mise à jour de Tauri. `U` est exactement
//!   `https://hearth.voikyrioh.fr/api/releases/{X.Y.Z}/Hearth_{X.Y.Z}_x64-setup.exe` (sans `v`).
//!
//! Aucun secret ici : la signature de l'installateur se fait dans le flux de publication, avec la
//! clé secrète des secrets du dépôt (runbook `docs/runbooks/publier-une-version-du-client.md`).

use std::path::{Path, PathBuf};

use crate::release_core::*;

pub(crate) fn root() -> Result<PathBuf, String> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "racine du dépôt introuvable".to_owned())
}

pub(crate) fn option(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|at| args.get(at + 1))
        .cloned()
}

pub(crate) fn required(args: &[String], name: &str) -> Result<String, String> {
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
    let host = crate::host::System::new();
    check_tag_is_free(&repository, &|tag| tag_lookup(&host, &root, tag))?;
    println!("publication possible : version {repository}, clé publique de production");
    Ok(())
}

/// `git ls-remote --exit-code --tags origin refs/tags/{tag}` : 0 = l'étiquette existe, 2 = elle n'existe
/// pas, autre = interrogation impossible. Lecture seule, derrière `Host` (faux dans les tests). Le nom
/// vient de `check_tag_is_free` ou de `release-preflight`, déjà validé `vX.Y.Z` ; arguments en liste,
/// aucun interpréteur. Seule lecture d'étiquette de `xtask`.
pub(crate) fn tag_lookup(
    host: &dyn crate::host::Host,
    root: &Path,
    tag: &str,
) -> Result<TagLookup, String> {
    let argv = [
        "-C".to_owned(),
        root.to_string_lossy().into_owned(),
        "ls-remote".to_owned(),
        "--exit-code".to_owned(),
        "--tags".to_owned(),
        "origin".to_owned(),
        format!("refs/tags/{tag}"),
    ];
    let output = host.run(&crate::host::Call::new("git", &argv))?;
    match output.code {
        Some(0) => Ok(TagLookup::Present),
        Some(2) => Ok(TagLookup::Absent),
        other => Err(format!(
            "git ls-remote : code {other:?} {}",
            output.stderr.lines().next().unwrap_or("").trim()
        )),
    }
}

/// `cargo xtask client-version` : affiche la version du dépôt, rien d'autre.
pub fn run_version() -> Result<(), String> {
    let cargo = read(&root()?.join("Cargo.toml").to_string_lossy())?;
    let version = workspace_version(&cargo).ok_or("version du dépôt illisible")?;
    check_version(&version)?;
    println!("{version}");
    Ok(())
}

fn repository_key() -> Result<String, String> {
    read(
        &root()?
            .join("apps/desktop/src-tauri/update-key.pub")
            .to_string_lossy(),
    )
}

/// `cargo xtask client-sign --installer F --version X.Y.Z` (secrets dans l'environnement).
pub fn run_sign(args: &[String]) -> Result<(), String> {
    let installer = required(args, "--installer")?;
    let version = required(args, "--version")?;
    let secret = std::env::var("HEARTH_CLIENT_SIGNING_KEY").unwrap_or_default();
    let password = std::env::var("HEARTH_CLIENT_SIGNING_KEY_PASSWORD").unwrap_or_default();
    if secret.trim().is_empty() {
        return Err("HEARTH_CLIENT_SIGNING_KEY est absent ou vide".to_owned());
    }
    check_key(&repository_key()?)?;
    let data = std::fs::read(&installer).map_err(|error| format!("{installer} : {error}"))?;
    let name = Path::new(&installer)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or("nom de fichier illisible")?;
    if !name.contains(&version) {
        return Err(format!(
            "{name} ne porte pas la version {version} : mauvais installateur ?"
        ));
    }
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let signature = sign_installer(&secret, &password, &data, &name, &version, timestamp)?;
    // La paire : relue contre la clé publique du dépôt, avec la version.
    verify_signature(&repository_key()?, &signature, &data, &version)?;
    let out = format!("{installer}.sig");
    std::fs::write(&out, format!("{signature}\n")).map_err(|error| format!("{out} : {error}"))?;
    println!("signature écrite et vérifiée contre update-key.pub : {out}");
    Ok(())
}

/// `cargo xtask client-manifest …`.
pub fn run_manifest(args: &[String]) -> Result<(), String> {
    // Ce que le client refuserait n'est jamais publié : signature vérifiée contre la clé du dépôt.
    let installer = required(args, "--installer-file")?;
    verify_signature(
        &repository_key()?,
        &read(&required(args, "--signature-file")?)?,
        &std::fs::read(&installer).map_err(|error| format!("{installer} : {error}"))?,
        &required(args, "--version")?,
    )?;
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
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD;

    use super::*;

    const URL: &str = "https://hearth.voikyrioh.fr/api/releases/1.2.0/Hearth_1.2.0_x64-setup.exe";

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

    fn pair(password: &str) -> (String, String) {
        let keys =
            minisign::KeyPair::generate_encrypted_keypair(Some(password.to_owned())).unwrap();
        (
            keys.sk.to_box(None).unwrap().to_string(),
            keys.pk.to_box().unwrap().to_string(),
        )
    }

    #[test]
    fn a_signature_carries_the_version_and_verifies_against_the_public_key() {
        let (secret, public) = pair("mot de passe");
        let data = b"installateur";
        let signature = sign_installer(
            &secret,
            "mot de passe",
            data,
            "Hearth_1.2.0_x64-setup.exe",
            "1.2.0",
            7,
        )
        .unwrap();
        assert!(verify_signature(&public, &signature, data, "1.2.0").is_ok());
        // Format de `tauri signer generate` : le même texte en base64.
        let tauri_secret = STANDARD.encode(&secret);
        let again = sign_installer(
            &tauri_secret,
            "mot de passe",
            data,
            "x_1.2.0_x.exe",
            "1.2.0",
            7,
        )
        .unwrap();
        assert!(verify_signature(&STANDARD.encode(&public), &again, data, "1.2.0").is_ok());
    }

    #[test]
    fn a_wrong_version_file_key_or_password_is_refused() {
        let (secret, public) = pair("pw");
        let (_, other_public) = pair("pw");
        let data = b"installateur";
        let signature = sign_installer(
            &secret,
            "pw",
            data,
            "Hearth_1.2.0_x64-setup.exe",
            "1.2.0",
            7,
        )
        .unwrap();
        assert!(
            verify_signature(&public, &signature, data, "1.2.1")
                .unwrap_err()
                .contains("1.2.0")
        );
        assert!(verify_signature(&public, &signature, b"autre", "1.2.0").is_err());
        assert!(
            verify_signature(&other_public, &signature, data, "1.2.0")
                .unwrap_err()
                .contains("paire")
        );
        assert!(verify_signature(&public, "pas du base64 !", data, "1.2.0").is_err());
        assert!(sign_installer(&secret, "faux", data, "x", "1.2.0", 7).is_err());
        assert!(
            sign_installer(&secret, "", data, "x", "1.2.0", 7)
                .unwrap_err()
                .contains("obligatoire")
        );
        assert!(sign_installer("pas une clé", "pw", data, "x", "1.2.0", 7).is_err());
    }

    #[test]
    fn versions_are_strict() {
        assert!(check_version("1.2.0").is_ok());
        for bad in [
            "1.2",
            "1.2.0-beta.1",
            "v1.2.0",
            "1..0",
            "a.b.c",
            "",
            "01.2.0",
            "1.02.0",
        ] {
            assert!(check_version(bad).is_err(), "{bad}");
        }
        assert!(check_version("0.0.0").is_ok());
        assert!(check_version("10.20.30").is_ok());
    }

    #[test]
    fn a_tag_that_already_exists_on_origin_refuses_the_publication() {
        let seen = std::cell::RefCell::new(Vec::new());
        let lookup = |answer: Result<TagLookup, String>| {
            let seen = &seen;
            move |tag: &str| {
                seen.borrow_mut().push(tag.to_owned());
                answer.clone()
            }
        };
        assert!(check_tag_is_free("1.2.0", &lookup(Ok(TagLookup::Absent))).is_ok());
        assert_eq!(
            seen.borrow().as_slice(),
            ["v1.2.0"],
            "l'étiquette demandée est vX.Y.Z"
        );
        let error = check_tag_is_free("1.2.0", &lookup(Ok(TagLookup::Present))).unwrap_err();
        assert!(
            error.contains("v1.2.0") && error.contains("brûlé"),
            "{error}"
        );
        // Sans réponse (réseau coupé, origin absent) : refusé, jamais « libre par défaut ».
        let error =
            check_tag_is_free("1.2.0", &lookup(Err("délai dépassé".to_owned()))).unwrap_err();
        assert!(error.contains("délai dépassé") && error.contains("rien n'est signé"));
        // Une version mal formée n'interroge rien.
        seen.borrow_mut().clear();
        assert!(check_tag_is_free("v1.2.0", &lookup(Ok(TagLookup::Absent))).is_err());
        assert!(seen.borrow().is_empty());
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
        let date = "2026-10-05T20:00:00Z";
        for bad in [
            // Pas HTTPS, autre hôte, hôte en suffixe, ancienne source (GitHub).
            "http://hearth.voikyrioh.fr/api/releases/1.2.0/Hearth_1.2.0_x64-setup.exe",
            "https://example.com/api/releases/1.2.0/Hearth_1.2.0_x64-setup.exe",
            "https://hearth.voikyrioh.fr.evil.test/api/releases/1.2.0/Hearth_1.2.0_x64-setup.exe",
            "https://github.com/Voikyrioh/hearth/releases/download/v1.2.0/Hearth_1.2.0_x64-setup.exe",
            // `v` devant la version (ancien format), autre dossier, autre fichier, hors liste fermée.
            "https://hearth.voikyrioh.fr/api/releases/v1.2.0/Hearth_1.2.0_x64-setup.exe",
            "https://hearth.voikyrioh.fr/api/releases/1.1.0/Hearth_1.2.0_x64-setup.exe",
            "https://hearth.voikyrioh.fr/api/releases/1.2.0/x.exe",
            "https://hearth.voikyrioh.fr/api/releases/1.2.0/hearth-agent-linux-x86_64",
            "https://hearth.voikyrioh.fr/api/releases/1.2.0/Hearth_1.2.0_x64-setup.exe.sig",
            // Port, identifiant, requête, fragment.
            "https://hearth.voikyrioh.fr:8443/api/releases/1.2.0/Hearth_1.2.0_x64-setup.exe",
            "https://u:p@hearth.voikyrioh.fr/api/releases/1.2.0/Hearth_1.2.0_x64-setup.exe",
            "https://hearth.voikyrioh.fr/api/releases/1.2.0/Hearth_1.2.0_x64-setup.exe?x=1",
            "https://hearth.voikyrioh.fr/api/releases/1.2.0/Hearth_1.2.0_x64-setup.exe#a",
        ] {
            assert!(ok(bad, date, "1.2.0", "s").is_err(), "{bad}");
        }
        assert!(ok(URL, "hier", "1.2.0", "s").is_err());
        assert!(ok(URL, "2026-10-05T20:00:00Z", "1.2.0", "  ").is_err());
        // L'adresse doit être celle de la version annoncée.
        assert!(ok(URL, "2026-10-05T20:00:00Z", "1.3.0", "s").is_err());
    }
}

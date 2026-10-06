//! Publication de la cible de l'AGENT (HRT-17, ADR-0021) : ajoute la section `agent` au `latest.json` du
//! client, le fichier que le client lit pour proposer la mise à jour de l'agent (une seule requête).
//!
//! - `agent-manifest --manifest latest.json --version X.Y.Z --binary-file F --signature-file S --url U
//!   [--out O]` : calcule la somme SHA-256 du binaire, VÉRIFIE la signature minisign (`minisign -S`,
//!   faite à la main avec la clé secrète de l'agent) contre `crates/hearth-agent/update-key.pub` (la clé
//!   embarquée dans l'agent : une signature qu'il refuserait n'est jamais publiée), exige que `--version`
//!   soit celle du dépôt et que le fichier soit déjà un manifeste valable du client, puis y ajoute (ou y
//!   remplace) la section `agent`, le reste du fichier étant conservé (par défaut sur place).
//!
//! Aucun secret ici : la signature est faite par le détenteur du dépôt, hors du dépôt et hors de la CI
//! (runbook `docs/runbooks/mettre-a-jour-agent.md`).

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::release_core::*;

fn root() -> Result<PathBuf, String> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "racine du dépôt introuvable".to_owned())
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

fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// `cargo xtask agent-manifest …`.
pub fn run_manifest(args: &[String]) -> Result<(), String> {
    let binary_path = required(args, "--binary-file")?;
    let binary = std::fs::read(&binary_path).map_err(|error| format!("{binary_path} : {error}"))?;
    let signature_path = required(args, "--signature-file")?;
    let signature = std::fs::read_to_string(&signature_path)
        .map_err(|error| format!("{signature_path} : {error}"))?;
    let key = std::fs::read_to_string(root()?.join("crates/hearth-agent/update-key.pub"))
        .map_err(|error| format!("update-key.pub de l'agent : {error}"))?;
    let version = required(args, "--version")?;
    let cargo = std::fs::read_to_string(root()?.join("Cargo.toml"))
        .map_err(|error| format!("Cargo.toml : {error}"))?;
    check_repository_version(&version, &cargo)?;
    // Ce que l'agent refuserait n'est jamais publié : signature vérifiée contre SA clé embarquée.
    verify_agent_signature(&key, &signature, &binary)?;
    let section = agent_section(
        DOWNLOAD_PREFIX,
        &version,
        &signature,
        &sha256_hex(&binary),
        &required(args, "--url")?,
    )?;
    let manifest_path = required(args, "--manifest")?;
    let manifest = std::fs::read_to_string(&manifest_path)
        .map_err(|error| format!("{manifest_path} : {error}"))?;
    let text = add_agent_section(&manifest, section)?;
    let out = option(args, "--out").unwrap_or(manifest_path);
    std::fs::write(
        &out,
        format!(
            "{text}
"
        ),
    )
    .map_err(|error| format!("{out} : {error}"))?;
    println!(
        "section de l'agent ajoutée à {out} (somme {})",
        sha256_hex(&binary)
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str =
        "https://github.com/Voikyrioh/hearth/releases/download/v0.2.0/hearth-agent-linux-x86_64";

    fn pair() -> (minisign::KeyPair, String) {
        let keys = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        let public = keys.pk.to_box().unwrap().to_string();
        (keys, public)
    }

    fn sign(keys: &minisign::KeyPair, data: &[u8]) -> String {
        minisign::sign(
            None,
            &keys.sk,
            std::io::Cursor::new(data),
            Some("trusted"),
            Some("signature de test"),
        )
        .unwrap()
        .to_string()
    }

    #[test]
    fn a_signature_of_the_embedded_key_verifies_in_both_forms_and_any_other_is_refused() {
        let (keys, public) = pair();
        let (other, _) = pair();
        let data = b"agent";
        let signature = sign(&keys, data);
        assert!(verify_agent_signature(&public, &signature, data).is_ok());
        let encoded =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &signature);
        assert!(verify_agent_signature(&public, &encoded, data).is_ok());
        assert!(
            verify_agent_signature(&public, &sign(&other, data), data)
                .unwrap_err()
                .contains("update-key.pub")
        );
        assert!(verify_agent_signature(&public, &signature, b"autre").is_err());
        assert!(verify_agent_signature(&public, "pas une signature", data).is_err());
    }

    #[test]
    fn the_version_must_be_the_one_of_the_repository() {
        let toml = "[workspace.package]
version = \"0.2.0\"
";
        assert!(check_repository_version("0.2.0", toml).is_ok());
        assert!(
            check_repository_version("0.3.0", toml)
                .unwrap_err()
                .contains("0.2.0")
        );
        assert!(
            check_repository_version(
                "0.2.0",
                "[package]
"
            )
            .is_err()
        );
    }

    const CLIENT: &str = r#"{"version":"1.2.0","notes":"n","pub_date":"2026-10-06T10:00:00Z","platforms":{"windows-x86_64":{"signature":"c2ln","url":"https://github.com/Voikyrioh/hearth/releases/download/v1.2.0/Hearth_1.2.0_x64-setup.exe"}}}"#;

    #[test]
    fn the_agent_section_is_added_to_the_client_manifest_which_is_otherwise_kept() {
        let (keys, _) = pair();
        let section = agent_section(
            DOWNLOAD_PREFIX,
            "0.2.0",
            &sign(&keys, b"agent"),
            &sha256_hex(b"agent"),
            URL,
        )
        .unwrap();
        let text = add_agent_section(CLIENT, section).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            value["version"], "1.2.0",
            "le manifeste du client est conservé"
        );
        assert_eq!(value["platforms"][TARGET]["signature"], "c2ln");
        assert_eq!(value["agent"]["version"], "0.2.0");
        assert_eq!(value["agent"]["platforms"][AGENT_TARGET]["url"], URL);
        assert_eq!(
            value["agent"]["platforms"][AGENT_TARGET]["sha256"],
            sha256_hex(b"agent")
        );
        // Jamais une section de l'agent sans manifeste du client valable.
        assert!(add_agent_section("{}", value["agent"].clone()).is_err());
        assert!(add_agent_section("pas du json", value["agent"].clone()).is_err());
    }

    #[test]
    fn the_section_refuses_what_the_client_or_the_agent_would_refuse() {
        let (keys, _) = pair();
        let signature = sign(&keys, b"agent");
        let ok = |version: &str, sha: &str, url: &str| {
            agent_section(DOWNLOAD_PREFIX, version, &signature, sha, url)
        };
        let sha = sha256_hex(b"agent");
        assert!(ok("0.2.0", &sha, URL).is_ok());
        assert!(ok("0.2", &sha, URL).is_err());
        assert!(ok("0.2.0", "ab", URL).is_err());
        assert!(
            ok(
                "0.2.0",
                &sha,
                "http://github.com/Voikyrioh/hearth/releases/download/v0.2.0/a"
            )
            .is_err()
        );
        assert!(ok("0.2.0", &sha, "https://exemple.org/hearth-agent").is_err());
        assert!(
            ok("0.3.0", &sha, URL).is_err(),
            "l'adresse doit être celle de la version annoncée"
        );
        assert!(agent_section(DOWNLOAD_PREFIX, "0.2.0", "  ", &sha, URL).is_err());
    }
}

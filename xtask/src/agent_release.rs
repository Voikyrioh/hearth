//! Publication de la cible de l'AGENT (HRT-17, ADR-0021) : fabrique `agent.json`, le fichier que le
//! client lit dans la même release que `latest.json`, pour proposer la mise à jour de l'agent.
//!
//! - `agent-manifest --version X.Y.Z --binary-file F --signature-file S --url U --date D --out O` :
//!   calcule la somme SHA-256 du binaire, VÉRIFIE la signature minisign (`minisign -S`, faite à la main
//!   avec la clé secrète de l'agent) contre `crates/hearth-agent/update-key.pub` (la clé embarquée dans
//!   l'agent : une signature qu'il refuserait n'est jamais publiée), puis écrit le fichier.
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
    // Ce que l'agent refuserait n'est jamais publié : signature vérifiée contre SA clé embarquée.
    verify_agent_signature(&key, &signature, &binary)?;
    let text = agent_manifest(
        DOWNLOAD_PREFIX,
        &required(args, "--version")?,
        &signature,
        &sha256_hex(&binary),
        &required(args, "--url")?,
        &required(args, "--date")?,
    )?;
    let out = required(args, "--out")?;
    std::fs::write(&out, format!("{text}\n")).map_err(|error| format!("{out} : {error}"))?;
    println!(
        "cible de l'agent écrite : {out} (somme {})",
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
    fn the_manifest_has_the_shape_the_client_reads() {
        let (keys, _) = pair();
        let signature = sign(&keys, b"agent");
        let text = agent_manifest(
            DOWNLOAD_PREFIX,
            "0.2.0",
            &signature,
            &sha256_hex(b"agent"),
            URL,
            "2026-10-06T10:00:00Z",
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["version"], "0.2.0");
        assert_eq!(value["platforms"][AGENT_TARGET]["url"], URL);
        assert_eq!(
            value["platforms"][AGENT_TARGET]["sha256"],
            sha256_hex(b"agent")
        );
        assert!(
            value["platforms"][AGENT_TARGET]["signature"]
                .as_str()
                .unwrap()
                .starts_with("untrusted comment:")
        );
    }

    #[test]
    fn the_manifest_refuses_what_the_client_or_the_agent_would_refuse() {
        let (keys, _) = pair();
        let signature = sign(&keys, b"agent");
        let ok = |version: &str, sha: &str, url: &str, date: &str| {
            agent_manifest(DOWNLOAD_PREFIX, version, &signature, sha, url, date)
        };
        let sha = sha256_hex(b"agent");
        assert!(ok("0.2.0", &sha, URL, "2026-10-06T10:00:00Z").is_ok());
        assert!(ok("0.2", &sha, URL, "2026-10-06T10:00:00Z").is_err());
        assert!(ok("0.2.0", "ab", URL, "2026-10-06T10:00:00Z").is_err());
        assert!(
            ok(
                "0.2.0",
                &sha,
                "http://github.com/Voikyrioh/hearth/releases/download/v0.2.0/a",
                "2026-10-06T10:00:00Z"
            )
            .is_err()
        );
        assert!(
            ok(
                "0.2.0",
                &sha,
                "https://exemple.org/hearth-agent",
                "2026-10-06T10:00:00Z"
            )
            .is_err()
        );
        // L'adresse doit être celle de la version annoncée.
        assert!(ok("0.3.0", &sha, URL, "2026-10-06T10:00:00Z").is_err());
        assert!(ok("0.2.0", &sha, URL, "hier").is_err());
        assert!(
            agent_manifest(
                DOWNLOAD_PREFIX,
                "0.2.0",
                "  ",
                &sha,
                URL,
                "2026-10-06T10:00:00Z"
            )
            .is_err()
        );
    }
}

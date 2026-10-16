//! `cargo xtask agent-sign --binary F --version X.Y.Z --out S` (HRT-173, conception 7.2) : signe le binaire
//! de l'agent avec la clé minisign des variables d'environnement, à la place du `minisign -S -t
//! "version:X.Y.Z"` fait à la main (runbook `mettre-a-jour-agent.md`).
//!
//! - La clé secrète est dans `HEARTH_AGENT_SIGNING_KEY` (contenu du fichier de clé) et son mot de passe,
//!   OBLIGATOIRE, dans `HEARTH_AGENT_SIGNING_KEY_PASSWORD` : jamais en argument, jamais dans une trace,
//!   un message d'erreur ou un `Debug` (type `Secret`).
//! - Le commentaire signé (de confiance) est exactement `version:X.Y.Z` : ce que l'agent exige avant tout
//!   échange de binaire (BR-UPDATE-035).
//! - La signature est relue contre `crates/hearth-agent/update-key.pub` avant d'être écrite : une clé qui
//!   n'est pas la paire de la clé embarquée est refusée avant toute publication.

use std::io::Cursor;
use std::path::Path;

use crate::client_release::required;
use crate::release_core::*;
use crate::secret::Secret;
use zeroize::Zeroizing;

pub const KEY_VAR: &str = "HEARTH_AGENT_SIGNING_KEY";
pub const PASSWORD_VAR: &str = "HEARTH_AGENT_SIGNING_KEY_PASSWORD";

/// Les deux secrets de la signature. `Debug` ne montre aucune valeur.
#[derive(Debug)]
pub struct SigningInputs {
    pub key: Secret,
    pub password: Secret,
}

impl SigningInputs {
    pub fn from_env() -> Self {
        Self {
            key: Secret::from_env(KEY_VAR),
            password: Secret::from_env(PASSWORD_VAR),
        }
    }
}

/// Signe `data` : le texte du fichier `.minisig`. Relit la signature contre `public_key_file`.
pub fn sign_agent(
    inputs: &SigningInputs,
    public_key_file: &str,
    data: &[u8],
    version: &str,
) -> Result<String, String> {
    check_version(version)?;
    if inputs.key.is_blank() {
        return Err(format!("{KEY_VAR} est absent ou vide"));
    }
    if inputs.password.is_blank() {
        return Err(format!(
            "le mot de passe de la clé de signature de l'agent est obligatoire (secret {PASSWORD_VAR}) : chiffre la clé d'abord (minisign -C)"
        ));
    }
    check_key(public_key_file)?;
    // La copie du mot de passe donnée à minisign est effacée dès que la clé est déchiffrée.
    let password = Zeroizing::new(inputs.password.expose().to_owned());
    let secret = minisign::SecretKeyBox::from_string(&decode_key_file(inputs.key.expose()))
        .and_then(|boxed| boxed.into_secret_key(Some((*password).clone())))
        .map_err(|error| {
            format!("clé secrète de l'agent illisible (ou mot de passe faux) : {error}")
        })?;
    drop(password);
    let comment = format!("version:{version}");
    let signature = minisign::sign(
        None,
        &secret,
        Cursor::new(data),
        Some(&comment),
        Some("signature Hearth agent"),
    )
    .map_err(|error| format!("signature impossible : {error}"))?
    .to_string();
    verify_agent_signature(public_key_file, &signature, data, version).map_err(|error| {
        format!(
            "la clé secrète de l'agent n'est pas la paire de la clé publique embarquée : {error}"
        )
    })?;
    Ok(signature)
}

/// `cargo xtask agent-sign --binary F --version X.Y.Z --out S`.
pub fn run(args: &[String]) -> Result<(), String> {
    crate::release_stage::check_flags(args, &["--binary", "--version", "--out"], &[])?;
    let binary = required(args, "--binary")?;
    let version = required(args, "--version")?;
    let out = required(args, "--out")?;
    let root = crate::repo_root::repo_root()?;
    let cargo = std::fs::read_to_string(root.join("Cargo.toml"))
        .map_err(|error| format!("Cargo.toml : {error}"))?;
    check_repository_version(&version, &cargo)?;
    let public = std::fs::read_to_string(root.join("crates/hearth-agent/update-key.pub"))
        .map_err(|error| format!("update-key.pub de l'agent : {error}"))?;
    let data = std::fs::read(&binary).map_err(|error| format!("{binary} : {error}"))?;
    let signature = sign_agent(&SigningInputs::from_env(), &public, &data, &version)?;
    std::fs::write(Path::new(&out), &signature).map_err(|error| format!("{out} : {error}"))?;
    println!(
        "signature de l'agent écrite et vérifiée contre update-key.pub (version:{version}) : {out}"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PASSWORD: &str = "mot-de-passe-sentinelle";

    /// Paire jetable chiffrée, générée par le test : aucune clé réelle, rien d'écrit.
    fn pair(password: &str) -> (String, String) {
        let keys =
            minisign::KeyPair::generate_encrypted_keypair(Some(password.to_owned())).unwrap();
        (
            keys.sk.to_box(None).unwrap().to_string(),
            keys.pk.to_box().unwrap().to_string(),
        )
    }

    fn inputs(secret: &str, password: &str) -> SigningInputs {
        SigningInputs {
            key: Secret::new(secret.to_owned()),
            password: Secret::new(password.to_owned()),
        }
    }

    #[test]
    fn the_signature_carries_exactly_the_version_and_verifies_against_the_embedded_key() {
        let (secret, public) = pair(PASSWORD);
        let data = b"binaire de l'agent";
        let signature = sign_agent(&inputs(&secret, PASSWORD), &public, data, "0.1.0").unwrap();
        assert!(verify_agent_signature(&public, &signature, data, "0.1.0").is_ok());
        let parsed = minisign::SignatureBox::from_string(&signature).unwrap();
        assert_eq!(parsed.trusted_comment().unwrap(), "version:0.1.0");
        // Une autre version, un autre contenu : la vérification de l'agent les refuse.
        assert!(verify_agent_signature(&public, &signature, data, "0.1.1").is_err());
        assert!(verify_agent_signature(&public, &signature, b"autre", "0.1.0").is_err());
    }

    #[test]
    fn a_key_that_is_not_the_pair_of_the_embedded_key_is_refused() {
        let (secret, _) = pair(PASSWORD);
        let (_, other_public) = pair(PASSWORD);
        let error =
            sign_agent(&inputs(&secret, PASSWORD), &other_public, b"x", "0.1.0").unwrap_err();
        assert!(error.contains("pas la paire"), "{error}");
    }

    #[test]
    fn a_missing_key_a_missing_password_or_a_wrong_one_is_refused_without_echoing_secrets() {
        let (secret, public) = pair(PASSWORD);
        assert!(
            sign_agent(&inputs("  ", PASSWORD), &public, b"x", "0.1.0")
                .unwrap_err()
                .contains(KEY_VAR)
        );
        assert!(
            sign_agent(&inputs(&secret, ""), &public, b"x", "0.1.0")
                .unwrap_err()
                .contains("obligatoire")
        );
        let wrong = "faux-mot-de-passe-sentinelle";
        let error = sign_agent(&inputs(&secret, wrong), &public, b"x", "0.1.0").unwrap_err();
        assert!(
            !error.contains(wrong) && !error.contains(PASSWORD) && !error.contains(&secret),
            "{error}"
        );
        assert!(sign_agent(&inputs("pas une clé", PASSWORD), &public, b"x", "0.1.0").is_err());
    }

    #[test]
    fn a_version_that_is_not_strict_or_a_development_key_is_refused() {
        let (secret, public) = pair(PASSWORD);
        for bad in ["v0.1.0", "0.1", "0.1.0-beta.1", ""] {
            assert!(
                sign_agent(&inputs(&secret, PASSWORD), &public, b"x", bad).is_err(),
                "{bad}"
            );
        }
        let dev = "untrusted comment: Hearth DEV public key. No secret key exists.\nRWQabc\n";
        assert!(
            sign_agent(&inputs(&secret, PASSWORD), dev, b"x", "0.1.0")
                .unwrap_err()
                .contains("DÉVELOPPEMENT")
        );
    }

    #[test]
    fn the_secrets_are_never_shown_by_debug() {
        let (secret, _) = pair(PASSWORD);
        let value = inputs(&secret, PASSWORD);
        let text = format!("{value:?} {:?}", Some(&value));
        assert!(!text.contains(PASSWORD), "{text}");
        assert!(!text.contains(&secret[..40]), "{text}");
        assert!(text.contains("masqué"));
    }

    #[test]
    fn the_arguments_are_checked() {
        assert!(run(&["--inconnu".to_owned()]).is_err());
        let partial = ["--binary".to_owned(), "x".to_owned()];
        assert!(run(&partial).is_err());
        // Aucun argument ne peut porter le mot de passe.
        let with_password = ["--password".to_owned(), "x".to_owned()];
        assert!(
            run(&with_password)
                .unwrap_err()
                .contains("argument inconnu")
        );
    }
}

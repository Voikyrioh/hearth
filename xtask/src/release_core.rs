//! Coeur de la publication du client, sans E/S ni dépendance du dépôt : signature, vérification,
//! manifeste. Inclus tel quel par les tests du client (`#[path]`) pour que le test de bout en bout
//! passe par LE code que `cargo xtask client-sign` et `client-manifest` appellent.

use std::io::Cursor;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::json;

/// Cible du manifeste : doit rester celle du client (`update/feed.rs::TARGET`).
pub const TARGET: &str = "windows-x86_64";
/// Entrée de l'agent dans `agent.json` : doit rester celle que le client lit
/// (`agent_update/domain.rs::AGENT_PLATFORM`).
pub const AGENT_TARGET: &str = "linux-x86_64";
/// Marque du commentaire de la clé de développement (`update/feed.rs::DEV_KEY_MARK`).
pub const DEV_KEY_MARK: &str = "DEV public key";
/// Les installateurs ne sont téléchargés que de ce dépôt (`update/domain.rs`).
pub const DOWNLOAD_PREFIX: &str = "https://github.com/Voikyrioh/hearth/releases/download/";

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

fn text_of_key_file(file: &str) -> String {
    decode_key_file(file)
}

/// Signe `data` : le contenu du `.sig` en base64, commentaire de confiance avec la version.
pub fn sign_installer(
    secret_key_file: &str,
    password: &str,
    data: &[u8],
    file_name: &str,
    version: &str,
    timestamp: u64,
) -> Result<String, String> {
    check_version(version)?;
    if password.is_empty() {
        return Err("le mot de passe de la clé de signature est obligatoire (secret HEARTH_CLIENT_SIGNING_KEY_PASSWORD)".to_owned());
    }
    let text = text_of_key_file(secret_key_file);
    let secret = minisign::SecretKeyBox::from_string(&text)
        .and_then(|boxed| boxed.into_secret_key(Some(password.to_owned())))
        .map_err(|error| format!("clé secrète illisible (ou mot de passe faux) : {error}"))?;
    let comment = format!("timestamp:{timestamp}\tfile:{file_name}\tversion:{version}");
    let signature = minisign::sign(
        None,
        &secret,
        Cursor::new(data),
        Some(&comment),
        Some("signature Hearth"),
    )
    .map_err(|error| format!("signature impossible : {error}"))?;
    Ok(STANDARD.encode(signature.to_string()))
}

/// Vérifie une signature (le contenu du `.sig` en base64) contre la clé publique du dépôt, et que le
/// commentaire de confiance porte la version attendue : ce que le client exigera
/// (`requireSignedVersion`).
pub fn verify_signature(
    public_key_file: &str,
    signature_b64: &str,
    data: &[u8],
    version: &str,
) -> Result<(), String> {
    let public = minisign::PublicKeyBox::from_string(&decode_key_file(public_key_file))
        .and_then(minisign::PublicKeyBox::into_public_key)
        .map_err(|error| format!("clé publique illisible : {error}"))?;
    let bytes = STANDARD
        .decode(signature_b64.split_whitespace().collect::<String>())
        .map_err(|_| "signature : pas du base64".to_owned())?;
    let text = String::from_utf8(bytes).map_err(|_| "signature : pas du texte".to_owned())?;
    let signature = minisign::SignatureBox::from_string(&text)
        .map_err(|error| format!("signature illisible : {error}"))?;
    minisign::verify(&public, &signature, Cursor::new(data), true, false, true).map_err(|_| {
        "la signature ne correspond pas à update-key.pub : la clé secrète des secrets n'est pas la paire de la clé publique du dépôt, ou le fichier a changé".to_owned()
    })?;
    let comment = signature
        .trusted_comment()
        .map_err(|error| error.to_string())?;
    let signed = comment
        .split('\t')
        .find_map(|field| field.strip_prefix("version:"));
    if signed != Some(version) {
        return Err(format!(
            "la signature porte la version {} et non {version}",
            signed.unwrap_or("(aucune)")
        ));
    }
    Ok(())
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
    manifest_for(DOWNLOAD_PREFIX, version, notes, signature, url, pub_date)
}

/// Comme `manifest`, pour une autre racine d'adresses (le serveur de versions local des tests du
/// client) : tout le reste est identique.
pub fn manifest_for(
    prefix: &str,
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
    let expected = format!("{prefix}v{version}/");
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

/// La signature d'un binaire de l'AGENT (faite à la main par `minisign -S`, ADR-0014) : le contenu du
/// fichier `.minisig`, ou son encodage base64, vérifié contre la clé publique embarquée dans l'agent.
/// Le commentaire de confiance n'est pas contrôlé : l'agent ne l'exige pas (il contrôle la version en
/// exécutant le binaire).
pub fn verify_agent_signature(
    public_key_file: &str,
    signature: &str,
    data: &[u8],
) -> Result<(), String> {
    let public = minisign::PublicKeyBox::from_string(&decode_key_file(public_key_file))
        .and_then(minisign::PublicKeyBox::into_public_key)
        .map_err(|error| format!("clé publique de l'agent illisible : {error}"))?;
    let text = signature_text(signature)?;
    let signature = minisign::SignatureBox::from_string(&text)
        .map_err(|error| format!("signature illisible : {error}"))?;
    minisign::verify(&public, &signature, Cursor::new(data), true, false, true).map_err(|_| {
        "la signature ne correspond pas à crates/hearth-agent/update-key.pub : le binaire n'a pas été signé par la paire de la clé embarquée dans l'agent, ou le fichier a changé".to_owned()
    })
}

/// Le texte d'une signature minisign : tel quel, ou décodé s'il est en base64.
fn signature_text(signature: &str) -> Result<String, String> {
    let signature = signature.trim();
    if signature.starts_with("untrusted comment:") {
        return Ok(signature.to_owned());
    }
    let bytes = STANDARD
        .decode(signature.split_whitespace().collect::<String>())
        .map_err(|_| "signature : ni un fichier .minisig ni du base64".to_owned())?;
    let text = String::from_utf8(bytes).map_err(|_| "signature : pas du texte".to_owned())?;
    if text.trim().starts_with("untrusted comment:") {
        Ok(text.trim().to_owned())
    } else {
        Err("signature : pas une signature minisign".to_owned())
    }
}

/// Le fichier de cibles de l'agent (`agent.json`), à joindre à la même release que `latest.json`
/// (ADR-0021) : `version`, `pub_date`, et l'entrée `linux-x86_64` (`url`, `signature`, `sha256`).
/// `sha256` est la somme du binaire, en hexadécimal : l'agent exige la signature ET la somme.
pub fn agent_manifest(
    prefix: &str,
    version: &str,
    signature: &str,
    sha256: &str,
    url: &str,
    pub_date: &str,
) -> Result<String, String> {
    check_version(version)?;
    let signature = signature_text(signature)?;
    if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("somme SHA-256 : 64 caractères hexadécimaux attendus".to_owned());
    }
    let expected = format!("{prefix}v{version}/");
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
        "pub_date": pub_date,
        "platforms": {
            AGENT_TARGET: {
                "url": url,
                "signature": signature,
                "sha256": sha256.to_ascii_lowercase(),
            }
        },
    }))
    .map_err(|error| error.to_string())
}

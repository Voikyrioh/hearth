//! Coeur de la publication du client, sans E/S ni dépendance du dépôt : signature, vérification,
//! manifeste. Inclus tel quel par les tests du client (`#[path]`) pour que le test de bout en bout
//! passe par LE code que `cargo xtask client-sign` et `client-manifest` appellent.

use std::io::Cursor;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::json;

/// Cible du manifeste : doit rester celle du client (`update/feed.rs::TARGET`).
pub const TARGET: &str = "windows-x86_64";
/// Entrée de l'agent dans la section `agent` du `latest.json` : doit rester celle que le client lit
/// (`agent_update/domain.rs::AGENT_PLATFORM`).
pub const AGENT_TARGET: &str = "linux-x86_64";
/// Marque du commentaire de la clé de développement (`update/feed.rs::DEV_KEY_MARK`).
pub const DEV_KEY_MARK: &str = "DEV public key";
/// Les installateurs et l'agent ne sont téléchargés que de ce préfixe : `DOWNLOAD_HOST` +
/// `DOWNLOAD_PATH_PREFIX` de `update/domain.rs` (ADR-0035). Adresse d'un fichier :
/// `{DOWNLOAD_PREFIX}{X.Y.Z}/{fichier}`, SANS `v` devant la version (format de l'API).
pub const DOWNLOAD_PREFIX: &str = "https://hearth.voikyrioh.fr/api/releases/";
/// Binaire de l'agent dans la liste fermée de l'API (`hearth-releases-api`,
/// `release-files.ts::AGENT_BINARY`) : aussi la contrainte de `deploy/install.sh` (BR-INSTALL-013, fiche de la PR de install.sh).
pub const AGENT_BINARY: &str = "hearth-agent-linux-x86_64";

/// Nom de l'installateur du client dans la liste fermée de l'API (`release-files.ts`).
pub fn installer_file_name(version: &str) -> String {
    format!("Hearth_{version}_x64-setup.exe")
}

/// L'adresse exacte d'un fichier de version : `{prefix}{version}/{file}`, `version` strictement
/// `X.Y.Z`. Le dossier est la version ANNONCÉE, le fichier un nom de la liste fermée : toute autre
/// adresse (autre dossier, autre fichier, `v` devant la version, requête, fragment) est refusée.
fn check_release_url(prefix: &str, version: &str, file: &str, url: &str) -> Result<(), String> {
    let expected = format!("{prefix}{version}/{file}");
    if url == expected {
        Ok(())
    } else {
        Err(format!("l'adresse doit être exactement {expected}"))
    }
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
    // `allow_legacy = true` : aligné sur le client. Le greffon vérifie par `PublicKey::verify(data,
    // &signature, true)` (tauri-plugin-updater 2.13.1, `updater.rs:1648`) et `minisign-verify` 0.2.5
    // accepte alors une signature NON pré-hachée (`lib.rs:365-369` ; refus seulement avec `false`).
    // Cette fonction n'est donc pas plus lâche que le client. `sign_installer` signe toujours en
    // pré-haché.
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

/// `X.Y.Z` strict (pas de préversion : le flux publié n'en propose jamais), sans zéro en tête : la
/// même forme que l'API (`^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$`).
pub fn check_version(version: &str) -> Result<(), String> {
    let parts: Vec<&str> = version.split('.').collect();
    let numeric = |part: &&str| {
        !part.is_empty()
            && part.bytes().all(|b| b.is_ascii_digit())
            && (*part == "0" || !part.starts_with('0'))
    };
    if parts.len() == 3 && parts.iter().all(numeric) {
        Ok(())
    } else {
        Err(format!("version « {version} » : attendu X.Y.Z"))
    }
}

/// Réponse de la recherche de l'étiquette `vX.Y.Z` sur le dépôt distant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagLookup {
    Present,
    Absent,
}

/// Garde contre la resignature d'un numéro déjà étiqueté : un numéro étiqueté est brûlé, jamais
/// resigné (conception de la distribution, 8.2). L'étiquette n'est posée qu'à la fin, à la main :
/// entre la signature et l'étiquette (ou après un essai qui signe puis échoue) rien n'empêche de
/// relancer : cette garde ne couvre PAS cette fenêtre. `release-upload` la ferme à partir du premier
/// objet déposé (il refuse d'écrire une clé de version déjà présente avec un autre contenu,
/// BR-RELEASE-001) ; un numéro signé puis jamais déposé reste resignable. Refuse si l'étiquette `v{version}` existe déjà sur le dépôt
/// distant. `lookup` interroge le dépôt (`git ls-remote`, lecture seule) : injectée pour les tests.
/// Une interrogation impossible (réseau coupé) refuse aussi : sans réponse on ne sait pas.
pub fn check_tag_is_free(
    version: &str,
    lookup: &dyn Fn(&str) -> Result<TagLookup, String>,
) -> Result<(), String> {
    check_version(version)?;
    let tag = format!("v{version}");
    match lookup(&tag).map_err(|error| {
        format!(
            "impossible de vérifier que l'étiquette {tag} n'existe pas sur origin ({error}) : rien n'est signé"
        )
    })? {
        TagLookup::Absent => Ok(()),
        TagLookup::Present => Err(format!(
            "l'étiquette {tag} existe déjà sur origin : ce numéro est brûlé, jamais resigné (l'étiquette n'est posée qu'à la fin, à la main : ce refus ne couvre pas un numéro signé mais pas encore étiqueté). Passe à la version suivante (Cargo.toml et apps/desktop/package.json)"
        )),
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
    check_release_url(prefix, version, &installer_file_name(version), url)?;
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

/// La version demandée doit être celle du dépôt (`[workspace.package]`), seule source du numéro : l'agent
/// construit par `cargo xtask agent` porte cette version (il la vérifie), et c'est celle qu'il annoncera
/// en s'exécutant. Une section `agent` « 0.3.0 » posée sur un binaire 0.2.0 serait refusé (`bad_binary`) par
/// chaque agent, puis reproposé sans fin.
pub fn check_repository_version(requested: &str, cargo_toml: &str) -> Result<(), String> {
    let repository = workspace_version(cargo_toml).ok_or("version du dépôt illisible")?;
    if requested == repository {
        Ok(())
    } else {
        Err(format!(
            "la version demandée ({requested}) n'est pas celle du dépôt ({repository}) : l'agent construit porte la version de [workspace.package] dans Cargo.toml"
        ))
    }
}

/// La signature d'un binaire de l'AGENT (faite à la main par `minisign -S`, ADR-0014) : le contenu du
/// fichier `.minisig`, ou son encodage base64, vérifié contre la clé publique embarquée dans l'agent.
/// Le commentaire de confiance doit porter `version:X.Y.Z` égal à `version` : ce que l'agent exige
/// avant tout échange de binaire (BR-UPDATE-035). Même forme stricte que lui : un seul champ
/// `version:`, ni `v`, ni pré-version (`check_version` contrôle la forme de `version`, et la comparaison
/// de chaînes refuse tout écart, zéros en tête compris). La signature se fait à la main avec
/// `minisign -S -t "version:X.Y.Z" …` (runbook `mettre-a-jour-agent.md`).
pub fn verify_agent_signature(
    public_key_file: &str,
    signature: &str,
    data: &[u8],
    version: &str,
) -> Result<(), String> {
    check_version(version)?;
    let public = minisign::PublicKeyBox::from_string(&decode_key_file(public_key_file))
        .and_then(minisign::PublicKeyBox::into_public_key)
        .map_err(|error| format!("clé publique de l'agent illisible : {error}"))?;
    let text = signature_text(signature)?;
    let signature = minisign::SignatureBox::from_string(&text)
        .map_err(|error| format!("signature illisible : {error}"))?;
    minisign::verify(&public, &signature, Cursor::new(data), true, false, false).map_err(|_| {
        "la signature ne correspond pas à crates/hearth-agent/update-key.pub : le binaire n'a pas été signé par la paire de la clé embarquée dans l'agent, ou le fichier a changé".to_owned()
    })?;
    let comment = signature
        .trusted_comment()
        .map_err(|error| error.to_string())?;
    let mut fields = comment
        .split('\t')
        .filter_map(|field| field.strip_prefix("version:"));
    let signed = fields.next();
    if fields.next().is_some() {
        return Err(
            "le commentaire signé porte plusieurs champs version: (l'agent le refuserait)"
                .to_owned(),
        );
    }
    if signed != Some(version) {
        return Err(format!(
            "la signature porte la version {} et non {version} : l'agent la refuserait avant tout échange. Signer avec `minisign -S -t \"version:{version}\"`",
            signed.unwrap_or("(aucune)")
        ));
    }
    Ok(())
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

/// La section `agent` à ajouter au manifeste `latest.json` du client (ADR-0021) : `version` et l'entrée
/// `linux-x86_64` (`url`, `signature`, `sha256`). `sha256` est la somme du binaire, en hexadécimal :
/// l'agent exige la signature ET la somme.
pub fn agent_section(
    prefix: &str,
    version: &str,
    signature: &str,
    sha256: &str,
    url: &str,
) -> Result<serde_json::Value, String> {
    check_version(version)?;
    let signature = signature_text(signature)?;
    if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("somme SHA-256 : 64 caractères hexadécimaux attendus".to_owned());
    }
    // Dossier = version de l'AGENT, fichier = `hearth-agent-linux-x86_64` : `install.sh` n'accepte que
    // ce chemin (BR-INSTALL-013, fiche de la PR de install.sh).
    check_release_url(prefix, version, AGENT_BINARY, url)?;
    Ok(json!({
        "version": version,
        "platforms": {
            AGENT_TARGET: { "url": url, "signature": signature, "sha256": sha256.to_ascii_lowercase() }
        },
    }))
}

/// Ajoute (ou remplace) la section `agent` d'un `latest.json` DÉJÀ valable pour le client : un seul
/// manifeste cohérent, jamais une section de l'agent sans le manifeste du client (version et entrée
/// `windows-x86_64` exigées), et le reste du fichier est conservé tel quel.
pub fn add_agent_section(manifest: &str, section: serde_json::Value) -> Result<String, String> {
    let mut value: serde_json::Value = serde_json::from_str(manifest)
        .map_err(|error| format!("latest.json illisible : {error}"))?;
    let client_ok = value["version"].is_string()
        && value["platforms"][TARGET]["url"].is_string()
        && value["platforms"][TARGET]["signature"].is_string();
    if !client_ok {
        return Err(format!(
            "latest.json n'est pas un manifeste du client valable (version et entrée {TARGET} attendues) : fabrique-le d'abord avec client-manifest"
        ));
    }
    value["agent"] = section;
    serde_json::to_string_pretty(&value).map_err(|error| error.to_string())
}

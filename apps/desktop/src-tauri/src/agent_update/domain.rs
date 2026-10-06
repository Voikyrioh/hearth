//! Règles pures de la mise à jour de l'AGENT depuis le client (HRT-17, ADR-0021) : la cible publiée
//! dans le flux de versions, ce qu'on en croit, et quand une mise à jour est « disponible ». Ni E/S,
//! ni réseau, ni Tauri.
//!
//! Qui fait confiance à quoi : la SÉCURITÉ ne repose pas sur ce module. L'agent vérifie la somme
//! SHA-256 et la signature minisign contre SA clé embarquée avant d'écrire quoi que ce soit, et il
//! refuse une adresse non HTTPS ou locale, une version qui n'est pas plus récente. Ce module garantit
//! autre chose : que le client n'envoie JAMAIS une cible qu'il sait mauvaise (rétrogradation,
//! adresse non HTTPS, locale ou hors du dépôt prévu, version illisible), même si le flux, ou le
//! fichier de l'état, étaient altérés. La WebView, elle, ne fournit aucun de ces champs.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use serde::{Deserialize, Serialize};
use url::{Host, Url};

use crate::update::domain::{DownloadPolicy, SIGNATURE_MAX_LEN};

/// Adresse du fichier de cibles de l'agent : fixée à la compilation, jamais saisie (ADR-0021). Le
/// même dépôt public que le flux du client, la même release (`releases/latest`).
pub const AGENT_FEED_URL: &str =
    "https://github.com/Voikyrioh/hearth/releases/latest/download/agent.json";
/// Entrée du fichier de cibles : l'agent est un binaire statique Linux x86_64 (ADR-0003).
pub const AGENT_PLATFORM: &str = "linux-x86_64";
/// Taille maximale du fichier de cibles : quelques centaines d'octets en pratique.
pub const MANIFEST_MAX_BYTES: usize = 64 * 1024;
/// Longueur maximale de l'adresse du binaire (celle que l'agent accepte).
pub const URL_MAX_LEN: usize = 2048;

/// Ce que le fichier de cibles annonce, avant toute confiance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentCandidate {
    pub version: String,
    pub url: String,
    pub signature: String,
    pub sha256: String,
}

/// La cible de l'agent telle qu'elle est retenue dans l'état du client (`update.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTargetRecord {
    pub version: String,
    pub url: String,
    pub signature: String,
    pub sha256: String,
}

impl From<&AgentTarget> for AgentTargetRecord {
    fn from(target: &AgentTarget) -> Self {
        Self {
            version: target.version.to_string(),
            url: target.url.to_string(),
            signature: target.signature.clone(),
            sha256: target.sha256.clone(),
        }
    }
}

/// Une cible VALIDÉE : on ne la fabrique que par [`validate_target`] (champs privés).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTarget {
    version: semver::Version,
    url: Url,
    signature: String,
    sha256: String,
}

impl AgentTarget {
    pub fn version(&self) -> &semver::Version {
        &self.version
    }

    pub fn url(&self) -> &Url {
        &self.url
    }

    pub fn signature(&self) -> &str {
        &self.signature
    }

    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    /// Relit une cible retenue : elle est validée de nouveau (le fichier de l'état est modifiable
    /// par quiconque écrit sur le PC).
    pub fn from_record(
        record: &AgentTargetRecord,
        policy: &DownloadPolicy,
    ) -> Result<Self, TargetRejection> {
        validate_target(
            &AgentCandidate {
                version: record.version.clone(),
                url: record.url.clone(),
                signature: record.signature.clone(),
                sha256: record.sha256.clone(),
            },
            policy,
        )
    }
}

/// Pourquoi une cible n'est pas retenue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TargetRejection {
    #[error("cible mal formée : {0}")]
    Malformed(&'static str),
    /// L'adresse n'est pas en HTTPS (l'agent la refuserait aussi).
    #[error("adresse non HTTPS")]
    NotHttps,
    /// Adresse de bouclage, privée, de lien local ou sans nom public (BR-UPDATE-027, côté agent).
    #[error("adresse locale ou privée")]
    LocalAddress,
    /// L'adresse n'est pas celle des releases du dépôt prévu.
    #[error("source de téléchargement refusée")]
    UntrustedSource,
}

/// Vérifie une cible annoncée avant de la retenir ou de l'envoyer. Aucune version n'est comparée à
/// celle de l'agent ici (voir [`is_newer`]) : on ne connaît que le flux.
pub fn validate_target(
    candidate: &AgentCandidate,
    policy: &DownloadPolicy,
) -> Result<AgentTarget, TargetRejection> {
    let version = semver::Version::parse(candidate.version.trim())
        .map_err(|_| TargetRejection::Malformed("numéro de version"))?;
    if !version.pre.is_empty() || !version.build.is_empty() {
        return Err(TargetRejection::Malformed(
            "préversion ou métadonnées de construction",
        ));
    }
    let sha256 = candidate.sha256.trim().to_ascii_lowercase();
    if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(TargetRejection::Malformed("somme SHA-256"));
    }
    let signature = candidate.signature.trim();
    if signature.is_empty()
        || signature.len() > SIGNATURE_MAX_LEN
        || signature
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        return Err(TargetRejection::Malformed("signature"));
    }
    if candidate.url.len() > URL_MAX_LEN {
        return Err(TargetRejection::Malformed("adresse trop longue"));
    }
    let url = Url::parse(candidate.url.trim())
        .map_err(|_| TargetRejection::Malformed("adresse de téléchargement"))?;
    check_address(&url, policy)?;
    Ok(AgentTarget {
        version,
        url,
        signature: signature.to_owned(),
        sha256,
    })
}

/// L'adresse du binaire : HTTPS, sans identifiant ni fragment, hôte public, et source permise par la
/// politique (les releases du dépôt public). Les deux dernières vérifications sont celles que le
/// client applique déjà à son propre installateur ; la première est celle de l'agent
/// (BR-UPDATE-027), refaite ici pour ne jamais lui envoyer ce qu'il refuserait.
fn check_address(url: &Url, policy: &DownloadPolicy) -> Result<(), TargetRejection> {
    if policy.https_only() {
        if url.scheme() != "https" {
            return Err(TargetRejection::NotHttps);
        }
        if url.port().is_some_and(|port| port != 443) {
            return Err(TargetRejection::UntrustedSource);
        }
        if host_is_local(url) {
            return Err(TargetRejection::LocalAddress);
        }
    }
    if url.fragment().is_some() || !policy.allows(url) {
        return Err(TargetRejection::UntrustedSource);
    }
    Ok(())
}

/// Hôte de bouclage, privé, de lien local, partagé, sans point (nom interne) ou de suffixe local.
/// Table volontairement plus stricte que celle de l'agent sur les noms : une adresse refusée ici
/// n'est jamais envoyée ; une adresse que l'agent refuserait seul reste refusée par lui.
pub fn host_is_local(url: &Url) -> bool {
    match url.host() {
        None => true,
        Some(Host::Ipv4(ip)) => ipv4_is_local(ip),
        Some(Host::Ipv6(ip)) => ipv6_is_local(ip),
        Some(Host::Domain(name)) => {
            let name = name.trim_end_matches('.').to_ascii_lowercase();
            name.is_empty()
                || !name.contains('.')
                || name == "localhost"
                || [
                    "localhost",
                    "local",
                    "localdomain",
                    "internal",
                    "lan",
                    "home",
                ]
                .iter()
                .any(|suffix| name.ends_with(&format!(".{suffix}")))
        }
    }
}

fn ipv4_is_local(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_documentation()
        // 100.64.0.0/10 (espace partagé), 192.0.0.0/24, 198.18.0.0/15, 240.0.0.0/4.
        || (a == 100 && (64..128).contains(&b))
        || (a == 192 && b == 0)
        || (a == 198 && (18..20).contains(&b))
        || a >= 240
        || a == 0
}

fn ipv6_is_local(ip: Ipv6Addr) -> bool {
    if let Some(mapped) = ip.to_ipv4_mapped() {
        return ipv4_is_local(mapped);
    }
    let first = ip.segments()[0];
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        // fc00::/7 (privées), fe80::/10 (lien local), 2001:db8::/32 (documentation), ::/96 (compatibles v4).
        || (first & 0xfe00) == 0xfc00
        || (first & 0xffc0) == 0xfe80
        || (first == 0x2001 && ip.segments()[1] == 0x0db8)
        || ip.segments()[..6].iter().all(|segment| *segment == 0)
        // 64:ff9b::/96 (NAT64) : décidée par l'IPv4 incorporée.
        || (ip.segments()[..6] == [0x0064, 0xff9b, 0, 0, 0, 0]
            && ipv4_is_local(Ipv4Addr::from(
                (u32::from(ip.segments()[6]) << 16) | u32::from(ip.segments()[7]),
            )))
}

/// Une adresse IP littérale quelconque (pour les tests et les messages).
pub fn is_local_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ipv4_is_local(ip),
        IpAddr::V6(ip) => ipv6_is_local(ip),
    }
}

/// La cible est-elle STRICTEMENT plus récente que l'agent installé ? Jamais de rétrogradation ni de
/// réinstallation de la même version (BR-UPDATE-022). Une version d'agent illisible, ou avec une
/// préversion, ne se compare pas : rien n'est proposé.
pub fn is_newer(current: &str, target: &AgentTarget) -> bool {
    semver::Version::parse(current.trim())
        .is_ok_and(|current| current.pre.is_empty() && target.version > current)
}

/// Ce que lit le service du manifeste des cibles (`agent.json`) : JSON ordinaire, une entrée par
/// plateforme. Un fichier absent ou sans l'entrée de l'agent n'est pas une erreur : rien à proposer.
pub fn parse_manifest(bytes: &[u8]) -> Result<Option<AgentCandidate>, ManifestError> {
    if bytes.len() > MANIFEST_MAX_BYTES {
        return Err(ManifestError::TooLarge);
    }
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| ManifestError::Unreadable)?;
    let Some(entry) = value
        .get("platforms")
        .and_then(|platforms| platforms.get(AGENT_PLATFORM))
    else {
        return Ok(None);
    };
    let text = |value: &serde_json::Value, key: &str| {
        value
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    match (
        text(&value, "version"),
        text(entry, "url"),
        text(entry, "signature"),
        text(entry, "sha256"),
    ) {
        (Some(version), Some(url), Some(signature), Some(sha256)) => Ok(Some(AgentCandidate {
            version,
            url,
            signature,
            sha256,
        })),
        _ => Err(ManifestError::Unreadable),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ManifestError {
    #[error("fichier de cibles trop volumineux")]
    TooLarge,
    #[error("fichier de cibles illisible")]
    Unreadable,
}

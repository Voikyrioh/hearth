//! La description durable d'un service créé par Hearth (HRT-83, ADR-0044, BR-EDIT-035).
//!
//! **La description dit ce qui est VOULU ; le moteur dit ce qui est OBSERVÉ** (`drift`). Elle contient
//! de quoi recréer le service sans le moteur, jamais un identifiant de conteneur, un état ni une mesure.
//! Elle est numérotée : chaque modification est une **révision**, la précédente reste lisible.
//!
//! Deux parties, qui ne se mélangent jamais :
//! - [`SpecBody`] : tout ce qui n'est pas secret (image, ports, montages, limites). Écrite en clair dans
//!   `service_specs.spec_json`.
//! - [`SpecSecrets`] : noms et valeurs de variables, commande, réponses secrètes d'une fiche. Types
//!   `Secret` (jamais affichés, effacés de la mémoire), scellés un à un (`domain::sealed`) avant d'aller en
//!   base. Même le NOM d'une variable est scellé (`STEAM_PASSWORD` dit déjà quelque chose).
//!
//! Ni l'une ni l'autre n'a de `Display`. Aucune des deux n'est envoyée au client.

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;

use super::key::ServiceKey;
use super::limits::{
    MAX_COMMAND_ARG_CHARS, MAX_COMMAND_ARGS, MAX_ENV_NAME_CHARS, MAX_ENV_NAMES,
    MAX_IMAGE_REF_CHARS, MAX_IMAGE_TAG_CHARS, MAX_MOUNTS, MAX_NAME_CHARS, MAX_PATH_CHARS,
    MAX_PORTS,
};
use crate::domain::sealed::{AnswerId, MAX_PLAIN_BYTES, SecretField};
use crate::domain::secret::Secret;

/// Nombre maximal d'éléments ignorés des sauvegardes gardés dans les réglages.
pub const MAX_BACKUP_IGNORED: usize = 256;
/// Nombre maximal de réponses secrètes d'une fiche.
pub const MAX_ANSWERS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpecProtocol {
    Tcp,
    Udp,
    Sctp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpecRestart {
    No,
    Always,
    UnlessStopped,
    OnFailure,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecImage {
    pub repository: String,
    pub tag: String,
    /// Empreinte épinglée (`sha256:...`), facultative.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SpecPort {
    pub host_port: u16,
    pub container_port: u16,
    pub protocol: SpecProtocol,
}

/// D'où vient un montage : un emplacement géré par Hearth (sous sa racine de données) ou un dossier du
/// serveur ouvert aux services.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpecMountKind {
    Managed,
    HostFolder,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SpecMount {
    pub kind: SpecMountKind,
    /// Nom de l'emplacement géré, ou chemin du dossier du serveur.
    pub source: String,
    pub target: String,
    #[serde(default)]
    pub read_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IgnoredItem {
    /// Racine de montage (cible dans le service), puis chemin relatif.
    pub root: String,
    pub path: String,
}

/// Les réglages que le moteur ne sait pas garder.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disk_alert_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_address: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub backup_ignored: Vec<IgnoredItem>,
}

/// Nombre maximal de contrats acceptés gardés avec une fiche.
pub const MAX_AGREEMENTS: usize = 4;
/// Longueur maximale de l'identifiant d'une réponse ou d'un contrat.
pub const MAX_FICHE_ID_CHARS: usize = 64;
/// Longueur maximale d'une réponse non secrète gardée en clair.
pub const MAX_ANSWER_VALUE_CHARS: usize = 1024;

/// Une réponse donnée à une question de la fiche (HRT-101, BR-FICHE-029). La valeur n'est gardée EN CLAIR que
/// pour une question non secrète : pour une question secrète, `value` est absent et la valeur est scellée dans
/// [`SpecSecrets::answers`] sous le même identifiant (ADR-0044).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecAnswer {
    /// Identifiant du réglage de la fiche (`[a-z0-9_]`).
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default)]
    pub secret: bool,
    /// Question figée par la fiche au moment de l'installation : une mise à jour de la fiche ne la change pas
    /// (BR-EDIT-035).
    #[serde(default)]
    pub frozen: bool,
}

/// Un contrat accepté à l'installation : qui, quand (BR-FICHE-058).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecAgreement {
    pub id: String,
    pub accepted_by: String,
    /// Secondes Unix.
    pub accepted_at: i64,
}

/// Le service vient d'une fiche de serveur de jeu : sa fiche, sa version, son empreinte, la variante choisie,
/// les réponses (les secrètes scellées à part) et les contrats acceptés. Les champs ajoutés par HRT-101 sont
/// facultatifs : une révision plus ancienne reste lisible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecFiche {
    pub id: String,
    pub version: u32,
    pub digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub answers: Vec<SpecAnswer>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agreements: Vec<SpecAgreement>,
}

impl SpecFiche {
    /// Une fiche sans variante, réponse ni contrat.
    pub fn new(id: impl Into<String>, version: u32, digest: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            version,
            digest: digest.into(),
            variant: None,
            answers: Vec::new(),
            agreements: Vec::new(),
        }
    }

    fn validate(&self) -> Result<(), SpecError> {
        let id_ok = |id: &str| {
            !id.is_empty()
                && id.chars().count() <= MAX_FICHE_ID_CHARS
                && id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
        };
        if !id_ok(&self.id) || self.digest.is_empty() {
            return Err(SpecError::Fiche);
        }
        if self.variant.as_deref().is_some_and(|v| !id_ok(v)) {
            return Err(SpecError::Fiche);
        }
        let mut seen = std::collections::HashSet::new();
        if self.answers.len() > MAX_ANSWERS
            || self.answers.iter().any(|answer| {
                !id_ok(&answer.id)
                    || !seen.insert(answer.id.as_str())
                    // Une valeur secrète n'est jamais en clair ici ; une non secrète est bornée.
                    || (answer.secret && answer.value.is_some())
                    || answer
                        .value
                        .as_ref()
                        .is_some_and(|v| v.chars().count() > MAX_ANSWER_VALUE_CHARS)
            })
        {
            return Err(SpecError::Fiche);
        }
        let mut agreed = std::collections::HashSet::new();
        if self.agreements.len() > MAX_AGREEMENTS
            || self.agreements.iter().any(|a| {
                !id_ok(&a.id)
                    || !agreed.insert(a.id.as_str())
                    || a.accepted_by.is_empty()
                    || a.accepted_by.chars().count() > MAX_NAME_CHARS
            })
        {
            return Err(SpecError::Fiche);
        }
        Ok(())
    }
}

/// La partie de la description qui n'est pas secrète.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecBody {
    pub name: String,
    pub image: SpecImage,
    #[serde(default)]
    pub ports: Vec<SpecPort>,
    #[serde(default)]
    pub mounts: Vec<SpecMount>,
    pub restart: SpecRestart,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_grace_s: Option<u32>,
    #[serde(default)]
    pub stdin_open: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_millis: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_bytes: Option<u64>,
    #[serde(default)]
    pub settings: SpecSettings,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fiche: Option<SpecFiche>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SpecError {
    #[error("le nom du service est vide ou trop long")]
    Name,
    #[error("l'image du service est vide ou trop longue")]
    Image,
    #[error("trop de ports, ou un port à 0")]
    Ports,
    #[error("trop de montages, ou un montage sans chemin")]
    Mounts,
    #[error("une limite à 0")]
    Limits,
    #[error("trop d'éléments ignorés des sauvegardes")]
    Ignored,
    #[error("trop de variables, ou un nom de variable vide ou trop long")]
    Env,
    #[error("commande vide ou trop longue")]
    Command,
    #[error("trop de réponses, ou deux réponses à la même question")]
    Answers,
    #[error("la fiche du service est mal formée (identifiant, réponses ou contrats)")]
    Fiche,
    #[error("un secret dépasse {MAX_PLAIN_BYTES} octets")]
    SecretTooLong,
}

impl SpecBody {
    /// Les bornes de la description (BR-EDIT-035) : rien d'illisible ni de démesuré n'entre dans la base.
    pub fn validate(&self) -> Result<(), SpecError> {
        if self.name.trim().is_empty() || self.name.chars().count() > MAX_NAME_CHARS {
            return Err(SpecError::Name);
        }
        let image = &self.image;
        if image.repository.trim().is_empty()
            || image.repository.chars().count() > MAX_IMAGE_REF_CHARS
            || image.tag.chars().count() > MAX_IMAGE_TAG_CHARS
        {
            return Err(SpecError::Image);
        }
        if self.ports.len() > MAX_PORTS
            || self
                .ports
                .iter()
                .any(|port| port.host_port == 0 || port.container_port == 0)
        {
            return Err(SpecError::Ports);
        }
        if self.mounts.len() > MAX_MOUNTS
            || self.mounts.iter().any(|mount| {
                mount.source.is_empty()
                    || mount.target.is_empty()
                    || mount.source.chars().count() > MAX_PATH_CHARS
                    || mount.target.chars().count() > MAX_PATH_CHARS
            })
        {
            return Err(SpecError::Mounts);
        }
        if self.cpu_millis == Some(0) || self.memory_bytes == Some(0) {
            return Err(SpecError::Limits);
        }
        if self.settings.backup_ignored.len() > MAX_BACKUP_IGNORED {
            return Err(SpecError::Ignored);
        }
        if let Some(fiche) = &self.fiche {
            fiche.validate()?;
        }
        Ok(())
    }

    /// Le nom, sans casse ni espaces autour : la clé d'unicité des noms (BR-DEL-019).
    pub fn name_folded(&self) -> String {
        fold_name(&self.name)
    }
}

/// Le nom replié : deux services du même nom, à la casse près, ne coexistent pas.
pub fn fold_name(name: &str) -> String {
    name.trim().to_lowercase()
}

/// Une variable posée par Hearth : son nom ET sa valeur sont des secrets.
pub struct EnvEntry {
    pub name: Secret,
    pub value: Secret,
}

impl fmt::Debug for EnvEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("EnvEntry(***)")
    }
}

/// La réponse secrète à une question d'une fiche.
pub struct AnswerEntry {
    pub id: AnswerId,
    pub value: Secret,
}

impl fmt::Debug for AnswerEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AnswerEntry({}, ***)", self.id.as_str())
    }
}

/// Tout ce que la description garde de secret.
#[derive(Debug, Default)]
pub struct SpecSecrets {
    pub env: Vec<EnvEntry>,
    /// La commande de lancement, élément par élément ; `None` = celle de l'image.
    pub command: Option<Vec<Secret>>,
    pub answers: Vec<AnswerEntry>,
}

impl SpecSecrets {
    pub fn is_empty(&self) -> bool {
        self.env.is_empty() && self.command.is_none() && self.answers.is_empty()
    }

    pub fn validate(&self) -> Result<(), SpecError> {
        if self.env.len() > MAX_ENV_NAMES
            || self.env.iter().any(|entry| {
                entry.name.is_empty() || entry.name.expose().chars().count() > MAX_ENV_NAME_CHARS
            })
        {
            return Err(SpecError::Env);
        }
        if let Some(command) = &self.command
            && (command.is_empty()
                || command.len() > MAX_COMMAND_ARGS
                || command
                    .iter()
                    .any(|arg| arg.expose().chars().count() > MAX_COMMAND_ARG_CHARS))
        {
            return Err(SpecError::Command);
        }
        let mut seen = std::collections::HashSet::new();
        if self.answers.len() > MAX_ANSWERS
            || !self
                .answers
                .iter()
                .all(|answer| seen.insert(answer.id.as_str().to_owned()))
        {
            return Err(SpecError::Answers);
        }
        Ok(())
    }

    /// Les champs à sceller, dans l'ordre stable : variables par rang, commande, réponses.
    pub fn fields(
        &self,
    ) -> Result<Vec<(SecretField, crate::domain::sealed::PlainBytes)>, SpecError> {
        self.validate()?;
        let mut out = Vec::with_capacity(self.env.len() + 1 + self.answers.len());
        for (rank, entry) in self.env.iter().enumerate() {
            let rank = u16::try_from(rank).map_err(|_| SpecError::Env)?;
            out.push((
                SecretField::Env(rank),
                encode_parts(&[entry.name.expose(), entry.value.expose()])?,
            ));
        }
        if let Some(command) = &self.command {
            let parts: Vec<&str> = command.iter().map(Secret::expose).collect();
            out.push((SecretField::Command, encode_parts(&parts)?));
        }
        for answer in &self.answers {
            out.push((
                SecretField::Answer(answer.id.clone()),
                encode_parts(&[answer.value.expose()])?,
            ));
        }
        Ok(out)
    }

    /// Reconstruit les secrets depuis leurs champs ouverts. Un champ mal formé, un doublon ou un rang
    /// manquant est un refus : rien n'est deviné.
    pub fn from_fields(
        fields: Vec<(SecretField, crate::domain::sealed::PlainBytes)>,
    ) -> Result<Self, SecretsDecodeError> {
        let mut env: Vec<(u16, EnvEntry)> = Vec::new();
        let mut command = None;
        let mut answers = Vec::new();
        for (field, plain) in fields {
            let mut parts = decode_parts(plain.expose()).ok_or(SecretsDecodeError)?;
            match field {
                SecretField::Env(rank) => {
                    let (Some(value), Some(name), None) = (parts.pop(), parts.pop(), parts.pop())
                    else {
                        return Err(SecretsDecodeError);
                    };
                    env.push((
                        rank,
                        EnvEntry {
                            name: Secret::new(name),
                            value: Secret::new(value),
                        },
                    ));
                }
                // Le manifeste est vérifié à part, avant : il n'est jamais une donnée de la description.
                SecretField::Manifest => return Err(SecretsDecodeError),
                SecretField::Command => {
                    if command.is_some() {
                        return Err(SecretsDecodeError);
                    }
                    command = Some(parts.into_iter().map(Secret::new).collect());
                }
                SecretField::Answer(id) => {
                    let (Some(value), None) = (parts.pop(), parts.pop()) else {
                        return Err(SecretsDecodeError);
                    };
                    answers.push(AnswerEntry {
                        id,
                        value: Secret::new(value),
                    });
                }
            }
        }
        env.sort_by_key(|(rank, _)| *rank);
        // Les rangs se suivent de 0 sans trou ni doublon : sinon un champ a été retiré ou ajouté.
        let contiguous = env
            .iter()
            .enumerate()
            .all(|(index, (rank, _))| usize::from(*rank) == index);
        if !contiguous {
            return Err(SecretsDecodeError);
        }
        Ok(Self {
            env: env.into_iter().map(|(_, entry)| entry).collect(),
            command,
            answers,
        })
    }
}

/// Étiquette et version du manifeste scellé d'une révision (premier texte du bloc).
const MANIFEST_LABEL: &str = "hearth/spec-manifest/1";

/// Les noms des champs, dans l'ordre canonique (alphabétique du nom stocké), sans le manifeste.
fn canonical_names(fields: &[SecretField]) -> Vec<String> {
    let mut names: Vec<String> = fields
        .iter()
        .filter(|field| **field != SecretField::Manifest)
        .map(SecretField::as_text)
        .collect();
    names.sort();
    names
}

/// Le contenu du manifeste d'une révision : étiquette, nombre de champs, puis leurs noms dans l'ordre
/// canonique. Il est scellé comme un secret, lié au service et à la révision par les données associées.
pub fn manifest_plain(
    fields: &[SecretField],
) -> Result<crate::domain::sealed::PlainBytes, SpecError> {
    let names = canonical_names(fields);
    let count = names.len().to_string();
    let mut parts: Vec<&str> = Vec::with_capacity(names.len() + 2);
    parts.push(MANIFEST_LABEL);
    parts.push(&count);
    parts.extend(names.iter().map(String::as_str));
    encode_parts(&parts)
}

/// Le manifeste ouvert décrit EXACTEMENT les champs trouvés en base : ni un de moins (retrait), ni un de
/// plus (ajout ou recopie), ni un autre (permutation), et le nombre dit est le nombre trouvé.
pub fn manifest_matches(manifest: &[u8], stored: &[SecretField]) -> bool {
    let Some(mut parts) = decode_parts(manifest) else {
        return false;
    };
    if parts.len() < 2 || parts[0] != MANIFEST_LABEL {
        return false;
    }
    let names = parts.split_off(2);
    let expected = canonical_names(stored);
    parts[1] == expected.len().to_string() && names == expected
}

/// Les champs ouverts ne forment pas une description lisible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("les secrets ouverts ne forment pas une description lisible")]
pub struct SecretsDecodeError;

/// Compose des textes en un seul bloc : nombre de textes (u16), puis chaque texte précédé de sa longueur
/// (u32). Sans séparateur ambigu : un nom ou une valeur peut contenir n'importe quel caractère.
fn encode_parts(parts: &[&str]) -> Result<crate::domain::sealed::PlainBytes, SpecError> {
    let mut out = Vec::new();
    out.extend_from_slice(
        &u16::try_from(parts.len())
            .map_err(|_| SpecError::Command)?
            .to_be_bytes(),
    );
    for part in parts {
        out.extend_from_slice(
            &u32::try_from(part.len())
                .map_err(|_| SpecError::SecretTooLong)?
                .to_be_bytes(),
        );
        out.extend_from_slice(part.as_bytes());
    }
    if out.len() > MAX_PLAIN_BYTES {
        zeroize::Zeroize::zeroize(&mut out);
        return Err(SpecError::SecretTooLong);
    }
    Ok(crate::domain::sealed::PlainBytes::new(out))
}

fn decode_parts(bytes: &[u8]) -> Option<Vec<String>> {
    let (count, mut rest) = bytes.split_first_chunk::<2>()?;
    let count = usize::from(u16::from_be_bytes(*count));
    let mut parts = Vec::with_capacity(count.min(64));
    for _ in 0..count {
        let (length, tail) = rest.split_first_chunk::<4>()?;
        let length = usize::try_from(u32::from_be_bytes(*length)).ok()?;
        if tail.len() < length {
            return None;
        }
        let (text, tail) = tail.split_at(length);
        parts.push(String::from_utf8(text.to_vec()).ok()?);
        rest = tail;
    }
    rest.is_empty().then_some(parts)
}

/// Une révision de la description, telle que le dépôt la rend. Les secrets sont rendus à part
/// ([`crate::domain::services::spec::SecretsAccess`]) : une clé illisible n'empêche pas de lire le reste.
#[derive(Debug)]
pub struct SpecRevision {
    pub key: ServiceKey,
    pub rev: u32,
    pub body: SpecBody,
    /// Identifiant du compte qui a écrit cette révision.
    pub updated_by: String,
    pub created_at: OffsetDateTime,
    /// C'est la révision courante du service.
    pub current: bool,
}

/// Les secrets d'une révision : ouverts, ou illisibles (et pourquoi). Ne dit jamais de valeur.
#[derive(Debug)]
pub enum SecretsAccess {
    Opened(SpecSecrets),
    /// La clé est inutilisable : la description se lit, ses secrets non.
    Unreadable(crate::domain::sealed::KeyProblem),
    /// Un scellé ne s'ouvre pas : il a été recollé ailleurs, altéré, ou la clé n'est pas la bonne.
    Tampered,
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn body(name: &str) -> SpecBody {
        SpecBody {
            name: name.to_owned(),
            image: SpecImage {
                repository: "ghcr.io/acme/terraria".into(),
                tag: "1.4.4".into(),
                digest: None,
            },
            ports: vec![SpecPort {
                host_port: 7777,
                container_port: 7777,
                protocol: SpecProtocol::Tcp,
            }],
            mounts: vec![SpecMount {
                kind: SpecMountKind::Managed,
                source: "world".into(),
                target: "/data".into(),
                read_only: false,
            }],
            restart: SpecRestart::UnlessStopped,
            stop_grace_s: Some(30),
            stdin_open: false,
            cpu_millis: None,
            memory_bytes: Some(2 << 30),
            settings: SpecSettings::default(),
            fiche: None,
        }
    }

    fn secrets() -> SpecSecrets {
        SpecSecrets {
            env: vec![
                EnvEntry {
                    name: Secret::from("SERVER_PASSWORD"),
                    value: Secret::from("p=\u{0}ass\nword"),
                },
                EnvEntry {
                    name: Secret::from("EMPTY"),
                    value: Secret::from(""),
                },
            ],
            command: Some(vec![Secret::from("run"), Secret::from("--token=abc def")]),
            answers: vec![AnswerEntry {
                id: AnswerId::parse("password").unwrap(),
                value: Secret::from("hunter2"),
            }],
        }
    }

    #[test]
    fn a_body_round_trips_through_json_and_tolerates_unknown_and_missing_fields() {
        let original = body("Terraria");
        let json = serde_json::to_string(&original).unwrap();
        assert_eq!(serde_json::from_str::<SpecBody>(&json).unwrap(), original);
        // Un champ ajouté par une version plus récente est ignoré ; un champ facultatif absent prend son défaut.
        let minimal =
            r#"{"name":"A","image":{"repository":"r","tag":"t"},"restart":"always","future":42}"#;
        let read: SpecBody = serde_json::from_str(minimal).unwrap();
        assert!(read.ports.is_empty() && read.fiche.is_none() && !read.stdin_open);
    }

    #[test]
    fn the_body_has_no_room_for_a_container_id_a_state_or_a_measure() {
        let json = serde_json::to_value(body("A")).unwrap();
        let keys: Vec<&String> = json.as_object().unwrap().keys().collect();
        for forbidden in [
            "container_id",
            "id",
            "state",
            "status",
            "stats",
            "env",
            "command",
            "answers",
        ] {
            assert!(!keys.iter().any(|key| *key == forbidden), "{forbidden}");
        }
    }

    #[test]
    fn the_bounds_refuse_what_is_unreadable_or_absurd() {
        assert!(body("A").validate().is_ok());
        let mut bad = body(" ");
        assert_eq!(bad.validate(), Err(SpecError::Name));
        bad = body(&"x".repeat(MAX_NAME_CHARS + 1));
        assert_eq!(bad.validate(), Err(SpecError::Name));
        bad = body("A");
        bad.image.repository.clear();
        assert_eq!(bad.validate(), Err(SpecError::Image));
        bad = body("A");
        bad.ports[0].host_port = 0;
        assert_eq!(bad.validate(), Err(SpecError::Ports));
        bad = body("A");
        bad.mounts[0].target.clear();
        assert_eq!(bad.validate(), Err(SpecError::Mounts));
        bad = body("A");
        bad.cpu_millis = Some(0);
        assert_eq!(bad.validate(), Err(SpecError::Limits));
    }

    fn fiche_body() -> SpecBody {
        let mut body = body("Terraria");
        body.fiche = Some(SpecFiche {
            id: "terraria".into(),
            version: 3,
            digest: "sha256:abc".into(),
            variant: Some("vanilla".into()),
            answers: vec![
                SpecAnswer {
                    id: "max_players".into(),
                    value: Some("8".into()),
                    secret: false,
                    frozen: true,
                },
                SpecAnswer {
                    id: "password".into(),
                    value: None,
                    secret: true,
                    frozen: false,
                },
            ],
            agreements: vec![SpecAgreement {
                id: "eula".into(),
                accepted_by: "voiky".into(),
                accepted_at: 1_790_000_000,
            }],
        });
        body
    }

    #[test]
    fn a_fiche_description_keeps_fiche_version_digest_answers_agreements_and_frozen_questions() {
        let original = fiche_body();
        assert!(original.validate().is_ok());
        let json = serde_json::to_string(&original).unwrap();
        let read: SpecBody = serde_json::from_str(&json).unwrap();
        assert_eq!(read, original);
        let fiche = read.fiche.unwrap();
        assert_eq!((fiche.id.as_str(), fiche.version), ("terraria", 3));
        assert_eq!(fiche.digest, "sha256:abc");
        assert_eq!(fiche.variant.as_deref(), Some("vanilla"));
        assert!(fiche.answers[0].frozen && !fiche.answers[1].frozen);
        // Une réponse secrète n'a jamais de valeur en clair dans la description.
        assert!(fiche.answers[1].value.is_none());
        assert_eq!(fiche.agreements[0].id, "eula");
        // Une révision plus ancienne, sans ces champs, reste lisible.
        let old = r#"{"name":"A","image":{"repository":"r","tag":"t"},"restart":"always",
            "fiche":{"id":"terraria","version":1,"digest":"sha256:x"}}"#;
        let old: SpecBody = serde_json::from_str(old).unwrap();
        let fiche = old.fiche.unwrap();
        assert!(fiche.variant.is_none() && fiche.answers.is_empty() && fiche.agreements.is_empty());
    }

    #[test]
    fn a_fiche_description_refuses_a_clear_secret_a_duplicate_or_too_many() {
        let bad = |edit: &dyn Fn(&mut SpecFiche)| {
            let mut body = fiche_body();
            edit(body.fiche.as_mut().unwrap());
            body.validate()
        };
        assert_eq!(
            bad(&|f| f.answers[1].value = Some("x".into())),
            Err(SpecError::Fiche)
        );
        assert_eq!(
            bad(&|f| f.answers[1].id = "max_players".into()),
            Err(SpecError::Fiche)
        );
        assert_eq!(bad(&|f| f.id = "Terraria".into()), Err(SpecError::Fiche));
        assert_eq!(
            bad(&|f| f.agreements = (0..=MAX_AGREEMENTS)
                .map(|n| SpecAgreement {
                    id: format!("c{n}"),
                    accepted_by: "v".into(),
                    accepted_at: 0
                })
                .collect()),
            Err(SpecError::Fiche)
        );
        assert_eq!(
            bad(&|f| f.answers = (0..=MAX_ANSWERS)
                .map(|n| SpecAnswer {
                    id: format!("q{n}"),
                    value: None,
                    secret: true,
                    frozen: false
                })
                .collect()),
            Err(SpecError::Fiche)
        );
    }

    #[test]
    fn names_fold_for_uniqueness() {
        assert_eq!(body("  Terraria ").name_folded(), "terraria");
        assert_eq!(fold_name("TERRARIA"), fold_name("terraria"));
    }

    #[test]
    fn secrets_become_one_field_each_in_a_stable_order_and_come_back_identical() {
        let original = secrets();
        let fields = original.fields().unwrap();
        let names: Vec<String> = fields.iter().map(|(field, _)| field.as_text()).collect();
        assert_eq!(names, ["env:0", "env:1", "command", "answer:password"]);
        // Aucun nom de champ ne contient un nom de variable.
        assert!(
            names
                .iter()
                .all(|name| !name.contains("PASSWORD") && !name.contains("EMPTY"))
        );
        let back = SpecSecrets::from_fields(fields).unwrap();
        assert_eq!(back.env[0].name.expose(), "SERVER_PASSWORD");
        assert_eq!(back.env[0].value.expose(), "p=\u{0}ass\nword");
        assert_eq!(back.env[1].value.expose(), "");
        let command: Vec<&str> = back
            .command
            .as_ref()
            .unwrap()
            .iter()
            .map(Secret::expose)
            .collect();
        assert_eq!(command, ["run", "--token=abc def"]);
        assert_eq!(back.answers[0].value.expose(), "hunter2");
    }

    #[test]
    fn no_secret_type_shows_a_value_in_debug() {
        let shown = format!("{:?}", secrets());
        for sentinel in [
            "SERVER_PASSWORD",
            "abc def",
            "token",
            "hunter2",
            "EMPTY",
            "run",
        ] {
            assert!(!shown.contains(sentinel), "{sentinel} dans {shown}");
        }
        assert!(
            shown.contains("password"),
            "l'identifiant d'une question n'est pas un secret : {shown}"
        );
    }

    #[test]
    fn decoding_refuses_a_missing_duplicated_or_malformed_field() {
        use crate::domain::sealed::PlainBytes;
        let fields = secrets().fields().unwrap();
        // Un rang qui manque (env:0 retiré) : refus.
        let without_first: Vec<_> = secrets()
            .fields()
            .unwrap()
            .into_iter()
            .filter(|(field, _)| *field != SecretField::Env(0))
            .collect();
        assert!(SpecSecrets::from_fields(without_first).is_err());
        // Octets qui ne sont pas un bloc de textes.
        assert!(
            SpecSecrets::from_fields(vec![(SecretField::Command, PlainBytes::new(vec![0, 5, 1]))])
                .is_err()
        );
        // Une variable doit avoir exactement un nom et une valeur.
        let one = encode_parts(&["seul"]).unwrap();
        assert!(SpecSecrets::from_fields(vec![(SecretField::Env(0), one)]).is_err());
        // Deux commandes.
        let cmd = || (SecretField::Command, encode_parts(&["a"]).unwrap());
        assert!(SpecSecrets::from_fields(vec![cmd(), cmd()]).is_err());
        drop(fields);
    }

    #[test]
    fn the_bounds_apply_to_secrets_too() {
        let mut too_many = SpecSecrets::default();
        for _ in 0..=MAX_ENV_NAMES {
            too_many.env.push(EnvEntry {
                name: Secret::from("A"),
                value: Secret::from("v"),
            });
        }
        assert_eq!(too_many.validate(), Err(SpecError::Env));
        let empty_name = SpecSecrets {
            env: vec![EnvEntry {
                name: Secret::from(""),
                value: Secret::from("v"),
            }],
            ..SpecSecrets::default()
        };
        assert_eq!(empty_name.validate(), Err(SpecError::Env));
        let empty_command = SpecSecrets {
            command: Some(Vec::new()),
            ..SpecSecrets::default()
        };
        assert_eq!(empty_command.validate(), Err(SpecError::Command));
        let twice = SpecSecrets {
            answers: vec![
                AnswerEntry {
                    id: AnswerId::parse("a").unwrap(),
                    value: Secret::from("1"),
                },
                AnswerEntry {
                    id: AnswerId::parse("a").unwrap(),
                    value: Secret::from("2"),
                },
            ],
            ..SpecSecrets::default()
        };
        assert_eq!(twice.validate(), Err(SpecError::Answers));
        let huge = SpecSecrets {
            answers: vec![AnswerEntry {
                id: AnswerId::parse("a").unwrap(),
                value: Secret::from("x".repeat(MAX_PLAIN_BYTES)),
            }],
            ..SpecSecrets::default()
        };
        assert_eq!(huge.fields().unwrap_err(), SpecError::SecretTooLong);
    }

    fn names() -> Vec<SecretField> {
        vec![
            SecretField::Env(0),
            SecretField::Env(1),
            SecretField::Command,
            SecretField::Answer(AnswerId::parse("password").unwrap()),
        ]
    }

    /// Le manifeste dit EXACTEMENT les champs : un de moins, un de plus, un autre, ou un doublon ne
    /// correspondent plus ; l'ordre de présentation ne compte pas (ordre canonique).
    #[test]
    fn the_manifest_matches_exactly_the_fields_it_lists() {
        let plain = manifest_plain(&names()).unwrap();
        let manifest = plain.expose();
        assert!(manifest_matches(manifest, &names()));
        let mut shuffled = names();
        shuffled.reverse();
        assert!(manifest_matches(manifest, &shuffled), "ordre canonique");

        let mut fewer = names();
        fewer.pop();
        assert!(!manifest_matches(manifest, &fewer), "retrait");
        let mut more = names();
        more.push(SecretField::Env(2));
        assert!(!manifest_matches(manifest, &more), "ajout");
        let mut swapped = names();
        swapped[1] = SecretField::Env(5);
        assert!(!manifest_matches(manifest, &swapped), "autre champ");
        let mut twice = names();
        twice.push(SecretField::Command);
        assert!(!manifest_matches(manifest, &twice), "doublon");
        assert!(!manifest_matches(&[], &names()));
        assert!(!manifest_matches(&[0, 0], &names()));
        // Le manifeste lui-même n'est jamais compté parmi les champs.
        let mut with_manifest = names();
        with_manifest.push(SecretField::Manifest);
        assert!(manifest_matches(manifest, &with_manifest));
    }

    #[test]
    fn a_manifest_is_never_a_description_field() {
        let plain = manifest_plain(&names()).unwrap();
        assert!(SpecSecrets::from_fields(vec![(SecretField::Manifest, plain)]).is_err());
    }
}

//! Ce que la coquille dit à l'interface sur la liaison : types sérialisés (typés par tauri-specta),
//! conversions depuis `hearth-link`, et le carnet des états (numéro de séquence par serveur).
//! Pur : sans E/S, sans Tauri. Les dates sont des millisecondes depuis l'époque (`f64`, un
//! `i64` n'existe pas côté web), les compteurs des `u32`.

use std::collections::HashMap;

use hearth_link::domain::book::BookError;
use hearth_link::domain::compat::Compatibility;
use hearth_link::domain::event::StateInfo;
use hearth_link::domain::pending_ops::Outcome;
use hearth_link::domain::server::{ServerId, ServerRecord};
use hearth_link::domain::state::{Blocked, LinkState, Reason};
use hearth_link::{InputField, LinkError};
use hearth_proto::api::accounts::RoleName;
use hearth_proto::error::UpgradeTarget;
use hearth_proto::fingerprint::Fingerprint;
use hearth_proto::product::DEFAULT_PORT;
use serde::{Deserialize, Serialize};
use specta::Type;

/// Noms des événements Tauri (conception technique, section 9).
pub mod events {
    pub const SERVERS: &str = "link://servers";
    pub const STATE: &str = "link://state";
    pub const OPERATION: &str = "link://operation";
    pub const FINGERPRINT: &str = "link://fingerprint";
    pub const NOTICE: &str = "link://notice";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum RoleDto {
    Admin,
    Readonly,
}

impl From<RoleName> for RoleDto {
    fn from(role: RoleName) -> Self {
        match role {
            RoleName::Admin => Self::Admin,
            RoleName::Readonly => Self::Readonly,
        }
    }
}

/// Un serveur du carnet, sans secret.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ServerDto {
    pub id: String,
    pub name: String,
    /// Adresse telle qu'on l'affiche : le port n'apparaît que s'il n'est pas celui par défaut.
    pub address: String,
    pub host: String,
    pub port: u16,
    /// Numéro de la palette de 8 couleurs (1 à 8).
    pub color: u8,
    /// Rôle à la dernière connexion ; lecture seule tant qu'on n'est jamais connecté.
    pub role: RoleDto,
    pub username: String,
    pub remember: bool,
}

/// Numéro de couleur (1 à 8) mémorisé comme texte dans le carnet ; 1 si illisible.
pub fn color_number(text: &str) -> u8 {
    text.trim()
        .parse::<u8>()
        .ok()
        .filter(|n| (1..=8).contains(n))
        .unwrap_or(1)
}

fn display_address(host: &str, port: u16) -> String {
    let host = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_owned()
    };
    if port == DEFAULT_PORT {
        host
    } else {
        format!("{host}:{port}")
    }
}

impl From<&ServerRecord> for ServerDto {
    fn from(record: &ServerRecord) -> Self {
        Self {
            id: record.id.to_string(),
            name: record.name.clone(),
            address: display_address(&record.host, record.port),
            host: record.host.clone(),
            port: record.port,
            color: color_number(&record.color),
            role: record.role.map_or(RoleDto::Readonly, RoleDto::from),
            username: record.username.clone(),
            remember: record.remember,
        }
    }
}

/// Les serveurs dans un ordre stable (nom, sans tenir compte de la casse, puis identifiant).
pub fn servers_list(records: &[ServerRecord]) -> Vec<ServerDto> {
    let mut sorted: Vec<&ServerRecord> = records.iter().collect();
    sorted.sort_by_cached_key(|record| (record.name.to_lowercase(), record.id.to_string()));
    sorted.into_iter().map(ServerDto::from).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum LinkStateName {
    Connected,
    Reconnecting,
    Offline,
    SessionExpired,
    AccessRevoked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum BlockedDto {
    FingerprintChanged,
    IncompatibleAgent,
    IncompatibleClient,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ReasonDto {
    NoSession,
    Expired,
    StoredPasswordRefused,
    UserDisconnected,
    Revoked,
}

/// État du lien d'un serveur (`link://state`). `seq` croît strictement par serveur : l'interface
/// écarte tout événement dont `seq` n'est pas supérieur au dernier connu.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LinkStateDto {
    pub server_id: String,
    pub seq: u32,
    pub state: LinkStateName,
    pub since: f64,
    pub last_contact_at: Option<f64>,
    pub next_retry_at: Option<f64>,
    pub blocked: Option<BlockedDto>,
    pub reason: Option<ReasonDto>,
    pub failed_attempts: u32,
}

fn millis(time: hearth_link::domain::time::WallTime) -> f64 {
    // Des millisecondes depuis l'époque tiennent dans 53 bits : la conversion est exacte.
    time.as_millis() as f64
}

impl LinkStateDto {
    pub fn from_info(server: &ServerId, seq: u32, info: &StateInfo) -> Self {
        Self {
            server_id: server.to_string(),
            seq,
            state: match info.state {
                LinkState::Connected => LinkStateName::Connected,
                LinkState::Reconnecting => LinkStateName::Reconnecting,
                LinkState::Offline => LinkStateName::Offline,
                LinkState::SessionExpired => LinkStateName::SessionExpired,
                LinkState::AccessRevoked => LinkStateName::AccessRevoked,
            },
            since: millis(info.since),
            last_contact_at: info.last_contact_at.map(millis),
            next_retry_at: info.next_retry_at.map(millis),
            blocked: info.blocked.map(|blocked| match blocked {
                Blocked::FingerprintChanged => BlockedDto::FingerprintChanged,
                Blocked::IncompatibleVersion(UpgradeTarget::Agent) => BlockedDto::IncompatibleAgent,
                Blocked::IncompatibleVersion(UpgradeTarget::Client) => {
                    BlockedDto::IncompatibleClient
                }
            }),
            reason: info.reason.map(|reason| match reason {
                Reason::NoSession => ReasonDto::NoSession,
                Reason::Expired => ReasonDto::Expired,
                Reason::StoredPasswordRefused => ReasonDto::StoredPasswordRefused,
                Reason::UserDisconnected => ReasonDto::UserDisconnected,
                Reason::Revoked => ReasonDto::Revoked,
            }),
            failed_attempts: info.failed_attempts,
        }
    }

    /// Même contenu, sans tenir compte du numéro de séquence.
    fn same_content(&self, other: &Self) -> bool {
        Self {
            seq: other.seq,
            ..self.clone()
        } == *other
    }
}

/// Carnet des derniers états annoncés : donne à chaque état son numéro de séquence (par serveur,
/// strictement croissant) et rejoue l'état courant à l'abonnement de l'interface avec le même
/// numéro que celui déjà envoyé, pour qu'elle ne le prenne pas pour un nouveau changement.
#[derive(Debug, Default)]
pub struct StateBook {
    last: HashMap<String, LinkStateDto>,
}

impl StateBook {
    /// Nouvel état d'un serveur. `None` si rien n'a changé depuis le dernier annoncé.
    pub fn apply(&mut self, server: &ServerId, info: &StateInfo) -> Option<LinkStateDto> {
        let key = server.to_string();
        let previous = self.last.get(&key);
        let candidate = LinkStateDto::from_info(server, previous.map_or(1, |p| p.seq + 1), info);
        if previous.is_some_and(|p| p.same_content(&candidate)) {
            return None;
        }
        self.last.insert(key, candidate.clone());
        Some(candidate)
    }

    /// État courant de chaque serveur, d'après l'état vivant de la liaison : numéro inchangé si
    /// le contenu est celui déjà annoncé, numéro suivant sinon. Les serveurs disparus sont oubliés.
    pub fn snapshot(&mut self, live: &[(ServerId, StateInfo)]) -> Vec<LinkStateDto> {
        let mut states = Vec::with_capacity(live.len());
        let mut keep = HashMap::with_capacity(live.len());
        for (server, info) in live {
            let key = server.to_string();
            let state = match self.last.get(&key) {
                Some(previous) => {
                    let candidate = LinkStateDto::from_info(server, previous.seq, info);
                    if previous.same_content(&candidate) {
                        previous.clone()
                    } else {
                        LinkStateDto::from_info(server, previous.seq + 1, info)
                    }
                }
                None => LinkStateDto::from_info(server, 1, info),
            };
            keep.insert(key, state.clone());
            states.push(state);
        }
        self.last = keep;
        states
    }

    pub fn forget(&mut self, server: &ServerId) {
        self.last.remove(server.as_str());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeDto {
    Done,
    NotExecuted,
    Unknown,
}

impl From<&Outcome> for OutcomeDto {
    fn from(outcome: &Outcome) -> Self {
        match outcome {
            Outcome::DoneDuringOutage { .. } => Self::Done,
            Outcome::NotExecuted => Self::NotExecuted,
            Outcome::StillUnknown => Self::Unknown,
        }
    }
}

// ── Charges des événements ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ServersEvent {
    pub servers: Vec<ServerDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OperationEventDto {
    pub op_id: String,
    pub server_id: String,
    pub outcome: OutcomeDto,
}

/// Le certificat présenté n'est plus celui qui a été confirmé (BR-CONN-003). Les empreintes sont
/// en 8 groupes de 4 pour l'affichage ; `presented_hex` est la forme complète à renvoyer pour
/// l'accepter.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FingerprintEvent {
    pub server_id: String,
    pub expected: String,
    pub presented: String,
    pub presented_hex: String,
}

impl FingerprintEvent {
    pub fn new(server: &ServerId, expected: &Fingerprint, presented: &Fingerprint) -> Self {
        Self {
            server_id: server.to_string(),
            expected: expected.short(),
            presented: presented.short(),
            presented_hex: presented.to_hex(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum NoticeKind {
    /// Les suivis d'actions d'un serveur étaient illisibles : vérifie l'état avant de relancer.
    OperationsLost,
    /// L'écoute a pris du retard : des changements ont pu être manqués (états relus).
    Lagged,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NoticeEvent {
    /// Numéro de l'avis retenu par la coquille (acquittement, dédoublonnage) ; 0 : non retenu.
    pub id: u32,
    pub kind: NoticeKind,
    pub server_id: Option<String>,
}

// ── Retours de commandes ───────────────────────────────────────────────────────────────────

/// Première prise de contact : l'empreinte à faire confirmer.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProbeDto {
    /// Forme complète (64 caractères hexadécimaux), à renvoyer à `add_server`.
    pub fingerprint: String,
    /// 8 groupes de 4 caractères, pour l'affichage.
    pub display: String,
    pub machine_name: String,
    pub agent_version: String,
    pub mac_addresses: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LoginDto {
    pub role: RoleDto,
    pub username: String,
}

/// Ce que l'assistant envoie à sa dernière étape : le serveur confirmé et les identifiants. Pas de
/// `Debug` : il porte un mot de passe.
#[derive(Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AddServerInput {
    pub name: String,
    pub color: u8,
    pub host: String,
    pub port: Option<u16>,
    /// Empreinte confirmée par l'utilisateur, forme complète : celle de la dernière sonde.
    pub fingerprint: String,
    pub mac_addresses: Vec<String>,
    pub username: String,
    pub password: String,
    pub remember: bool,
}

/// Échec d'une commande de liaison, sans texte : l'interface choisit le message d'après `kind`.
#[derive(Debug, Clone, PartialEq, Serialize, Type, thiserror::Error)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LinkFailure {
    /// Adresse injoignable ou délai dépassé.
    #[error("serveur injoignable")]
    Unreachable,
    /// Quelque chose répond, mais ce n'est pas un agent Hearth (BR-CONN-012).
    #[error("ce n'est pas un agent Hearth")]
    NotAgent,
    /// L'agent est trop ancien (BR-CONN-014).
    #[error("agent trop ancien")]
    IncompatibleAgent,
    /// Le client est trop ancien (BR-CONN-014).
    #[error("client trop ancien")]
    IncompatibleClient,
    /// Identifiant ou mot de passe refusé, sans dire lequel (BR-CONN-013).
    #[error("identifiants refusés")]
    InvalidCredentials,
    #[error("trop de tentatives")]
    TooManyAttempts { retry_after_s: u32 },
    #[error("l'empreinte a changé")]
    FingerprintChanged,
    #[error("nom déjà utilisé")]
    NameTaken,
    #[error("serveur déjà enregistré")]
    AlreadyExists,
    /// Champ invalide : `name`, `address`, `port` ou `credentials`.
    #[error("saisie invalide")]
    InvalidInput { field: InvalidField },
    #[error("nouvelle vérification de l'empreinte requise")]
    VerificationRequired,
    #[error("serveur inconnu")]
    UnknownServer,
    #[error("accès au disque impossible")]
    Storage,
    #[error("coffre de Windows inaccessible")]
    Vault,
    /// Le lien n'est pas « Connecté » : rien n'a été envoyé (BR-RESIL-008).
    #[error("lien non établi")]
    NotConnected,
    /// Le suivi de l'action n'a pas pu être écrit sur le disque : l'action n'a PAS été lancée.
    #[error("suivi de l'action impossible")]
    TrackingUnavailable,
    /// Le disque est trop lent pour écrire le suivi à temps : l'action n'a PAS été lancée.
    #[error("disque trop lent")]
    TrackingSlow,
    #[error("incident interne")]
    Internal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum InvalidField {
    Name,
    Address,
    Port,
    Credentials,
    Fingerprint,
    Other,
}

impl From<LinkError> for LinkFailure {
    fn from(error: LinkError) -> Self {
        match error {
            LinkError::UnknownServer => Self::UnknownServer,
            LinkError::AlreadyExists => Self::AlreadyExists,
            LinkError::NameTaken => Self::NameTaken,
            LinkError::VerificationRequired => Self::VerificationRequired,
            LinkError::InvalidInput(field) => Self::InvalidInput {
                field: match field {
                    InputField::Name => InvalidField::Name,
                    InputField::Address => InvalidField::Address,
                    InputField::Port => InvalidField::Port,
                    InputField::Credentials => InvalidField::Credentials,
                    InputField::Fingerprint => InvalidField::Fingerprint,
                },
            },
            LinkError::FingerprintChanged => Self::FingerprintChanged,
            LinkError::Incompatible(Compatibility::UpdateAgent) => Self::IncompatibleAgent,
            LinkError::Incompatible(_) => Self::IncompatibleClient,
            LinkError::InvalidCredentials => Self::InvalidCredentials,
            LinkError::TooManyAttempts { retry_after_s } => Self::TooManyAttempts {
                retry_after_s: u32::try_from(retry_after_s).unwrap_or(u32::MAX),
            },
            LinkError::Unreachable(_) | LinkError::Timeout => Self::Unreachable,
            // Réponse qui n'est pas celle d'un agent Hearth (produit inconnu, corps illisible).
            LinkError::Protocol(_) => Self::NotAgent,
            LinkError::Store(_) => Self::Storage,
            LinkError::Vault(_) => Self::Vault,
            LinkError::NotConnected => Self::NotConnected,
            LinkError::TrackingUnavailable => Self::TrackingUnavailable,
            LinkError::TrackingSlow => Self::TrackingSlow,
            LinkError::Rejected(_)
            | LinkError::TooManyPending
            | LinkError::TaskRestarted
            | LinkError::Stopped => Self::Internal,
        }
    }
}

impl From<BookError> for LinkFailure {
    fn from(error: BookError) -> Self {
        LinkError::from(error).into()
    }
}

/// `Fingerprint` depuis le texte hexadécimal complet envoyé par l'interface.
pub fn parse_fingerprint(text: &str) -> Result<Fingerprint, LinkFailure> {
    Fingerprint::from_hex(text.trim()).map_err(|_| LinkFailure::InvalidInput {
        field: InvalidField::Fingerprint,
    })
}

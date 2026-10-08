//! Ce que la coquille dit à l'interface sur la sécurité d'un serveur. Des états, des dates, des
//! compteurs et des booléens : jamais une clé (privée ou publique), une empreinte de clé, un défi, une
//! signature, un jeton, ni le NOM d'un autre compte visé (l'agent n'en donne que le nombre). Aucun
//! mot de passe n'apparaît ici : il ne traverse que le paramètre de la commande.

use hearth_proto::api::security::{
    AlertInfo, AttackModeEnd, AttackModeInfo, AttackModeState, SecurityResponse, SecurityView,
    SessionDevice,
};
use serde::Serialize;
use specta::Type;

/// Événement de l'état de sécurité d'un serveur (nom côté fenêtre).
pub const EVENT: &str = "link://security";

/// L'alerte « attaque probable » (BR-TRUST-008).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AlertDto {
    /// L'identifiant du compte connecté est visé en ce moment.
    pub own: bool,
    /// Début de l'épisode (RFC 3339, UTC), seulement quand `own`.
    pub since: Option<String>,
    /// Administrateur seulement : combien d'AUTRES comptes sont visés.
    pub others: Option<u32>,
}

impl From<&AlertInfo> for AlertDto {
    fn from(alert: &AlertInfo) -> Self {
        Self {
            own: alert.own,
            since: alert.since.clone(),
            others: alert.others,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AttackModeStateDto {
    Off,
    Active,
    Suspended,
}

impl From<AttackModeState> for AttackModeStateDto {
    fn from(state: AttackModeState) -> Self {
        match state {
            AttackModeState::Off => Self::Off,
            AttackModeState::Active => Self::Active,
            AttackModeState::Suspended => Self::Suspended,
        }
    }
}

/// Comment le dernier mode attaque s'est terminé.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AttackModeEndDto {
    Manual,
    Auto,
    Cli,
}

impl From<AttackModeEnd> for AttackModeEndDto {
    fn from(end: AttackModeEnd) -> Self {
        match end {
            AttackModeEnd::Manual => Self::Manual,
            AttackModeEnd::Auto => Self::Auto,
            AttackModeEnd::Cli => Self::Cli,
        }
    }
}

/// Le mode attaque du serveur.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AttackModeDto {
    pub state: AttackModeStateDto,
    pub since: Option<String>,
    /// Seulement « suspendu » : secondes avant la reprise.
    pub resumes_in_s: Option<u32>,
    pub last_end: Option<AttackModeEndDto>,
}

impl From<&AttackModeInfo> for AttackModeDto {
    fn from(info: &AttackModeInfo) -> Self {
        Self {
            state: info.state.into(),
            since: info.since.clone(),
            resumes_in_s: info
                .resumes_in_s
                .map(|seconds| u32::try_from(seconds).unwrap_or(u32::MAX)),
            last_end: info.last_end.map(Into::into),
        }
    }
}

/// Ce que l'agent dit de la session de ce poste : prouvée par la clé d'un poste inscrit, ou non.
/// `Unknown` : pas encore lu (le message du flux ne le porte pas, seule la lecture le dit).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SecurityDeviceDto {
    Proven,
    None,
    Unknown,
}

impl From<SessionDevice> for SecurityDeviceDto {
    fn from(device: SessionDevice) -> Self {
        match device {
            SessionDevice::Proven => Self::Proven,
            SessionDevice::None => Self::None,
        }
    }
}

/// L'état de sécurité d'un serveur : charge de `link://security` et de `get_security`. `seq` croît
/// strictement par serveur : l'interface écarte tout état dont `seq` n'est pas supérieur au dernier
/// connu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SecurityEvent {
    pub server_id: String,
    pub seq: u32,
    pub alert: AlertDto,
    pub attack_mode: AttackModeDto,
    pub device: SecurityDeviceDto,
    /// Ce PC garde une clé d'appareil pour ce serveur (un booléen : la clé elle-même ne sort pas).
    pub key_at_hand: bool,
    /// L'agent dit que l'effacement physique des anciennes empreintes de requêtes est en attente (HRT-32,
    /// ADR-0034) : il ne le dit qu'aux administrateurs, et la dernière lecture fait foi (le flux ne le porte pas).
    pub erasure_pending: bool,
}

/// Lecture de l'état de sécurité : l'état, ou « cette fonction n'existe pas sur ce serveur » (agent
/// d'avant l'alerte et le mode attaque : l'agent répond `404`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SecurityRead {
    Known { snapshot: SecurityEvent },
    Unsupported,
}

/// Pourquoi une activation ou une désactivation est refusée, par l'agent. Sans texte : l'interface
/// choisit le message d'après `kind`. Le rôle insuffisant (`LinkFailure::Forbidden`) et le poste non
/// reconnu (`LinkFailure::NotRecognized`) sont des échecs typés, pas des refus.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AttackModeRefusal {
    /// Le mot de passe actuel est faux.
    WrongPassword,
    /// Trop d'essais de mot de passe : réessayer plus tard.
    TooManyAttempts {
        retry_after_s: u32,
    },
    /// L'agent est saturé : réessayer dans un instant.
    Busy,
    /// L'élévation de 5 minutes s'est fermée côté agent et le mot de passe n'était pas dans la requête
    /// (jamais le cas du mode attaque, qui n'est pas couvert : la fenêtre le redemande quand même).
    PasswordRequired,
    /// L'agent ne connaît pas cette fonction (agent d'avant le mode attaque).
    Unsupported,
    SessionEnded,
    SessionRevoked,
    Other,
}

/// Issue d'une activation ou d'une désactivation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AttackModeOutcome {
    /// L'agent a changé le mode (ou il l'était déjà) : l'état après le changement.
    Done {
        attack_mode: AttackModeDto,
    },
    Refused {
        refusal: AttackModeRefusal,
    },
    /// Le lien est tombé avant la réponse : on ne sait pas, l'action n'est JAMAIS rejouée. L'issue
    /// arrive par `link://operation` sous cet identifiant ; l'état se relit au retour du lien.
    Unknown {
        op_id: String,
    },
}

/// Ce que `GET /security` et le message du flux ont en commun.
pub fn view_parts(view: &SecurityView) -> (AlertDto, AttackModeDto) {
    (
        AlertDto::from(&view.alert),
        AttackModeDto::from(&view.attack_mode),
    )
}

/// Les parties d'une lecture `GET /security`.
pub fn response_parts(response: &SecurityResponse) -> (AlertDto, AttackModeDto, SecurityDeviceDto) {
    (
        AlertDto::from(&response.alert),
        AttackModeDto::from(&response.attack_mode),
        response.device.into(),
    )
}

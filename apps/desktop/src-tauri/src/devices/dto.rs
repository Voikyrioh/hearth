//! Ce que la coquille dit à l'interface sur les postes de confiance. Des noms, des dates, une
//! adresse et des booléens : jamais une clé (privée ou publique), une empreinte de clé, un défi, une
//! signature ni un jeton. Aucun mot de passe n'apparaît ici : il ne traverse que le paramètre de la
//! commande, jamais un type qui dérive `Debug`.

use hearth_proto::api::devices::DeviceItem;
use serde::Serialize;
use specta::Type;

/// Un poste de confiance du compte de la session. Les dates sont celles de l'agent (RFC 3339, UTC) ;
/// l'interface les met en forme.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TrustedDeviceDto {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub last_proved_at: String,
    pub last_addr: String,
    /// C'est ce PC : il ne se retire pas depuis lui-même.
    pub current: bool,
}

impl From<DeviceItem> for TrustedDeviceDto {
    fn from(item: DeviceItem) -> Self {
        Self {
            id: item.id,
            name: item.name,
            created_at: item.created_at,
            last_proved_at: item.last_proved_at,
            last_addr: item.last_addr,
            current: item.current,
        }
    }
}

/// La liste des postes, ou « cette fonction n'existe pas sur ce serveur » (agent d'avant la clé
/// d'appareil : l'agent répond `404`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TrustedDevicesDto {
    Listed {
        devices: Vec<TrustedDeviceDto>,
        max: u32,
    },
    Unsupported,
}

/// Pourquoi un retrait est refusé. Sans texte : l'interface choisit le message d'après `kind`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DeviceRemovalRefusal {
    /// Le mot de passe actuel est faux.
    WrongPassword,
    /// C'est le poste d'où part la demande : il ne se retire pas depuis lui-même.
    CurrentDevice,
    /// Ce PC n'a pas de clé inscrite (client mis à jour sans reconnexion par mot de passe, coffre
    /// sans clé) : il ne peut rien retirer.
    NoDeviceKey,
    /// L'agent n'a pas accepté la preuve de la clé (défi périmé ou rejoué, autre poste).
    ProofRefused,
    /// Le poste n'existe plus (déjà retiré ailleurs) : la liste se relit.
    NotFound,
    /// Trop d'essais de mot de passe : réessayer plus tard.
    TooManyAttempts {
        retry_after_s: u32,
    },
    /// L'agent est saturé : réessayer dans un instant.
    Busy,
    /// L'agent ne connaît pas cette fonction (agent d'avant la clé d'appareil).
    Unsupported,
    /// La session a expiré pendant l'action.
    SessionEnded,
    /// La session a été fermée.
    SessionRevoked,
    Other,
}

/// Issue d'un retrait de poste.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DeviceRemovalOutcome {
    /// L'agent a retiré le poste (et fermé ses sessions).
    Done,
    Refused {
        refusal: DeviceRemovalRefusal,
    },
    /// Le lien est tombé avant la réponse : on ne sait pas, l'action n'est JAMAIS rejouée. L'issue
    /// arrive par `link://operation` sous cet identifiant ; la liste se relit au retour du lien.
    Unknown {
        op_id: String,
    },
}

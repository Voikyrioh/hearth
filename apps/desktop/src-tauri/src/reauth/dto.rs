//! Ce que la coquille dit à l'interface sur la confirmation des actes. Aucun mot de passe, clé, défi ni
//! signature ne figure ici, en entrée comme en sortie.

use hearth_link::ReauthState;
use hearth_proto::admin_act::{PasswordNeed, TrustLevel};
use hearth_proto::api::reauth::{AdminReauthInfo, ReauthMode};
use serde::{Deserialize, Serialize};
use specta::Type;

/// Le réglage « Demander mon mot de passe » du compte (Q19).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ReauthModeDto {
    /// Une saisie vaut 5 minutes (défaut).
    Window,
    /// À chaque action.
    Each,
    // Une valeur que cette version ne connaît pas (agent plus récent) : transmise telle quelle à l'interface,
    // jamais prise pour une valeur connue (FIX:01M4VR6Y4Q4AVQVT9FCME7Z2SB).
    Unknown,
}

impl From<ReauthMode> for ReauthModeDto {
    fn from(mode: ReauthMode) -> Self {
        match mode {
            ReauthMode::Window => Self::Window,
            ReauthMode::Each => Self::Each,
            ReauthMode::Unknown => Self::Unknown,
        }
    }
}

impl From<ReauthModeDto> for ReauthMode {
    fn from(mode: ReauthModeDto) -> Self {
        match mode {
            ReauthModeDto::Window => Self::Window,
            ReauthModeDto::Each => Self::Each,
            // Jamais offert par l'interface ; l'agent refuse un réglage inconnu (`422`).
            ReauthModeDto::Unknown => Self::Unknown,
        }
    }
}

/// Le genre d'un acte d'administration, tel que la fenêtre le connaît (sans cible ni secret).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AdminActKindDto {
    AccountCreate,
    AccountRole,
    AccountPassword,
    AccountDelete,
    SessionsRevoke,
    AgentUpdate,
    AttackModeEnable,
    AttackModeDisable,
    AccountPasswordOwn,
    ReauthSetting,
    /// Décider d'une opération de service (HRT-84) : un acte de pilotage depuis HRT-194.
    OperationDecide,
    /// Démarrer, arrêter, redémarrer un service (HRT-65) : actes de pilotage (HRT-194, BR-LIFE-016).
    ServiceStart,
    ServiceStop,
    ServiceRestart,
}

/// L'état de confiance du compte que l'agent annonce (HRT-194, BR-TRUST-052) : de lui dépend si piloter un
/// service demande le mot de passe. Indicatif, l'agent le recalcule à l'acte ; le client ne l'envoie jamais.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TrustLevelDto {
    Normal,
    Alert,
    Attack,
    // Une valeur que cette version ne connaît pas (agent plus récent) : transmise telle quelle, l'interface
    // la traite comme le mode attaque (le plus strict).
    Unknown,
}

impl From<TrustLevel> for TrustLevelDto {
    fn from(trust: TrustLevel) -> Self {
        match trust {
            TrustLevel::Normal => Self::Normal,
            TrustLevel::Alert => Self::Alert,
            TrustLevel::Attack => Self::Attack,
            TrustLevel::Unknown => Self::Unknown,
        }
    }
}

impl From<TrustLevelDto> for TrustLevel {
    fn from(trust: TrustLevelDto) -> Self {
        match trust {
            TrustLevelDto::Normal => Self::Normal,
            TrustLevelDto::Alert => Self::Alert,
            TrustLevelDto::Attack => Self::Attack,
            TrustLevelDto::Unknown => Self::Unknown,
        }
    }
}

/// Ce que l'agent exige du mot de passe pour un acte (HRT-194, BR-GRANT-017) : aucun, sauf élévation en
/// cours, ou à chaque fois. La règle est celle de `hearth_proto::admin_act::password_need`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum PasswordNeedDto {
    No,
    UnlessElevated,
    Yes,
}

impl From<PasswordNeed> for PasswordNeedDto {
    fn from(need: PasswordNeed) -> Self {
        match need {
            PasswordNeed::No => Self::No,
            PasswordNeed::UnlessElevated => Self::UnlessElevated,
            PasswordNeed::Yes => Self::Yes,
        }
    }
}

/// L'état de la confirmation des actes pour un serveur : lu de l'agent à l'ouverture d'une fenêtre
/// (l'interface ne devine pas l'élévation).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReauthStateDto {
    /// L'agent annonce la confirmation des actes. Faux : aucun acte ne lui part (agent à mettre à jour).
    pub supported: bool,
    /// L'agent l'exige (faux tant qu'il ne fait que l'accepter).
    pub required: bool,
    /// Le réglage du compte.
    pub mode: ReauthModeDto,
    /// Secondes restantes de l'élévation de cette session depuis cette adresse ; 0 sinon.
    pub elevated_for_s: u32,
    /// Ce PC a une clé d'appareil au coffre pour ce serveur. Faux : aucun acte ne part d'ici.
    pub has_device_key: bool,
    /// L'état de confiance annoncé par l'agent. Absent : agent d'avant HRT-194, la règle d'avant s'applique
    /// (BR-COMPAT-001), jamais « normal » par défaut.
    pub trust: Option<TrustLevelDto>,
}

impl From<ReauthState> for ReauthStateDto {
    fn from(state: ReauthState) -> Self {
        let has_device_key = state.has_device_key;
        match state.agent {
            Some(AdminReauthInfo {
                required,
                password,
                elevated_for_s,
                trust,
                ..
            }) => Self {
                supported: true,
                required,
                mode: password.into(),
                elevated_for_s: u32::try_from(elevated_for_s).unwrap_or(u32::MAX),
                has_device_key,
                trust: trust.map(TrustLevelDto::from),
            },
            None => Self {
                supported: false,
                required: false,
                mode: ReauthModeDto::Window,
                elevated_for_s: 0,
                has_device_key,
                trust: None,
            },
        }
    }
}

/// Pourquoi l'agent a refusé le réglage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReauthSettingRefusal {
    WrongPassword,
    /// L'élévation s'est fermée entre-temps et le mot de passe n'était pas dans la requête.
    PasswordRequired,
    TooManyAttempts {
        retry_after_s: u32,
    },
    Busy,
    /// L'agent ne connaît pas le réglage (agent d'avant).
    Unsupported,
    SessionEnded,
    SessionRevoked,
    Other,
}

/// Issue du changement de réglage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReauthSettingOutcome {
    Done {
        mode: ReauthModeDto,
    },
    Refused {
        refusal: ReauthSettingRefusal,
    },
    /// Le lien est tombé avant la réponse : on ne sait pas, l'action n'est JAMAIS rejouée.
    Unknown {
        op_id: String,
    },
}

//! Ce que la coquille dit à l'interface sur la confirmation des actes. Aucun mot de passe, clé, défi ni
//! signature ne figure ici, en entrée comme en sortie.

use hearth_link::ReauthState;
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
}

impl From<ReauthMode> for ReauthModeDto {
    fn from(mode: ReauthMode) -> Self {
        match mode {
            ReauthMode::Window => Self::Window,
            ReauthMode::Each => Self::Each,
        }
    }
}

impl From<ReauthModeDto> for ReauthMode {
    fn from(mode: ReauthModeDto) -> Self {
        match mode {
            ReauthModeDto::Window => Self::Window,
            ReauthModeDto::Each => Self::Each,
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
}

impl From<ReauthState> for ReauthStateDto {
    fn from(state: ReauthState) -> Self {
        let has_device_key = state.has_device_key;
        match state.agent {
            Some(AdminReauthInfo {
                required,
                password,
                elevated_for_s,
                ..
            }) => Self {
                supported: true,
                required,
                mode: password.into(),
                elevated_for_s: u32::try_from(elevated_for_s).unwrap_or(u32::MAX),
                has_device_key,
            },
            None => Self {
                supported: false,
                required: false,
                mode: ReauthModeDto::Window,
                elevated_for_s: 0,
                has_device_key,
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

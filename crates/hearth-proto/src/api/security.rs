//! État de sécurité du compte de l'appelant : `GET /security` et message `security` du flux
//! (HRT-24, BR-TRUST-008).
//!
//! Aucun nom d'identifiant, aucune adresse, aucune clé : l'alerte dit seulement si **son** identifiant
//! est visé et, pour un administrateur, **combien** d'autres comptes le sont. Un compte qui n'est pas
//! administrateur n'apprend jamais qu'un autre identifiant est visé : `others` n'existe pas pour lui.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::reauth::AdminReauthInfo;
use super::sessions::{DeviceProof, lenient_proof};

/// L'alerte « attaque probable » sur l'identifiant de l'appelant (BR-TRUST-008).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlertInfo {
    /// L'identifiant de l'appelant est visé en ce moment.
    pub own: bool,
    /// Début de l'épisode, quand `own` est vrai.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
    /// Administrateur seulement : combien d'AUTRES comptes existants sont visés en ce moment. Jamais
    /// leurs noms (ils se lisent au journal d'activité).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub others: Option<u32>,
}

/// État du mode attaque du serveur (HRT-25).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttackModeState {
    Off,
    Active,
    Suspended,
}

/// Comment le dernier mode attaque s'est terminé.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttackModeEnd {
    Manual,
    Auto,
    Cli,
}

/// Le mode attaque du serveur, tel que le client le voit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttackModeInfo {
    pub state: AttackModeState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resumes_in_s: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_end: Option<AttackModeEnd>,
}

impl AttackModeInfo {
    /// Le mode attaque est éteint.
    pub fn off() -> Self {
        Self {
            state: AttackModeState::Off,
            since: None,
            resumes_in_s: None,
            last_end: None,
        }
    }
}

/// Corps de `PUT /security/attack-mode` (HRT-25). Activer comme désactiver est **un acte
/// d'administration** (Q14 point 3, Q16) : le corps porte le **mot de passe actuel** de
/// l'administrateur et la **preuve de possession d'une clé inscrite pour son compte** (défi
/// `purpose: "attack_mode"`, signature d'usage `0x03` liée au jeton et à la valeur demandée).
///
/// Le mot de passe et la preuve ont une valeur par défaut : un champ absent ou illisible n'est pas une
/// erreur de lecture mais un refus typé pour un utilisateur déjà authentifié.
#[derive(Clone, Serialize, Deserialize)]
pub struct SetAttackModeRequest {
    /// `true` : activer ; `false` : désactiver.
    pub active: bool,
    #[serde(default)]
    pub password: String,
    #[serde(default, deserialize_with = "lenient_proof")]
    pub device: Option<DeviceProof>,
}

impl fmt::Debug for SetAttackModeRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SetAttackModeRequest")
            .field("active", &self.active)
            .field("password", &"***")
            .field("device", &self.device)
            .finish()
    }
}

/// `details.reason` de `409 POST_NOT_RECOGNIZED` (`details.field` = `device`).
pub mod attack_mode_refusal {
    /// Aucune preuve de clé n'accompagne la requête (client sans clé, poste non inscrit).
    pub const PROOF_MISSING: &str = "proof_missing";
    /// La preuve manque de validité : illisible, périmée, rejouée, d'un autre usage ou de l'autre
    /// geste, signée par une clé qui n'est pas inscrite pour le compte de l'appelant.
    pub const PROOF_INVALID: &str = "proof_invalid";
}

/// Ce que le client voit de la sécurité du serveur : le contenu du message `security` du flux.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityView {
    pub alert: AlertInfo,
    pub attack_mode: AttackModeInfo,
}

/// La session de l'appelant a-t-elle été ouverte ou prouvée par la clé d'un poste inscrit ?
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionDevice {
    Proven,
    None,
}

/// Réponse de `GET /security`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityResponse {
    pub alert: AlertInfo,
    pub attack_mode: AttackModeInfo,
    pub device: SessionDevice,
    /// La confirmation des actes d'administration (HRT-28). Absent : agent d'avant ce ticket.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub admin_reauth: Option<AdminReauthInfo>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_quiet_account_sees_no_date_and_no_count() {
        let view = SecurityView {
            alert: AlertInfo {
                own: false,
                since: None,
                others: None,
            },
            attack_mode: AttackModeInfo::off(),
        };
        assert_eq!(
            serde_json::to_value(&view).expect("json"),
            json!({ "alert": { "own": false }, "attack_mode": { "state": "off" } })
        );
    }

    #[test]
    fn an_administrator_sees_the_count_of_the_others_and_never_a_name() {
        let response = SecurityResponse {
            alert: AlertInfo {
                own: true,
                since: Some("2026-10-07T01:00:00Z".into()),
                others: Some(2),
            },
            attack_mode: AttackModeInfo::off(),
            device: SessionDevice::Proven,
            admin_reauth: None,
        };
        let value = serde_json::to_value(&response).expect("json");
        assert_eq!(value["alert"]["others"], 2);
        assert_eq!(value["alert"]["since"], "2026-10-07T01:00:00Z");
        assert_eq!(value["device"], "proven");
        assert_eq!(
            serde_json::from_value::<SecurityResponse>(value).expect("round trip"),
            response
        );
    }

    #[test]
    fn the_states_of_the_attack_mode_have_their_names_on_the_wire() {
        for (state, name) in [
            (AttackModeState::Off, "off"),
            (AttackModeState::Active, "active"),
            (AttackModeState::Suspended, "suspended"),
        ] {
            assert_eq!(serde_json::to_value(state).expect("json"), json!(name));
        }
        assert_eq!(
            serde_json::to_value(AttackModeEnd::Cli).expect("json"),
            json!("cli")
        );
    }

    #[test]
    fn the_attack_mode_request_reads_with_missing_fields_and_hides_the_password() {
        let bare: SetAttackModeRequest = serde_json::from_str(r#"{"active":true}"#).expect("json");
        assert!(bare.active);
        assert_eq!(bare.password, "");
        assert_eq!(bare.device, None);
        let lenient: SetAttackModeRequest =
            serde_json::from_str(r#"{"active":false,"password":"x","device":42}"#).expect("json");
        assert!(!lenient.active);
        assert_eq!(
            lenient.device, None,
            "une preuve illisible vaut une preuve absente"
        );
        let shown = format!(
            "{:?}",
            SetAttackModeRequest {
                active: true,
                password: "Correct-Horse-9".into(),
                device: None,
            }
        );
        assert!(!shown.contains("Correct-Horse-9"));
        assert!(
            serde_json::from_str::<SetAttackModeRequest>("{}").is_err(),
            "active est obligatoire"
        );
    }
}

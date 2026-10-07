//! Postes de confiance de l'utilisateur : `GET /me/devices`, `DELETE /me/devices/{id}`
//! (HRT-22, BR-TRUST-004, 022).
//!
//! Aucune clé, aucune empreinte de clé, aucun défi : la liste ne montre que des noms, des dates et
//! une adresse.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::sessions::{DeviceProof, lenient_proof};

/// Postes reconnus au plus par compte (BR-TRUST-022) : le 9e n'est pas inscrit, sans éviction.
pub const MAX_DEVICES_PER_ACCOUNT: usize = 8;

/// Un poste de confiance du compte de l'appelant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceItem {
    /// Identifiant du poste (ULID), celui de `DELETE /me/devices/{id}`.
    pub id: String,
    /// Nom annoncé par le client à l'inscription (`X-Hearth-Client`, nettoyé).
    pub name: String,
    pub created_at: String,
    /// Dernière preuve de la clé (connexion ou ouverture du flux).
    pub last_proved_at: String,
    /// Adresse d'où le poste a prouvé sa clé pour la dernière fois.
    pub last_addr: String,
    /// C'est le poste de la session qui fait la requête (il ne se retire pas depuis lui-même).
    pub current: bool,
}

/// Réponse de `GET /me/devices`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevicesResponse {
    pub devices: Vec<DeviceItem>,
    pub max: usize,
}

/// Corps de `DELETE /me/devices/{id}` : retirer un poste est un acte d'administration (Q16 : mot de
/// passe et clé privée). Une session seule ne suffit pas : la requête porte le **mot de passe actuel** du
/// compte et la **preuve de possession de la clé du poste courant** (défi `purpose: "device_removal"`,
/// signature liée au jeton et à l'identifiant du poste visé).
///
/// Les deux champs ont une valeur par défaut : un champ absent ou illisible n'est pas une erreur de
/// lecture mais un refus typé (`422 VALIDATION_ERROR`, `details.reason`), pour un utilisateur déjà
/// authentifié.
#[derive(Clone, Serialize, Deserialize)]
pub struct RemoveDeviceRequest {
    #[serde(default)]
    pub password: String,
    #[serde(default, deserialize_with = "lenient_proof")]
    pub device: Option<DeviceProof>,
}

impl fmt::Debug for RemoveDeviceRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RemoveDeviceRequest")
            .field("password", &"***")
            .field("device", &self.device)
            .finish()
    }
}

/// `details.reason` du refus d'un retrait (`422 VALIDATION_ERROR`, `details.field` = `device`).
pub mod removal_refusal {
    /// La session courante n'a pas de poste inscrit (client ancien, ou jamais inscrit) : ce poste ne
    /// peut rien retirer ; retirer depuis un poste qui a une clé inscrite.
    pub const DEVICE_REQUIRED: &str = "device_required";
    /// La preuve de clé manque, est illisible, périmée, rejouée, d'un autre compte, d'un autre poste visé
    /// ou n'est pas celle du poste courant.
    pub const PROOF_INVALID: &str = "proof_invalid";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_removal_body_reads_with_missing_or_malformed_fields_and_hides_the_password() {
        let empty: RemoveDeviceRequest = serde_json::from_str("{}").unwrap();
        assert!(empty.password.is_empty() && empty.device.is_none());
        let bad: RemoveDeviceRequest =
            serde_json::from_str(r#"{"password":"Secret-Pass-123","device":5}"#).unwrap();
        assert!(bad.device.is_none());
        assert!(!format!("{bad:?}").contains("Secret-Pass"));
    }
}

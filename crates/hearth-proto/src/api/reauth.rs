//! La confirmation d'un acte d'administration (HRT-28, BR-TRUST-036, 039, 040, 042) : le membre `reauth`
//! du corps de chaque acte, la capacité annoncée par `GET /security` et le réglage de fréquence du mot de
//! passe (`PUT /me/reauth`).
//!
//! Le mot de passe ne voyage jamais dans un en-tête (comme à la connexion) ni dans le message signé :
//! il est dans le corps, sous `reauth.password`. `Debug` est écrit à la main : mot de passe masqué, ni
//! défi ni signature.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};

use super::sessions::{DeviceProof, lenient_proof};

/// Le membre `reauth` du corps d'un acte. Lecture tolérante : un membre absent ou illisible n'est jamais
/// une erreur de lecture mais un refus typé.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reauth {
    /// Le mot de passe actuel de l'appelant. Absent (ou vide) seulement sous élévation, pour un acte
    /// couvert (BR-TRUST-043).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub password: String,
    /// La preuve de possession d'une clé inscrite du compte, d'usage `0x05` (`device_proof`).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient_proof"
    )]
    pub device: Option<DeviceProof>,
}

impl fmt::Debug for Reauth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Reauth")
            .field("password", &"***")
            .field("device", &self.device)
            .finish()
    }
}

/// Lit un membre `reauth` sans jamais échouer : une valeur de la mauvaise forme vaut « confirmation
/// présente, sans rien d'utilisable » (refus typé), pas « absente » (qui serait un client ancien).
pub fn lenient_reauth<'de, D>(deserializer: D) -> Result<Option<Reauth>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(value
        .filter(|value| !value.is_null())
        .map(|value| serde_json::from_value(value).unwrap_or_default()))
}

/// Le réglage de fréquence du mot de passe, par compte (Q19).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReauthMode {
    /// Une saisie du mot de passe ouvre une élévation de 5 minutes (défaut).
    Window,
    /// Le mot de passe est demandé à chaque acte.
    Each,
}

impl ReauthMode {
    /// Le texte du fil et du message signé.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Window => "window",
            Self::Each => "each",
        }
    }
}

/// Ce que `GET /security` annonce de la confirmation des actes (`admin_reauth`). Absent : agent d'avant
/// ce ticket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminReauthInfo {
    /// L'agent exige la confirmation (faux tant qu'il ne fait que l'accepter).
    pub required: bool,
    /// Ce qu'il faut réunir : `["password", "device_key"]` aujourd'hui.
    pub factors: Vec<String>,
    /// Le réglage du compte.
    pub password: ReauthMode,
    /// Secondes restantes de l'élévation de cette session depuis cette adresse, 0 sinon. Indicatif :
    /// l'agent décide à l'acte.
    pub elevated_for_s: u64,
}

/// Corps de `PUT /me/reauth`. Le champ `password` porte le **réglage** (`each` ou `window`) ; le mot de
/// passe de confirmation est dans `reauth`, comme pour tout acte. Toujours confirmé (mot de passe et clé).
#[derive(Clone, Serialize, Deserialize)]
pub struct SetReauthRequest {
    pub password: ReauthMode,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient_reauth"
    )]
    pub reauth: Option<Reauth>,
}

impl fmt::Debug for SetReauthRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SetReauthRequest")
            .field("mode", &self.password)
            .field("reauth", &self.reauth)
            .finish()
    }
}

/// `details.reason` des refus de confirmation. `409 POST_NOT_RECOGNIZED` pour les trois premières
/// (`details.field` = `reauth.device` ou `reauth.password`), `426 INCOMPATIBLE_VERSION` pour la dernière.
pub mod reauth_refusal {
    /// `reauth` est présent sans preuve de clé lisible.
    pub const PROOF_MISSING: &str = "proof_missing";
    /// La preuve vaut pour un autre acte, une autre cible, un autre compte, un autre jeton, est
    /// périmée, rejouée, ou signée par une clé non inscrite pour le compte.
    pub const PROOF_INVALID: &str = "proof_invalid";
    /// L'élévation est absente ou fermée et le mot de passe n'est pas dans la requête.
    pub const PASSWORD_REQUIRED: &str = "password_required";
    /// L'acte arrive sans `reauth` à un agent qui l'exige : le client est trop ancien.
    pub const REAUTH_REQUIRED: &str = "reauth_required";
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn debug_never_writes_the_password_the_challenge_or_the_signature() {
        let reauth = Reauth {
            password: "Secret-Horse-1".into(),
            device: Some(DeviceProof {
                algorithm: "ed25519".into(),
                public_key: "cle".into(),
                challenge: "defi-secret".into(),
                signature: "signature-secrete".into(),
            }),
        };
        let shown = format!(
            "{reauth:?}{:?}",
            SetReauthRequest {
                password: ReauthMode::Each,
                reauth: Some(reauth.clone()),
            }
        );
        for secret in ["Secret-Horse-1", "defi-secret", "signature-secrete"] {
            assert!(!shown.contains(secret), "{shown}");
        }
    }

    #[test]
    fn an_unreadable_reauth_is_present_and_empty_never_absent_never_an_error() {
        #[derive(Deserialize)]
        struct Body {
            #[serde(default, deserialize_with = "lenient_reauth")]
            reauth: Option<Reauth>,
        }
        let read = |value: serde_json::Value| serde_json::from_value::<Body>(value).unwrap().reauth;
        assert_eq!(read(json!({})), None);
        assert_eq!(read(json!({ "reauth": null })), None);
        assert_eq!(read(json!({ "reauth": 42 })), Some(Reauth::default()));
        assert_eq!(read(json!({ "reauth": "x" })), Some(Reauth::default()));
        let partial = read(json!({ "reauth": { "password": "p", "device": 7 } })).unwrap();
        assert_eq!(partial.password, "p");
        assert_eq!(
            partial.device, None,
            "une preuve illisible vaut une preuve absente"
        );
        let unknown = read(json!({ "reauth": { "password": "p", "future": true } })).unwrap();
        assert_eq!(unknown.password, "p", "membres inconnus ignorés");
    }

    #[test]
    fn the_modes_and_the_capability_have_their_names_on_the_wire() {
        assert_eq!(
            serde_json::to_value(ReauthMode::Window).unwrap(),
            json!("window")
        );
        assert_eq!(
            serde_json::to_value(ReauthMode::Each).unwrap(),
            json!("each")
        );
        let info = AdminReauthInfo {
            required: false,
            factors: vec!["password".into(), "device_key".into()],
            password: ReauthMode::Window,
            elevated_for_s: 0,
        };
        assert_eq!(
            serde_json::to_value(&info).unwrap(),
            json!({
                "required": false,
                "factors": ["password", "device_key"],
                "password": "window",
                "elevated_for_s": 0
            })
        );
    }
}

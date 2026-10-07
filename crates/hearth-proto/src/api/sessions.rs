//! Sessions : `POST /sessions`, `DELETE /sessions/current`, `GET /me`.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};

use super::accounts::AccountInfo;

/// Corps de `POST /sessions`.
#[derive(Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

impl fmt::Debug for LoginRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LoginRequest")
            .field("username", &self.username)
            .field("password", &"***")
            .finish()
    }
}

/// Réponse de `POST /sessions` (`201`).
#[derive(Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    /// Jeton de session : 32 octets aléatoires en hexadécimal minuscule (64 caractères), à
    /// envoyer en `Authorization: Bearer`. Rendu une seule fois, jamais relisible ensuite.
    pub token: String,
    pub expires_at: String,
    pub account: AccountInfo,
}

impl fmt::Debug for LoginResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LoginResponse")
            .field("token", &"***")
            .field("expires_at", &self.expires_at)
            .field("account", &self.account)
            .finish()
    }
}

/// À quoi servira la preuve demandée à `POST /sessions/challenge`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChallengePurpose {
    /// Connexion par mot de passe.
    Login,
    /// Ouverture du flux d'une session.
    Session,
    /// Activation ou désactivation du mode attaque.
    AttackMode,
    /// Retrait d'un poste de confiance (`DELETE /me/devices/{id}`) : un usage distinct de la connexion
    /// et du flux.
    DeviceRemoval,
}

/// Corps de `POST /sessions/challenge` (route publique, sans effet, sans lecture en base).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChallengeRequest {
    /// L'identifiant saisi, existant ou non : la réponse est la même.
    pub username: String,
    pub purpose: ChallengePurpose,
}

/// Réponse de `POST /sessions/challenge` (`200`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChallengeResponse {
    /// Le défi : base64 de 56 octets, à signer (voir `device_proof`), valable `expires_in_s`.
    pub challenge: String,
    pub expires_in_s: u64,
}

/// La preuve de possession de la clé d'appareil, jointe à la connexion ou au premier message du
/// flux. Aucun champ n'est un secret (une clé publique, un défi, une signature) ; le `Debug`
/// n'écrit pourtant ni le défi ni la signature : un journal n'en a pas besoin.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceProof {
    /// `ed25519`.
    pub algorithm: String,
    /// Base64 de 32 octets.
    pub public_key: String,
    /// Le défi reçu, tel quel.
    pub challenge: String,
    /// Base64 de 64 octets.
    pub signature: String,
}

impl fmt::Debug for DeviceProof {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceProof")
            .field("algorithm", &self.algorithm)
            .field("challenge", &"***")
            .field("signature", &"***")
            .finish_non_exhaustive()
    }
}

/// Ce que la connexion a fait de la clé présentée (champ `device` de la réponse).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceStatus {
    /// Clé déjà inscrite, preuve valide.
    Proven,
    /// Clé inscrite par cette connexion.
    Enrolled,
    /// Le compte a déjà son maximum de postes : rien n'est inscrit.
    Limit,
    /// Preuve valide, inscription gelée (mode attaque).
    Deferred,
}

/// `POST /sessions` avec, en option, la preuve de la clé d'appareil. Même corps que
/// [`LoginRequest`] plus `device` : un client sans clé n'envoie rien de plus et l'agent le traite
/// comme avant. `device` absent ou invalide : la connexion se déroule comme sans clé, jamais une
/// erreur propre à la clé.
#[derive(Clone, Serialize, Deserialize)]
pub struct DeviceLoginRequest {
    #[serde(flatten)]
    pub login: LoginRequest,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient_proof"
    )]
    pub device: Option<DeviceProof>,
}

/// Lit une preuve sans jamais échouer : un champ `device` de la mauvaise forme (un nombre, un objet
/// auquel il manque un champ) vaut « pas de preuve », jamais une erreur de lecture du corps. Sinon
/// un client dont la clé est abîmée ne pourrait plus se connecter (la clé ne décide d'aucun accès).
pub fn lenient_proof<'de, D>(deserializer: D) -> Result<Option<DeviceProof>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(value.and_then(|value| serde_json::from_value(value).ok()))
}

impl fmt::Debug for DeviceLoginRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceLoginRequest")
            .field("login", &self.login)
            .field("device", &self.device)
            .finish()
    }
}

/// Réponse `201` de `POST /sessions` : les champs de [`LoginResponse`] plus `device`, absent quand
/// aucune clé n'a été prise en compte (un client qui ne connaît pas le champ l'ignore).
#[derive(Clone, Serialize, Deserialize)]
pub struct DeviceLoginResponse {
    #[serde(flatten)]
    pub login: LoginResponse,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<DeviceStatus>,
}

impl fmt::Debug for DeviceLoginResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceLoginResponse")
            .field("login", &self.login)
            .field("device", &self.device)
            .finish()
    }
}

/// Réponse de `GET /me`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeResponse {
    pub account: AccountInfo,
    /// Fin de la session courante (glissante : repoussée par l'activité).
    pub session_expires_at: String,
}

#[cfg(test)]
mod tests {
    use super::super::accounts::RoleName;
    use super::*;

    #[test]
    fn debug_hides_the_password_and_the_token() {
        let request = LoginRequest {
            username: "marie".into(),
            password: "Secret-Pass-123".into(),
        };
        let response = LoginResponse {
            token: "ab".repeat(32),
            expires_at: "2026-11-03T10:30:15.250Z".into(),
            account: AccountInfo {
                id: "A".into(),
                username: "marie".into(),
                role: RoleName::Admin,
            },
        };
        let text = format!("{request:?} {response:?}");
        assert!(!text.contains("Secret-Pass"), "{text}");
        assert!(!text.contains(&"ab".repeat(32)), "{text}");
    }

    #[test]
    fn the_device_is_optional_on_the_wire_and_the_old_shape_still_parses() {
        let old = r#"{"username":"marie","password":"Secret-Pass-123"}"#;
        let parsed: DeviceLoginRequest = serde_json::from_str(old).unwrap();
        assert_eq!(parsed.login.username, "marie");
        assert!(parsed.device.is_none());
        // Un client qui n'envoie pas de clé n'écrit pas le champ.
        let text = serde_json::to_string(&parsed).unwrap();
        assert!(!text.contains("device"), "{text}");
        // L'ancien type lit un corps qui porte une clé : le champ en plus est ignoré.
        let with_device = r#"{"username":"marie","password":"x","device":{"algorithm":"ed25519","public_key":"a","challenge":"b","signature":"c"}}"#;
        let parsed: DeviceLoginRequest = serde_json::from_str(with_device).unwrap();
        assert_eq!(parsed.device.unwrap().algorithm, "ed25519");
        let old_reading: LoginRequest = serde_json::from_str(with_device).unwrap();
        assert_eq!(old_reading.username, "marie");
    }

    #[test]
    fn the_login_response_with_a_device_still_reads_as_the_old_response() {
        let response = DeviceLoginResponse {
            login: LoginResponse {
                token: "ab".repeat(32),
                expires_at: "2026-11-03T10:30:15.250Z".into(),
                account: AccountInfo {
                    id: "A".into(),
                    username: "marie".into(),
                    role: super::super::accounts::RoleName::Admin,
                },
            },
            device: Some(DeviceStatus::Enrolled),
        };
        let text = serde_json::to_string(&response).unwrap();
        assert!(text.contains(r#""device":"enrolled""#), "{text}");
        let old: LoginResponse = serde_json::from_str(&text).unwrap();
        assert_eq!(old.expires_at, "2026-11-03T10:30:15.250Z");
        let none = DeviceLoginResponse {
            device: None,
            ..response
        };
        assert!(!serde_json::to_string(&none).unwrap().contains("device"));
    }

    #[test]
    fn the_purposes_and_statuses_have_their_documented_names() {
        for (purpose, name) in [
            (ChallengePurpose::Login, "login"),
            (ChallengePurpose::Session, "session"),
            (ChallengePurpose::AttackMode, "attack_mode"),
            (ChallengePurpose::DeviceRemoval, "device_removal"),
        ] {
            assert_eq!(
                serde_json::to_string(&purpose).unwrap(),
                format!("\"{name}\"")
            );
        }
        for (status, name) in [
            (DeviceStatus::Proven, "proven"),
            (DeviceStatus::Enrolled, "enrolled"),
            (DeviceStatus::Limit, "limit"),
            (DeviceStatus::Deferred, "deferred"),
        ] {
            assert_eq!(
                serde_json::to_string(&status).unwrap(),
                format!("\"{name}\"")
            );
        }
        // Une preuve de la mauvaise forme n'est pas une erreur : c'est une preuve absente.
        for bad in [
            r#"{"username":"m","password":"p","device":5}"#,
            r#"{"username":"m","password":"p","device":{"algorithm":"ed25519"}}"#,
            r#"{"username":"m","password":"p","device":"x"}"#,
            r#"{"username":"m","password":"p","device":null}"#,
        ] {
            let parsed: DeviceLoginRequest = serde_json::from_str(bad).unwrap();
            assert!(parsed.device.is_none(), "{bad}");
            assert_eq!(parsed.login.password, "p");
        }
        let unknown: Result<ChallengeRequest, _> =
            serde_json::from_str(r#"{"username":"a","purpose":"autre"}"#);
        assert!(unknown.is_err());
    }

    #[test]
    fn the_debug_of_a_proof_writes_neither_the_challenge_nor_the_signature() {
        let proof = DeviceProof {
            algorithm: "ed25519".into(),
            public_key: "cle-publique".into(),
            challenge: "DEFI-SECRET".into(),
            signature: "SIGNATURE-SECRETE".into(),
        };
        let text = format!("{proof:?}");
        assert!(
            !text.contains("DEFI-SECRET") && !text.contains("SIGNATURE-SECRETE"),
            "{text}"
        );
    }
}

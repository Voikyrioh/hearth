//! Comptes : `GET|POST /accounts`, `PATCH|DELETE /accounts/{id}`, `PUT /accounts/{id}/password`,
//! `PUT /me/password`, `DELETE /accounts/{id}/sessions`.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Rôle d'un compte sur le fil.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoleName {
    Admin,
    Readonly,
}

/// Le droit « gérer les services » (HRT-76, ADR-0041, BR-GRANT-001) dans `rights`. Indépendant du rôle,
/// donné depuis le serveur seulement : aucune route ne l'accorde ni ne le retire (BR-GRANT-005).
pub const RIGHT_SERVICES: &str = "services";

/// Le compte tel que le client le connaît : sans date ni haché.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountInfo {
    pub id: String,
    pub username: String,
    pub role: RoleName,
    /// Les droits du compte en plus de son rôle (`RIGHT_SERVICES`). Absent chez un agent ancien : lu
    /// comme une liste vide (BR-GRANT-016). Une valeur inconnue est tolérée : un agent plus récent peut
    /// ajouter un droit, un client ne retient que ceux qu'il connaît.
    #[serde(default)]
    pub rights: Vec<String>,
}

impl AccountInfo {
    /// Le compte peut-il gérer les services ? (`false` chez un agent qui n'annonce pas `rights`.)
    pub fn can_manage_services(&self) -> bool {
        has_right(&self.rights, RIGHT_SERVICES)
    }
}

/// `rights` contient-il ce droit ? Les valeurs inconnues sont ignorées.
pub fn has_right(rights: &[String], right: &str) -> bool {
    rights.iter().any(|candidate| candidate == right)
}

/// Un compte dans la liste d'administration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountItem {
    pub id: String,
    pub username: String,
    pub role: RoleName,
    pub created_at: String,
    /// Absent tant que le compte ne s'est jamais connecté.
    pub last_login_at: Option<String>,
    /// Sessions dont l'expiration est dans le futur.
    pub sessions_open: u64,
    /// Les droits du compte en plus de son rôle (voir `AccountInfo::rights`). Absent chez un agent
    /// ancien : lu comme une liste vide.
    #[serde(default)]
    pub rights: Vec<String>,
}

/// Réponse de `GET /accounts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountsResponse {
    pub accounts: Vec<AccountItem>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CreateAccountRequest {
    pub username: String,
    pub password: String,
    pub role: RoleName,
}

impl fmt::Debug for CreateAccountRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreateAccountRequest")
            .field("username", &self.username)
            .field("password", &"***")
            .field("role", &self.role)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeRoleRequest {
    pub role: RoleName,
}

/// Corps de `PUT /accounts/{id}/password` : un administrateur définit le mot de passe d'autrui.
#[derive(Clone, Serialize, Deserialize)]
pub struct SetPasswordRequest {
    pub password: String,
}

impl fmt::Debug for SetPasswordRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SetPasswordRequest")
            .field("password", &"***")
            .finish()
    }
}

/// Corps de `PUT /me/password` : le titulaire change son mot de passe.
#[derive(Clone, Serialize, Deserialize)]
pub struct ChangeOwnPasswordRequest {
    pub current: String,
    pub password: String,
    /// Garder l'adresse d'où part cette requête parmi les adresses retenues du compte (Q15,
    /// BR-CONN-019). Absent ou faux : toutes les adresses apprises sans clé sont oubliées, celle-ci
    /// comprise. Un client plus ancien, qui n'envoie pas le champ, garde ce comportement.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub keep_address: bool,
}

impl fmt::Debug for ChangeOwnPasswordRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChangeOwnPasswordRequest")
            .field("current", &"***")
            .field("password", &"***")
            .field("keep_address", &self.keep_address)
            .finish()
    }
}

/// Corps (facultatif) de `DELETE /accounts/{id}` : l'identifiant retapé quand on supprime son
/// propre compte (BR-ACCT-012).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteAccountRequest {
    #[serde(default)]
    pub confirmation: Option<String>,
}

/// Réponse des routes qui ferment des sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionsClosedResponse {
    pub sessions_closed: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn roles_travel_in_lowercase() {
        assert_eq!(
            serde_json::to_value(RoleName::Admin).unwrap(),
            json!("admin")
        );
        assert_eq!(
            serde_json::from_value::<RoleName>(json!("readonly")).unwrap(),
            RoleName::Readonly
        );
    }

    #[test]
    fn debug_hides_every_password() {
        let create = CreateAccountRequest {
            username: "marie".into(),
            password: "Secret-Pass-123".into(),
            role: RoleName::Admin,
        };
        let own = ChangeOwnPasswordRequest {
            current: "Old-Secret-123".into(),
            password: "New-Secret-123".into(),
            keep_address: false,
        };
        let set = SetPasswordRequest {
            password: "Another-Secret-1".into(),
        };
        for text in [
            format!("{create:?}"),
            format!("{own:?}"),
            format!("{set:?}"),
        ] {
            assert!(!text.contains("Secret"), "{text}");
        }
    }

    #[test]
    fn keeping_the_address_is_an_additive_field_absent_means_the_current_behaviour() {
        let old: ChangeOwnPasswordRequest =
            serde_json::from_value(json!({ "current": "a", "password": "b" })).unwrap();
        assert!(
            !old.keep_address,
            "un client ancien garde le comportement actuel"
        );
        let keep: ChangeOwnPasswordRequest = serde_json::from_value(
            json!({ "current": "a", "password": "b", "keep_address": true }),
        )
        .unwrap();
        assert!(keep.keep_address);
        // Faux : le champ n'est pas écrit, le corps reste celui d'avant.
        assert_eq!(
            serde_json::to_value(&old).unwrap(),
            json!({ "current": "a", "password": "b" })
        );
        assert_eq!(serde_json::to_value(&keep).unwrap()["keep_address"], true);
    }

    #[test]
    fn the_delete_body_is_optional_field_by_field() {
        let empty: DeleteAccountRequest = serde_json::from_value(json!({})).unwrap();
        assert_eq!(empty.confirmation, None);
    }

    #[test]
    fn an_item_without_login_serializes_a_null_date() {
        let item = AccountItem {
            id: "A".into(),
            username: "marie".into(),
            role: RoleName::Readonly,
            created_at: "2026-10-04T10:30:15.250Z".into(),
            last_login_at: None,
            sessions_open: 0,
            rights: Vec::new(),
        };
        assert_eq!(
            serde_json::to_value(item).unwrap()["last_login_at"],
            json!(null)
        );
    }

    #[test]
    fn rights_are_an_additive_field_absent_means_none_and_an_unknown_right_is_tolerated() {
        // Un agent ancien n'écrit pas `rights` : le client lit une liste vide.
        let old: AccountInfo =
            serde_json::from_value(json!({ "id": "A", "username": "marie", "role": "admin" }))
                .unwrap();
        assert!(old.rights.is_empty() && !old.can_manage_services());
        let old_item: AccountItem = serde_json::from_value(json!({
            "id": "A", "username": "marie", "role": "readonly",
            "created_at": "2026-10-04T10:30:15.250Z", "last_login_at": null, "sessions_open": 0
        }))
        .unwrap();
        assert!(old_item.rights.is_empty());
        // Un agent plus récent peut ajouter un droit : il est ignoré, `services` reste lu.
        let newer: AccountInfo = serde_json::from_value(json!({
            "id": "A", "username": "marie", "role": "readonly", "rights": ["console", "services"]
        }))
        .unwrap();
        assert!(newer.can_manage_services());
        let unknown_only: AccountInfo = serde_json::from_value(json!({
            "id": "A", "username": "marie", "role": "readonly", "rights": ["console"]
        }))
        .unwrap();
        assert!(!unknown_only.can_manage_services());
        // Un client ancien, qui ne connaît pas le champ, lit toujours le compte.
        #[derive(Deserialize)]
        struct OldInfo {
            #[allow(dead_code)]
            username: String,
        }
        let text = serde_json::to_string(&newer).unwrap();
        assert!(serde_json::from_str::<OldInfo>(&text).is_ok());
        // Le nom du droit sur le fil est celui de la spec.
        assert_eq!(RIGHT_SERVICES, "services");
    }
}

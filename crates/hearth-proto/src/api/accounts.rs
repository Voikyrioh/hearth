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

/// Le compte tel que le client le connaît : sans date ni haché.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountInfo {
    pub id: String,
    pub username: String,
    pub role: RoleName,
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
}

impl fmt::Debug for ChangeOwnPasswordRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChangeOwnPasswordRequest")
            .field("current", &"***")
            .field("password", &"***")
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
        };
        assert_eq!(
            serde_json::to_value(item).unwrap()["last_login_at"],
            json!(null)
        );
    }
}

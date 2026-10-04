//! Sessions : `POST /sessions`, `DELETE /sessions/current`, `GET /me`.

use std::fmt;

use serde::{Deserialize, Serialize};

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
}

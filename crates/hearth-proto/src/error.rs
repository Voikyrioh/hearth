//! Format d'erreur commun à toutes les routes : `{ "error": { "code", "message", "details" } }`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Code d'erreur stable, sérialisé en `SCREAMING_SNAKE_CASE`.
///
/// Les textes affichés à l'utilisateur sont indexés par ce code côté interface ;
/// le `message` du corps ne sert qu'au diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    Unauthenticated,
    /// Identifiant ou mot de passe refusé, sans préciser lequel (BR-CONN-013).
    InvalidCredentials,
    SessionExpired,
    SessionRevoked,
    ForbiddenRole,
    OperationInProgress,
    ValidationError,
    IncompatibleVersion,
    TooManyAttempts,
    /// Identifiant déjà pris par un autre compte.
    UsernameTaken,
    /// Mot de passe refusé ; `details.rules` liste les règles non respectées.
    WeakPassword,
    /// Mot de passe actuel incorrect lors d'un changement de son propre mot de passe.
    WrongPassword,
    /// L'opération laisserait le serveur sans administrateur.
    LastAdmin,
    /// Conflit avec un changement concurrent : réessayer.
    Conflict,
    InternalError,
    /// Route ou ressource inconnue.
    NotFound,
    /// Méthode HTTP non prise en charge par la route.
    MethodNotAllowed,
}

impl ErrorCode {
    /// Statut HTTP associé au code.
    pub const fn http_status(self) -> u16 {
        match self {
            Self::Unauthenticated
            | Self::InvalidCredentials
            | Self::SessionExpired
            | Self::SessionRevoked => 401,
            Self::ForbiddenRole => 403,
            Self::NotFound => 404,
            Self::MethodNotAllowed => 405,
            Self::OperationInProgress | Self::UsernameTaken | Self::LastAdmin | Self::Conflict => {
                409
            }
            Self::ValidationError | Self::WeakPassword | Self::WrongPassword => 422,
            Self::IncompatibleVersion => 426,
            Self::TooManyAttempts => 429,
            Self::InternalError => 500,
        }
    }
}

/// Contenu de l'enveloppe `error`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorDetail {
    pub code: ErrorCode,
    pub message: String,
    /// Données complémentaires propres au code (objet vide par défaut).
    #[serde(default = "empty_details")]
    pub details: Value,
}

/// Corps de toute réponse d'erreur.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub error: ErrorDetail,
}

impl ErrorBody {
    /// Erreur sans détails.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::with_details(code, message, empty_details())
    }

    pub fn with_details(code: ErrorCode, message: impl Into<String>, details: Value) -> Self {
        Self {
            error: ErrorDetail {
                code,
                message: message.into(),
                details,
            },
        }
    }
}

/// Qui doit se mettre à jour quand les versions d'interface sont incompatibles (`details.upgrade`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpgradeTarget {
    Client,
    Agent,
}

impl ErrorBody {
    /// `426 INCOMPATIBLE_VERSION` avec `details.upgrade`.
    pub fn incompatible_version(upgrade: UpgradeTarget, message: impl Into<String>) -> Self {
        Self::with_details(
            ErrorCode::IncompatibleVersion,
            message,
            serde_json::json!({ "upgrade": upgrade }),
        )
    }

    /// `429 TOO_MANY_ATTEMPTS` avec `details.retry_after_s`.
    pub fn too_many_attempts(retry_after_s: u64, message: impl Into<String>) -> Self {
        Self::with_details(
            ErrorCode::TooManyAttempts,
            message,
            serde_json::json!({ "retry_after_s": retry_after_s }),
        )
    }

    /// `422 WEAK_PASSWORD` avec `details.rules` : les règles non respectées, par code stable.
    pub fn weak_password(rules: &[&str], message: impl Into<String>) -> Self {
        Self::with_details(
            ErrorCode::WeakPassword,
            message,
            serde_json::json!({ "rules": rules }),
        )
    }
}

fn empty_details() -> Value {
    Value::Object(serde_json::Map::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ALL: [(ErrorCode, &str, u16); 17] = [
        (ErrorCode::Unauthenticated, "UNAUTHENTICATED", 401),
        (ErrorCode::InvalidCredentials, "INVALID_CREDENTIALS", 401),
        (ErrorCode::UsernameTaken, "USERNAME_TAKEN", 409),
        (ErrorCode::WeakPassword, "WEAK_PASSWORD", 422),
        (ErrorCode::WrongPassword, "WRONG_PASSWORD", 422),
        (ErrorCode::LastAdmin, "LAST_ADMIN", 409),
        (ErrorCode::Conflict, "CONFLICT", 409),
        (ErrorCode::SessionExpired, "SESSION_EXPIRED", 401),
        (ErrorCode::SessionRevoked, "SESSION_REVOKED", 401),
        (ErrorCode::ForbiddenRole, "FORBIDDEN_ROLE", 403),
        (ErrorCode::OperationInProgress, "OPERATION_IN_PROGRESS", 409),
        (ErrorCode::ValidationError, "VALIDATION_ERROR", 422),
        (ErrorCode::IncompatibleVersion, "INCOMPATIBLE_VERSION", 426),
        (ErrorCode::TooManyAttempts, "TOO_MANY_ATTEMPTS", 429),
        (ErrorCode::InternalError, "INTERNAL_ERROR", 500),
        (ErrorCode::NotFound, "NOT_FOUND", 404),
        (ErrorCode::MethodNotAllowed, "METHOD_NOT_ALLOWED", 405),
    ];

    #[test]
    fn codes_serialize_in_screaming_snake_case() {
        for (code, name, _) in ALL {
            assert_eq!(
                serde_json::to_value(code).expect("serialization"),
                json!(name)
            );
        }
    }

    #[test]
    fn codes_map_to_their_http_status() {
        for (code, _, status) in ALL {
            assert_eq!(code.http_status(), status);
        }
    }

    #[test]
    fn body_has_the_documented_envelope() {
        let body = ErrorBody::with_details(
            ErrorCode::IncompatibleVersion,
            "client trop ancien",
            json!({ "upgrade": "client" }),
        );
        assert_eq!(
            serde_json::to_value(&body).expect("serialization"),
            json!({
                "error": {
                    "code": "INCOMPATIBLE_VERSION",
                    "message": "client trop ancien",
                    "details": { "upgrade": "client" }
                }
            })
        );
    }

    #[test]
    fn helpers_build_the_documented_details() {
        let body = ErrorBody::incompatible_version(UpgradeTarget::Agent, "agent trop ancien");
        assert_eq!(body.error.details, json!({ "upgrade": "agent" }));
        let body = ErrorBody::too_many_attempts(60, "attends");
        assert_eq!(body.error.code, ErrorCode::TooManyAttempts);
        assert_eq!(body.error.details, json!({ "retry_after_s": 60 }));
        let body = ErrorBody::weak_password(&["min_length", "digit"], "faible");
        assert_eq!(
            body.error.details,
            json!({ "rules": ["min_length", "digit"] })
        );
    }

    #[test]
    fn details_default_to_an_empty_object() {
        let body = ErrorBody::new(ErrorCode::InternalError, "oups");
        assert_eq!(body.error.details, json!({}));
        let parsed: ErrorBody =
            serde_json::from_value(json!({ "error": { "code": "NOT_FOUND", "message": "x" } }))
                .expect("deserialization");
        assert_eq!(parsed.error.details, json!({}));
    }
}

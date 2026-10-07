use std::fmt::Display;

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::http::{HeaderValue, StatusCode, header::RETRY_AFTER};
use axum::response::{IntoResponse, Response};
use hearth_proto::error::{ErrorBody, ErrorCode, UpgradeTarget};
use serde_json::json;

use crate::application::accounts::AccountError;
use crate::application::audit::AuditError;
use crate::application::ports::{HashError, StoreError};
use crate::application::sessions::{AuthError, LoginError};
use crate::domain::compat::Incompatibility;
use crate::domain::lockout::retry_after_seconds;
use crate::domain::sessions::SessionEnd;

/// Posé sur toute réponse d'erreur de l'API : le code du protocole, que la couche d'accès lit pour
/// consigner l'échec d'une requête qui modifie (jamais le corps).
#[derive(Debug, Clone, Copy)]
pub struct ErrorMark(pub ErrorCode);

/// Posé par la couche de confirmation des actes (`reauth.rs`) sur un refus qu'elle a consigné avec sa
/// propre raison : la couche d'accès l'écrit tel quel au journal, à la place de la règle par code
/// d'erreur (`auth::failure_of`).
#[derive(Debug, Clone, Copy)]
pub struct OutcomeMark(pub crate::domain::audit::Outcome);

/// Erreur d'API : toujours rendue au format `ErrorBody`, avec le statut du code.
#[derive(Debug)]
pub struct ApiError(pub ErrorBody);

impl ApiError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self(ErrorBody::new(code, message))
    }

    /// Erreur de validation sur un champ (`details.field`).
    pub fn invalid(field: &str, message: impl Into<String>) -> Self {
        Self(ErrorBody::with_details(
            ErrorCode::ValidationError,
            message,
            json!({ "field": field }),
        ))
    }

    /// Erreur interne : le détail va au journal (une fois), jamais au client.
    pub fn internal(error: &dyn Display) -> Self {
        tracing::error!(%error, "erreur interne");
        Self::new(ErrorCode::InternalError, "erreur interne")
    }
}

impl From<ErrorBody> for ApiError {
    fn from(body: ErrorBody) -> Self {
        Self(body)
    }
}

/// Corps JSON illisible ou invalide : erreur de validation, jamais le texte brut d'axum.
impl From<JsonRejection> for ApiError {
    fn from(rejection: JsonRejection) -> Self {
        Self::new(ErrorCode::ValidationError, rejection.body_text())
    }
}

/// Saturation : `503 BUSY` ; toute autre erreur de hachage est interne.
impl From<HashError> for ApiError {
    fn from(error: HashError) -> Self {
        match error {
            HashError::Busy => Self::new(
                ErrorCode::Busy,
                "L'agent est occupé. Réessaie dans un instant.",
            ),
            other => Self::internal(&other),
        }
    }
}

impl From<StoreError> for ApiError {
    fn from(error: StoreError) -> Self {
        Self::internal(&error)
    }
}

impl From<Incompatibility> for ApiError {
    fn from(incompatibility: Incompatibility) -> Self {
        let (upgrade, message) = match incompatibility {
            Incompatibility::ClientTooOld => (
                UpgradeTarget::Client,
                "Le client est trop ancien. Mets à jour le client sur ce PC.",
            ),
            Incompatibility::AgentTooOld => (
                UpgradeTarget::Agent,
                "L'agent de ce serveur est trop ancien. Mets à jour l'agent sur le serveur.",
            ),
        };
        Self(ErrorBody::incompatible_version(upgrade, message))
    }
}

impl From<AuthError> for ApiError {
    fn from(error: AuthError) -> Self {
        match error {
            AuthError::Malformed => Self::new(
                ErrorCode::Unauthenticated,
                "Jeton de session absent ou illisible",
            ),
            // La même réponse, au mot près, qu'une session expirée : rien ne dit que le mode attaque est
            // actif (BR-TRUST-013).
            AuthError::Ended(SessionEnd::Expired) | AuthError::NotRecognized => Self::new(
                ErrorCode::SessionExpired,
                "Session expirée. Reconnecte-toi.",
            ),
            AuthError::Ended(SessionEnd::Revoked) => Self::new(
                ErrorCode::SessionRevoked,
                "Accès révoqué. Contacte l'administrateur.",
            ),
            AuthError::Store(error) => Self::internal(&error),
        }
    }
}

impl From<LoginError> for ApiError {
    fn from(error: LoginError) -> Self {
        match error {
            LoginError::InvalidCredentials => Self::new(
                ErrorCode::InvalidCredentials,
                "Identifiant ou mot de passe incorrect.",
            ),
            LoginError::TooManyAttempts { retry_after } => {
                let seconds = retry_after_seconds(retry_after);
                Self(ErrorBody::too_many_attempts(
                    seconds,
                    format!("Trop de tentatives. Attends {seconds} s avant de réessayer."),
                ))
            }
            // C'est le client qui déborde, pas l'agent : 429, comme un verrouillage.
            LoginError::Busy => Self(ErrorBody::too_many_attempts(
                1,
                "Trop de connexions en attente depuis cette adresse. Réessaie dans un instant.",
            )),
            LoginError::Store(error) => Self::internal(&error),
            LoginError::Hash(error) => Self::from(error),
            LoginError::Token(error) => Self::internal(&error),
        }
    }
}

impl From<AccountError> for ApiError {
    fn from(error: AccountError) -> Self {
        let message = error.to_string();
        match error {
            AccountError::Username(_) => Self::invalid("username", message),
            AccountError::WeakPassword(rejected) => {
                let rules: Vec<&str> = rejected.rules.iter().map(|rule| rule.code()).collect();
                Self(ErrorBody::weak_password(&rules, message))
            }
            AccountError::UsernameTaken => Self::new(ErrorCode::UsernameTaken, message),
            AccountError::NotFound => Self::new(ErrorCode::NotFound, message),
            AccountError::LastAdmin(_) => Self::new(ErrorCode::LastAdmin, message),
            AccountError::OldPasswordIncorrect => Self::new(ErrorCode::WrongPassword, message),
            AccountError::PasswordChangedMeanwhile => Self::new(ErrorCode::Conflict, message),
            AccountError::SelfDeletion(_) => Self::invalid("confirmation", message),
            AccountError::Store(error) => Self::internal(&error),
            AccountError::Hash(error) => Self::from(error),
        }
    }
}

impl From<AuditError> for ApiError {
    fn from(error: AuditError) -> Self {
        match error {
            AuditError::Forbidden => Self::new(
                ErrorCode::ForbiddenRole,
                "Tu n'as pas la permission de lire le journal d'activité",
            ),
            AuditError::Store(error) => Self::internal(&error),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let code = self.0.error.code;
        let status =
            StatusCode::from_u16(code.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let busy = code == ErrorCode::Busy;
        let mut response = (status, Json(self.0)).into_response();
        response.extensions_mut().insert(ErrorMark(code));
        if busy {
            response
                .headers_mut()
                .insert(RETRY_AFTER, HeaderValue::from_static("1"));
        }
        response
    }
}

/// Réponse des routes inconnues.
pub async fn not_found() -> ApiError {
    ApiError::new(ErrorCode::NotFound, "route inconnue")
}

/// Réponse d'une route existante appelée avec une autre méthode.
pub async fn method_not_allowed() -> ApiError {
    ApiError::new(ErrorCode::MethodNotAllowed, "méthode non prise en charge")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::accounts::{PasswordRejected, PasswordRule};
    use time::Duration;

    #[test]
    fn invalid_credentials_say_nothing_about_which_part_is_wrong() {
        let error = ApiError::from(LoginError::InvalidCredentials);
        assert_eq!(error.0.error.code, ErrorCode::InvalidCredentials);
        let message = error.0.error.message.to_lowercase();
        assert!(!message.contains("inconnu") && !message.contains("introuvable"));
    }

    #[test]
    fn the_lockout_carries_the_wait_rounded_up_in_seconds() {
        let error = ApiError::from(LoginError::TooManyAttempts {
            retry_after: Duration::milliseconds(59_100),
        });
        assert_eq!(error.0.error.code, ErrorCode::TooManyAttempts);
        assert_eq!(error.0.error.details, json!({ "retry_after_s": 60 }));
    }

    #[test]
    fn a_weak_password_lists_every_unmet_rule_by_code() {
        let error = ApiError::from(AccountError::WeakPassword(PasswordRejected {
            rules: vec![PasswordRule::MinLength, PasswordRule::Digit],
        }));
        assert_eq!(error.0.error.code, ErrorCode::WeakPassword);
        assert_eq!(
            error.0.error.details,
            json!({ "rules": ["min_length", "digit"] })
        );
    }

    #[test]
    fn session_ends_map_to_distinct_codes() {
        let expired = ApiError::from(AuthError::Ended(SessionEnd::Expired));
        let revoked = ApiError::from(AuthError::Ended(SessionEnd::Revoked));
        assert_eq!(expired.0.error.code, ErrorCode::SessionExpired);
        assert_eq!(revoked.0.error.code, ErrorCode::SessionRevoked);
    }

    #[test]
    fn incompatibilities_say_who_must_upgrade() {
        let client = ApiError::from(Incompatibility::ClientTooOld);
        let agent = ApiError::from(Incompatibility::AgentTooOld);
        assert_eq!(client.0.error.details, json!({ "upgrade": "client" }));
        assert_eq!(agent.0.error.details, json!({ "upgrade": "agent" }));
        assert_eq!(client.0.error.code, ErrorCode::IncompatibleVersion);
    }

    #[test]
    fn a_saturated_hasher_answers_503_busy_with_retry_after() {
        let error = ApiError::from(LoginError::Hash(HashError::Busy));
        assert_eq!(error.0.error.code, ErrorCode::Busy);
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.headers().get(RETRY_AFTER).unwrap(), "1");
        let error = ApiError::from(AccountError::Hash(HashError::Busy));
        assert_eq!(error.0.error.code, ErrorCode::Busy);
    }

    #[test]
    fn a_full_connection_queue_answers_429_with_the_wait() {
        let error = ApiError::from(LoginError::Busy);
        assert_eq!(error.0.error.code, ErrorCode::TooManyAttempts);
        assert_eq!(error.0.error.details, json!({ "retry_after_s": 1 }));
        assert_eq!(
            error.into_response().status(),
            StatusCode::TOO_MANY_REQUESTS
        );
    }

    #[test]
    fn an_internal_error_never_leaks_its_cause() {
        let error = ApiError::from(StoreError::Unavailable {
            resource: "accounts",
            message: "disque plein".into(),
        });
        assert_eq!(error.0.error.code, ErrorCode::InternalError);
        assert!(!error.0.error.message.contains("disque"));
    }
}

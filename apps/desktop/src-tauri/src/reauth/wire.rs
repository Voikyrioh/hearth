//! La lecture des refus de CONFIRMATION d'un acte (HRT-30, BR-TRUST-040, 045, 046), partagée par toutes
//! les actions d'administration : un seul endroit dit ce que `409`, `422`, `426`, `429` et `503` veulent
//! dire pour une confirmation. Pur : sans E/S, sans Tauri. Le code d'erreur et `details.reason` sont
//! stables ; le texte de l'agent n'est jamais lu.

use hearth_proto::api::reauth::reauth_refusal;
use hearth_proto::error::{ErrorBody, ErrorCode};

use crate::link_dto::LinkFailure;

/// Un refus qui porte sur la confirmation et que la fenêtre de l'acte montre à l'endroit du champ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirmation {
    /// Le mot de passe de confirmation est faux (les compteurs de la connexion ont avancé).
    WrongPassword,
    /// L'élévation s'est fermée et le mot de passe n'était pas dans la requête : le redemander.
    PasswordRequired,
    /// Trop d'essais de mot de passe.
    TooManyAttempts { retry_after_s: u32 },
    /// L'agent est saturé.
    Busy,
}

/// Le sens d'un refus pour la confirmation :
/// - `Ok(Some(_))` : un refus de mot de passe (montré dans la fenêtre) ;
/// - `Err(NotRecognized)` : l'agent n'a pas reconnu la preuve de la clé de ce PC (`409`, sauf
///   `password_required`) ;
/// - `Err(IncompatibleClient)` : l'agent exige la confirmation et ce client est trop ancien (`426`) ;
/// - `Ok(None)` : pas un refus de confirmation, l'action le lit comme avant.
pub fn confirmation_of(error: &ErrorBody) -> Result<Option<Confirmation>, LinkFailure> {
    let error = &error.error;
    Ok(match error.code {
        ErrorCode::WrongPassword => Some(Confirmation::WrongPassword),
        ErrorCode::TooManyAttempts => Some(Confirmation::TooManyAttempts {
            retry_after_s: error.details["retry_after_s"]
                .as_u64()
                .and_then(|seconds| u32::try_from(seconds).ok())
                .unwrap_or(60),
        }),
        ErrorCode::Busy => Some(Confirmation::Busy),
        ErrorCode::PostNotRecognized => {
            if error.details["reason"].as_str() == Some(reauth_refusal::PASSWORD_REQUIRED) {
                Some(Confirmation::PasswordRequired)
            } else {
                return Err(LinkFailure::NotRecognized);
            }
        }
        // Le seul `426` d'une action : l'agent exige la confirmation et ce client ne l'envoie pas
        // (BR-TRUST-045). La connexion et la lecture ne sont pas touchées.
        ErrorCode::IncompatibleVersion => return Err(LinkFailure::IncompatibleClient),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn body(code: &str, details: serde_json::Value) -> ErrorBody {
        serde_json::from_value(
            json!({ "error": { "code": code, "message": "x", "details": details } }),
        )
        .unwrap()
    }

    #[test]
    fn each_refusal_of_the_confirmation_has_one_meaning() {
        let read = |code, details| confirmation_of(&body(code, details));
        assert_eq!(
            read("WRONG_PASSWORD", json!(null)),
            Ok(Some(Confirmation::WrongPassword))
        );
        assert_eq!(
            read("TOO_MANY_ATTEMPTS", json!({ "retry_after_s": 42 })),
            Ok(Some(Confirmation::TooManyAttempts { retry_after_s: 42 }))
        );
        assert_eq!(
            read("TOO_MANY_ATTEMPTS", json!(null)),
            Ok(Some(Confirmation::TooManyAttempts { retry_after_s: 60 }))
        );
        assert_eq!(read("BUSY", json!(null)), Ok(Some(Confirmation::Busy)));
        assert_eq!(
            read(
                "POST_NOT_RECOGNIZED",
                json!({ "reason": "password_required" })
            ),
            Ok(Some(Confirmation::PasswordRequired))
        );
        for reason in ["proof_missing", "proof_invalid", "device_required"] {
            assert_eq!(
                read("POST_NOT_RECOGNIZED", json!({ "reason": reason })),
                Err(LinkFailure::NotRecognized),
                "{reason}"
            );
        }
        assert_eq!(
            read("POST_NOT_RECOGNIZED", json!(null)),
            Err(LinkFailure::NotRecognized)
        );
        assert_eq!(
            read(
                "INCOMPATIBLE_VERSION",
                json!({ "upgrade": "client", "reason": "reauth_required" })
            ),
            Err(LinkFailure::IncompatibleClient)
        );
        assert_eq!(read("LAST_ADMIN", json!(null)), Ok(None));
    }
}

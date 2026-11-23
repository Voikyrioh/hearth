//! La console d'un serveur de jeu et les joueurs connectés depuis le client (HRT-291, ADR-0067, `BR-CONSOLE-001` à
//! `011`) : décrire ce que la console permet, lire les joueurs, envoyer UNE commande (acte `ConsoleCommand`). Chaque
//! requête est construite par la liaison (`LinkManager::{describe_console, read_players, send_console}`, chemins
//! fixes, clé vérifiée puis encodée). Une commande n'est jamais rejouée : un lien tombé avant la réponse donne
//! `Unknown` (elle a peut-être été exécutée). Les refus se lisent par leur CODE, jamais par leur texte. Le texte d'une
//! commande et la réponse du jeu ne sont ni gardés ni tracés. Sans Tauri.

use hearth_link::domain::secret::Secret;
use hearth_link::domain::server::ServerId;
use hearth_link::{ActionOutcome, LinkError, LinkManager};
use hearth_proto::api::console::{ConsoleReply, ConsoleRequest};
use hearth_proto::error::{ErrorBody, ErrorCode};
use serde_json::Value;

use super::console_dto::{
    ConsoleOutcome, ConsoleReadRefusal, ConsoleRefusal, ConsoleReplyDto, ConsoleSendOutcome,
    ConsoleViewDto, PlayersOutcome, PlayersViewDto,
};
use crate::link_dto::LinkFailure;
use crate::reauth::wire::{Confirmation, confirmation_of};

fn read_refusal(error: LinkError) -> Result<ConsoleReadRefusal, LinkFailure> {
    match error {
        LinkError::Rejected(Some(code)) => Ok(match code {
            ErrorCode::ServiceNotFound | ErrorCode::NotFound | ErrorCode::NotAGameService => {
                ConsoleReadRefusal::ServiceNotFound
            }
            _ => ConsoleReadRefusal::Other,
        }),
        other => Err(other.into()),
    }
}

/// Ce que la console permet (`GET /services/{key}/console`) : rien n'est envoyé au jeu.
pub async fn describe(
    manager: &LinkManager,
    id: &ServerId,
    key: &str,
) -> Result<ConsoleOutcome, LinkFailure> {
    match manager.describe_console(id, key).await {
        Ok(view) => Ok(ConsoleOutcome::Ready {
            view: Box::new(ConsoleViewDto::from(&view)),
        }),
        Err(error) => read_refusal(error).map(|refusal| ConsoleOutcome::Refused { refusal }),
    }
}

/// Les joueurs connectés (`GET /services/{key}/players`).
pub async fn players(
    manager: &LinkManager,
    id: &ServerId,
    key: &str,
) -> Result<PlayersOutcome, LinkFailure> {
    match manager.read_players(id, key).await {
        Ok(view) => Ok(PlayersOutcome::Ready {
            view: Box::new(PlayersViewDto::from(&view)),
        }),
        Err(error) => read_refusal(error).map(|refusal| PlayersOutcome::Refused { refusal }),
    }
}

/// Envoie une commande. `class` est celle que l'interface a jugée (`1` libre, `2` sensible) ; une autre valeur ne
/// part jamais. Le mot de passe est celui de la confirmation (toujours pour une commande sensible).
pub async fn send(
    manager: &LinkManager,
    id: &ServerId,
    key: &str,
    command: String,
    class: u8,
    admin_password: Option<&Secret>,
) -> Result<ConsoleSendOutcome, LinkFailure> {
    use hearth_proto::api::console::{CLASS_FREE, CLASS_SENSITIVE};
    if class != CLASS_FREE && class != CLASS_SENSITIVE {
        return Ok(ConsoleSendOutcome::Refused {
            refusal: ConsoleRefusal::ClassMismatch,
        });
    }
    let request = ConsoleRequest { command, class };
    match manager
        .send_console(id, key, &request, admin_password)
        .await?
    {
        ActionOutcome::Completed { status, body, .. } => interpret(status, &body),
        ActionOutcome::ResultUnknown { id } => Ok(ConsoleSendOutcome::Unknown {
            op_id: id.as_str().to_owned(),
        }),
    }
}

fn detail_text(body: &Value, name: &str) -> Option<String> {
    body.pointer(&format!("/error/details/{name}"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn detail_number(body: &Value, name: &str) -> Option<u32> {
    body.pointer(&format!("/error/details/{name}"))
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
}

/// La réponse de l'agent à `POST /services/{key}/console` : `200` avec la réponse du jeu, ou un refus typé.
pub fn interpret(status: u16, body: &Value) -> Result<ConsoleSendOutcome, LinkFailure> {
    if (200..300).contains(&status) {
        let reply: ConsoleReply =
            serde_json::from_value(body.clone()).map_err(|_| LinkFailure::NotAgent)?;
        return Ok(ConsoleSendOutcome::Replied {
            reply: ConsoleReplyDto::from(&reply),
        });
    }
    let refused = |refusal| Ok(ConsoleSendOutcome::Refused { refusal });
    let Ok(parsed) = serde_json::from_value::<ErrorBody>(body.clone()) else {
        if status == 403 {
            return Err(LinkFailure::Forbidden);
        }
        return refused(ConsoleRefusal::Other);
    };
    if let Some(confirmation) = confirmation_of(&parsed)? {
        return refused(match confirmation {
            Confirmation::WrongPassword => ConsoleRefusal::WrongPassword,
            Confirmation::PasswordRequired => ConsoleRefusal::PasswordRequired,
            Confirmation::TooManyAttempts { retry_after_s } => {
                ConsoleRefusal::TooManyAttempts { retry_after_s }
            }
            Confirmation::Busy => ConsoleRefusal::Busy,
        });
    }
    match parsed.error.code.or_from_status(status) {
        ErrorCode::ForbiddenRole => Err(LinkFailure::forbidden(&parsed.error)),
        ErrorCode::ServiceNotFound | ErrorCode::NotFound | ErrorCode::NotAGameService => {
            refused(ConsoleRefusal::ServiceNotFound)
        }
        ErrorCode::ServiceAdopted => refused(ConsoleRefusal::ServiceAdopted),
        ErrorCode::ConsoleNotAvailable => refused(ConsoleRefusal::NotAvailable),
        ErrorCode::ConsoleCommandInvalid => refused(ConsoleRefusal::Invalid {
            reason: detail_text(body, "reason"),
        }),
        ErrorCode::ConsoleCommandRefused => refused(ConsoleRefusal::VerbRefused {
            reason: detail_text(body, "reason"),
        }),
        ErrorCode::ConsoleClassMismatch => refused(ConsoleRefusal::ClassMismatch),
        ErrorCode::ServiceNotRunning => refused(ConsoleRefusal::NotRunning),
        ErrorCode::ServiceStarting => refused(ConsoleRefusal::Starting),
        ErrorCode::OperationInProgress | ErrorCode::ServiceBusy => {
            refused(ConsoleRefusal::OperationInProgress)
        }
        ErrorCode::ConsoleRateLimited => refused(ConsoleRefusal::RateLimited {
            retry_after_s: detail_number(body, "retry_after_s"),
        }),
        ErrorCode::ConsoleUnreachable => refused(ConsoleRefusal::Unreachable),
        _ => refused(ConsoleRefusal::Other),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn error(code: &str, details: Value) -> Value {
        json!({ "error": { "code": code, "message": "x", "details": details } })
    }

    fn refusal(status: u16, body: Value) -> ConsoleRefusal {
        match interpret(status, &body).unwrap() {
            ConsoleSendOutcome::Refused { refusal } => refusal,
            other => panic!("refus attendu, reçu {other:?}"),
        }
    }

    #[test]
    fn the_reply_of_the_game_is_carried_and_never_shown_by_debug() {
        let outcome = interpret(
            200,
            &json!({ "outcome": "answered", "reply": "Made Secret a server operator", "truncated": false }),
        )
        .unwrap();
        let ConsoleSendOutcome::Replied { reply } = &outcome else {
            panic!("réponse attendue");
        };
        assert_eq!(reply.outcome, "answered");
        assert_eq!(
            reply.reply.as_deref(),
            Some("Made Secret a server operator")
        );
        assert!(!format!("{outcome:?}").contains("Secret"));
        assert!(matches!(
            interpret(200, &json!({ "nope": 1 })),
            Err(LinkFailure::NotAgent)
        ));
    }

    #[test]
    fn each_refusal_is_read_by_its_code() {
        assert_eq!(
            refusal(422, error("WRONG_PASSWORD", json!(null))),
            ConsoleRefusal::WrongPassword
        );
        assert_eq!(
            refusal(
                422,
                error("CONSOLE_COMMAND_INVALID", json!({ "reason": "too_long" }))
            ),
            ConsoleRefusal::Invalid {
                reason: Some("too_long".into())
            }
        );
        assert_eq!(
            refusal(
                422,
                error(
                    "CONSOLE_COMMAND_REFUSED",
                    json!({ "reason": "use_hearth_stop" })
                )
            ),
            ConsoleRefusal::VerbRefused {
                reason: Some("use_hearth_stop".into())
            }
        );
        assert_eq!(
            refusal(
                429,
                error("CONSOLE_RATE_LIMITED", json!({ "retry_after_s": 12 }))
            ),
            ConsoleRefusal::RateLimited {
                retry_after_s: Some(12)
            }
        );
        assert_eq!(
            refusal(409, error("SERVICE_NOT_RUNNING", json!({}))),
            ConsoleRefusal::NotRunning
        );
        assert_eq!(
            refusal(409, error("SERVICE_STARTING", json!({}))),
            ConsoleRefusal::Starting
        );
        assert_eq!(
            refusal(409, error("OPERATION_IN_PROGRESS", json!({}))),
            ConsoleRefusal::OperationInProgress
        );
        assert_eq!(
            refusal(502, error("CONSOLE_UNREACHABLE", json!({}))),
            ConsoleRefusal::Unreachable
        );
        assert_eq!(
            refusal(422, error("CONSOLE_CLASS_MISMATCH", json!({}))),
            ConsoleRefusal::ClassMismatch
        );
        assert_eq!(
            refusal(418, error("FROM_THE_FUTURE", json!({}))),
            ConsoleRefusal::Other
        );
    }

    #[test]
    fn the_right_withdrawn_in_between_is_a_failure_the_interface_reads_apart() {
        let body = error("FORBIDDEN_ROLE", json!({ "missing": "services" }));
        assert!(matches!(
            interpret(403, &body),
            Err(LinkFailure::MissingServicesRight)
        ));
    }
}

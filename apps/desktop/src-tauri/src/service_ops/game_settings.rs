//! Les réglages d'un serveur de jeu (HRT-109, ADR-0051, `BR-FICHE-037`, `040` à `049`, `051` à `054`) : deux lectures (la vue, le texte
//! collé) et l'enregistrement, un acte d'administration confirmé, couvert par l'élévation. Les requêtes sont
//! construites par la liaison (`LinkManager::{read,analyze,save}_game_settings`, chemins fixes, clé vérifiée puis
//! encodée) ; les refus se lisent par leur CODE, jamais par leur texte : aucune valeur reçue n'est répétée. L'agent ne
//! redémarre rien (BR-JEU-035) : redémarrer est un autre geste de l'interface (`service_restart`). Sans Tauri.

use hearth_link::domain::secret::Secret;
use hearth_link::domain::server::ServerId;
use hearth_link::{ActionOutcome, Analysis, LinkError, LinkManager};
use hearth_proto::api::game_settings::SaveSettingsResponse;
use hearth_proto::error::{ErrorBody, ErrorCode};
use serde_json::Value;

use super::game_settings_dto::{
    AnalyzeOutcomeDto, ImportViewDto, SaveOutcome, SaveRefusal, SaveSettingsRequestDto,
    SettingsReadDto, SettingsViewDto,
};
use crate::link_dto::LinkFailure;
use crate::reauth::wire::{Confirmation, confirmation_of};

/// Ce service n'a pas de réglages de jeu : `NOT_A_GAME_SERVICE` (pas né d'une fiche) ou `NOT_FOUND` (un agent plus
/// ancien, sans la route). Une clé mal formée (`SERVICE_NOT_FOUND`) n'en est PAS : c'est une erreur.
fn has_no_settings(error: &LinkError) -> bool {
    matches!(
        error,
        LinkError::Rejected(Some(ErrorCode::NotAGameService | ErrorCode::NotFound))
    )
}

/// Les réglages voulus et l'état du fichier (`GET /services/{key}/settings`). Un service qui n'est pas né d'une fiche
/// (ou un agent plus ancien) n'a pas de réglages de jeu : ce n'est pas une panne, c'est `Absent`.
pub async fn read(
    manager: &LinkManager,
    id: &ServerId,
    key: &str,
) -> Result<SettingsReadDto, LinkFailure> {
    match manager.read_game_settings(id, key).await {
        Ok(view) => Ok(SettingsReadDto::Found {
            view: Box::new(SettingsViewDto::from(&view)),
        }),
        Err(error) if has_no_settings(&error) => Ok(SettingsReadDto::Absent),
        Err(other) => Err(other.into()),
    }
}

/// Lit un texte collé (`POST /services/{key}/settings/analyze`) : une consultation, rien n'est écrit. Le texte peut
/// porter un mot de passe : il ne traverse que cet appel.
pub async fn analyze(
    manager: &LinkManager,
    id: &ServerId,
    key: &str,
    text: &Secret,
) -> Result<AnalyzeOutcomeDto, LinkFailure> {
    Ok(match manager.analyze_game_settings(id, key, text).await? {
        Analysis::Read(view) => AnalyzeOutcomeDto::Read {
            view: Box::new(ImportViewDto::from(&view)),
        },
        Analysis::Unreadable { reason, line } => AnalyzeOutcomeDto::Unreadable {
            reason: reason.and_then(|reason| stable_code(&reason)),
            line,
        },
    })
}

/// Enregistre des réglages. `admin_password` : le mot de passe de confirmation (`None` : l'élévation de cinq
/// minutes est ouverte ; l'agent est l'arbitre). Jamais rejoué : un lien tombé avant la réponse donne `Unknown`.
pub async fn save(
    manager: &LinkManager,
    id: &ServerId,
    key: &str,
    request: SaveSettingsRequestDto,
    admin_password: Option<&Secret>,
) -> Result<SaveOutcome, LinkFailure> {
    let request = request.into_request()?;
    match manager
        .save_game_settings(id, key, &request, admin_password)
        .await?
    {
        ActionOutcome::Completed { status, body, .. } => interpret_save(status, &body),
        ActionOutcome::ResultUnknown { id } => Ok(SaveOutcome::Unknown {
            op_id: id.as_str().to_owned(),
        }),
    }
}

/// Un code stable de l'agent (`reason`) : minuscules, chiffres et tiret bas, court. Tout autre texte n'est pas un
/// code et n'est jamais répété.
fn stable_code(text: &str) -> Option<String> {
    let valid = !text.is_empty()
        && text.len() <= 48
        && text
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    valid.then(|| text.to_owned())
}

/// Les identifiants d'un refus (`details.ids`) : seuls ceux qui ont la forme d'un identifiant de la fiche sont
/// rendus, au plus ce qu'un enregistrement peut porter.
fn ids_of(details: &Value) -> Vec<String> {
    details
        .get("ids")
        .and_then(Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(Value::as_str)
                .filter(|id| hearth_link::is_stable_id(id))
                .take(hearth_proto::api::game_settings::MAX_CHANGES)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// Au plus UN caractère : celui que l'agent cite pour un texte refusé. Jamais une chaîne plus longue.
fn single_char(details: &Value) -> Option<String> {
    let text = details.get("char").and_then(Value::as_str)?;
    let mut chars = text.chars();
    let first = chars.next()?;
    chars.next().is_none().then(|| first.to_string())
}

/// Un nom de compte tel que l'agent l'écrit : court et imprimable, sinon rien (jamais un texte libre répété).
fn actor_of(details: &Value) -> Option<String> {
    let text = details.get("by").and_then(Value::as_str)?;
    (!text.is_empty() && text.chars().count() <= 64 && !text.chars().any(char::is_control))
        .then(|| text.to_owned())
}

fn refused(refusal: SaveRefusal) -> Result<SaveOutcome, LinkFailure> {
    Ok(SaveOutcome::Refused { refusal })
}

/// La réponse de l'agent à `PUT /services/{key}/settings` : `200` avec la nouvelle révision, ou un refus typé par son
/// code. Les refus de la confirmation et le droit retiré se lisent comme pour tout acte de la coquille.
pub fn interpret_save(status: u16, body: &Value) -> Result<SaveOutcome, LinkFailure> {
    if (200..300).contains(&status) {
        let saved: SaveSettingsResponse =
            serde_json::from_value(body.clone()).map_err(|_| LinkFailure::NotAgent)?;
        return Ok(SaveOutcome::from(&saved));
    }
    let Ok(parsed) = serde_json::from_value::<ErrorBody>(body.clone()) else {
        return match status {
            403 => Err(LinkFailure::Forbidden),
            _ => refused(SaveRefusal::Other),
        };
    };
    if let Some(confirmation) = confirmation_of(&parsed)? {
        return refused(match confirmation {
            Confirmation::WrongPassword => SaveRefusal::WrongPassword,
            Confirmation::PasswordRequired => SaveRefusal::PasswordRequired,
            Confirmation::TooManyAttempts { retry_after_s } => {
                SaveRefusal::TooManyAttempts { retry_after_s }
            }
            Confirmation::Busy => SaveRefusal::Busy,
        });
    }
    let details = &parsed.error.details;
    match parsed.error.code.or_from_status(status) {
        // Le droit retiré entre-temps : l'interface le relit et le dit (BR-GRANT-010).
        ErrorCode::ForbiddenRole => Err(LinkFailure::forbidden(&parsed.error)),
        ErrorCode::SettingsConflict => refused(SaveRefusal::Conflict {
            by: actor_of(details),
            effect_mismatch: details.get("reason").and_then(Value::as_str) == Some("effect"),
        }),
        ErrorCode::SettingsFileChanged => refused(SaveRefusal::FileChanged {
            ids: ids_of(details),
        }),
        ErrorCode::SettingsFileUnreadable => refused(SaveRefusal::FileUnreadable {
            reason: details
                .get("reason")
                .and_then(Value::as_str)
                .and_then(stable_code),
            line: details
                .get("line")
                .and_then(Value::as_u64)
                .and_then(|line| u32::try_from(line).ok()),
        }),
        ErrorCode::SettingRefused => refused(SaveRefusal::SettingRefused {
            id: details
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| hearth_link::is_stable_id(id))
                .map(str::to_owned),
            reason: details
                .get("reason")
                .and_then(Value::as_str)
                .and_then(stable_code)
                .unwrap_or_else(|| "invalid".to_owned()),
            character: single_char(details),
        }),
        ErrorCode::RecreateNotAvailable => refused(SaveRefusal::RecreateNotAvailable {
            ids: ids_of(details),
        }),
        ErrorCode::OperationInProgress => refused(SaveRefusal::OperationInProgress),
        ErrorCode::ServiceNotFound => refused(SaveRefusal::ServiceNotFound),
        ErrorCode::ValidationError => refused(SaveRefusal::Invalid),
        _ => refused(SaveRefusal::Other),
    }
}

#[cfg(test)]
mod tests {
    use hearth_proto::api::game_settings::SettingEffect;
    use serde_json::json;

    use super::super::game_settings_dto::SettingEffectDto;
    use super::*;

    fn error(code: &str, details: Value) -> Value {
        json!({ "error": { "code": code, "message": "x", "details": details } })
    }

    fn read_refusal(status: u16, body: Value) -> SaveRefusal {
        match interpret_save(status, &body).unwrap() {
            SaveOutcome::Refused { refusal } => refusal,
            other => panic!("refus attendu, reçu {other:?}"),
        }
    }

    #[test]
    fn only_a_service_that_is_not_a_game_server_or_an_old_agent_has_no_settings() {
        for absent in [ErrorCode::NotAGameService, ErrorCode::NotFound] {
            assert!(has_no_settings(&LinkError::Rejected(Some(absent))));
        }
        // Une clé mal formée, un refus de droit, une panne : ce sont des erreurs, jamais « pas de réglages ».
        for error in [
            LinkError::Rejected(Some(ErrorCode::ServiceNotFound)),
            LinkError::Rejected(Some(ErrorCode::ForbiddenRole)),
            LinkError::Rejected(None),
            LinkError::Timeout,
        ] {
            assert!(!has_no_settings(&error), "{error:?}");
        }
    }

    #[test]
    fn a_saved_answer_carries_the_new_revision_the_heaviest_effect_and_what_was_adopted() {
        let body = json!({
            "rev": 4, "effect": "restart", "file_written": true, "changed": 2, "adopted": ["motd"]
        });
        assert_eq!(
            interpret_save(200, &body).unwrap(),
            SaveOutcome::Saved {
                rev: 4,
                effect: SettingEffectDto::Restart,
                file_written: true,
                changed: 2,
                adopted: vec!["motd".into()],
            }
        );
        assert_eq!(
            SettingEffect::from(SettingEffectDto::Recreate),
            SettingEffect::Recreate
        );
        assert!(matches!(
            interpret_save(200, &json!({ "nope": 1 })),
            Err(LinkFailure::NotAgent)
        ));
    }

    #[test]
    fn each_refusal_is_read_by_its_code_and_names_only_what_is_safe_to_name() {
        assert_eq!(
            read_refusal(422, error("WRONG_PASSWORD", json!(null))),
            SaveRefusal::WrongPassword
        );
        assert_eq!(
            read_refusal(
                409,
                error("SETTINGS_CONFLICT", json!({ "by": "marie", "at": "x" }))
            ),
            SaveRefusal::Conflict {
                by: Some("marie".into()),
                effect_mismatch: false
            }
        );
        assert_eq!(
            read_refusal(
                409,
                error("SETTINGS_CONFLICT", json!({ "reason": "effect" }))
            ),
            SaveRefusal::Conflict {
                by: None,
                effect_mismatch: true
            }
        );
        // Un identifiant qui n'en est pas un n'est jamais répété.
        assert_eq!(
            read_refusal(
                409,
                error(
                    "SETTINGS_FILE_CHANGED",
                    json!({ "ids": ["motd", "../etc", "Max Players"] })
                )
            ),
            SaveRefusal::FileChanged {
                ids: vec!["motd".into()]
            }
        );
        assert_eq!(
            read_refusal(
                422,
                error(
                    "SETTINGS_FILE_UNREADABLE",
                    json!({ "reason": "unclosed_tag", "line": 12 })
                )
            ),
            SaveRefusal::FileUnreadable {
                reason: Some("unclosed_tag".into()),
                line: Some(12)
            }
        );
        assert_eq!(
            read_refusal(
                422,
                error(
                    "SETTING_REFUSED",
                    json!({ "id": "world_name", "reason": "charset", "char": " " })
                )
            ),
            SaveRefusal::SettingRefused {
                id: Some("world_name".into()),
                reason: "charset".into(),
                character: Some(" ".into())
            }
        );
        // Jamais plus d'UN caractère, jamais un identifiant inconnu de la fiche.
        assert_eq!(
            read_refusal(
                422,
                error(
                    "SETTING_REFUSED",
                    json!({ "id": "Secret value!", "reason": "Bounds; DROP", "char": "abc" })
                )
            ),
            SaveRefusal::SettingRefused {
                id: None,
                reason: "invalid".into(),
                character: None
            }
        );
        assert_eq!(
            read_refusal(
                409,
                error("RECREATE_NOT_AVAILABLE", json!({ "ids": ["java_args"] }))
            ),
            SaveRefusal::RecreateNotAvailable {
                ids: vec!["java_args".into()]
            }
        );
        assert_eq!(
            read_refusal(409, error("OPERATION_IN_PROGRESS", json!({}))),
            SaveRefusal::OperationInProgress
        );
        assert_eq!(
            read_refusal(418, error("FROM_THE_FUTURE", json!({}))),
            SaveRefusal::Other
        );
    }

    #[test]
    fn the_right_withdrawn_in_between_is_a_failure_the_interface_reads_apart() {
        let body = error("FORBIDDEN_ROLE", json!({ "missing": "services" }));
        assert!(matches!(
            interpret_save(403, &body),
            Err(LinkFailure::MissingServicesRight)
        ));
    }

    #[test]
    fn a_request_without_a_file_stamp_never_leaves_and_a_debug_never_shows_a_value() {
        use super::super::game_settings_dto::{
            ChangeActionDto, SaveOriginDto, SettingChangeDto, SettingEffectDto as Effect,
        };
        let request = |stamp: &str| SaveSettingsRequestDto {
            base_rev: 1,
            file_stamp: stamp.into(),
            origin: SaveOriginDto::Form,
            effect: Effect::Restart,
            changes: vec![SettingChangeDto {
                id: "password".into(),
                action: ChangeActionDto::Set,
                value: Some("SENTINEL-9f3a".into()),
            }],
            others: Vec::new(),
            drift_choices: Vec::new(),
            import_text: Some("password=SENTINEL-9f3a".into()),
        };
        assert!(request("").into_request().is_err());
        let ok = request("ab12");
        assert!(!format!("{ok:?}").contains("SENTINEL"));
        let sent = ok.into_request().unwrap();
        assert_eq!(sent.file_stamp, "ab12");
        assert!(!format!("{sent:?}").contains("SENTINEL"));
    }
}

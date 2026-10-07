//! Cas d'usage de la confirmation des actes. Sans Tauri : les commandes ne font que l'appeler. Les règles
//! (mot de passe, preuve de la clé, élévation de 5 minutes) sont à l'AGENT ; rien ici ne décide ce qui
//! passe. Le mot de passe est enveloppé dans un `Secret` dès l'entrée, effacé à la libération.

use hearth_link::domain::secret::Secret;
use hearth_link::domain::server::ServerId;
use hearth_link::ports::transport::Method;
use hearth_link::{ActionOutcome, ActionRequest, LinkManager};
use hearth_proto::admin_act::{AdminAct, covered_by_elevation};
use hearth_proto::api::accounts::RoleName;
use hearth_proto::api::reauth::{AdminReauthInfo, ReauthMode};
use hearth_proto::error::ErrorBody;
use serde_json::{Value, json};

use super::dto::{
    AdminActKindDto, ReauthModeDto, ReauthSettingOutcome, ReauthSettingRefusal, ReauthStateDto,
};
use super::wire::{Confirmation, confirmation_of};
use crate::link_dto::{LinkFailure, RoleDto};

const SETTING_PATH: &str = "/me/reauth";

/// L'identifiant d'un serveur du carnet reçu de l'interface.
pub fn server(text: &str) -> Result<ServerId, LinkFailure> {
    ServerId::parse(text).map_err(|_| LinkFailure::UnknownServer)
}

/// Ce que l'agent annonce, lu à l'instant (une lecture, sans suivi) : capacité, réglage, élévation
/// restante, et la présence d'une clé au coffre. Hors « Connecté » : `NotConnected`.
pub async fn state(manager: &LinkManager, id: &ServerId) -> Result<ReauthStateDto, LinkFailure> {
    Ok(manager.admin_reauth(id).await?.into())
}

/// L'élévation couvre-t-elle cet acte ? La règle est celle de l'agent (`hearth_proto`, source unique).
/// Un rôle absent pour un acte qui le porte vaut « Administrateur » : le plus strict.
pub fn covers(kind: AdminActKindDto, role: Option<RoleDto>) -> bool {
    let role = match role {
        Some(RoleDto::Readonly) => RoleName::Readonly,
        Some(RoleDto::Admin) | None => RoleName::Admin,
    };
    let act = match kind {
        AdminActKindDto::AccountCreate => AdminAct::AccountCreate { username: "", role },
        AdminActKindDto::AccountRole => AdminAct::AccountRole { target: "", role },
        AdminActKindDto::AccountPassword => AdminAct::AccountPassword { target: "" },
        AdminActKindDto::AccountDelete => AdminAct::AccountDelete { target: "" },
        AdminActKindDto::SessionsRevoke => AdminAct::SessionsRevoke { target: "" },
        AdminActKindDto::AgentUpdate => AdminAct::AgentUpdate {
            version: "",
            sha256: "",
        },
        AdminActKindDto::AttackModeEnable => AdminAct::AttackMode { enable: true },
        AdminActKindDto::AttackModeDisable => AdminAct::AttackMode { enable: false },
        AdminActKindDto::AccountPasswordOwn => AdminAct::AccountPasswordOwn,
        AdminActKindDto::ReauthSetting => AdminAct::ReauthSetting {
            mode: ReauthMode::Each,
        },
    };
    covered_by_elevation(&act)
}

/// Change le réglage « Demander mon mot de passe » : un acte d'administration comme les autres (mot de
/// passe ET preuve de la clé de ce PC), jamais couvert par l'élévation.
pub async fn set_setting(
    manager: &LinkManager,
    id: &ServerId,
    mode: ReauthModeDto,
    password: &Secret,
) -> Result<ReauthSettingOutcome, LinkFailure> {
    let mode_name = ReauthMode::from(mode).as_str();
    let action = ActionRequest {
        method: Method::Put,
        path: SETTING_PATH.to_owned(),
        body: Some(json!({ "password": mode_name })),
    };
    match manager.execute_act(id, action, Some(password)).await? {
        ActionOutcome::Completed { status, body, .. } => interpret(status, &body),
        ActionOutcome::ResultUnknown { id } => Ok(ReauthSettingOutcome::Unknown {
            op_id: id.as_str().to_owned(),
        }),
    }
}

/// La réponse de l'agent à `PUT /me/reauth` : `200` avec l'objet `admin_reauth`, ou un refus typé.
pub fn interpret(status: u16, body: &Value) -> Result<ReauthSettingOutcome, LinkFailure> {
    if (200..300).contains(&status) {
        return serde_json::from_value::<AdminReauthInfo>(body.clone())
            .map(|info| ReauthSettingOutcome::Done {
                mode: info.password.into(),
            })
            .map_err(|_| LinkFailure::NotAgent);
    }
    let refused = |refusal| Ok(ReauthSettingOutcome::Refused { refusal });
    let Ok(error) = serde_json::from_value::<ErrorBody>(body.clone()) else {
        return refused(match status {
            404 | 405 => ReauthSettingRefusal::Unsupported,
            _ => ReauthSettingRefusal::Other,
        });
    };
    if let Some(confirmation) = confirmation_of(&error)? {
        return refused(match confirmation {
            Confirmation::WrongPassword => ReauthSettingRefusal::WrongPassword,
            Confirmation::PasswordRequired => ReauthSettingRefusal::PasswordRequired,
            Confirmation::TooManyAttempts { retry_after_s } => {
                ReauthSettingRefusal::TooManyAttempts { retry_after_s }
            }
            Confirmation::Busy => ReauthSettingRefusal::Busy,
        });
    }
    use hearth_proto::error::ErrorCode;
    match error.error.code {
        ErrorCode::ForbiddenRole => Err(LinkFailure::Forbidden),
        ErrorCode::NotFound => refused(ReauthSettingRefusal::Unsupported),
        ErrorCode::Unauthenticated | ErrorCode::SessionExpired => {
            refused(ReauthSettingRefusal::SessionEnded)
        }
        ErrorCode::SessionRevoked => refused(ReauthSettingRefusal::SessionRevoked),
        _ => refused(ReauthSettingRefusal::Other),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn the_elevation_covers_what_the_agent_covers_and_nothing_else() {
        use AdminActKindDto as Kind;
        for (kind, role, covered) in [
            (Kind::AccountCreate, Some(RoleDto::Readonly), true),
            (Kind::AccountCreate, Some(RoleDto::Admin), false),
            (Kind::AccountCreate, None, false),
            (Kind::AccountRole, Some(RoleDto::Readonly), true),
            (Kind::AccountRole, Some(RoleDto::Admin), false),
            (Kind::AccountRole, None, false),
            (Kind::AccountDelete, None, true),
            (Kind::SessionsRevoke, None, true),
            (Kind::AccountPassword, None, false),
            (Kind::AgentUpdate, None, false),
            (Kind::AttackModeEnable, None, false),
            (Kind::AttackModeDisable, None, false),
            (Kind::AccountPasswordOwn, None, false),
            (Kind::ReauthSetting, None, false),
        ] {
            assert_eq!(covers(kind, role), covered, "{kind:?} {role:?}");
        }
    }

    #[test]
    fn the_setting_outcome_is_read_by_code_and_a_missing_key_or_an_old_client_are_failures() {
        let error = |code: &str, details: Value| json!({ "error": { "code": code, "message": "x", "details": details } });
        let refused = |status, body: Value| match interpret(status, &body).unwrap() {
            ReauthSettingOutcome::Refused { refusal } => refusal,
            other => panic!("refus attendu, reçu {other:?}"),
        };
        assert_eq!(
            refused(422, error("WRONG_PASSWORD", json!(null))),
            ReauthSettingRefusal::WrongPassword
        );
        assert_eq!(
            refused(
                429,
                error("TOO_MANY_ATTEMPTS", json!({ "retry_after_s": 30 }))
            ),
            ReauthSettingRefusal::TooManyAttempts { retry_after_s: 30 }
        );
        assert_eq!(
            refused(404, error("NOT_FOUND", json!(null))),
            ReauthSettingRefusal::Unsupported
        );
        assert_eq!(
            interpret(
                409,
                &error("POST_NOT_RECOGNIZED", json!({ "reason": "proof_invalid" }))
            ),
            Err(LinkFailure::NotRecognized)
        );
        assert_eq!(
            interpret(
                426,
                &error(
                    "INCOMPATIBLE_VERSION",
                    json!({ "upgrade": "client", "reason": "reauth_required" })
                )
            ),
            Err(LinkFailure::IncompatibleClient)
        );
        assert_eq!(
            interpret(403, &error("FORBIDDEN_ROLE", json!(null))),
            Err(LinkFailure::Forbidden)
        );
        assert_eq!(
            interpret(
                200,
                &json!({
                    "required": true,
                    "factors": ["password", "device_key"],
                    "password": "each",
                    "elevated_for_s": 0
                })
            ),
            Ok(ReauthSettingOutcome::Done {
                mode: ReauthModeDto::Each
            })
        );
    }
}

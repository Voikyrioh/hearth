//! Cas d'usage de la confirmation des actes. Sans Tauri : les commandes ne font que l'appeler. Les règles
//! (mot de passe, preuve de la clé, élévation de 5 minutes) sont à l'AGENT ; rien ici ne décide ce qui
//! passe. Le mot de passe est enveloppé dans un `Secret` dès l'entrée, effacé à la libération.

use hearth_link::domain::secret::Secret;
use hearth_link::domain::server::ServerId;
use hearth_link::ports::transport::Method;
use hearth_link::{ActionOutcome, ActionRequest, LinkManager};
use hearth_proto::admin_act::{AdminAct, password_need_announced};
use hearth_proto::api::accounts::RoleName;
use hearth_proto::api::reauth::{AdminReauthInfo, ReauthMode};
use hearth_proto::error::ErrorBody;
use serde_json::{Value, json};

use super::dto::{
    AdminActKindDto, PasswordNeedDto, ReauthModeDto, ReauthSettingOutcome, ReauthSettingRefusal,
    ReauthStateDto, TrustLevelDto,
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

/// Que demande l'agent du mot de passe pour cet acte, dans cet état de confiance ? La règle est celle de
/// l'agent (`hearth_proto::admin_act::password_need`, source unique) : la fenêtre ne la recopie pas. Un état
/// absent (agent d'avant HRT-194) garde la règle d'avant : l'élévation seule. Un état inconnu vaut le mode
/// attaque. Un rôle absent pour un acte qui le porte vaut « Administrateur » : le plus strict.
pub fn password_need_of(
    kind: AdminActKindDto,
    role: Option<RoleDto>,
    trust: Option<TrustLevelDto>,
) -> PasswordNeedDto {
    let act = act_of(kind, role);
    password_need_announced(&act, trust.map(Into::into)).into()
}

/// Un acte du genre demandé, sans cible ni secret : de quoi interroger la règle.
fn act_of(kind: AdminActKindDto, role: Option<RoleDto>) -> AdminAct<'static> {
    let role = match role {
        Some(RoleDto::Readonly) => RoleName::Readonly,
        // Un rôle inconnu (agent plus récent) vaut « Administrateur » : le plus strict.
        Some(RoleDto::Admin | RoleDto::Unknown) | None => RoleName::Admin,
    };
    match kind {
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
        AdminActKindDto::OperationDecide => AdminAct::OperationDecide {
            target: "",
            choice: hearth_proto::api::service_ops::OperationChoice::Continue,
        },
        // Déployer et annuler : le genre seul décide (la table de l'élévation), ni la cible ni le corps ne comptent.
        AdminActKindDto::ServiceDeploy => AdminAct::ServiceDeploy {
            name: "".into(),
            image: "".into(),
            digest: [0; hearth_proto::admin_act::FINGERPRINT_LEN],
            env_count: 0,
            command: false,
        },
        AdminActKindDto::OperationCancel => AdminAct::OperationCancel { target: "" },
        AdminActKindDto::ServiceStart => AdminAct::ServiceStart { target: "".into() },
        AdminActKindDto::ServiceStop => AdminAct::ServiceStop { target: "".into() },
        AdminActKindDto::ServiceRestart => AdminAct::ServiceRestart { target: "".into() },
    }
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
    // Code inconnu (agent plus récent) : le sens du statut HTTP (BR-COMPAT-001 point 8).
    match error.error.code.or_from_status(status) {
        ErrorCode::ForbiddenRole => Err(LinkFailure::forbidden(&error.error)),
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
    fn the_password_need_is_the_grid_of_the_agent_for_piloting_and_the_elevation_table_for_the_rest()
     {
        use AdminActKindDto as Kind;
        use PasswordNeedDto::{No, UnlessElevated, Yes};
        use TrustLevelDto::{Alert, Attack, Normal, Unknown};
        // Hors pilotage : la table de l'élévation, quel que soit l'état.
        for trust in [Some(Normal), Some(Alert), Some(Attack), Some(Unknown), None] {
            for (kind, role, need) in [
                (Kind::AccountCreate, Some(RoleDto::Readonly), UnlessElevated),
                (Kind::AccountCreate, Some(RoleDto::Admin), Yes),
                (Kind::AccountCreate, None, Yes),
                (Kind::AccountRole, Some(RoleDto::Readonly), UnlessElevated),
                (Kind::AccountRole, Some(RoleDto::Admin), Yes),
                (Kind::AccountRole, None, Yes),
                (Kind::AccountDelete, None, UnlessElevated),
                (Kind::SessionsRevoke, None, UnlessElevated),
                (Kind::AccountPassword, None, Yes),
                (Kind::AgentUpdate, None, Yes),
                (Kind::AttackModeEnable, None, Yes),
                (Kind::AttackModeDisable, None, Yes),
                (Kind::AccountPasswordOwn, None, Yes),
                (Kind::ReauthSetting, None, Yes),
                // Déployer : jamais couvert par l'élévation (BR-DEPLOY-030). Annuler : la table de l'élévation.
                (Kind::ServiceDeploy, None, Yes),
                (Kind::OperationCancel, None, UnlessElevated),
            ] {
                assert_eq!(
                    password_need_of(kind, role, trust),
                    need,
                    "{kind:?} {role:?} {trust:?}"
                );
            }
        }
        // Pilotage : la grille de confiance de l'agent.
        for kind in [
            Kind::ServiceStart,
            Kind::ServiceStop,
            Kind::ServiceRestart,
            Kind::OperationDecide,
        ] {
            assert_eq!(password_need_of(kind, None, Some(Normal)), No, "{kind:?}");
            assert_eq!(
                password_need_of(kind, None, Some(Alert)),
                UnlessElevated,
                "{kind:?}"
            );
            assert_eq!(password_need_of(kind, None, Some(Attack)), Yes, "{kind:?}");
            // Un état inconnu vaut le mode attaque.
            assert_eq!(password_need_of(kind, None, Some(Unknown)), Yes, "{kind:?}");
        }
        // Agent d'avant HRT-194 (état absent) : la règle d'avant, jamais « normal » par défaut.
        for kind in [Kind::ServiceStart, Kind::ServiceStop, Kind::ServiceRestart] {
            assert_eq!(password_need_of(kind, None, None), UnlessElevated);
        }
        assert_eq!(password_need_of(Kind::OperationDecide, None, None), Yes);
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

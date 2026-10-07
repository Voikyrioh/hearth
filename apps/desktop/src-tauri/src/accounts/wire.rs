//! Les requêtes de comptes et la lecture de leurs réponses. Pur : sans E/S, sans Tauri. Chaque
//! action a SA requête, construite ici avec la méthode et le chemin du contrat de l'agent
//! (`docs/open-api/accounts.md`) : rien de tout cela ne vient de l'interface (ADR-0016).

use hearth_link::ActionRequest;
use hearth_link::ports::transport::Method;
use hearth_proto::account_rules::{
    InputCheck, PasswordRule, check_input, check_username, unmet_password_rules,
};
use hearth_proto::api::accounts::{
    AccountItem, ChangeOwnPasswordRequest, ChangeRoleRequest, CreateAccountRequest,
    DeleteAccountRequest, RoleName, SessionsClosedResponse, SetPasswordRequest,
};
use hearth_proto::error::{ErrorBody, ErrorCode};
use serde::Serialize;
use serde_json::Value;

use super::dto::{AccountDto, AccountOutcome, AccountRefusal, PasswordRuleDto, UsernameProblemDto};
use crate::link_dto::{InvalidField, LinkFailure, RoleDto};

/// Plus long qu'un identifiant de compte (un ULID : 26 caractères).
const ACCOUNT_ID_MAX_LEN: usize = 64;

/// Ce que le succès d'une action rend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expect {
    /// `201` : le compte créé.
    Account,
    /// `200 { sessions_closed }`.
    Closed,
    /// `204`, sans corps.
    Nothing,
}

/// Une action prête à partir : la requête et ce qu'on attend en retour.
#[derive(Debug, Clone, PartialEq)]
pub struct Planned {
    pub request: ActionRequest,
    pub expect: Expect,
}

/// Un identifiant de compte de l'agent, sûr à placer dans un chemin : lettres et chiffres, rien
/// d'autre (jamais `/`, `..`, `?`).
pub fn account_id(text: &str) -> Result<&str, LinkFailure> {
    if !text.is_empty()
        && text.len() <= ACCOUNT_ID_MAX_LEN
        && text.bytes().all(|b| b.is_ascii_alphanumeric())
    {
        Ok(text)
    } else {
        Err(LinkFailure::InvalidInput {
            field: InvalidField::Other,
        })
    }
}

fn role_name(role: RoleDto) -> RoleName {
    match role {
        RoleDto::Admin => RoleName::Admin,
        RoleDto::Readonly => RoleName::Readonly,
    }
}

fn json(value: &impl Serialize) -> Result<Value, LinkFailure> {
    serde_json::to_value(value).map_err(|_| LinkFailure::Internal)
}

/// Pourquoi une action ne part pas : refusée par la validation locale (l'interface le montre comme
/// un refus de l'agent), ou impossible à construire (échec typé).
#[derive(Debug, Clone, PartialEq)]
pub enum Stop {
    Refused(AccountRefusal),
    Failed(LinkFailure),
}

impl From<LinkFailure> for Stop {
    fn from(failure: LinkFailure) -> Self {
        Self::Failed(failure)
    }
}

impl From<AccountRefusal> for Stop {
    fn from(refusal: AccountRefusal) -> Self {
        Self::Refused(refusal)
    }
}

/// La saisie refusée AVANT tout envoi : l'agent reste l'arbitre, ceci ne fait qu'épargner un
/// aller-retour (et un mot de passe voué à l'échec).
fn refusal_of(check: InputCheck) -> Option<AccountRefusal> {
    if let Some(problem) = check.username {
        return Some(AccountRefusal::InvalidUsername {
            problem: Some(UsernameProblemDto::from(problem)),
        });
    }
    weak(check.password)
}

fn weak(rules: Vec<PasswordRule>) -> Option<AccountRefusal> {
    (!rules.is_empty()).then(|| AccountRefusal::WeakPassword {
        rules: rules.into_iter().map(PasswordRuleDto::from).collect(),
    })
}

/// `POST /accounts`.
pub fn create(username: &str, password: &str, role: RoleDto) -> Result<Planned, Stop> {
    if let Some(refusal) = refusal_of(check_input(username, password)) {
        return Err(refusal.into());
    }
    let body = CreateAccountRequest {
        username: username.to_owned(),
        password: password.to_owned(),
        role: role_name(role),
    };
    Ok(Planned {
        request: ActionRequest {
            method: Method::Post,
            path: "/accounts".into(),
            body: Some(json(&body)?),
        },
        expect: Expect::Account,
    })
}

/// `PATCH /accounts/{id}`.
pub fn change_role(account: &str, role: RoleDto) -> Result<Planned, LinkFailure> {
    let account = account_id(account)?;
    Ok(Planned {
        request: ActionRequest {
            method: Method::Patch,
            path: format!("/accounts/{account}"),
            body: Some(json(&ChangeRoleRequest {
                role: role_name(role),
            })?),
        },
        expect: Expect::Nothing,
    })
}

/// `PUT /accounts/{id}/password`. La règle « ne contient pas l'identifiant » est jugée par l'agent,
/// qui lit le compte lui-même, et par la saisie en direct (`check_account_input`) : ici seules les
/// règles qui ne dépendent pas de l'identifiant sont contrôlées avant l'envoi.
pub fn set_password(account: &str, password: &str) -> Result<Planned, Stop> {
    let account = account_id(account)?;
    if let Some(refusal) = weak(unmet_password_rules(password, "")) {
        return Err(refusal.into());
    }
    Ok(Planned {
        request: ActionRequest {
            method: Method::Put,
            path: format!("/accounts/{account}/password"),
            body: Some(json(&SetPasswordRequest {
                password: password.to_owned(),
            })?),
        },
        expect: Expect::Closed,
    })
}

/// `PUT /me/password` : `username` est celui du compte connecté (connu du carnet).
pub fn change_own_password(username: &str, current: &str, password: &str) -> Result<Planned, Stop> {
    let normalized = check_username(username).unwrap_or_default();
    if let Some(refusal) = weak(unmet_password_rules(password, &normalized)) {
        return Err(refusal.into());
    }
    Ok(Planned {
        request: ActionRequest {
            method: Method::Put,
            path: "/me/password".into(),
            body: Some(json(&ChangeOwnPasswordRequest {
                current: current.to_owned(),
                password: password.to_owned(),
                // Le choix « garder ce poste reconnu » est câblé par HRT-26.
                keep_address: false,
            })?),
        },
        expect: Expect::Closed,
    })
}

/// `DELETE /accounts/{id}/sessions`.
pub fn close_sessions(account: &str) -> Result<Planned, LinkFailure> {
    let account = account_id(account)?;
    Ok(Planned {
        request: ActionRequest {
            method: Method::Delete,
            path: format!("/accounts/{account}/sessions"),
            body: None,
        },
        expect: Expect::Closed,
    })
}

/// `DELETE /accounts/{id}` ; `confirmation` : l'identifiant retapé quand on supprime son propre
/// compte (BR-ACCT-012), que l'agent compare.
pub fn delete(account: &str, confirmation: Option<String>) -> Result<Planned, LinkFailure> {
    let account = account_id(account)?;
    let body = confirmation.map(|confirmation| DeleteAccountRequest {
        confirmation: Some(confirmation),
    });
    Ok(Planned {
        request: ActionRequest {
            method: Method::Delete,
            path: format!("/accounts/{account}"),
            body: body.as_ref().map(json).transpose()?,
        },
        expect: Expect::Closed,
    })
}

/// Ce que dit un refus de l'agent : le code stable de l'erreur, jamais son texte.
/// Le refus de rôle est `LinkFailure::Forbidden` (une seule façon de le dire dans l'interface).
pub fn refusal_from_error(status: u16, body: &Value) -> Result<AccountRefusal, LinkFailure> {
    let Ok(error) = serde_json::from_value::<ErrorBody>(body.clone()) else {
        return match status {
            403 => Err(LinkFailure::Forbidden),
            404 => Ok(AccountRefusal::NotFound),
            _ => Ok(AccountRefusal::Other),
        };
    };
    let error = error.error;
    Ok(match error.code {
        ErrorCode::ForbiddenRole => return Err(LinkFailure::Forbidden),
        ErrorCode::UsernameTaken => AccountRefusal::UsernameTaken,
        ErrorCode::WrongPassword => AccountRefusal::WrongPassword,
        ErrorCode::LastAdmin => AccountRefusal::LastAdmin,
        ErrorCode::NotFound => AccountRefusal::NotFound,
        ErrorCode::Conflict => AccountRefusal::Conflict,
        ErrorCode::Busy => AccountRefusal::Busy,
        ErrorCode::Unauthenticated | ErrorCode::SessionExpired => AccountRefusal::SessionEnded,
        ErrorCode::SessionRevoked => AccountRefusal::SessionRevoked,
        ErrorCode::WeakPassword => {
            let rules = error.details["rules"]
                .as_array()
                .map(|rules| {
                    rules
                        .iter()
                        .filter_map(Value::as_str)
                        .filter_map(PasswordRule::from_code)
                        .map(PasswordRuleDto::from)
                        .collect()
                })
                .unwrap_or_default();
            AccountRefusal::WeakPassword { rules }
        }
        ErrorCode::ValidationError => match error.details["field"].as_str() {
            Some("username") => AccountRefusal::InvalidUsername { problem: None },
            Some("confirmation") => AccountRefusal::ConfirmationMismatch,
            _ => AccountRefusal::Other,
        },
        _ => AccountRefusal::Other,
    })
}

/// La réponse de l'agent à une action : succès (2xx, forme attendue) ou refus typé.
pub fn interpret(expect: Expect, status: u16, body: &Value) -> Result<AccountOutcome, LinkFailure> {
    if !(200..300).contains(&status) {
        return Ok(AccountOutcome::Refused {
            refusal: refusal_from_error(status, body)?,
        });
    }
    match expect {
        Expect::Account => {
            let item: AccountItem =
                serde_json::from_value(body.clone()).map_err(|_| LinkFailure::NotAgent)?;
            Ok(AccountOutcome::Done {
                account: Some(AccountDto::from(item)),
                sessions_closed: 0,
            })
        }
        Expect::Closed => {
            let closed: SessionsClosedResponse =
                serde_json::from_value(body.clone()).map_err(|_| LinkFailure::NotAgent)?;
            Ok(AccountOutcome::Done {
                account: None,
                sessions_closed: u32::try_from(closed.sessions_closed).unwrap_or(u32::MAX),
            })
        }
        Expect::Nothing => Ok(AccountOutcome::Done {
            account: None,
            sessions_closed: 0,
        }),
    }
}

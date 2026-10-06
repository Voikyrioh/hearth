//! HRT-13 : les requêtes de comptes et la lecture des réponses, sans réseau. Chaque action a sa
//! méthode et son chemin construits ICI (ADR-0016) ; un identifiant de compte ne sort jamais du
//! chemin ; un mot de passe ne figure dans aucun `Debug` ; les refus de l'agent sont rendus par
//! code stable, jamais par texte.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use hearth_desktop_lib::accounts::dto::{
    AccountOutcome, AccountRefusal, PasswordRuleDto, UsernameProblemDto,
};
use hearth_desktop_lib::accounts::wire::{self, Expect, Stop};
use hearth_desktop_lib::link_dto::{InvalidField, LinkFailure, RoleDto};
use hearth_link::ports::transport::Method;
use serde_json::{Value, json};

const GOOD: &str = "Correct-Horse-9";

fn planned(result: Result<wire::Planned, Stop>) -> wire::Planned {
    match result {
        Ok(planned) => planned,
        Err(stop) => panic!("action attendue, reçu {stop:?}"),
    }
}

#[test]
fn each_action_builds_its_own_method_path_and_body() {
    let create = planned(wire::create("marie", GOOD, RoleDto::Readonly));
    assert_eq!(create.request.method, Method::Post);
    assert_eq!(create.request.path, "/accounts");
    assert_eq!(
        create.request.body.unwrap(),
        json!({ "username": "marie", "password": GOOD, "role": "readonly" })
    );
    assert_eq!(create.expect, Expect::Account);

    let role = wire::change_role("01J9ZY0G3Q8M2K6W4T7V5N1B9D", RoleDto::Admin).unwrap();
    assert_eq!(role.request.method, Method::Patch);
    assert_eq!(role.request.path, "/accounts/01J9ZY0G3Q8M2K6W4T7V5N1B9D");
    assert_eq!(role.request.body.unwrap(), json!({ "role": "admin" }));
    assert_eq!(role.expect, Expect::Nothing);

    let password = planned(wire::set_password("ABC123", GOOD));
    assert_eq!(password.request.method, Method::Put);
    assert_eq!(password.request.path, "/accounts/ABC123/password");
    assert_eq!(password.request.body.unwrap(), json!({ "password": GOOD }));
    assert_eq!(password.expect, Expect::Closed);

    let own = planned(wire::change_own_password("marie", "Old-Pass-12345", GOOD));
    assert_eq!(own.request.method, Method::Put);
    assert_eq!(own.request.path, "/me/password");
    assert_eq!(
        own.request.body.unwrap(),
        json!({ "current": "Old-Pass-12345", "password": GOOD })
    );

    let sessions = wire::close_sessions("ABC123").unwrap();
    assert_eq!(sessions.request.method, Method::Delete);
    assert_eq!(sessions.request.path, "/accounts/ABC123/sessions");
    assert_eq!(sessions.request.body, None);

    let plain = wire::delete("ABC123", None).unwrap();
    assert_eq!(plain.request.method, Method::Delete);
    assert_eq!(plain.request.path, "/accounts/ABC123");
    assert_eq!(plain.request.body, None);
    let own_account = wire::delete("ABC123", Some("marie".into())).unwrap();
    assert_eq!(
        own_account.request.body.unwrap(),
        json!({ "confirmation": "marie" })
    );
}

#[test]
fn an_account_identifier_can_never_leave_the_path() {
    for bad in [
        "",
        "..",
        "a/b",
        "a/../b",
        "a?x=1",
        "a#b",
        "a b",
        "%2e%2e",
        "é",
        &"A".repeat(65),
    ] {
        let expected = LinkFailure::InvalidInput {
            field: InvalidField::Other,
        };
        assert_eq!(
            wire::change_role(bad, RoleDto::Admin).unwrap_err(),
            expected,
            "{bad:?}"
        );
        assert_eq!(wire::close_sessions(bad).unwrap_err(), expected, "{bad:?}");
        assert_eq!(wire::delete(bad, None).unwrap_err(), expected, "{bad:?}");
        assert!(
            matches!(
                wire::set_password(bad, GOOD),
                Err(Stop::Failed(f)) if f == expected
            ),
            "{bad:?}"
        );
    }
}

#[test]
fn an_invalid_input_is_refused_locally_before_anything_is_sent() {
    match wire::create("a b", GOOD, RoleDto::Admin) {
        Err(Stop::Refused(AccountRefusal::InvalidUsername { problem })) => {
            assert_eq!(problem, Some(UsernameProblemDto::InvalidChars));
        }
        other => panic!("{other:?}"),
    }
    match wire::create("marie", "abc", RoleDto::Admin) {
        Err(Stop::Refused(AccountRefusal::WeakPassword { rules })) => assert_eq!(
            rules,
            [
                PasswordRuleDto::MinLength,
                PasswordRuleDto::Digit,
                PasswordRuleDto::Uppercase
            ]
        ),
        other => panic!("{other:?}"),
    }
    // « ne contient pas l'identifiant » : l'agent le juge (il lit le compte lui-même), pas la coquille.
    assert!(wire::set_password("ABC", "xxPaulxxxxxx12A").is_ok());
    match wire::set_password("ABC", "abc") {
        Err(Stop::Refused(AccountRefusal::WeakPassword { rules })) => assert_eq!(
            rules,
            [
                PasswordRuleDto::MinLength,
                PasswordRuleDto::Digit,
                PasswordRuleDto::Uppercase
            ]
        ),
        other => panic!("{other:?}"),
    }
    match wire::change_own_password("marie", "x", "") {
        Err(Stop::Refused(AccountRefusal::WeakPassword { rules })) => {
            assert_eq!(rules, [PasswordRuleDto::Required]);
        }
        other => panic!("{other:?}"),
    }
}

fn error(status_code: &str, details: Value) -> Value {
    json!({ "error": { "code": status_code, "message": "peu importe", "details": details } })
}

#[test]
fn every_refusal_of_the_agent_is_told_by_its_code() {
    let cases = [
        (409, "USERNAME_TAKEN", AccountRefusal::UsernameTaken),
        (422, "WRONG_PASSWORD", AccountRefusal::WrongPassword),
        (409, "LAST_ADMIN", AccountRefusal::LastAdmin),
        (404, "NOT_FOUND", AccountRefusal::NotFound),
        (409, "CONFLICT", AccountRefusal::Conflict),
        (503, "BUSY", AccountRefusal::Busy),
        (401, "SESSION_REVOKED", AccountRefusal::SessionRevoked),
        (401, "SESSION_EXPIRED", AccountRefusal::SessionEnded),
        (500, "INTERNAL_ERROR", AccountRefusal::Other),
    ];
    for (status, code, expected) in cases {
        let outcome = wire::interpret(Expect::Closed, status, &error(code, json!({}))).unwrap();
        assert_eq!(
            outcome,
            AccountOutcome::Refused { refusal: expected },
            "{code}"
        );
    }
    let weak = error(
        "WEAK_PASSWORD",
        json!({ "rules": ["min_length", "digit", "?"] }),
    );
    assert_eq!(
        wire::interpret(Expect::Closed, 422, &weak).unwrap(),
        AccountOutcome::Refused {
            refusal: AccountRefusal::WeakPassword {
                rules: vec![PasswordRuleDto::MinLength, PasswordRuleDto::Digit]
            }
        }
    );
    let confirmation = error("VALIDATION_ERROR", json!({ "field": "confirmation" }));
    assert_eq!(
        wire::interpret(Expect::Closed, 422, &confirmation).unwrap(),
        AccountOutcome::Refused {
            refusal: AccountRefusal::ConfirmationMismatch
        }
    );
    let username = error("VALIDATION_ERROR", json!({ "field": "username" }));
    assert_eq!(
        wire::interpret(Expect::Account, 422, &username).unwrap(),
        AccountOutcome::Refused {
            refusal: AccountRefusal::InvalidUsername { problem: None }
        }
    );
    // Un corps qui n'est pas au format d'erreur : on se fie au statut, jamais au texte. Le refus de
    // rôle est `LinkFailure::Forbidden`, comme pour la lecture (une seule façon de le dire).
    assert_eq!(
        wire::interpret(Expect::Closed, 403, &json!("<html>")).unwrap_err(),
        LinkFailure::Forbidden
    );
    assert_eq!(
        wire::interpret(Expect::Closed, 403, &error("FORBIDDEN_ROLE", json!({}))).unwrap_err(),
        LinkFailure::Forbidden
    );
}

#[test]
fn a_success_is_read_in_the_shape_the_action_expects() {
    let account = json!({
        "id": "A1", "username": "marie", "role": "admin",
        "created_at": "2026-10-04T10:30:15.250Z", "last_login_at": null, "sessions_open": 0
    });
    match wire::interpret(Expect::Account, 201, &account).unwrap() {
        AccountOutcome::Done {
            account: Some(account),
            sessions_closed: 0,
        } => {
            assert_eq!(account.username, "marie");
            assert_eq!(account.last_login_at, None);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        wire::interpret(Expect::Closed, 200, &json!({ "sessions_closed": 3 })).unwrap(),
        AccountOutcome::Done {
            account: None,
            sessions_closed: 3
        }
    );
    assert_eq!(
        wire::interpret(Expect::Nothing, 204, &Value::Null).unwrap(),
        AccountOutcome::Done {
            account: None,
            sessions_closed: 0
        }
    );
    // Une réponse de succès qui n'a pas la forme attendue n'est pas celle d'un agent Hearth.
    assert_eq!(
        wire::interpret(Expect::Closed, 200, &json!({ "autre": 1 })).unwrap_err(),
        LinkFailure::NotAgent
    );
}

#[test]
fn no_password_shows_in_the_debug_of_a_planned_action_nor_of_a_refusal() {
    let create = planned(wire::create("marie", GOOD, RoleDto::Admin));
    let own = planned(wire::change_own_password("marie", "Old-Pass-12345", GOOD));
    let text = format!("{create:?} {own:?}");
    for secret in [GOOD, "Old-Pass-12345"] {
        assert!(!text.contains(secret), "{text}");
    }
    let refusal = wire::create("marie", "abc", RoleDto::Admin).unwrap_err();
    assert!(!format!("{refusal:?}").contains("abc"));
}

#[test]
fn the_live_check_is_the_rule_of_the_agent() {
    let check = hearth_desktop_lib::accounts::service::check_input(
        "Marie",
        &hearth_link::domain::secret::Secret::new("xxmariexx12A"),
    );
    assert_eq!(check.username, None);
    assert_eq!(check.password, [PasswordRuleDto::ContainsUsername]);
    let check = hearth_desktop_lib::accounts::service::check_input(
        "",
        &hearth_link::domain::secret::Secret::new(""),
    );
    assert_eq!(check.username, Some(UsernameProblemDto::Empty));
    assert_eq!(check.password, [PasswordRuleDto::Required]);
}

// ── Le coffre après un changement de SON mot de passe : liste fermée des cas « ancien remis » ──

fn refused(refusal: AccountRefusal) -> Result<AccountOutcome, LinkFailure> {
    Ok(AccountOutcome::Refused { refusal })
}

#[test]
fn the_old_password_stands_after_an_explicit_refusal_of_the_agent() {
    use hearth_desktop_lib::accounts::service::old_password_stands;
    assert!(old_password_stands(&refused(AccountRefusal::WrongPassword)));
    assert!(old_password_stands(&refused(
        AccountRefusal::WeakPassword {
            rules: vec![PasswordRuleDto::Digit]
        }
    )));
    assert!(old_password_stands(&Err(LinkFailure::Forbidden)));
}

#[test]
fn the_old_password_stands_when_the_request_provably_never_left() {
    use hearth_desktop_lib::accounts::service::old_password_stands;
    for failure in [
        LinkFailure::NotConnected,
        LinkFailure::TrackingUnavailable,
        LinkFailure::TrackingSlow,
    ] {
        assert!(old_password_stands(&Err(failure.clone())), "{failure:?}");
    }
}

#[test]
fn the_entry_is_erased_when_the_result_is_unknown_or_the_answer_unreadable() {
    use hearth_desktop_lib::accounts::service::old_password_stands;
    assert!(!old_password_stands(&Ok(AccountOutcome::Unknown {
        op_id: "OP".into()
    })));
    // Réponse 2xx illisible : classée `NotAgent`, l'agent a pu exécuter l'action.
    assert!(!old_password_stands(&Err(LinkFailure::NotAgent)));
    // Tâche redémarrée ou arrêtée : `Internal`.
    assert!(!old_password_stands(&Err(LinkFailure::Internal)));
    assert!(!old_password_stands(&Err(LinkFailure::Unreachable)));
    assert!(!old_password_stands(&Err(LinkFailure::Storage)));
}

#[test]
fn the_entry_is_erased_after_any_refusal_that_does_not_prove_nothing_changed() {
    use hearth_desktop_lib::accounts::service::old_password_stands;
    for refusal in [
        AccountRefusal::Conflict,
        AccountRefusal::Busy,
        AccountRefusal::NotFound,
        AccountRefusal::SessionEnded,
        AccountRefusal::SessionRevoked,
        AccountRefusal::Other,
        AccountRefusal::UsernameTaken,
        AccountRefusal::LastAdmin,
        AccountRefusal::ConfirmationMismatch,
        AccountRefusal::InvalidUsername { problem: None },
    ] {
        assert!(
            !old_password_stands(&refused(refusal.clone())),
            "{refusal:?}"
        );
    }
}

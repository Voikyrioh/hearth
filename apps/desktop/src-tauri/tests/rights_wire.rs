//! HRT-78 : le droit « gérer les services » vu par la coquille, sans réseau. (1) La forme des types qui
//! traversent vers l'interface est comparée, champ par champ et valeur par valeur, à la fiche
//! `tests/fixtures/rights.json` (que le pont simulé et le pont réel de l'interface relisent aussi) ;
//! (2) le refus faute du droit (`403 FORBIDDEN_ROLE` + `details.missing`), tel que `ErrorBody::missing_right` le
//! fabrique (la même fonction que l'agent), arrive à l'interface comme `MissingServicesRight` par TOUS les
//! chemins qui lisaient un refus de rôle, et un refus de rôle ordinaire reste un refus de rôle (BR-GRANT-004,
//! BR-GRANT-014) ; (3) aucune commande de la coquille n'accorde ni ne retire le droit (BR-GRANT-005).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use hearth_desktop_lib::accounts::dto::{AccountDto, AccountListDto};
use hearth_desktop_lib::accounts::wire;
use hearth_desktop_lib::agent_update::wire as agent_update_wire;
use hearth_desktop_lib::devices::service as devices;
use hearth_desktop_lib::link_dto::LinkFailure;
use hearth_desktop_lib::reauth::service as reauth;
use hearth_desktop_lib::security::service as security;
use hearth_desktop_lib::services::dto::ServiceRightsDto;
use hearth_link::{LinkError, ServiceRightsRead};
use hearth_proto::api::accounts::{AccountItem, RoleName};
use hearth_proto::error::{ErrorBody, ErrorCode};
use serde_json::Value;

fn fixture() -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/rights.json"
    ))
    .unwrap();
    serde_json::from_str(&text).unwrap()
}

fn read(known: bool, granted: bool, account: &str) -> ServiceRightsDto {
    ServiceRightsRead {
        agent_knows: known,
        granted,
        account: account.to_owned(),
    }
    .into()
}

#[test]
fn the_right_of_the_connected_account_has_the_shape_of_the_reference_sheet_for_every_profile() {
    let sheet = fixture();
    let profiles = [
        ("adminGranted", read(true, true, "marie")),
        ("adminNone", read(true, false, "marie")),
        ("readonlyGranted", read(true, true, "paul")),
        ("readonlyNone", read(true, false, "paul")),
        ("oldAgent", read(false, false, "marie")),
    ];
    for (name, dto) in profiles {
        assert_eq!(
            serde_json::to_value(&dto).unwrap(),
            sheet["serviceRights"][name],
            "{name}"
        );
    }
}

fn item(id: &str, username: &str, role: RoleName, rights: &[&str]) -> AccountItem {
    AccountItem {
        id: id.to_owned(),
        username: username.to_owned(),
        role,
        created_at: if username == "marie" {
            "2026-10-01T08:00:00Z"
        } else {
            "2026-10-02T08:00:00Z"
        }
        .to_owned(),
        last_login_at: (username == "marie").then(|| "2026-10-13T09:00:00Z".to_owned()),
        sessions_open: u64::from(username == "marie"),
        rights: rights.iter().map(|r| (*r).to_owned()).collect(),
    }
}

#[test]
fn the_list_of_accounts_carries_the_right_of_each_account_in_the_shape_of_the_reference_sheet() {
    let list = AccountListDto {
        accounts: vec![
            AccountDto::from(item(
                "01J9ZD6Q8E5W3N2K7M4T1V0XAB",
                "marie",
                RoleName::Admin,
                &["services"],
            )),
            // Une valeur inconnue d'un agent plus récent est ignorée : le compte n'a pas le droit pour autant.
            AccountDto::from(item(
                "01J9ZD6Q8E5W3N2K7M4T1V0XAC",
                "paul",
                RoleName::Readonly,
                &["console"],
            )),
        ],
        me: "01J9ZD6Q8E5W3N2K7M4T1V0XAB".to_owned(),
        rights_known: true,
    };
    assert_eq!(
        serde_json::to_value(&list).unwrap(),
        fixture()["accountList"]
    );
}

fn missing(right: &str) -> Value {
    serde_json::to_value(ErrorBody::missing_right(right, "pas le droit")).unwrap()
}

fn role_only() -> Value {
    serde_json::to_value(ErrorBody::new(ErrorCode::ForbiddenRole, "rôle")).unwrap()
}

/// Les cinq lectures d'un refus qui rendaient `Forbidden`.
fn failures(body: &Value) -> Vec<(&'static str, LinkFailure)> {
    vec![
        ("comptes", wire::refusal_from_error(403, body).unwrap_err()),
        (
            "mise à jour de l'agent",
            agent_update_wire::refusal_from_error(403, body).unwrap_err(),
        ),
        ("postes", devices::interpret(403, body).unwrap_err()),
        ("réglage", reauth::interpret(403, body).unwrap_err()),
        ("mode attaque", security::interpret(403, body).unwrap_err()),
    ]
}

#[test]
fn a_refusal_for_a_missing_right_reaches_the_interface_by_every_path_that_read_a_role_refusal() {
    for (path, failure) in failures(&missing("services")) {
        assert_eq!(failure, LinkFailure::MissingServicesRight, "{path}");
    }
    assert_eq!(
        serde_json::to_value(LinkFailure::MissingServicesRight).unwrap(),
        fixture()["failure"]
    );
}

#[test]
fn a_plain_role_refusal_and_an_unknown_missing_right_stay_a_role_refusal() {
    for body in [role_only(), missing("console"), missing("")] {
        for (path, failure) in failures(&body) {
            assert_eq!(failure, LinkFailure::Forbidden, "{path} {body}");
        }
    }
}

#[test]
fn a_typed_read_refused_for_a_missing_right_becomes_the_same_failure() {
    assert_eq!(
        LinkFailure::from(LinkError::MissingRight("services".to_owned())),
        LinkFailure::MissingServicesRight
    );
    assert_eq!(
        LinkFailure::from(LinkError::MissingRight("autre".to_owned())),
        LinkFailure::Forbidden
    );
    assert_eq!(
        LinkFailure::from(LinkError::Rejected(Some(ErrorCode::ForbiddenRole))),
        LinkFailure::Forbidden
    );
}

#[test]
fn the_shell_has_one_command_for_the_right_and_it_only_reads() {
    // BR-GRANT-005 : la liste blanche des commandes ne compte aucune commande qui accorde ou retire le droit.
    let build = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/build.rs")).unwrap();
    let mut named: Vec<&str> = build
        .lines()
        .map(str::trim)
        .filter_map(|line| line.strip_prefix('"')?.strip_suffix("\","))
        .filter(|name| name.contains("right") || name.contains("grant") || name.contains("revoke"))
        .collect();
    named.sort_unstable();
    assert_eq!(named, ["read_service_rights"]);
}

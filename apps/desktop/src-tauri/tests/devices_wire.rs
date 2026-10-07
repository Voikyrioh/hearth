//! HRT-23 : la lecture des réponses de l'agent pour les postes de confiance, sans réseau. Les refus
//! se lisent par code stable (jamais par texte) ; un identifiant de poste ne sort jamais du chemin ;
//! un agent d'avant la clé d'appareil est « non pris en charge », pas une erreur ; aucun type ne
//! porte de clé.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use hearth_desktop_lib::devices::dto::{
    DeviceRemovalOutcome, DeviceRemovalRefusal, TrustedDeviceDto, TrustedDevicesDto,
};
use hearth_desktop_lib::devices::service::{self, device_id, interpret, listed, removed};
use hearth_desktop_lib::link_dto::{InvalidField, LinkFailure};
use hearth_link::LinkError;
use hearth_proto::api::devices::{DeviceItem, DevicesResponse};
use hearth_proto::error::ErrorCode;
use serde_json::{Value, json};

fn error(code: &str, details: Value) -> Value {
    json!({ "error": { "code": code, "message": "texte de l'agent", "details": details } })
}

fn refusal(status: u16, body: Value) -> DeviceRemovalRefusal {
    match interpret(status, &body).unwrap() {
        DeviceRemovalOutcome::Refused { refusal } => refusal,
        other => panic!("refus attendu, reçu {other:?}"),
    }
}

#[test]
fn a_success_is_done_and_every_refusal_is_read_by_its_stable_code() {
    assert_eq!(
        interpret(204, &Value::Null).unwrap(),
        DeviceRemovalOutcome::Done
    );
    assert_eq!(
        refusal(422, error("WRONG_PASSWORD", json!({}))),
        DeviceRemovalRefusal::WrongPassword
    );
    assert_eq!(
        refusal(422, error("VALIDATION_ERROR", json!({ "field": "id" }))),
        DeviceRemovalRefusal::CurrentDevice
    );
    assert_eq!(
        refusal(
            422,
            error(
                "VALIDATION_ERROR",
                json!({ "field": "device", "reason": "device_required" })
            )
        ),
        DeviceRemovalRefusal::NoDeviceKey
    );
    assert_eq!(
        refusal(
            422,
            error(
                "VALIDATION_ERROR",
                json!({ "field": "device", "reason": "proof_invalid" })
            )
        ),
        DeviceRemovalRefusal::ProofRefused
    );
    assert_eq!(
        refusal(404, error("NOT_FOUND", json!({}))),
        DeviceRemovalRefusal::NotFound
    );
    assert_eq!(
        refusal(
            429,
            error("TOO_MANY_ATTEMPTS", json!({ "retry_after_s": 90 }))
        ),
        DeviceRemovalRefusal::TooManyAttempts { retry_after_s: 90 }
    );
    assert_eq!(
        refusal(503, error("BUSY", json!({}))),
        DeviceRemovalRefusal::Busy
    );
    assert_eq!(
        refusal(401, error("SESSION_EXPIRED", json!({}))),
        DeviceRemovalRefusal::SessionEnded
    );
    assert_eq!(
        refusal(401, error("SESSION_REVOKED", json!({}))),
        DeviceRemovalRefusal::SessionRevoked
    );
    assert_eq!(
        refusal(422, error("VALIDATION_ERROR", json!({ "field": "autre" }))),
        DeviceRemovalRefusal::Other
    );
    // Un corps qui n'est pas celui de l'agent : jamais une panique.
    assert_eq!(refusal(500, json!("bizarre")), DeviceRemovalRefusal::Other);
    assert_eq!(refusal(404, Value::Null), DeviceRemovalRefusal::NotFound);
    assert_eq!(
        interpret(403, &error("FORBIDDEN_ROLE", json!({}))).unwrap_err(),
        LinkFailure::Forbidden
    );
}

#[test]
fn an_old_agent_is_unsupported_not_an_error() {
    assert_eq!(
        listed(Err(LinkError::Rejected(Some(ErrorCode::NotFound)))).unwrap(),
        TrustedDevicesDto::Unsupported
    );
    assert_eq!(
        removed(Err(LinkError::Rejected(Some(ErrorCode::NotFound)))).unwrap(),
        DeviceRemovalOutcome::Refused {
            refusal: DeviceRemovalRefusal::Unsupported
        }
    );
    assert_eq!(
        removed(Err(LinkError::NoDeviceKey)).unwrap(),
        DeviceRemovalOutcome::Refused {
            refusal: DeviceRemovalRefusal::NoDeviceKey
        }
    );
    // Les autres échecs restent typés : lien non établi, agent injoignable.
    assert_eq!(
        listed(Err(LinkError::NotConnected)).unwrap_err(),
        LinkFailure::NotConnected
    );
    assert_eq!(
        removed(Err(LinkError::NotConnected)).unwrap_err(),
        LinkFailure::NotConnected
    );
    assert_eq!(
        listed(Err(LinkError::Timeout)).unwrap_err(),
        LinkFailure::Unreachable
    );
}

#[test]
fn the_list_keeps_the_agents_order_and_flags_and_exposes_no_key() {
    let list = DevicesResponse {
        devices: vec![
            DeviceItem {
                id: "01J9ZY0G3Q8M2K6W4T7V5N1B9D".into(),
                name: "salon".into(),
                created_at: "2026-10-07T00:12:03.100Z".into(),
                last_proved_at: "2026-10-07T08:30:15.250Z".into(),
                last_addr: "192.168.1.20".into(),
                current: true,
            },
            DeviceItem {
                id: "01J9ZY0G3Q8M2K6W4T7V5N1B9E".into(),
                name: "bureau".into(),
                created_at: "2026-10-01T00:12:03.100Z".into(),
                last_proved_at: "2026-10-02T08:30:15.250Z".into(),
                last_addr: "192.168.1.21".into(),
                current: false,
            },
        ],
        max: 8,
    };
    let TrustedDevicesDto::Listed { devices, max } = listed(Ok(list)).unwrap() else {
        panic!("liste attendue");
    };
    assert_eq!(max, 8);
    assert_eq!(
        devices.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
        ["salon", "bureau"]
    );
    assert!(devices[0].current && !devices[1].current);
    let json = serde_json::to_value(&devices[0]).unwrap();
    let mut keys: Vec<&str> = json
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "createdAt",
            "current",
            "id",
            "lastAddr",
            "lastProvedAt",
            "name"
        ],
        "rien d'autre que des noms, des dates, une adresse et un booléen"
    );
    let _: TrustedDeviceDto = devices[0].clone();
}

#[test]
fn a_device_id_never_leaves_the_path() {
    assert_eq!(
        device_id("01J9ZY0G3Q8M2K6W4T7V5N1B9D").unwrap(),
        "01J9ZY0G3Q8M2K6W4T7V5N1B9D"
    );
    for bad in ["", "../accounts", "a/b", "a?b", "a b", "é", &"a".repeat(65)] {
        assert_eq!(
            device_id(bad).unwrap_err(),
            LinkFailure::InvalidInput {
                field: InvalidField::Other
            },
            "{bad}"
        );
    }
    assert_eq!(
        service::server("pas un identifiant !").unwrap_err(),
        LinkFailure::UnknownServer
    );
}

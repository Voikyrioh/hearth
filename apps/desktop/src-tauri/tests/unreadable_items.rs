//! BR-COMPAT-001 point 10 : un élément illisible d'une liste est écarté ET COMPTÉ, et le compte arrive jusqu'à
//! l'interface (comptes, postes de confiance, journal d'activité, services). Une liste partielle ne se tait pas.
//! Les listes sont lues depuis du JSON brut, comme un agent plus récent les enverrait.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use hearth_desktop_lib::accounts::service::listed as accounts_listed;
use hearth_desktop_lib::audit::AuditPageDto;
use hearth_desktop_lib::devices::dto::TrustedDevicesDto;
use hearth_desktop_lib::devices::service::listed as devices_listed;
use hearth_desktop_lib::services::dto::ServicesViewDto;
use hearth_link::AccountsRead;
use hearth_proto::api::accounts::AccountsResponse;
use hearth_proto::api::audit::AuditResponse;
use hearth_proto::api::devices::DevicesResponse;
use hearth_proto::api::services::ServicesState;
use serde_json::json;

/// Un élément dont la forme a changé (une valeur obligatoire n'a plus le même type) : illisible pour ce client.
fn broken() -> serde_json::Value {
    json!({"id": 12, "name": ["forme", "changée"]})
}

#[test]
fn the_account_list_says_how_many_accounts_it_could_not_read() {
    let list: AccountsResponse = serde_json::from_value(json!({
        "accounts": [
            {"id": "A1", "username": "marie", "role": "admin", "created_at": "2026-10-04T10:30:15.250Z",
             "last_login_at": null, "sessions_open": 1},
            broken(), broken(),
        ]
    }))
    .unwrap();
    let dto = accounts_listed(AccountsRead {
        accounts: list.accounts,
        me: "A1".into(),
        rights_known: true,
    });
    assert_eq!(dto.accounts.len(), 1);
    assert_eq!(dto.unreadable, 2);
    assert_eq!(serde_json::to_value(&dto).unwrap()["unreadable"], 2);
}

#[test]
fn the_trusted_devices_say_how_many_they_could_not_read() {
    let list: DevicesResponse = serde_json::from_value(json!({
        "devices": [broken()],
        "max": 8
    }))
    .unwrap();
    let TrustedDevicesDto::Listed {
        devices,
        unreadable,
        ..
    } = devices_listed(Ok(list)).unwrap()
    else {
        panic!("liste attendue");
    };
    assert!(devices.is_empty());
    assert_eq!(unreadable, 1);
}

#[test]
fn the_activity_log_page_says_how_many_entries_it_could_not_read() {
    let page: AuditResponse = serde_json::from_value(json!({
        "events": [broken(), broken(), broken()],
        "next_before": null
    }))
    .unwrap();
    let dto = AuditPageDto::from(&page);
    assert!(dto.events.is_empty());
    assert_eq!(dto.unreadable, 3);
}

#[test]
fn the_services_say_how_many_they_could_not_read() {
    let state: ServicesState = serde_json::from_value(json!({
        "rev": 4,
        "engine": {
            "state": "ready", "name": null, "version": null, "min_version": "20.10.0",
            "agent_account": "root", "agent_is_admin": true,
            "host": {"os_id": "debian", "os_like": [], "init": "systemd", "declarative": false},
            "last_ready_at": null, "checked_at": "2026-10-13T10:00:00Z", "list_read_at": null,
            "list_degraded": false, "at": null, "list_current_at": null, "external_count": 0
        },
        "services": [broken()]
    }))
    .unwrap();
    let ServicesViewDto::Supported {
        services,
        unreadable,
        ..
    } = ServicesViewDto::from_state(&state)
    else {
        panic!("vue attendue");
    };
    assert!(services.is_empty());
    assert_eq!(unreadable, 1);
}

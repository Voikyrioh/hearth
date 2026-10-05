//! Conversions de la liaison vers l'interface : séquences d'état, adresses, erreurs typées.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use hearth_desktop_lib::link_dto::{
    BlockedDto, InvalidField, LinkFailure, LinkStateName, ReasonDto, RoleDto, ServerDto, StateBook,
    color_number, servers_list,
};
use hearth_link::domain::compat::Compatibility;
use hearth_link::domain::event::StateInfo;
use hearth_link::domain::server::{ServerId, ServerRecord};
use hearth_link::domain::state::{Blocked, LinkState, Reason};
use hearth_link::domain::time::WallTime;
use hearth_link::{InputField, LinkError};
use hearth_proto::api::accounts::RoleName;
use hearth_proto::error::UpgradeTarget;
use hearth_proto::fingerprint::Fingerprint;

fn record(id: &str, name: &str, host: &str, port: u16) -> ServerRecord {
    ServerRecord {
        id: ServerId::parse(id).unwrap(),
        name: name.into(),
        color: "3".into(),
        host: host.into(),
        port,
        fingerprint: Fingerprint::from_bytes([1; 32]),
        username: "marie".into(),
        remember: true,
        mac_addresses: vec![],
        last_contact_at: None,
        signed_out: false,
        role: Some(RoleName::Admin),
    }
}

fn info(state: LinkState, since: i64) -> StateInfo {
    StateInfo {
        state,
        blocked: None,
        reason: None,
        since: WallTime::from_millis(since),
        last_contact_at: Some(WallTime::from_millis(since)),
        next_retry_at: None,
        failed_attempts: 0,
    }
}

#[test]
fn a_server_shows_its_address_without_the_default_port_and_a_readonly_role_until_logged_in() {
    let dto = ServerDto::from(&record("a", "Forge", "192.168.1.20", 7341));
    assert_eq!(dto.address, "192.168.1.20");
    assert_eq!(dto.color, 3);
    assert_eq!(dto.role, RoleDto::Admin);
    let other = ServerDto::from(&record("b", "Salon", "nas.lan", 7443));
    assert_eq!(other.address, "nas.lan:7443");
    let v6 = ServerDto::from(&record("c", "V6", "::1", 7443));
    assert_eq!(v6.address, "[::1]:7443");
    let mut never = record("d", "Neuf", "x.lan", 7341);
    never.role = None;
    assert_eq!(ServerDto::from(&never).role, RoleDto::Readonly);
}

#[test]
fn colors_outside_the_palette_fall_back_to_the_first() {
    assert_eq!(color_number("8"), 8);
    for bad in ["0", "9", "", "#7aa2f7", "-1"] {
        assert_eq!(color_number(bad), 1, "{bad}");
    }
}

#[test]
fn the_book_lists_servers_by_name_ignoring_case() {
    let list = servers_list(&[
        record("a", "salon", "a.lan", 1),
        record("b", "Forge", "b.lan", 1),
        record("c", "Cave", "c.lan", 1),
    ]);
    let names: Vec<&str> = list.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Cave", "Forge", "salon"]);
}

#[test]
fn every_state_gets_the_next_sequence_number_and_an_identical_one_is_not_repeated() {
    let id = ServerId::parse("a").unwrap();
    let mut book = StateBook::default();
    let first = book.apply(&id, &info(LinkState::Reconnecting, 10)).unwrap();
    assert_eq!(first.seq, 1);
    assert_eq!(first.state, LinkStateName::Reconnecting);
    assert!(
        book.apply(&id, &info(LinkState::Reconnecting, 10))
            .is_none()
    );
    let second = book.apply(&id, &info(LinkState::Connected, 20)).unwrap();
    assert_eq!(second.seq, 2);
    // Un autre serveur a sa propre numérotation.
    let other = ServerId::parse("b").unwrap();
    assert_eq!(
        book.apply(&other, &info(LinkState::Offline, 5))
            .unwrap()
            .seq,
        1
    );
}

#[test]
fn replaying_the_current_state_keeps_the_announced_number_so_the_interface_discards_it() {
    let id = ServerId::parse("a").unwrap();
    let mut book = StateBook::default();
    book.apply(&id, &info(LinkState::Reconnecting, 10));
    let sent = book.apply(&id, &info(LinkState::Connected, 20)).unwrap();
    let live = [(id.clone(), info(LinkState::Connected, 20))];
    let replay = book.snapshot(&live);
    assert_eq!(replay, vec![sent.clone()]);
    // Un état que le relais n'a pas encore annoncé reçoit le numéro suivant.
    let live = [(id.clone(), info(LinkState::Offline, 90))];
    let fresh = book.snapshot(&live);
    assert_eq!(fresh[0].seq, sent.seq + 1);
    // L'événement qui arrive ensuite pour ce même état n'est pas annoncé deux fois.
    assert!(book.apply(&id, &info(LinkState::Offline, 90)).is_none());
}

#[test]
fn a_removed_server_is_forgotten_and_starts_again_at_one() {
    let id = ServerId::parse("a").unwrap();
    let mut book = StateBook::default();
    book.apply(&id, &info(LinkState::Connected, 1));
    book.apply(&id, &info(LinkState::Offline, 2));
    book.forget(&id);
    assert_eq!(
        book.apply(&id, &info(LinkState::Connected, 3)).unwrap().seq,
        1
    );
    assert!(book.snapshot(&[]).is_empty());
}

#[test]
fn blocking_and_reasons_reach_the_interface() {
    let id = ServerId::parse("a").unwrap();
    let mut blocked = info(LinkState::Offline, 1);
    blocked.blocked = Some(Blocked::FingerprintChanged);
    let dto = StateBook::default().apply(&id, &blocked).unwrap();
    assert_eq!(dto.blocked, Some(BlockedDto::FingerprintChanged));
    blocked.blocked = Some(Blocked::IncompatibleVersion(UpgradeTarget::Agent));
    assert_eq!(
        StateBook::default().apply(&id, &blocked).unwrap().blocked,
        Some(BlockedDto::IncompatibleAgent)
    );
    let mut expired = info(LinkState::SessionExpired, 1);
    expired.reason = Some(Reason::StoredPasswordRefused);
    expired.failed_attempts = 2;
    let dto = StateBook::default().apply(&id, &expired).unwrap();
    assert_eq!(dto.reason, Some(ReasonDto::StoredPasswordRefused));
    assert_eq!(dto.failed_attempts, 2);
    assert_eq!(dto.since, 1.0);
}

#[test]
fn library_errors_become_typed_failures_without_any_text() {
    let failure = LinkFailure::from;
    assert_eq!(failure(LinkError::Timeout), LinkFailure::Unreachable);
    assert_eq!(
        failure(LinkError::Unreachable("x".into())),
        LinkFailure::Unreachable
    );
    assert_eq!(
        failure(LinkError::Protocol("produit".into())),
        LinkFailure::NotAgent
    );
    assert_eq!(
        failure(LinkError::Incompatible(Compatibility::UpdateAgent)),
        LinkFailure::IncompatibleAgent
    );
    assert_eq!(
        failure(LinkError::Incompatible(Compatibility::UpdateClient)),
        LinkFailure::IncompatibleClient
    );
    assert_eq!(
        failure(LinkError::InvalidCredentials),
        LinkFailure::InvalidCredentials
    );
    assert_eq!(
        failure(LinkError::TooManyAttempts { retry_after_s: 90 }),
        LinkFailure::TooManyAttempts { retry_after_s: 90 }
    );
    assert_eq!(failure(LinkError::NameTaken), LinkFailure::NameTaken);
    assert_eq!(
        failure(LinkError::TrackingUnavailable),
        LinkFailure::TrackingUnavailable
    );
    assert_eq!(failure(LinkError::TrackingSlow), LinkFailure::TrackingSlow);
    assert_eq!(failure(LinkError::NotConnected), LinkFailure::NotConnected);
    assert_eq!(
        failure(LinkError::FingerprintChanged),
        LinkFailure::FingerprintChanged
    );
    assert_eq!(
        failure(LinkError::InvalidInput(InputField::Port)),
        LinkFailure::InvalidInput {
            field: InvalidField::Port
        }
    );
    assert_eq!(
        failure(LinkError::InvalidInput(InputField::Name)),
        LinkFailure::InvalidInput {
            field: InvalidField::Name
        }
    );
    // Aucun message ne contient de détail réseau ni de secret : la sérialisation n'a que `kind`.
    let text = serde_json::to_string(&failure(LinkError::Vault("secret-xyz".into()))).unwrap();
    assert_eq!(text, r#"{"kind":"vault"}"#);
}

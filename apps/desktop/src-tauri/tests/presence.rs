//! Présence hors de la fenêtre : limiteur de notifications système (BR-RESIL-015, 018) et état de
//! l'icône de la zone de notification (BR-RESIL-016). Règles pures, temps en paramètre.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use hearth_desktop_lib::presence::{
    Alert, AlertKind, FAILURE_STEP, LinkPresence, NOTIFY_WINDOW_MS, NotificationGate, TrayStatus,
};
use hearth_link::domain::state::LinkState;

const MIN: u64 = NOTIFY_WINDOW_MS;

fn alert(server: &str, kind: AlertKind, suppressed: u32) -> Alert {
    Alert {
        server: server.into(),
        kind,
        suppressed,
    }
}

#[test]
fn going_offline_notifies_at_once_and_the_return_waits_for_the_one_per_minute_window() {
    let mut gate = NotificationGate::default();
    assert_eq!(gate.observe("a", LinkState::Reconnecting, 2, 0), None);
    let off = gate.observe("a", LinkState::Offline, 4, 30_000).unwrap();
    assert_eq!(off, alert("a", AlertKind::Offline, 0));
    // Une seule notification par minute et par serveur, toutes natures confondues (spec) : le retour
    // est retenu, annoncé à l'échéance, sans changement « absorbé » en plus.
    assert_eq!(gate.observe("a", LinkState::Connected, 0, 40_000), None);
    assert!(gate.poll(40_000 + 1).is_empty());
    assert!(gate.poll(30_000 + MIN - 1).is_empty());
    assert_eq!(gate.poll(30_000 + MIN), [alert("a", AlertKind::Back, 0)]);
    assert!(gate.poll(10 * MIN).is_empty());
}

#[test]
fn a_return_after_a_long_outage_is_announced_at_once() {
    let mut gate = NotificationGate::default();
    gate.observe("a", LinkState::Offline, 0, 0).unwrap();
    let back = gate.observe("a", LinkState::Connected, 0, 2 * MIN).unwrap();
    assert_eq!(back, alert("a", AlertKind::Back, 0));
}

#[test]
fn a_short_blip_that_never_went_offline_says_nothing() {
    let mut gate = NotificationGate::default();
    assert_eq!(gate.observe("a", LinkState::Reconnecting, 1, 0), None);
    assert_eq!(gate.observe("a", LinkState::Connected, 0, 10_000), None);
    assert!(gate.poll(10 * MIN).is_empty());
}

#[test]
fn session_expired_and_revoked_never_notify() {
    let mut gate = NotificationGate::default();
    assert_eq!(gate.observe("a", LinkState::SessionExpired, 0, 0), None);
    assert_eq!(gate.observe("a", LinkState::AccessRevoked, 0, 1), None);
}

#[test]
fn ten_cuts_in_a_minute_give_one_notification_then_the_last_situation_with_its_count() {
    let mut gate = NotificationGate::default();
    let mut sent = Vec::new();
    for flap in 0..10_u64 {
        let at = flap * 4_000;
        sent.extend(gate.observe("a", LinkState::Offline, 0, at));
        sent.extend(gate.observe("a", LinkState::Connected, 0, at + 2_000));
    }
    // Dix coupures en 40 s : une seule notification est partie (« hors ligne »), le reste est retenu.
    assert_eq!(sent, [alert("a", AlertKind::Offline, 0)]);
    // À l'échéance : la dernière situation (de retour), une fois, avec le nombre de changements
    // absorbés EN PLUS de celui-ci (19 observations retenues ou annulées, dont celle-ci).
    let later = gate.poll(MIN);
    assert_eq!(later, [alert("a", AlertKind::Back, 18)]);
    assert!(gate.poll(10 * MIN).is_empty());
}

#[test]
fn a_situation_already_announced_is_not_repeated() {
    let mut gate = NotificationGate::default();
    gate.observe("a", LinkState::Offline, 0, 0).unwrap();
    // Un essai de reconnexion échoue : « Reconnexion… » puis de nouveau « Hors ligne ».
    assert_eq!(gate.observe("a", LinkState::Reconnecting, 1, 5_000), None);
    assert_eq!(gate.observe("a", LinkState::Offline, 1, 10_000), None);
    assert_eq!(gate.observe("a", LinkState::Offline, 2, 3 * MIN), None);
    assert!(gate.poll(10 * MIN).is_empty());
}

#[test]
fn failure_steps_notify_once_per_step_never_per_attempt() {
    let mut gate = NotificationGate::default();
    // Hors ligne à 30 s avec déjà 7 échecs : « hors ligne » couvre le premier palier.
    assert_eq!(
        gate.observe("a", LinkState::Offline, 7, 0),
        Some(alert("a", AlertKind::Offline, 0))
    );
    // Tentatives suivantes (une toutes les 30 s) : rien tant que le palier suivant n'est pas franchi.
    for (n, at) in [(8, 30_000), (9, 60_000)] {
        assert_eq!(gate.observe("a", LinkState::Offline, n, at), None, "{n}");
    }
    // Palier de 10 échecs franchi, fenêtre ouverte : une notification.
    assert_eq!(
        gate.observe("a", LinkState::Offline, 10, 90_000),
        Some(alert("a", AlertKind::Failures(10), 0))
    );
    // Les tentatives du même palier ne renotifient pas.
    for n in 11..=14 {
        assert_eq!(
            gate.observe("a", LinkState::Offline, n, 90_000 + u64::from(n) * 30_000),
            None
        );
    }
}

#[test]
fn a_failure_step_inside_the_window_is_retained_and_announced_once() {
    let mut gate = NotificationGate::default();
    gate.observe("a", LinkState::Offline, 0, 0).unwrap();
    // Palier franchi 20 s plus tard : retenu par la limite.
    assert_eq!(
        gate.observe("a", LinkState::Offline, FAILURE_STEP, 20_000),
        None
    );
    assert_eq!(
        gate.observe("a", LinkState::Offline, FAILURE_STEP + 1, 25_000),
        None
    );
    assert_eq!(
        gate.poll(MIN),
        [alert("a", AlertKind::Failures(FAILURE_STEP), 0)]
    );
    assert!(gate.poll(10 * MIN).is_empty());
}

#[test]
fn a_new_outage_starts_the_steps_again() {
    let mut gate = NotificationGate::default();
    gate.observe("a", LinkState::Offline, 12, 0).unwrap();
    gate.observe("a", LinkState::Connected, 0, 2 * MIN).unwrap();
    assert_eq!(
        gate.observe("a", LinkState::Offline, 3, 4 * MIN),
        Some(alert("a", AlertKind::Offline, 0))
    );
    assert_eq!(
        gate.observe("a", LinkState::Offline, 5, 5 * MIN),
        Some(alert("a", AlertKind::Failures(5), 0))
    );
}

#[test]
fn each_server_has_its_own_window() {
    let mut gate = NotificationGate::default();
    assert!(gate.observe("a", LinkState::Offline, 0, 0).is_some());
    assert!(gate.observe("b", LinkState::Offline, 0, 5_000).is_some());
    // « a » : le retour est retenu par SA fenêtre ; « b » n'est pas concerné.
    assert_eq!(gate.observe("a", LinkState::Connected, 0, 6_000), None);
    assert!(gate.observe("b", LinkState::Connected, 0, 70_000).is_some());
    assert_eq!(gate.poll(MIN), [alert("a", AlertKind::Back, 0)]);
}

#[test]
fn forgetting_a_server_bounds_the_memory() {
    let mut gate = NotificationGate::default();
    for n in 0..1_000 {
        let name = format!("srv{n}");
        gate.observe(&name, LinkState::Offline, 0, 0);
        gate.forget(&name);
    }
    assert_eq!(gate.tracked(), 0);
}

#[test]
fn a_long_session_keeps_one_entry_per_server() {
    let mut gate = NotificationGate::default();
    // Trois jours de coupures toutes les 10 minutes.
    for n in 0..(3 * 24 * 6_u64) {
        let at = n * 10 * 60_000;
        gate.observe("a", LinkState::Offline, 0, at);
        gate.observe("a", LinkState::Connected, 0, at + 90_000);
        gate.poll(at + 90_000);
    }
    assert_eq!(gate.tracked(), 1);
}

#[test]
fn the_tray_follows_the_displayed_server() {
    let mut presence = LinkPresence::default();
    assert_eq!(presence.status(), TrayStatus::Idle);
    presence.set_state("a", LinkState::Connected);
    presence.set_state("b", LinkState::Offline);
    // Aucun serveur affiché : le pire état de tous.
    assert_eq!(presence.status(), TrayStatus::Critical);
    presence.set_displayed(Some("a"));
    assert_eq!(presence.status(), TrayStatus::Connected);
    presence.set_state("a", LinkState::Reconnecting);
    assert_eq!(presence.status(), TrayStatus::Warning);
    presence.set_displayed(Some("b"));
    assert_eq!(presence.status(), TrayStatus::Critical);
    // Le serveur affiché est supprimé : retour au pire état de ceux qui restent.
    presence.remove("b");
    assert_eq!(presence.displayed(), None);
    assert_eq!(presence.status(), TrayStatus::Warning);
    presence.remove("a");
    assert_eq!(presence.status(), TrayStatus::Idle);
    assert_eq!(presence.tracked(), 0);
}

#[test]
fn a_displayed_server_without_a_known_state_counts_as_reconnecting() {
    let mut presence = LinkPresence::default();
    presence.set_displayed(Some("new"));
    assert_eq!(presence.status(), TrayStatus::Warning);
}

#[test]
fn every_state_has_a_status() {
    assert_eq!(TrayStatus::of(LinkState::Connected), TrayStatus::Connected);
    assert_eq!(TrayStatus::of(LinkState::Reconnecting), TrayStatus::Warning);
    assert_eq!(
        TrayStatus::of(LinkState::SessionExpired),
        TrayStatus::Warning
    );
    assert_eq!(TrayStatus::of(LinkState::Offline), TrayStatus::Critical);
    assert_eq!(
        TrayStatus::of(LinkState::AccessRevoked),
        TrayStatus::Critical
    );
}

//! Présence hors de la fenêtre : limiteur de notifications système (BR-RESIL-015, 018) et état de
//! l'icône de la zone de notification (BR-RESIL-016). Règles pures, temps en paramètre.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use hearth_desktop_lib::presence::{
    Alert, AlertKind, LinkPresence, NOTIFY_WINDOW_MS, NotificationGate, TrayStatus,
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
    assert_eq!(gate.observe("a", LinkState::Reconnecting, 0), None);
    let off = gate.observe("a", LinkState::Offline, 30_000).unwrap();
    assert_eq!(off, alert("a", AlertKind::Offline, 0));
    // Une seule notification par minute et par serveur, toutes natures confondues (spec) : le retour
    // est retenu, annoncé à l'échéance, sans changement « absorbé » en plus.
    assert_eq!(gate.observe("a", LinkState::Connected, 40_000), None);
    assert!(gate.poll(40_000 + 1).is_empty());
    assert!(gate.poll(30_000 + MIN - 1).is_empty());
    assert_eq!(gate.poll(30_000 + MIN), [alert("a", AlertKind::Back, 0)]);
    assert!(gate.poll(10 * MIN).is_empty());
}

#[test]
fn a_return_after_a_long_outage_is_announced_at_once() {
    let mut gate = NotificationGate::default();
    gate.observe("a", LinkState::Offline, 0).unwrap();
    let back = gate.observe("a", LinkState::Connected, 2 * MIN).unwrap();
    assert_eq!(back, alert("a", AlertKind::Back, 0));
}

#[test]
fn a_short_blip_that_never_went_offline_says_nothing() {
    let mut gate = NotificationGate::default();
    assert_eq!(gate.observe("a", LinkState::Reconnecting, 0), None);
    assert_eq!(gate.observe("a", LinkState::Connected, 10_000), None);
    assert!(gate.poll(10 * MIN).is_empty());
}

#[test]
fn session_expired_and_revoked_never_notify() {
    let mut gate = NotificationGate::default();
    assert_eq!(gate.observe("a", LinkState::SessionExpired, 0), None);
    assert_eq!(gate.observe("a", LinkState::AccessRevoked, 1), None);
}

#[test]
fn ten_cuts_in_a_minute_give_one_notification_then_the_last_situation_with_its_count() {
    let mut gate = NotificationGate::default();
    let mut sent = Vec::new();
    for flap in 0..10_u64 {
        let at = flap * 4_000;
        sent.extend(gate.observe("a", LinkState::Offline, at));
        sent.extend(gate.observe("a", LinkState::Connected, at + 2_000));
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
    gate.observe("a", LinkState::Offline, 0).unwrap();
    // Un essai de reconnexion échoue : « Reconnexion… » puis de nouveau « Hors ligne ».
    assert_eq!(gate.observe("a", LinkState::Reconnecting, 5_000), None);
    assert_eq!(gate.observe("a", LinkState::Offline, 10_000), None);
    assert_eq!(gate.observe("a", LinkState::Offline, 3 * MIN), None);
    assert!(gate.poll(10 * MIN).is_empty());
}

#[test]
fn a_server_switched_off_for_a_day_or_a_week_gives_a_bounded_number_of_notifications() {
    // Invariant (revue round 2) : le nombre de notifications système d'une panne ne dépend pas de sa
    // durée. Tentatives toutes les 30 s, « Hors ligne » réannoncé à chacune, 24 h puis 7 jours.
    for days in [1_u64, 7] {
        let mut gate = NotificationGate::default();
        let mut sent = Vec::new();
        let end = days * 24 * 3_600_000;
        let mut at = 30_000;
        while at < end {
            // Chaque tentative : « Reconnexion… » le temps de l'essai, puis de nouveau « Hors ligne ».
            sent.extend(gate.observe("a", LinkState::Reconnecting, at));
            sent.extend(gate.observe("a", LinkState::Offline, at + 500));
            sent.extend(gate.poll(at + 500));
            at += 30_000;
        }
        assert_eq!(sent, [alert("a", AlertKind::Offline, 0)], "{days} j éteint");
        // Le retour : une de plus, et rien d'autre ensuite.
        sent.extend(gate.observe("a", LinkState::Connected, end));
        assert_eq!(sent.len(), 2, "{days} j : une pour la panne, une au retour");
        assert_eq!(sent[1].kind, AlertKind::Back);
        assert!(gate.poll(end + 10 * MIN).is_empty());
    }
}

#[test]
fn every_server_is_counted_on_its_own_and_a_new_outage_restarts_the_cycle() {
    let mut gate = NotificationGate::default();
    let mut counts = [0_u32; 2];
    let mut now = 0_u64;
    for cycle in 0..3 {
        for (n, name) in ["a", "b"].iter().enumerate() {
            let mut sent: Vec<Alert> = gate
                .observe(name, LinkState::Offline, now)
                .into_iter()
                .collect();
            for k in 1..2_000_u64 {
                sent.extend(gate.observe(name, LinkState::Offline, now + k * 30_000));
            }
            counts[n] += u32::try_from(sent.len()).unwrap();
            let back = gate.observe(name, LinkState::Connected, now + 3 * 24 * 3_600_000);
            counts[n] += u32::from(back.is_some());
        }
        now += 3 * 24 * 3_600_000 + 2 * MIN;
        // Les retours retenus par la fenêtre partent à l'échéance : on les compte aussi.
        let due = gate.poll(now);
        for alert in &due {
            counts[usize::from(alert.server == "b")] += 1;
        }
        let _ = cycle;
    }
    // Trois pannes de trois jours par serveur : une alerte de panne et une de retour chacune.
    assert_eq!(counts, [6, 6]);
}

#[test]
fn each_server_has_its_own_window() {
    let mut gate = NotificationGate::default();
    assert!(gate.observe("a", LinkState::Offline, 0).is_some());
    assert!(gate.observe("b", LinkState::Offline, 5_000).is_some());
    // « a » : le retour est retenu par SA fenêtre ; « b » n'est pas concerné.
    assert_eq!(gate.observe("a", LinkState::Connected, 6_000), None);
    assert!(gate.observe("b", LinkState::Connected, 70_000).is_some());
    assert_eq!(gate.poll(MIN), [alert("a", AlertKind::Back, 0)]);
}

#[test]
fn forgetting_a_server_bounds_the_memory() {
    let mut gate = NotificationGate::default();
    for n in 0..1_000 {
        let name = format!("srv{n}");
        gate.observe(&name, LinkState::Offline, 0);
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
        gate.observe("a", LinkState::Offline, at);
        gate.observe("a", LinkState::Connected, at + 90_000);
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

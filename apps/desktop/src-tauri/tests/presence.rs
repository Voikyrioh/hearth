//! Présence hors de la fenêtre : limiteur de notifications système (BR-RESIL-015, 018), état de
//! l'icône de la zone de notification (BR-RESIL-016) et pastille de couleur. Règles pures.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use hearth_desktop_lib::presence::{
    Alert, AlertKind, LinkPresence, NOTIFY_WINDOW_MS, NotificationGate, TrayStatus, paint_badge,
};
use hearth_link::domain::state::LinkState;

const MIN: u64 = NOTIFY_WINDOW_MS;

#[test]
fn goes_offline_then_back_each_with_its_own_notification() {
    let mut gate = NotificationGate::default();
    assert_eq!(gate.observe("a", LinkState::Reconnecting, 0), None);
    let off = gate.observe("a", LinkState::Offline, 30_000).unwrap();
    assert_eq!(off.kind, AlertKind::Offline);
    assert_eq!(off.suppressed, 0);
    // Le retour a sa propre limite : il part tout de suite.
    let back = gate.observe("a", LinkState::Connected, 40_000).unwrap();
    assert_eq!(
        back,
        Alert {
            server: "a".into(),
            kind: AlertKind::Back,
            suppressed: 0
        }
    );
    assert!(gate.is_idle());
}

#[test]
fn a_short_blip_that_never_went_offline_says_nothing() {
    let mut gate = NotificationGate::default();
    assert_eq!(gate.observe("a", LinkState::Reconnecting, 0), None);
    assert_eq!(gate.observe("a", LinkState::Connected, 10_000), None);
    assert!(gate.is_idle());
}

#[test]
fn session_expired_and_revoked_never_notify() {
    let mut gate = NotificationGate::default();
    assert_eq!(gate.observe("a", LinkState::SessionExpired, 0), None);
    assert_eq!(gate.observe("a", LinkState::AccessRevoked, 1), None);
}

#[test]
fn ten_cuts_in_a_minute_give_at_most_one_notification_of_each_kind() {
    let mut gate = NotificationGate::default();
    let mut sent = Vec::new();
    for flap in 0..10_u64 {
        let at = flap * 4_000;
        sent.extend(gate.observe("a", LinkState::Offline, at));
        sent.extend(gate.observe("a", LinkState::Connected, at + 2_000));
    }
    // Dix coupures en 40 s : « hors ligne » puis « de retour », une fois chacune.
    assert_eq!(
        sent.iter().map(|alert| alert.kind).collect::<Vec<_>>(),
        [AlertKind::Offline, AlertKind::Back]
    );
    // Ce qui était retenu est périmé (le dernier mot est « de retour », déjà dit) : rien ensuite.
    assert!(gate.poll(MIN).is_empty());
    assert!(gate.poll(10 * MIN).is_empty());
    assert!(gate.is_idle());
}

#[test]
fn a_retained_notification_is_announced_once_with_the_count_it_absorbed() {
    let mut gate = NotificationGate::default();
    gate.observe("a", LinkState::Offline, 0).unwrap();
    gate.observe("a", LinkState::Connected, 1_000).unwrap();
    // Hors ligne de nouveau dans la minute de la première alerte : retenu.
    assert_eq!(gate.observe("a", LinkState::Offline, 2_000), None);
    assert!(!gate.is_idle());
    assert!(gate.poll(MIN - 1).is_empty());
    assert_eq!(
        gate.poll(MIN),
        [Alert {
            server: "a".into(),
            kind: AlertKind::Offline,
            suppressed: 1
        }]
    );
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
    assert!(gate.is_idle());
}

#[test]
fn each_server_has_its_own_window() {
    let mut gate = NotificationGate::default();
    assert!(gate.observe("a", LinkState::Offline, 0).is_some());
    assert!(gate.observe("b", LinkState::Offline, 5_000).is_some());
    assert!(gate.observe("a", LinkState::Connected, 6_000).is_some());
    // « a » : hors ligne de nouveau dans sa minute ; « b » n'est pas concerné.
    assert_eq!(gate.observe("a", LinkState::Offline, 7_000), None);
    assert!(gate.observe("b", LinkState::Connected, 70_000).is_some());
    assert_eq!(gate.poll(MIN).len(), 1);
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
}

#[test]
fn a_displayed_server_without_a_known_state_counts_as_reconnecting() {
    let mut presence = LinkPresence::default();
    presence.set_displayed(Some("new"));
    assert_eq!(presence.status(), TrayStatus::Warning);
}

#[test]
fn every_state_has_a_status_and_the_colors_follow_the_pill() {
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
    assert_eq!(TrayStatus::Idle.color(), None);
    assert_eq!(TrayStatus::Connected.color(), Some([0x7e, 0xd3, 0x9a]));
    assert_eq!(TrayStatus::Warning.color(), Some([0xff, 0xc0, 0x4d]));
    assert_eq!(TrayStatus::Critical.color(), Some([0xff, 0x5a, 0x5a]));
}

#[test]
fn the_badge_is_painted_in_the_bottom_right_corner_only() {
    let (w, h) = (16_u32, 16_u32);
    let blank = vec![0_u8; (w * h * 4) as usize];
    let painted = paint_badge(&blank, w, h, TrayStatus::Critical);
    let at = |x: u32, y: u32| {
        let i = ((y * w + x) * 4) as usize;
        [painted[i], painted[i + 1], painted[i + 2], painted[i + 3]]
    };
    assert_eq!(at(12, 12), [0xff, 0x5a, 0x5a, 0xff], "pastille");
    assert_eq!(at(1, 1), [0, 0, 0, 0], "le reste n'est pas touché");
    // Neutre : l'image est rendue telle quelle.
    assert_eq!(paint_badge(&blank, w, h, TrayStatus::Idle), blank);
    // Taille incohérente : rendue telle quelle, sans panique.
    assert_eq!(
        paint_badge(&blank[..10], w, h, TrayStatus::Critical),
        &blank[..10]
    );
}

#[test]
fn the_three_colors_differ() {
    let blank = vec![0_u8; 16 * 16 * 4];
    let a = paint_badge(&blank, 16, 16, TrayStatus::Connected);
    let b = paint_badge(&blank, 16, 16, TrayStatus::Warning);
    let c = paint_badge(&blank, 16, 16, TrayStatus::Critical);
    assert_ne!(a, b);
    assert_ne!(b, c);
    assert_ne!(a, c);
}

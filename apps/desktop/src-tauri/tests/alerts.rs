//! Notifications système et icône de la zone de notification : la colle (`Alerts`) avec des ports
//! espions. Les règles pures (limite, agrégation, état de l'icône) sont dans `presence.rs`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use hearth_desktop_lib::alerts::{Alerts, Notifier, TrayPort};
use hearth_desktop_lib::link::StateObserver;
use hearth_desktop_lib::presence::{NOTIFY_WINDOW_MS, TrayIcon, TrayStatus};
use hearth_link::domain::state::LinkState;

#[derive(Default)]
struct Spy {
    notes: Mutex<Vec<(String, String)>>,
    icons: Mutex<Vec<(TrayStatus, String)>>,
    images: Mutex<Vec<TrayIcon>>,
}

impl Notifier for Spy {
    fn notify(&self, title: &str, body: &str) {
        self.notes
            .lock()
            .unwrap()
            .push((title.to_owned(), body.to_owned()));
    }
}

impl TrayPort for Spy {
    fn show_icon(&self, icon: TrayIcon) {
        self.images.lock().unwrap().push(icon);
    }

    fn show(&self, status: TrayStatus, tooltip: &str) {
        self.icons
            .lock()
            .unwrap()
            .push((status, tooltip.to_owned()));
    }
}

impl Spy {
    fn bodies(&self) -> Vec<String> {
        self.notes
            .lock()
            .unwrap()
            .iter()
            .map(|(_, body)| body.clone())
            .collect()
    }

    fn last_image(&self) -> Option<TrayIcon> {
        self.images.lock().unwrap().last().copied()
    }

    fn last_icon(&self) -> Option<(TrayStatus, String)> {
        self.icons.lock().unwrap().last().cloned()
    }
}

fn setup(enabled: bool) -> (Arc<Alerts>, Arc<Spy>, Arc<AtomicU64>) {
    let spy = Arc::new(Spy::default());
    let clock = Arc::new(AtomicU64::new(0));
    let reading = clock.clone();
    let alerts = Arc::new(Alerts::new(spy.clone(), spy.clone(), enabled, move || {
        reading.load(Ordering::SeqCst)
    }));
    (alerts, spy, clock)
}

#[test]
fn going_offline_notifies_at_once_and_the_return_leaves_on_the_tick_after_the_window() {
    let (alerts, spy, clock) = setup(true);
    alerts.on_state("a", "forge", LinkState::Connected, 0);
    alerts.on_state("a", "forge", LinkState::Reconnecting, 2);
    assert!(spy.bodies().is_empty(), "rien avant « Hors ligne »");
    clock.store(30_000, Ordering::SeqCst);
    alerts.on_state("a", "forge", LinkState::Offline, 4);
    clock.store(45_000, Ordering::SeqCst);
    alerts.on_state("a", "forge", LinkState::Connected, 0);
    // Une par minute et par serveur : le retour attend l'échéance.
    assert_eq!(spy.bodies(), ["forge est hors ligne."]);
    alerts.tick();
    assert_eq!(spy.bodies().len(), 1, "pas avant l'échéance");
    clock.store(30_000 + NOTIFY_WINDOW_MS, Ordering::SeqCst);
    alerts.tick();
    assert_eq!(
        spy.bodies(),
        ["forge est hors ligne.", "forge est de nouveau connecté."]
    );
    assert!(spy.notes.lock().unwrap().iter().all(|(t, _)| t == "Hearth"));
    alerts.tick();
    assert_eq!(spy.bodies().len(), 2, "une seule fois");
}

#[test]
fn repeated_failures_never_notify_the_system_however_long_the_server_stays_off() {
    let (alerts, spy, clock) = setup(true);
    alerts.on_state("a", "forge", LinkState::Offline, 5);
    assert_eq!(spy.bodies(), ["forge est hors ligne."]);
    // Une semaine éteint, une tentative toutes les 30 s : le compteur d'échecs monte, aucune
    // notification système de plus (le compteur vit dans les notifications de l'application).
    for n in 6..20_000_u32 {
        clock.store(u64::from(n) * 30_000, Ordering::SeqCst);
        alerts.on_state("a", "forge", LinkState::Offline, n);
        alerts.tick();
    }
    assert_eq!(spy.bodies().len(), 1);
    clock.store(20_000 * 30_000, Ordering::SeqCst);
    alerts.on_state("a", "forge", LinkState::Connected, 0);
    assert_eq!(spy.bodies().len(), 2);
}

#[test]
fn a_retained_notification_carries_the_changes_it_absorbed() {
    let (alerts, spy, clock) = setup(true);
    alerts.on_state("a", "forge", LinkState::Offline, 0);
    // Deux coupures de plus dans la minute : retenues.
    for at in [1_000, 3_000] {
        clock.store(at, Ordering::SeqCst);
        alerts.on_state("a", "forge", LinkState::Connected, 0);
        clock.store(at + 1_000, Ordering::SeqCst);
        alerts.on_state("a", "forge", LinkState::Offline, 0);
    }
    clock.store(10_000, Ordering::SeqCst);
    alerts.on_state("a", "forge", LinkState::Connected, 0);
    assert_eq!(spy.bodies().len(), 1);
    clock.store(NOTIFY_WINDOW_MS, Ordering::SeqCst);
    alerts.tick();
    assert_eq!(
        spy.bodies().last().unwrap(),
        "forge est de nouveau connecté. Le lien a changé 4 fois depuis la dernière alerte."
    );
}

#[test]
fn disabled_notifications_say_nothing_but_the_icon_still_follows() {
    let (alerts, spy, _) = setup(false);
    alerts.on_state("a", "forge", LinkState::Offline, 0);
    alerts.on_state("a", "forge", LinkState::Connected, 0);
    assert!(spy.bodies().is_empty());
    assert_eq!(spy.last_icon().unwrap().0, TrayStatus::Connected);
    // Réactivées : plus rien d'ancien ne ressort.
    alerts.set_enabled(true);
    alerts.tick();
    assert!(spy.bodies().is_empty());
    alerts.on_state("a", "forge", LinkState::Offline, 0);
    assert_eq!(spy.bodies(), ["forge est hors ligne."]);
}

#[test]
fn turning_notifications_off_drops_what_was_retained() {
    let (alerts, spy, clock) = setup(true);
    alerts.on_state("a", "forge", LinkState::Offline, 0);
    alerts.on_state("a", "forge", LinkState::Connected, 0);
    alerts.set_enabled(false);
    clock.store(NOTIFY_WINDOW_MS, Ordering::SeqCst);
    alerts.tick();
    assert_eq!(spy.bodies().len(), 1, "rien ne part une fois coupées");
}

#[test]
fn the_icon_follows_the_displayed_server_with_a_tooltip_and_is_only_redrawn_on_change() {
    let (alerts, spy, _) = setup(true);
    assert_eq!(spy.last_icon(), None);
    alerts.on_state("a", "forge", LinkState::Connected, 0);
    assert_eq!(
        spy.last_icon(),
        Some((TrayStatus::Connected, "Hearth : forge, Connecté".into()))
    );
    alerts.on_state("b", "nas-salon", LinkState::Offline, 0);
    // Aucun serveur affiché : le pire état de tous.
    assert_eq!(
        spy.last_icon(),
        Some((
            TrayStatus::Critical,
            "Hearth : nas-salon, Hors ligne".into()
        ))
    );
    alerts.set_displayed(Some("a"));
    assert_eq!(
        spy.last_icon(),
        Some((TrayStatus::Connected, "Hearth : forge, Connecté".into()))
    );
    let drawn = spy.icons.lock().unwrap().len();
    // Un état inchangé ne redessine rien.
    alerts.on_state("a", "forge", LinkState::Connected, 0);
    alerts.on_state("b", "nas-salon", LinkState::Offline, 0);
    assert_eq!(spy.icons.lock().unwrap().len(), drawn);
    alerts.on_state("a", "forge", LinkState::Reconnecting, 1);
    assert_eq!(spy.last_icon().unwrap().0, TrayStatus::Warning);
    alerts.on_state("a", "forge", LinkState::SessionExpired, 0);
    assert_eq!(
        spy.last_icon(),
        Some((
            TrayStatus::Warning,
            "Hearth : forge, Session expirée".into()
        ))
    );
    alerts.on_state("a", "forge", LinkState::AccessRevoked, 0);
    assert_eq!(spy.last_icon().unwrap().0, TrayStatus::Critical);
}

#[test]
fn each_of_the_six_states_asks_for_its_own_image() {
    let (alerts, spy, _) = setup(true);
    let mut seen = Vec::new();
    for (state, expected) in [
        (LinkState::Connected, TrayIcon::Connected),
        (LinkState::Reconnecting, TrayIcon::Reconnecting),
        (LinkState::Offline, TrayIcon::Offline),
        (LinkState::SessionExpired, TrayIcon::SessionExpired),
        (LinkState::AccessRevoked, TrayIcon::AccessRevoked),
    ] {
        alerts.on_state("a", "forge", state, 0);
        assert_eq!(spy.last_image(), Some(expected), "{state:?}");
        seen.push(expected);
    }
    alerts.on_removed("a");
    assert_eq!(spy.last_image(), Some(TrayIcon::NoServer));
    seen.push(TrayIcon::NoServer);
    // Six demandes, six images différentes : deux états ne partagent pas la leur (la couleur seule
    // ne doit pas les distinguer).
    for (at, image) in seen.iter().enumerate() {
        assert!(
            !seen[..at].contains(image),
            "{image:?} est demandée pour deux états différents"
        );
    }
}

#[test]
fn removing_the_displayed_server_falls_back_then_goes_neutral() {
    let (alerts, spy, _) = setup(true);
    alerts.on_state("a", "forge", LinkState::Connected, 0);
    alerts.on_state("b", "nas-salon", LinkState::Reconnecting, 1);
    alerts.set_displayed(Some("a"));
    alerts.on_removed("a");
    assert_eq!(spy.last_icon().unwrap().0, TrayStatus::Warning);
    alerts.on_removed("b");
    assert_eq!(
        spy.last_icon(),
        Some((TrayStatus::Idle, "Hearth".into())),
        "plus aucun serveur : icône neutre"
    );
}

#[test]
fn each_server_is_independent() {
    let (alerts, spy, clock) = setup(true);
    alerts.on_state("a", "forge", LinkState::Offline, 0);
    clock.store(1_000, Ordering::SeqCst);
    alerts.on_state("b", "nas-salon", LinkState::Offline, 0);
    assert_eq!(
        spy.bodies(),
        ["forge est hors ligne.", "nas-salon est hors ligne."]
    );
}

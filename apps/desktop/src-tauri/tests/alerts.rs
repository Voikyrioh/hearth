//! Notifications système et icône de la zone de notification : la colle (`Alerts`) avec des ports
//! espions. Les règles pures (limite, agrégation, état de l'icône) sont dans `presence.rs`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use hearth_desktop_lib::alerts::{Alerts, Notifier, TrayPort};
use hearth_desktop_lib::link::StateObserver;
use hearth_desktop_lib::presence::{NOTIFY_WINDOW_MS, TrayStatus};
use hearth_link::domain::state::LinkState;

#[derive(Default)]
struct Spy {
    notes: Mutex<Vec<(String, String)>>,
    icons: Mutex<Vec<(TrayStatus, String)>>,
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
fn going_offline_then_back_notifies_with_the_server_name() {
    let (alerts, spy, clock) = setup(true);
    alerts.on_state("a", "forge", LinkState::Connected);
    alerts.on_state("a", "forge", LinkState::Reconnecting);
    assert!(spy.bodies().is_empty(), "rien avant « Hors ligne »");
    clock.store(30_000, Ordering::SeqCst);
    alerts.on_state("a", "forge", LinkState::Offline);
    clock.store(45_000, Ordering::SeqCst);
    alerts.on_state("a", "forge", LinkState::Connected);
    assert_eq!(
        spy.bodies(),
        ["forge est hors ligne.", "forge est de nouveau connecté."]
    );
    assert!(spy.notes.lock().unwrap().iter().all(|(t, _)| t == "Hearth"));
}

#[test]
fn a_retained_notification_leaves_on_the_tick_with_its_count() {
    let (alerts, spy, clock) = setup(true);
    alerts.on_state("a", "forge", LinkState::Offline);
    alerts.on_state("a", "forge", LinkState::Connected);
    clock.store(2_000, Ordering::SeqCst);
    alerts.on_state("a", "forge", LinkState::Offline);
    assert_eq!(spy.bodies().len(), 2, "la troisième est retenue");
    alerts.tick();
    assert_eq!(spy.bodies().len(), 2, "pas avant l'échéance");
    clock.store(NOTIFY_WINDOW_MS, Ordering::SeqCst);
    alerts.tick();
    assert_eq!(
        spy.bodies().last().unwrap(),
        "forge est hors ligne. Le lien a changé 1 fois depuis la dernière alerte."
    );
    alerts.tick();
    assert_eq!(spy.bodies().len(), 3, "une seule fois");
}

#[test]
fn disabled_notifications_say_nothing_but_the_icon_still_follows() {
    let (alerts, spy, _) = setup(false);
    alerts.on_state("a", "forge", LinkState::Offline);
    alerts.on_state("a", "forge", LinkState::Connected);
    assert!(spy.bodies().is_empty());
    assert_eq!(spy.last_icon().unwrap().0, TrayStatus::Connected);
    // Réactivées : plus rien d'ancien ne ressort.
    alerts.set_enabled(true);
    alerts.tick();
    assert!(spy.bodies().is_empty());
    alerts.on_state("a", "forge", LinkState::Offline);
    assert_eq!(spy.bodies(), ["forge est hors ligne."]);
}

#[test]
fn turning_notifications_off_drops_what_was_retained() {
    let (alerts, spy, clock) = setup(true);
    alerts.on_state("a", "forge", LinkState::Offline);
    alerts.on_state("a", "forge", LinkState::Connected);
    alerts.on_state("a", "forge", LinkState::Offline);
    alerts.set_enabled(false);
    clock.store(NOTIFY_WINDOW_MS, Ordering::SeqCst);
    alerts.tick();
    assert_eq!(spy.bodies().len(), 2, "rien ne part une fois coupées");
}

#[test]
fn the_icon_follows_the_displayed_server_with_a_tooltip_and_is_only_redrawn_on_change() {
    let (alerts, spy, _) = setup(true);
    assert_eq!(spy.last_icon(), None);
    alerts.on_state("a", "forge", LinkState::Connected);
    assert_eq!(
        spy.last_icon(),
        Some((TrayStatus::Connected, "Hearth : forge, Connecté".into()))
    );
    alerts.on_state("b", "nas-salon", LinkState::Offline);
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
    alerts.on_state("a", "forge", LinkState::Connected);
    alerts.on_state("b", "nas-salon", LinkState::Offline);
    assert_eq!(spy.icons.lock().unwrap().len(), drawn);
    alerts.on_state("a", "forge", LinkState::Reconnecting);
    assert_eq!(spy.last_icon().unwrap().0, TrayStatus::Warning);
    alerts.on_state("a", "forge", LinkState::SessionExpired);
    assert_eq!(
        spy.last_icon(),
        Some((
            TrayStatus::Warning,
            "Hearth : forge, Session expirée".into()
        ))
    );
    alerts.on_state("a", "forge", LinkState::AccessRevoked);
    assert_eq!(spy.last_icon().unwrap().0, TrayStatus::Critical);
}

#[test]
fn removing_the_displayed_server_falls_back_then_goes_neutral() {
    let (alerts, spy, _) = setup(true);
    alerts.on_state("a", "forge", LinkState::Connected);
    alerts.on_state("b", "nas-salon", LinkState::Reconnecting);
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
    alerts.on_state("a", "forge", LinkState::Offline);
    clock.store(1_000, Ordering::SeqCst);
    alerts.on_state("b", "nas-salon", LinkState::Offline);
    assert_eq!(
        spy.bodies(),
        ["forge est hors ligne.", "nas-salon est hors ligne."]
    );
}

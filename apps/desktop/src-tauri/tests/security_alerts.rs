//! Alertes de sécurité (HRT-26, BR-TRUST-009, 019, 033) : la détection des épisodes, la limite d'une
//! notification par minute et par serveur partagée avec le lien (la sécurité d'abord), la colle
//! `Alerts` avec des ports espions, et le réglage « Alertes de sécurité » séparé de celui du lien.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use hearth_desktop_lib::alerts::{Alerts, Notifier, TrayPort};
use hearth_desktop_lib::link::StateObserver;
use hearth_desktop_lib::presence::{
    Alert, AlertKind, ModeKind, NOTIFY_WINDOW_MS, NotificationGate, SecurityNotice, SecurityWatch,
    SecurityWording, TrayIcon, TrayStatus,
};
use hearth_link::domain::state::LinkState;

const MIN: u64 = NOTIFY_WINDOW_MS;

fn alert(server: &str, kind: AlertKind) -> Alert {
    Alert {
        server: server.into(),
        kind,
        suppressed: 0,
    }
}

fn notice(alert_visible: bool, mode: ModeKind, auto: bool) -> SecurityNotice {
    SecurityNotice {
        wording: owner(),
        alert_visible,
        mode,
        ended_automatically: auto,
    }
}

fn owner() -> SecurityWording {
    SecurityWording::Owner {
        username: "marie".into(),
    }
}

// ── Règles pures ────────────────────────────────────────────────────────────────────────────

#[test]
fn an_alert_episode_notifies_once_however_many_messages_repeat_it() {
    let mut watch = SecurityWatch::default();
    assert!(
        !watch
            .observe("a", &notice(false, ModeKind::Off, false))
            .alert_started
    );
    assert!(
        watch
            .observe("a", &notice(true, ModeKind::Off, false))
            .alert_started
    );
    for _ in 0..5 {
        let again = watch.observe("a", &notice(true, ModeKind::Off, false));
        assert!(!again.alert_started && !again.alert_ended);
    }
    assert!(
        watch
            .observe("a", &notice(false, ModeKind::Off, false))
            .alert_ended
    );
    // Un nouvel épisode notifie de nouveau.
    assert!(
        watch
            .observe("a", &notice(true, ModeKind::Off, false))
            .alert_started
    );
}

#[test]
fn the_automatic_end_is_announced_only_after_having_seen_the_mode_on() {
    let mut watch = SecurityWatch::default();
    // Un mode déjà arrêté tout seul avant que l'application ne le connaisse : rien à dire.
    assert!(
        !watch
            .observe("a", &notice(false, ModeKind::Off, true))
            .mode_stopped_by_agent
    );
    watch.observe("a", &notice(false, ModeKind::Active, false));
    // Actif puis suspendu puis actif : ce n'est pas une fin.
    assert!(
        !watch
            .observe("a", &notice(false, ModeKind::Suspended, false))
            .mode_stopped_by_agent
    );
    watch.observe("a", &notice(false, ModeKind::Active, false));
    assert!(
        watch
            .observe("a", &notice(false, ModeKind::Off, true))
            .mode_stopped_by_agent
    );
    // Une désactivation à la main n'est pas une fin automatique.
    watch.observe("a", &notice(false, ModeKind::Active, false));
    assert!(
        !watch
            .observe("a", &notice(false, ModeKind::Off, false))
            .mode_stopped_by_agent
    );
}

#[test]
fn a_security_notification_shares_the_one_per_minute_window_with_the_link() {
    let mut gate = NotificationGate::default();
    let sent = gate
        .push_security("a", AlertKind::AttackProbable, 0)
        .unwrap();
    assert_eq!(sent, alert("a", AlertKind::AttackProbable));
    // Hors ligne dans la même minute : retenu, toutes natures confondues.
    assert_eq!(gate.observe("a", LinkState::Offline, 10_000), None);
    assert_eq!(gate.poll(MIN), [alert("a", AlertKind::Offline)]);
}

#[test]
fn a_retained_security_notification_goes_first_and_the_link_waits_behind_it() {
    let mut gate = NotificationGate::default();
    gate.observe("a", LinkState::Offline, 0).unwrap();
    // L'alerte arrive dans la minute : retenue. Le retour du lien aussi : il attend derrière elle.
    assert_eq!(
        gate.push_security("a", AlertKind::AttackProbable, 5_000),
        None
    );
    assert_eq!(gate.observe("a", LinkState::Connected, 6_000), None);
    assert!(gate.poll(MIN - 1).is_empty());
    assert_eq!(gate.poll(MIN), [alert("a", AlertKind::AttackProbable)]);
    // Le lien n'a pas été remplacé : il part à l'échéance suivante.
    assert!(gate.poll(MIN + 1).is_empty());
    assert_eq!(gate.poll(2 * MIN), [alert("a", AlertKind::Back)]);
}

#[test]
fn a_link_change_never_replaces_a_retained_security_notification() {
    let mut gate = NotificationGate::default();
    gate.push_security("a", AlertKind::AttackProbable, 0)
        .unwrap();
    assert_eq!(
        gate.push_security("a", AlertKind::AttackModeStopped, 1_000),
        None
    );
    assert_eq!(gate.observe("a", LinkState::Offline, 2_000), None);
    assert_eq!(gate.observe("a", LinkState::Connected, 3_000), None);
    // Une seule notification par minute : la sécurité d'abord, une à chaque échéance.
    assert_eq!(gate.poll(MIN), [alert("a", AlertKind::AttackModeStopped)]);
}

#[test]
fn the_attack_notification_goes_before_the_end_of_the_mode() {
    let mut gate = NotificationGate::default();
    gate.observe("a", LinkState::Offline, 0).unwrap();
    assert_eq!(
        gate.push_security("a", AlertKind::AttackModeStopped, 1_000),
        None
    );
    assert_eq!(
        gate.push_security("a", AlertKind::AttackProbable, 2_000),
        None
    );
    assert_eq!(gate.poll(MIN), [alert("a", AlertKind::AttackProbable)]);
    assert_eq!(
        gate.poll(2 * MIN),
        [alert("a", AlertKind::AttackModeStopped)]
    );
}

#[test]
fn an_alert_that_ended_before_its_turn_is_dropped_and_clearing_security_keeps_the_link() {
    let mut gate = NotificationGate::default();
    gate.observe("a", LinkState::Offline, 0).unwrap();
    gate.push_security("a", AlertKind::AttackProbable, 1_000);
    gate.drop_security("a", AlertKind::AttackProbable);
    assert!(gate.poll(MIN).is_empty());
    gate.push_security("a", AlertKind::AttackProbable, MIN + 1);
    gate.observe("a", LinkState::Connected, MIN + 2);
    gate.clear_security();
    assert_eq!(gate.poll(3 * MIN), [alert("a", AlertKind::Back)]);
}

#[test]
fn clearing_the_link_keeps_a_retained_security_notification() {
    let mut gate = NotificationGate::default();
    gate.push_security("a", AlertKind::AttackProbable, 0)
        .unwrap();
    gate.push_security("a", AlertKind::AttackModeStopped, 1_000);
    gate.clear_link();
    assert_eq!(gate.poll(MIN), [alert("a", AlertKind::AttackModeStopped)]);
}

// ── La colle `Alerts` ───────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct Spy {
    notes: Mutex<Vec<(String, String)>>,
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
    fn show_icon(&self, _icon: TrayIcon) {}

    fn show(&self, _status: TrayStatus, _tooltip: &str) {}
}

impl Spy {
    fn notes(&self) -> Vec<(String, String)> {
        self.notes.lock().unwrap().clone()
    }

    fn bodies(&self) -> Vec<String> {
        self.notes().into_iter().map(|(_, body)| body).collect()
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
fn an_attack_episode_notifies_once_with_the_server_in_the_title_and_the_account_in_the_text() {
    let (alerts, spy, _clock) = setup(true);
    alerts.on_security("a", "forge", &notice(true, ModeKind::Off, false));
    alerts.on_security("a", "forge", &notice(true, ModeKind::Off, false));
    assert_eq!(
        spy.notes(),
        [(
            "Hearth : forge".to_owned(),
            "Attaque probable sur ton compte marie. Clique pour activer le mode attaque."
                .to_owned()
        )]
    );
}

#[test]
fn the_text_says_details_when_the_account_cannot_activate_and_a_count_for_the_others() {
    let (alerts, spy, clock) = setup(true);
    let details = SecurityNotice {
        wording: SecurityWording::Details {
            username: "marie".into(),
        },
        ..notice(true, ModeKind::Off, false)
    };
    alerts.on_security("a", "forge", &details);
    clock.store(10 * MIN, Ordering::SeqCst);
    let others = SecurityNotice {
        wording: SecurityWording::Others { count: 2 },
        ..notice(true, ModeKind::Off, false)
    };
    alerts.on_security("b", "salon", &others);
    let bodies = spy.bodies();
    assert_eq!(
        bodies[0],
        "Attaque probable sur ton compte marie. Clique pour voir les détails."
    );
    assert_eq!(
        bodies[1],
        "Attaque probable sur 2 comptes du serveur. Clique pour voir les détails."
    );
    // Un seul autre compte : le singulier.
    clock.store(20 * MIN, Ordering::SeqCst);
    let one = SecurityNotice {
        wording: SecurityWording::Others { count: 1 },
        ..notice(true, ModeKind::Off, false)
    };
    alerts.on_security("c", "bureau", &one);
    assert_eq!(
        spy.bodies()[2],
        "Attaque probable sur 1 compte du serveur. Clique pour voir les détails."
    );
}

#[test]
fn the_automatic_end_of_the_attack_mode_notifies() {
    let (alerts, spy, clock) = setup(true);
    alerts.on_security("a", "forge", &notice(false, ModeKind::Active, false));
    assert!(spy.notes().is_empty(), "un mode actif ne notifie pas");
    clock.store(MIN, Ordering::SeqCst);
    alerts.on_security("a", "forge", &notice(false, ModeKind::Off, true));
    assert_eq!(
        spy.notes(),
        [(
            "Hearth : forge".to_owned(),
            "L'attaque semble terminée. Le mode attaque s'est arrêté automatiquement.".to_owned()
        )]
    );
}

#[test]
fn the_security_setting_is_separate_from_the_link_setting() {
    // Réglage du lien coupé : la sécurité notifie quand même.
    let (alerts, spy, _clock) = setup(false);
    alerts.on_security("a", "forge", &notice(true, ModeKind::Off, false));
    assert_eq!(spy.notes().len(), 1);
    // Sécurité coupée : rien, et le lien reste ce qu'il est.
    let (alerts, spy, clock) = setup(true);
    alerts.set_security_enabled(false);
    alerts.on_security("a", "forge", &notice(true, ModeKind::Off, false));
    assert!(spy.notes().is_empty());
    clock.store(5, Ordering::SeqCst);
    alerts.on_state("a", "forge", LinkState::Offline, 4);
    assert_eq!(spy.bodies(), ["forge est hors ligne."]);
}

#[test]
fn a_disabled_security_setting_does_not_replay_the_running_episode_when_turned_back_on() {
    let (alerts, spy, _clock) = setup(true);
    alerts.set_security_enabled(false);
    alerts.on_security("a", "forge", &notice(true, ModeKind::Off, false));
    alerts.set_security_enabled(true);
    alerts.on_security("a", "forge", &notice(true, ModeKind::Off, false));
    assert!(spy.notes().is_empty(), "le même épisode ne notifie pas");
}

#[test]
fn a_security_notification_held_back_by_the_window_leaves_on_the_tick() {
    let (alerts, spy, clock) = setup(true);
    alerts.on_state("a", "forge", LinkState::Offline, 4);
    clock.store(10_000, Ordering::SeqCst);
    alerts.on_security("a", "forge", &notice(true, ModeKind::Off, false));
    assert_eq!(spy.bodies(), ["forge est hors ligne."]);
    clock.store(MIN, Ordering::SeqCst);
    alerts.tick();
    assert_eq!(spy.notes().len(), 2);
    assert_eq!(spy.notes()[1].0, "Hearth : forge");
}

#[test]
fn removing_a_server_forgets_its_security_episode() {
    let (alerts, spy, clock) = setup(true);
    alerts.on_security("a", "forge", &notice(true, ModeKind::Off, false));
    alerts.on_removed("a");
    clock.store(10 * MIN, Ordering::SeqCst);
    alerts.on_security("a", "forge", &notice(true, ModeKind::Off, false));
    assert_eq!(spy.notes().len(), 2, "un serveur rajouté repart de zéro");
}

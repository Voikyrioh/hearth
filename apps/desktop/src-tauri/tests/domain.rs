//! Règles pures de la coquille (BR-CLIENT-004, 005, 006, 007, 011).

use hearth_desktop_lib::domain::*;

#[test]
fn first_close_explains_then_stays_silent() {
    assert_eq!(on_close_requested(false), CloseOutcome::HideAndExplain);
    assert_eq!(on_close_requested(true), CloseOutcome::HideSilently);
}

#[test]
fn a_missing_or_malformed_flag_defaults_to_false() {
    assert!(!flag_from(None));
    assert!(!flag_from(Some(&serde_json::json!("oui"))));
    assert!(flag_from(Some(&serde_json::json!(true))));
}

#[test]
fn tray_menu_has_open_and_quit_only() {
    assert_eq!(tray_action(MENU_OPEN), Some(TrayAction::Open));
    assert_eq!(tray_action(MENU_QUIT), Some(TrayAction::Quit));
    assert_eq!(tray_action("autre"), None);
}

#[test]
fn minimized_launch_is_detected_from_the_startup_argument() {
    assert!(is_minimized_launch(["hearth.exe", MINIMIZED_FLAG]));
    assert!(!is_minimized_launch(["hearth.exe"]));
    assert!(!is_minimized_launch(Vec::<String>::new()));
}

#[test]
fn settings_serialize_in_camel_case() {
    let json = serde_json::to_value(Settings {
        launch_at_startup: true,
        close_hint_seen: false,
    });
    assert_eq!(
        json.ok(),
        Some(serde_json::json!({ "launchAtStartup": true, "closeHintSeen": false }))
    );
}

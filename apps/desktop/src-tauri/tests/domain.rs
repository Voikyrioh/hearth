//! Règles pures de la coquille (BR-CLIENT-004, 005, 006, 007, 011).
#![allow(clippy::unwrap_used, clippy::expect_used)] // tests : les helpers peuvent paniquer

use hearth_desktop_lib::domain::*;

#[test]
fn only_the_main_window_hides_on_close() {
    assert!(hides_on_close(MAIN_WINDOW));
    assert!(!hides_on_close("autre"));
}

#[test]
fn the_close_explanation_is_given_once() {
    assert!(should_explain_close(false));
    assert!(!should_explain_close(true));
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
fn settings_expose_only_the_startup_option() {
    let json = serde_json::to_value(Settings {
        launch_at_startup: true,
    });
    assert_eq!(
        json.ok(),
        Some(serde_json::json!({ "launchAtStartup": true }))
    );
}

#[test]
fn identifier_and_main_window_match_the_tauri_config() {
    let raw = std::fs::read_to_string("tauri.conf.json").unwrap();
    let config: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(config["identifier"], IDENTIFIER);
    assert_eq!(config["app"]["windows"][0]["label"], MAIN_WINDOW);
}

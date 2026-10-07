//! Fenêtre : explication de fermeture unique et mémorisée. Le runtime simulé de Tauri ne
//! rend pas la visibilité ni la destruction observables : « autre fenêtre = fermée » repose
//! sur `domain::hides_on_close` (testé) et la garde de `on_window_event`.
#![allow(clippy::unwrap_used, clippy::expect_used)] // tests : les helpers peuvent paniquer

#[path = "../../../../crates/hearth-agent/tests/support/tmp.rs"]
mod tmp;

use std::cell::Cell;

use hearth_desktop_lib::settings::close_hint_seen;
use hearth_desktop_lib::window::{hide_to_tray, on_window_event};
use tauri::test::{MockRuntime, mock_builder, mock_context, noop_assets};
use tauri::{WebviewUrl, WebviewWindowBuilder};

fn app() -> tauri::App<MockRuntime> {
    mock_builder()
        .plugin(tauri_plugin_store::Builder::new().build())
        .on_window_event(on_window_event)
        .build(mock_context(noop_assets()))
        .unwrap()
}

fn open(app: &tauri::App<MockRuntime>, label: &str) -> tauri::WebviewWindow<MockRuntime> {
    WebviewWindowBuilder::new(app, label, WebviewUrl::default())
        .build()
        .unwrap()
}

#[test]
fn closing_main_hides_it_and_explains_only_once() {
    let dir = tmp::tempdir().unwrap();
    let file = dir.path().join("settings.json");
    let app = app();
    let main = open(&app, "main");
    let asked = Cell::new(0);

    hide_to_tray(&main.as_ref().window(), &file, || {
        asked.set(asked.get() + 1)
    });
    assert_eq!(asked.get(), 1);
    assert!(close_hint_seen(app.handle(), &file).unwrap());

    hide_to_tray(&main.as_ref().window(), &file, || {
        asked.set(asked.get() + 1)
    });
    assert_eq!(asked.get(), 1, "pas de seconde explication");
}

#[test]
fn a_corrupt_settings_file_is_treated_as_missing_so_the_explanation_is_given_once_and_the_file_rewritten()
 {
    let dir = tmp::tempdir().unwrap();
    let file = dir.path().join("settings.json");
    std::fs::write(&file, "{ pas du json").unwrap();
    let app = app();
    let main = open(&app, "main");
    let asked = Cell::new(0);
    hide_to_tray(&main.as_ref().window(), &file, || {
        asked.set(asked.get() + 1)
    });
    hide_to_tray(&main.as_ref().window(), &file, || {
        asked.set(asked.get() + 1)
    });
    assert_eq!(asked.get(), 1);
    let rewritten: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(rewritten["closeHintSeen"], true);
}

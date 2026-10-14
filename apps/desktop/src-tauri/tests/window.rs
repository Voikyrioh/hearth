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

/// HRT-67 : l'événement d'une lecture de journaux va à la seule fenêtre qui l'a ouverte (`emit_to` par étiquette),
/// jamais aux autres fenêtres.
#[test]
fn a_window_sink_reaches_only_its_own_window() {
    use std::sync::{Arc, Mutex};

    use hearth_desktop_lib::link::UiSink;
    use hearth_desktop_lib::window::WindowSink;
    use tauri::Listener as _;

    let app = app();
    let main = open(&app, "main");
    let other = open(&app, "autre");
    let heard: Arc<Mutex<Vec<&'static str>>> = Arc::default();
    let log = heard.clone();
    main.listen("link://test", move |_| log.lock().unwrap().push("main"));
    let log = heard.clone();
    other.listen("link://test", move |_| log.lock().unwrap().push("autre"));

    let sink = WindowSink::new(app.handle().clone(), "main".to_owned());
    sink.emit("link://test", serde_json::json!({ "n": 1 }));
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert_eq!(*heard.lock().unwrap(), vec!["main"]);
}

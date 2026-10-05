//! Réglages : entrée de démarrage simulée, fichier de réglages réel (runtime Tauri simulé).
#![allow(clippy::unwrap_used, clippy::expect_used)] // tests : les helpers peuvent paniquer

use std::cell::Cell;

use hearth_desktop_lib::error::AppError;
use hearth_desktop_lib::settings::{
    Autostart, close_hint_seen, mark_close_hint_seen, read, set_launch_at_startup,
};
use tauri::test::{MockRuntime, mock_builder, mock_context, noop_assets};

#[derive(Default)]
struct FakeAutostart {
    on: Cell<bool>,
    broken: bool,
}

impl Autostart for FakeAutostart {
    fn is_enabled(&self) -> Result<bool, AppError> {
        if self.broken {
            return Err(AppError::Autostart("registre".into()));
        }
        Ok(self.on.get())
    }

    fn set_enabled(&self, enabled: bool) -> Result<(), AppError> {
        if self.broken {
            return Err(AppError::Autostart("registre".into()));
        }
        self.on.set(enabled);
        Ok(())
    }
}

fn app() -> tauri::App<MockRuntime> {
    mock_builder()
        .plugin(tauri_plugin_store::Builder::new().build())
        .build(mock_context(noop_assets()))
        .unwrap()
}

#[test]
fn startup_is_off_by_default_then_written_and_read_back() {
    let autostart = FakeAutostart::default();
    assert!(!read(&autostart).unwrap().launch_at_startup);
    assert!(
        set_launch_at_startup(&autostart, true)
            .unwrap()
            .launch_at_startup
    );
    assert!(read(&autostart).unwrap().launch_at_startup);
    assert!(
        !set_launch_at_startup(&autostart, false)
            .unwrap()
            .launch_at_startup
    );
}

#[test]
fn a_failing_startup_entry_is_a_typed_error() {
    let autostart = FakeAutostart {
        broken: true,
        ..FakeAutostart::default()
    };
    assert!(matches!(read(&autostart), Err(AppError::Autostart(_))));
    assert!(matches!(
        set_launch_at_startup(&autostart, true),
        Err(AppError::Autostart(_))
    ));
}

#[test]
fn the_close_hint_flag_is_remembered_in_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("settings.json");
    let app = app();
    assert!(!close_hint_seen(app.handle(), &file).unwrap());
    mark_close_hint_seen(app.handle(), &file).unwrap();
    assert!(close_hint_seen(app.handle(), &file).unwrap());
    assert!(
        std::fs::read_to_string(&file)
            .unwrap()
            .contains("closeHintSeen")
    );
}

#[test]
fn a_corrupt_settings_file_reads_as_never_seen_and_never_blocks_the_startup_setting() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("settings.json");
    std::fs::write(&file, "{ pas du json").unwrap();
    let app = app();
    // Le greffon avale l'erreur de lecture : un fichier corrompu vaut « jamais vu ».
    assert!(!close_hint_seen(app.handle(), &file).unwrap());
    // Le réglage de démarrage ne lit jamais ce fichier.
    let autostart = FakeAutostart::default();
    assert!(set_launch_at_startup(&autostart, true).is_ok());
}

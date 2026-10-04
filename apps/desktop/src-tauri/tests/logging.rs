//! Journal du client : fichier dans le dossier demandé, panique écrite dedans.
//! Le journal et le crochet de panique sont globaux au processus : un seul test les initialise.
#![allow(clippy::unwrap_used, clippy::expect_used)] // tests : les helpers peuvent paniquer

use std::path::Path;

use hearth_desktop_lib::logging::{KEPT_FILES, init_in, log_dir};
use hearth_desktop_lib::texts::startup_failed_body;

fn read_all(dir: &Path) -> String {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap())
        .collect()
}

#[test]
fn logs_go_to_a_rotating_file_and_panics_are_recorded() {
    let dir = tempfile::tempdir().unwrap();
    init_in(dir.path()).unwrap();

    tracing::info!("ligne de test");
    let _ = std::panic::catch_unwind(|| panic!("boum de test"));

    let name = std::fs::read_dir(dir.path())
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .file_name()
        .to_string_lossy()
        .into_owned();
    assert!(
        name.starts_with("hearth.") && name.ends_with(".log"),
        "{name}"
    );
    let content = read_all(dir.path());
    assert!(content.contains("ligne de test"));
    assert!(
        content.contains("boum de test"),
        "la panique doit être journalisée"
    );
    assert_eq!(KEPT_FILES, 7);
}

#[test]
fn the_standard_log_dir_is_under_the_app_data_dir() {
    let dir = log_dir();
    assert!(dir.ends_with("logs"));
    assert!(
        dir.parent()
            .is_some_and(|p| p.ends_with("fr.voikyrioh.hearth"))
    );
}

#[test]
fn the_startup_failure_message_names_the_log_dir() {
    let body = startup_failed_body("zone de notification", Path::new("C:/x/logs"));
    assert!(body.contains("zone de notification"));
    assert!(body.contains("C:/x/logs"));
}

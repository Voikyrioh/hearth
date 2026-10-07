//! Journal du client : fichier dans le dossier demandé, panique écrite dedans.
//! Le journal et le crochet de panique sont globaux au processus : un seul test les initialise.
#![allow(clippy::unwrap_used, clippy::expect_used)] // tests : les helpers peuvent paniquer

#[path = "../../../../crates/hearth-agent/tests/support/tmp.rs"]
mod tmp;

use std::path::Path;

use hearth_desktop_lib::logging::{KEPT_FILES, MAX_TOTAL_BYTES, enforce_cap, init_in, log_dir};
use hearth_desktop_lib::texts::startup_failed_body;

fn read_all(dir: &Path) -> String {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap())
        .collect()
}

#[test]
fn logs_go_to_a_rotating_file_and_panics_are_recorded() {
    let dir = tmp::tempdir().unwrap();
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
    let body = startup_failed_body("zone de notification", Path::new("C:/x/logs"), None);
    assert!(body.contains("zone de notification"));
    assert!(body.contains("C:/x/logs"));
}

#[test]
fn the_startup_failure_message_says_when_the_log_could_not_be_written() {
    let body = startup_failed_body("fenêtre", Path::new("C:/x/logs"), Some("accès refusé"));
    assert!(body.contains("fenêtre"));
    assert!(body.contains("accès refusé"));
    assert!(
        !body.contains("C:/x/logs"),
        "ne pas renvoyer vers un journal inexistant"
    );
}

#[test]
fn an_unwritable_log_dir_is_an_error() {
    let dir = tmp::tempdir().unwrap();
    let blocker = dir.path().join("fichier");
    std::fs::write(&blocker, "x").unwrap();
    // Un fichier à la place du dossier : impossible d'y écrire un journal.
    assert!(init_in(&blocker.join("logs")).is_err());
}

#[test]
fn the_oldest_logs_are_deleted_past_the_total_cap_but_never_the_newest() {
    let dir = tmp::tempdir().unwrap();
    for day in ["2026-10-01", "2026-10-02", "2026-10-03"] {
        std::fs::write(
            dir.path().join(format!("hearth.{day}.log")),
            vec![b'x'; 100],
        )
        .unwrap();
    }
    std::fs::write(dir.path().join("autre.txt"), vec![b'x'; 1000]).unwrap();
    assert!(!enforce_cap(dir.path(), 250));
    let mut left: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert_eq!(
        left,
        [
            "autre.txt",
            "hearth.2026-10-02.log",
            "hearth.2026-10-03.log"
        ]
    );
    // Le plus récent seul dépasse encore : il reste, et on demande d'abandonner les écritures.
    assert!(enforce_cap(dir.path(), 50));
    assert!(dir.path().join("hearth.2026-10-03.log").exists());
    assert_eq!(MAX_TOTAL_BYTES, 16 * 1024 * 1024);
}

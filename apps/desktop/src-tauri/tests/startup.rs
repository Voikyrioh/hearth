//! Démarrage : une erreur de `start` prend le chemin d'échec, sans panique.
#![allow(clippy::unwrap_used, clippy::expect_used)] // tests : les helpers peuvent paniquer

use std::cell::RefCell;

use hearth_desktop_lib::{start, start_or_report};
use tauri::test::{MockRuntime, mock_builder, mock_context, noop_assets};

fn app() -> tauri::App<MockRuntime> {
    mock_builder().build(mock_context(noop_assets())).unwrap()
}

fn tray_failure() -> tauri::Error {
    tauri::Error::Io(std::io::Error::other("icône refusée"))
}

#[test]
fn a_tray_failure_is_reported_with_its_reason_and_reported_as_failed() {
    let app = app();
    let reasons = RefCell::new(Vec::new());
    let started = start_or_report(
        app.handle(),
        false,
        |_| Err(tray_failure()),
        |reason| reasons.borrow_mut().push(reason.to_owned()),
    );
    assert!(!started);
    let reasons = reasons.into_inner();
    assert_eq!(reasons.len(), 1);
    assert!(reasons[0].contains("zone de notification"), "{reasons:?}");
    assert!(reasons[0].contains("icône refusée"), "{reasons:?}");
}

#[test]
fn a_successful_start_reports_nothing() {
    let app = app();
    let reasons = RefCell::new(Vec::new());
    let started = start_or_report(
        app.handle(),
        true,
        |_| Ok(()),
        |reason| reasons.borrow_mut().push(reason.to_owned()),
    );
    assert!(started);
    assert!(reasons.into_inner().is_empty());
}

#[test]
fn start_returns_the_error_instead_of_panicking() {
    let app = app();
    let result = start(app.handle(), false, |_| Err(tray_failure()));
    assert!(result.is_err());
}

//! Forme sérialisée de l'erreur des commandes.
#![allow(clippy::unwrap_used, clippy::expect_used)] // tests : les helpers peuvent paniquer

use hearth_desktop_lib::error::AppError;

#[test]
fn serializes_as_kind_and_message() {
    let cases = [
        (AppError::Store("disque".into()), "store", "disque"),
        (
            AppError::Autostart("registre".into()),
            "autostart",
            "registre",
        ),
        (AppError::Logs("droits".into()), "logs", "droits"),
    ];
    for (error, kind, message) in cases {
        let json = serde_json::to_value(error);
        assert_eq!(
            json.ok(),
            Some(serde_json::json!({ "kind": kind, "message": message }))
        );
    }
}

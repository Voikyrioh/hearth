//! Forme sérialisée de l'erreur des commandes.

use hearth_desktop_lib::error::AppError;

#[test]
fn serializes_as_kind_and_message() {
    let json = serde_json::to_value(AppError::Store("disque".into()));
    assert_eq!(
        json.ok(),
        Some(serde_json::json!({ "kind": "store", "message": "disque" }))
    );
}

//! Les permissions de la fenêtre principale (build de production) : la liste blanche des commandes
//! est exacte, et aucune commande générique qui laisserait la WebView appeler une route quelconque
//! de l'agent n'existe (ADR-0013, ADR-0016) : une action = une commande typée, méthode et chemin
//! construits côté Rust.
#![allow(clippy::unwrap_used, clippy::expect_used)]

fn permissions() -> Vec<String> {
    let json = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/capabilities/default.json"
    ))
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    value["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn no_generic_action_command_is_allowed_to_the_webview() {
    for permission in permissions() {
        for forbidden in ["run-action", "run_action", "execute", "request", "http"] {
            assert!(
                !permission.contains(forbidden),
                "permission générique interdite : {permission}"
            );
        }
    }
}

#[test]
fn every_permission_is_a_known_command_or_the_event_listener() {
    let manifest =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/build.rs")).unwrap();
    for permission in permissions() {
        if let Some(command) = permission.strip_prefix("allow-") {
            let snake = command.replace('-', "_");
            assert!(
                manifest.contains(&format!("\"{snake}\"")),
                "permission sans commande déclarée dans build.rs : {permission}"
            );
        } else {
            assert!(
                permission.starts_with("core:event:"),
                "permission inattendue : {permission}"
            );
        }
    }
}

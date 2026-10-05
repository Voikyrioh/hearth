use tauri_build::{AppManifest, Attributes};

/// Commandes exposées à l'interface : liste blanche, gardée en phase avec
/// `capabilities/default.json` (une permission `allow-*` par commande).
const COMMANDS: &[&str] = &[
    "get_settings",
    "set_launch_at_startup",
    "get_app_version",
    "open_logs_folder",
    "log_frontend_error",
    "list_servers",
    "list_link_states",
    "probe_server",
    "add_and_login",
    "login",
    "logout",
    "retry_now",
    "accept_fingerprint",
    "update_server",
    "remove_server",
    "forget_credentials",
    "list_fingerprint_alerts",
    "list_link_notices",
    "ack_link_notices",
    "list_unread_operations",
    "ack_unread_operations",
    "get_notify_on_link_change",
    "set_notify_on_link_change",
    "set_displayed_server",
    "run_action",
];

fn main() {
    // Les tests d'intégration (`tests/`) ne reçoivent pas le manifeste de Tauri :
    // sous Windows ils échouent au chargement (STATUS_ENTRYPOINT_NOT_FOUND) faute de
    // Common Controls v6. On l'embarque pour les tests seulement.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-tests=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
        );
    }
    let attributes = Attributes::new().app_manifest(AppManifest::new().commands(COMMANDS));
    tauri_build::try_build(attributes).unwrap_or_else(|error| {
        eprintln!("{error:#}");
        std::process::exit(1);
    });
}

//! Coquille Windows de Hearth : fenêtre, zone de notification, instance
//! unique, démarrage avec Windows, réglages locaux. Aucun accès réseau ici :
//! tout le réseau vivra dans `hearth-link` (ADR-0002).

mod commands;
pub mod domain;
pub mod error;
pub mod logging;
pub mod settings;
pub mod texts;
mod tray;
pub mod window;

use tauri::Manager as _;
use tauri_plugin_autostart::MacosLauncher;
use tauri_specta::{Builder, collect_commands};

/// Chemin du fichier de types TypeScript généré, relatif à `src-tauri/`.
pub const BINDINGS_PATH: &str = "../src/bindings.ts";

pub fn typescript() -> specta_typescript::Typescript {
    specta_typescript::Typescript::default().header("// @ts-nocheck\n/* eslint-disable */")
}

/// Déclare les commandes typées. Séparé de `run` pour que les tests
/// vérifient que `bindings.ts` est à jour.
pub fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new().commands(collect_commands![
        commands::get_settings,
        commands::set_launch_at_startup,
        commands::get_app_version,
        commands::open_logs_folder,
    ])
}

pub fn run() {
    // Le journal d'abord : sans console, c'est la seule trace d'un échec.
    // Sans journal on démarre quand même ; un échec de démarrage sera dit à l'écran.
    let _ = logging::init();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "démarrage de Hearth");

    let builder = specta_builder();

    #[cfg(debug_assertions)]
    if let Err(error) = builder.export(typescript(), BINDINGS_PATH) {
        tracing::warn!(%error, "export de bindings.ts impossible");
    }

    let result = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            window::show_main(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![domain::MINIMIZED_FLAG]),
        ))
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(builder.invoke_handler())
        .on_window_event(window::on_window_event)
        .setup(|app| {
            tray::build(app.handle())?;
            if domain::is_minimized_launch(std::env::args()) {
                if let Some(main) = app.get_webview_window(domain::MAIN_WINDOW) {
                    main.hide()?;
                }
            } else {
                window::show_main(app.handle());
            }
            Ok(())
        })
        .run(tauri::generate_context!());

    if let Err(error) = result {
        fail_startup(&error.to_string());
    }
}

/// Le démarrage a échoué : journal, boîte de message système avec le chemin du
/// journal, code de sortie 1. Jamais de sortie silencieuse.
fn fail_startup(error: &str) -> ! {
    tracing::error!(error, "démarrage impossible");
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title(texts::STARTUP_FAILED_TITLE)
        .set_description(texts::startup_failed_body(error, &logging::log_dir()))
        .show();
    std::process::exit(1);
}

//! Coquille Windows de Hearth : fenêtre, zone de notification, instance
//! unique, démarrage avec Windows, réglages locaux. Aucun accès réseau ici :
//! tout le réseau vivra dans `hearth-link` (ADR-0002).

mod commands;
pub mod domain;
pub mod error;
mod settings;
mod texts;
mod tray;
mod window;

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
    ])
}

pub fn run() {
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
                if let Some(main) = app.get_webview_window(window::MAIN_WINDOW) {
                    main.hide()?;
                }
            } else {
                window::show_main(app.handle());
            }
            Ok(())
        })
        .run(tauri::generate_context!());

    if let Err(error) = result {
        eprintln!("Hearth n'a pas pu démarrer : {error}");
        std::process::exit(1);
    }
}

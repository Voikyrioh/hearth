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

use tauri::{AppHandle, Manager as _, Runtime};
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

/// Erreur de démarrage, avec l'étape qui a échoué.
#[derive(Debug, thiserror::Error)]
pub enum StartupError {
    #[error("icône de la zone de notification : {0}")]
    Tray(tauri::Error),
    #[error("fenêtre principale : {0}")]
    Window(tauri::Error),
}

/// Corps du démarrage (appelé par `setup`). `build_tray` est injectable pour
/// pouvoir simuler l'échec de l'icône.
pub fn start<R: Runtime>(
    app: &AppHandle<R>,
    minimized: bool,
    build_tray: impl FnOnce(&AppHandle<R>) -> tauri::Result<()>,
) -> Result<(), StartupError> {
    build_tray(app).map_err(StartupError::Tray)?;
    if minimized {
        if let Some(main) = app.get_webview_window(domain::MAIN_WINDOW)
            && let Err(error) = main.hide()
        {
            // L'icône existe déjà : la retirer, sinon elle reste en fantôme après la sortie.
            app.remove_tray_by_id(tray::TRAY_ID);
            return Err(StartupError::Window(error));
        }
    } else {
        window::show_main(app);
    }
    Ok(())
}

/// Lance `start` ; en cas d'erreur, appelle `report` avec la raison (en
/// production : `fail_startup`, qui la journalise, affiche la boîte de message
/// et sort).
/// Tauri panique si `setup` rend une erreur : on ne la lui rend donc jamais.
pub fn start_or_report<R: Runtime>(
    app: &AppHandle<R>,
    minimized: bool,
    build_tray: impl FnOnce(&AppHandle<R>) -> tauri::Result<()>,
    report: impl FnOnce(&str),
) -> bool {
    match start(app, minimized, build_tray) {
        Ok(()) => true,
        Err(error) => {
            report(&error.to_string());
            false
        }
    }
}

pub fn run() {
    // Le journal d'abord : sans console, c'est la seule trace d'un échec. S'il ne
    // s'ouvre pas, on démarre quand même ; la raison est gardée pour la boîte
    // de message d'un éventuel échec (`logging::init_failure`).
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
            let minimized = domain::is_minimized_launch(std::env::args());
            start_or_report(app.handle(), minimized, tray::build, |reason| {
                fail_startup(reason)
            });
            Ok(())
        })
        .run(tauri::generate_context!());

    if let Err(error) = result {
        fail_startup(&error.to_string());
    }
}

/// Le démarrage a échoué : journal, boîte de message système avec la raison et
/// le chemin du journal (ou le fait qu'il n'a pas pu être écrit), sortie avec le
/// code 1. Jamais de sortie silencieuse, jamais d'application sans fenêtre ni icône.
fn fail_startup(error: &str) -> ! {
    tracing::error!(error, "démarrage impossible");
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title(texts::STARTUP_FAILED_TITLE)
        .set_description(texts::startup_failed_body(
            error,
            &logging::log_dir(),
            logging::init_failure(),
        ))
        .show();
    std::process::exit(1);
}

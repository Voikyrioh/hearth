//! Coquille Windows de Hearth : fenêtre, zone de notification, instance
//! unique, démarrage avec Windows, réglages locaux. Aucun accès réseau ici :
//! tout le réseau vivra dans `hearth-link` (ADR-0002).

pub mod accounts;
pub mod alerts;
pub mod audit;
pub mod badge;
mod commands;
pub mod dashboard;
pub mod domain;
pub mod error;
pub mod link;
mod link_commands;
pub mod link_dto;
pub mod logging;
pub mod presence;
pub mod settings;
pub mod texts;
mod tray;
pub mod update;
pub mod vault;
pub mod window;

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

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
    Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            commands::get_settings,
            commands::set_launch_at_startup,
            commands::get_app_version,
            commands::open_logs_folder,
            commands::log_frontend_error,
            commands::get_notify_on_link_change,
            commands::set_notify_on_link_change,
            commands::set_displayed_server,
            link_commands::list_servers,
            link_commands::list_link_states,
            link_commands::probe_server,
            link_commands::add_and_login,
            link_commands::login,
            link_commands::logout,
            link_commands::retry_now,
            link_commands::accept_fingerprint,
            link_commands::update_server,
            link_commands::remove_server,
            link_commands::forget_credentials,
            link_commands::list_fingerprint_alerts,
            link_commands::list_link_notices,
            link_commands::ack_link_notices,
            link_commands::list_unread_operations,
            link_commands::ack_unread_operations,
            link_commands::get_dashboard,
            link_commands::read_audit,
            link_commands::export_audit,
            update::commands::get_update_state,
            update::commands::check_for_updates,
            update::commands::postpone_update,
            update::commands::install_update,
            accounts::commands::check_account_input,
            accounts::commands::list_accounts,
            accounts::commands::create_account,
            accounts::commands::change_account_role,
            accounts::commands::set_account_password,
            accounts::commands::change_own_password,
            accounts::commands::close_account_sessions,
            accounts::commands::delete_account,
        ])
        .typ::<link_dto::ServersEvent>()
        .typ::<link_dto::OperationEventDto>()
        .typ::<link_dto::FingerprintEvent>()
        .typ::<link_dto::NoticeEvent>()
        .typ::<dashboard::MetricsEvent>()
        .typ::<dashboard::SnapshotEvent>()
        .typ::<audit::AuditLiveEvent>()
        .typ::<update::dto::UpdateStateDto>()
        .typ::<accounts::dto::AccountOutcome>()
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

/// Coffre des secrets de la plateforme : le Gestionnaire d'identification de Windows. Ailleurs, pas
/// de coffre : le client ne démarre pas plutôt que de garder des mots de passe en clair.
#[cfg(windows)]
fn system_vault() -> Result<Arc<dyn hearth_link::ports::Vault>, String> {
    let backend = vault::WindowsCredentials::new()?;
    Ok(Arc::new(vault::CredentialVault::new(backend)))
}

#[cfg(not(windows))]
fn system_vault() -> Result<Arc<dyn hearth_link::ports::Vault>, String> {
    Err("coffre des secrets indisponible sur cette plateforme".to_owned())
}

/// Branche la liaison : carnet, dernières vues et suivis dans le dossier de données de
/// l'application, secrets au coffre de Windows, et relais des événements vers la fenêtre.
fn install_link<R: Runtime>(app: &tauri::App<R>) -> Result<(), String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("dossier de données : {error}"))?;
    let vault = system_vault()?;
    let client = link::client_name(env!("CARGO_PKG_VERSION"));
    let runtime = tauri::async_runtime::block_on(link::LinkRuntime::open(&dir, vault, &client))
        .map_err(|error| format!("liaison avec les serveurs : {error}"))?;
    tracing::info!(servers = runtime.servers().len(), data = %dir.display(), "liaison prête");
    let runtime = Arc::new(runtime);
    let events = runtime.manager().subscribe();
    app.manage(runtime.clone());
    // Notifications système (une par minute et par serveur) et icône de la zone de notification :
    // branchées avant le relais, l'état courant leur est donné tout de suite.
    let enabled = settings::notify_on_link_change(app.handle(), Path::new(settings::STORE_FILE))
        .unwrap_or_else(|error| {
            tracing::warn!(%error, "réglage des notifications illisible : activées");
            true
        });
    let started = Instant::now();
    let alerts = Arc::new(alerts::Alerts::new(
        Arc::new(tray::TauriNotifier(app.handle().clone())),
        Arc::new(tray::TauriTray(app.handle().clone())),
        enabled,
        move || u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    ));
    app.manage(alerts.clone());
    runtime.set_observer(alerts.clone());
    tauri::async_runtime::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(5));
        loop {
            tick.tick().await;
            alerts.tick();
        }
    });
    let sink = link_commands::TauriSink(app.handle().clone());
    tauri::async_runtime::spawn(async move { runtime.forward(events, &sink).await });
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
        .plugin(update::feed::plugin())
        .invoke_handler(builder.invoke_handler())
        .on_window_event(window::on_window_event)
        .setup(|app| {
            if let Err(error) = install_link(app) {
                fail_startup(&error);
            }
            // La mise à jour ne bloque jamais le démarrage : sans elle, le client reste utilisable.
            if let Err(error) = update::install(app) {
                tracing::error!(%error, "mise à jour du client indisponible");
            }
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

//! Mise à jour du client (HRT-16, ADR-0017) : règles pures (`domain`), cas d'usage (`service`),
//! ports (`ports`), adaptateurs (`store` : fichier, `feed` : greffon officiel de Tauri), types
//! sérialisés vers l'interface (`dto`) et commandes typées (`commands`).

pub mod commands;
pub mod domain;
pub mod dto;
pub mod feed;
pub mod ports;
pub mod service;
pub mod store;

use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter as _, Manager as _, Runtime};

use self::dto::{STATE_EVENT, UpdateStateDto};
use self::feed::TauriFeed;
use self::ports::StateSink;
use self::service::UpdateService;
use self::store::{FileUpdateStore, SystemClock};

/// La première vérification attend que la fenêtre soit affichée (pas de réseau pendant le démarrage).
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(10);
/// Battement de l'horloge : sans réseau tant que la règle des 24 h ne permet rien.
const TICK_PERIOD: Duration = Duration::from_secs(60 * 60);

/// Publie l'état de la mise à jour dans la fenêtre (événement `update://state`).
pub struct TauriStateSink<R: Runtime>(pub AppHandle<R>);

impl<R: Runtime> StateSink for TauriStateSink<R> {
    fn publish(&self, state: &UpdateStateDto) {
        if let Err(error) = self.0.emit(STATE_EVENT, state) {
            tracing::warn!(%error, "état de la mise à jour non publié");
        }
    }
}

/// Le service réel, tel que la coquille le range dans son état géré.
pub type SharedUpdates = Arc<UpdateService>;

/// Branche la mise à jour du client : service dans l'état géré, vérification planifiée. À appeler
/// une fois, après l'enregistrement du greffon (`feed::plugin`).
pub fn install<R: Runtime>(app: &tauri::App<R>) -> Result<(), String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("dossier de données : {error}"))?;
    let handle = app.handle().clone();
    // Une seule politique de source, partagée : le service refuse une annonce qui ne la respecte pas,
    // l'adaptateur ne retient ni ne télécharge rien d'autre et durcit les redirections avec elle.
    let policy = domain::DownloadPolicy::github_releases();
    let feed = TauriFeed::production(handle.clone(), policy.clone())
        .map_err(|error| format!("adresse du flux de versions : {error}"))?;
    // La cible de l'agent n'est lue que si un serveur est enregistré (la liaison est ouverte avant).
    let link = app
        .try_state::<Arc<crate::link::LinkRuntime>>()
        .map(|link| link.inner().clone());
    let service = Arc::new(
        UpdateService::new(
            Arc::new(SystemClock),
            Arc::new(FileUpdateStore::new(&dir)),
            Arc::new(feed),
            Arc::new(TauriStateSink(handle)),
            policy,
            &app.package_info().version.to_string(),
        )
        .with_agent_wanted(move || link.as_ref().is_none_or(|link| !link.servers().is_empty())),
    );
    if feed::embedded_key_is_development() {
        tracing::warn!("clé de mise à jour de développement : aucune mise à jour ne sera acceptée");
    }
    app.manage(service.clone());
    tauri::async_runtime::spawn(async move {
        service.run_scheduler(FIRST_CHECK_DELAY, TICK_PERIOD).await;
    });
    Ok(())
}

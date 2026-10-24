//! Commandes du moteur de conteneurs et des services exposées à l'interface (liste blanche : `build.rs`
//! et `capabilities/default.json`). UNE commande par geste (ADR-0016), aucun paramètre libre : ni route,
//! ni méthode, ni adresse. Lecture seule : « Vérifier à nouveau » relance la détection du moteur,
//! `POST /engine/recheck` ne modifie rien.

use std::sync::Arc;

use hearth_link::domain::server::ServerId;
use tauri::{Manager as _, State};

use super::dto::{EngineDto, EnvRevealDto, ServiceRightsDto, ServicesEvent};
use super::logs::LogSessions;
use crate::link::LinkRuntime;
use crate::link_dto::LinkFailure;
use crate::window::WindowSink;

type Runtime_<'a> = State<'a, Arc<LinkRuntime>>;

/// La documentation officielle d'installation de Docker : une constante de la coquille, jamais fournie par
/// l'interface (conception technique, section 8, écart 8).
pub const ENGINE_DOCS_URL: &str = "https://docs.docker.com/engine/install/";

/// Le dernier état des services de chaque serveur du carnet, sans lecture réseau : l'interface le rejoue à
/// son abonnement (ADR-0013 point 3). Un serveur dont la coquille n'a encore rien reçu en cette exécution
/// donne la dernière liste enregistrée sur le disque, périmée.
#[tauri::command]
#[specta::specta]
pub async fn list_services_states(link: Runtime_<'_>) -> Result<Vec<ServicesEvent>, LinkFailure> {
    Ok(link.services_states().await)
}

/// « Vérifier à nouveau » : relance la détection du moteur tout de suite et rend son état. Une lecture :
/// sans suivi, refaite par l'interface si le lien tombe. Hors « Connecté », `NotConnected` sans rien envoyer.
#[tauri::command]
#[specta::specta]
pub async fn engine_recheck(
    link: Runtime_<'_>,
    server_id: String,
) -> Result<EngineDto, LinkFailure> {
    let id = ServerId::parse(&server_id).map_err(|_| LinkFailure::UnknownServer)?;
    let engine = link.manager().engine_recheck(&id).await?;
    Ok(EngineDto::from(&engine))
}

/// Le droit « gérer les services » du compte connecté (BR-GRANT-011) : une lecture (`GET /me` et `GET /hello`),
/// relue à l'ouverture de la section, à chaque reconnexion et après tout refus pour absence du droit. Aucune
/// commande de cette coquille n'accorde ni ne retire le droit : il ne se donne que depuis le serveur
/// (BR-GRANT-005). Hors « Connecté » : `NotConnected` sans rien envoyer.
#[tauri::command]
#[specta::specta]
pub async fn read_service_rights(
    link: Runtime_<'_>,
    server_id: String,
) -> Result<ServiceRightsDto, LinkFailure> {
    let id = ServerId::parse(&server_id).map_err(|_| LinkFailure::UnknownServer)?;
    Ok(link.manager().service_rights(&id).await?.into())
}

/// Révèle les valeurs des variables d'UN service (HRT-63, `BR-SVC-009`) : une lecture que l'agent réserve aux
/// comptes qui ont le droit de gérer les services et qu'il consigne au journal. La clé est celle du service
/// ouvert (vérifiée par la liaison, qui construit le chemin). Les valeurs sont rendues à l'appelant et à lui
/// seul : la coquille ne les garde pas, ne les trace pas, ne les met pas dans un événement. Hors « Connecté » :
/// `NotConnected`, rien n'est envoyé ; sans le droit : `MissingServicesRight`.
#[tauri::command]
#[specta::specta]
pub async fn reveal_service_env(
    link: Runtime_<'_>,
    server_id: String,
    key: String,
) -> Result<EnvRevealDto, LinkFailure> {
    let id = ServerId::parse(&server_id).map_err(|_| LinkFailure::UnknownServer)?;
    let revealed = link.manager().reveal_service_env(&id, &key).await?;
    Ok(EnvRevealDto::from(&revealed))
}

/// Une page des journaux démarre (ou redémarre) : la coquille lui donne le numéro de sa génération et ferme tout
/// ce que la page précédente de CETTE fenêtre avait laissé ouvert (rechargement, plantage du rendu, navigation
/// brutale). À appeler une fois au démarrage de la page, avant toute ouverture (HRT-67, `BR-SLOG-026`).
#[tauri::command]
#[specta::specta]
pub fn announce_log_page(
    window: tauri::WebviewWindow,
    sessions: State<'_, Arc<LogSessions>>,
) -> u32 {
    sessions.announce_page(window.label())
}

/// Ouvre la lecture des journaux d'UN service (HRT-67, `BR-SLOG-*`) et rend l'identifiant de sa session. Les
/// lots arrivent à la SEULE fenêtre qui l'a ouverte, par l'événement `link://service-logs` ; la coquille ne garde
/// aucune ligne, n'en écrit aucune et n'en trace aucune. `page` : la génération rendue par `announce_log_page`.
/// Au plus trois lectures par serveur (le plafond d'un compte) et douze au total : au-delà `LogReadsFull`. La
/// lecture se reprend seule après une coupure (bornée) ; un refus de l'agent est une fin, jamais retentée. Hors
/// « Connecté » : `NotConnected`, rien n'est envoyé.
#[tauri::command]
#[specta::specta]
pub async fn open_service_logs(
    window: tauri::WebviewWindow,
    link: Runtime_<'_>,
    sessions: State<'_, Arc<LogSessions>>,
    page: u32,
    server_id: String,
    key: String,
) -> Result<String, LinkFailure> {
    let owner = window.label().to_owned();
    let sink = Arc::new(WindowSink::new(window.app_handle().clone(), owner.clone()));
    sessions.open(link.manager(), sink, &owner, page, &server_id, &key)
}

/// Ferme la lecture d'un onglet : le socket est fermé et la place rendue au compte. Sans effet sur une session
/// déjà finie.
#[tauri::command]
#[specta::specta]
pub fn close_service_logs(sessions: State<'_, Arc<LogSessions>>, session: String) {
    sessions.close(&session);
}

/// La fenêtre a affiché ces lots : la coquille peut en envoyer d'autres (au plus vingt sans accusé).
#[tauri::command]
#[specta::specta]
pub fn ack_service_logs(sessions: State<'_, Arc<LogSessions>>, session: String, batches: u32) {
    sessions.acknowledge(&session, batches);
}

/// Battement de vie de la page `page` : à envoyer toutes les dix secondes tant qu'une lecture est ouverte. Rend
/// les sessions de cette page que la coquille tient encore ; une session absente a été fermée par elle. Sans
/// battement pendant 45 secondes, la coquille ferme la session d'office.
#[tauri::command]
#[specta::specta]
pub fn keep_service_logs(
    window: tauri::WebviewWindow,
    sessions: State<'_, Arc<LogSessions>>,
    page: u32,
) -> Vec<String> {
    sessions.keep_alive(window.label(), page)
}

/// Ouvre la documentation officielle de Docker dans le navigateur du PC. Aucun paramètre : l'adresse est
/// la constante [`ENGINE_DOCS_URL`].
#[tauri::command]
#[specta::specta]
pub fn open_engine_docs() -> Result<(), LinkFailure> {
    if crate::browser::open_in_browser(ENGINE_DOCS_URL) {
        Ok(())
    } else {
        Err(LinkFailure::Internal)
    }
}

//! Les réglages d'un serveur de jeu (HRT-108, ADR-0051, BR-FICHE-037, 042, 044, 046 à 049, 052, 053) : `GET /services/{key}/settings`,
//! `POST /services/{key}/settings/analyze` et `PUT /services/{key}/settings`.
//!
//! Les trois exigent le droit « gérer les services » (niveau `Services`, jugé avant tout handler). Lire et analyser
//! ne modifient rien (l'analyse est un `POST` de `READ_ONLY_POSTS` : le texte collé, un mot de passe possible, est lu
//! puis oublié). Enregistrer est l'acte `GameSettingsSave` `0x1E`, confirmé par la couche d'accès, couvert par
//! l'élévation ; la réussite est inscrite au journal par le cas d'usage (noms des réglages non secrets changés,
//! « mot de passe modifié », jamais une valeur).
//!
//! Aucune erreur ne répète une clé, un nom, un chemin ni une valeur : les textes sont fixes ; un réglage n'est
//! nommé que s'il est un réglage de la fiche.

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Extension, Path, State};
use hearth_proto::api::game_settings::{
    AnalyzeRequest, ImportView, SaveSettingsRequest, SaveSettingsResponse, SettingsView,
};
use hearth_proto::error::{ErrorBody, ErrorCode};

use super::auth::Requester;
use super::{ApiError, AppState};
use crate::application::game_settings::SettingsError;
use crate::application::sessions::Reauthenticated;
use crate::domain::services::ServiceKey;

/// Une clé mal formée est un service inconnu, comme pour toute route de service.
fn key_of(raw: &str) -> Result<ServiceKey, ApiError> {
    ServiceKey::parse(raw)
        .ok_or_else(|| ApiError::new(ErrorCode::ServiceNotFound, "Ce service n'existe pas."))
}

/// Une clé qui n'est pas celle d'un serveur de jeu créé depuis une fiche (inconnue, adoptée, déployée à la main, ou
/// fiche que cet agent ne propose pas) : même réponse pour toutes, `NOT_A_GAME_SERVICE`.
fn not_a_game_service() -> ApiError {
    ApiError::new(
        ErrorCode::NotAGameService,
        "Ce service n'est pas un serveur de jeu créé depuis une fiche : il n'a pas de réglages.",
    )
}

fn settings_error(error: SettingsError) -> ApiError {
    match error {
        SettingsError::NotFound => not_a_game_service(),
        SettingsError::Conflict { by, at } => ApiError(ErrorBody::with_details(
            ErrorCode::SettingsConflict,
            "Les réglages ont changé depuis que vous les avez lus. Relisez-les, puis recommencez.",
            serde_json::json!({ "by": by, "at": at.and_then(|date| super::wire::date(date).ok()) }),
        )),
        SettingsError::FileChanged { ids, by, at } => ApiError(ErrorBody::with_details(
            ErrorCode::SettingsFileChanged,
            "Le fichier de réglages a changé depuis que vous l'avez lu. Relisez-le, puis choisissez, réglage par réglage, ce que vous gardez.",
            serde_json::json!({ "ids": ids, "by": by, "at": at.and_then(|date| super::wire::date(date).ok()) }),
        )),
        SettingsError::FileUnreadable { reason, line } => ApiError(ErrorBody::with_details(
            ErrorCode::SettingsFileUnreadable,
            "Le fichier de réglages n'est pas lisible : rien n'y a été changé.",
            serde_json::json!({ "reason": reason, "line": line }),
        )),
        SettingsError::Refused {
            id,
            reason,
            character,
        } => ApiError(ErrorBody::with_details(
            ErrorCode::SettingRefused,
            "Une valeur n'est pas valable.",
            serde_json::json!({ "id": id, "reason": reason, "char": character }),
        )),
        SettingsError::RecreateNotAvailable { ids } => ApiError(ErrorBody::with_details(
            ErrorCode::RecreateNotAvailable,
            "Ce réglage s'applique quand Hearth recrée le service, ce qu'il ne sait pas encore faire.",
            serde_json::json!({ "ids": ids }),
        )),
        SettingsError::EffectMismatch => ApiError(ErrorBody::with_details(
            ErrorCode::SettingsConflict,
            "L'effet annoncé n'est pas celui de ces changements. Relisez les réglages, puis recommencez.",
            serde_json::json!({ "reason": "effect" }),
        )),
        SettingsError::Busy => ApiError::new(
            ErrorCode::OperationInProgress,
            "Une action est en cours sur ce service. Réessayez quand elle sera finie.",
        ),
        SettingsError::SecretsUnreadable => ApiError::new(
            ErrorCode::InternalError,
            "Les secrets des services sont illisibles : rien n'a été changé.",
        ),
        SettingsError::Invalid(field) => ApiError(ErrorBody::with_details(
            ErrorCode::ValidationError,
            "Le corps de la demande n'est pas valable.",
            serde_json::json!({ "field": field }),
        )),
        SettingsError::Storage(message) => {
            tracing::error!(error = %message, "réglages d'un jeu en échec");
            ApiError::new(ErrorCode::InternalError, "Une erreur interne est survenue.")
        }
    }
}

/// `GET /api/v1/services/{key}/settings` : les valeurs voulues, l'état du fichier, les écarts.
pub async fn view(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<SettingsView>, ApiError> {
    let key = key_of(&key)?;
    state
        .game_settings
        .view(&key)
        .await
        .map(Json)
        .map_err(settings_error)
}

/// `POST /api/v1/services/{key}/settings/analyze` : lit un texte collé, n'écrit rien.
pub async fn analyze(
    State(state): State<AppState>,
    Path(key): Path<String>,
    body: Result<Json<AnalyzeRequest>, JsonRejection>,
) -> Result<Json<ImportView>, ApiError> {
    let key = key_of(&key)?;
    let Json(request) = body?;
    state
        .game_settings
        .analyze(&key, &request)
        .await
        .map(Json)
        .map_err(settings_error)
}

/// `PUT /api/v1/services/{key}/settings` (acte `GameSettingsSave` `0x1E`).
pub async fn save(
    State(state): State<AppState>,
    _confirmed: Extension<Reauthenticated>,
    Requester(actor): Requester,
    Path(key): Path<String>,
    body: Result<Json<SaveSettingsRequest>, JsonRejection>,
) -> Result<Json<SaveSettingsResponse>, ApiError> {
    let key = key_of(&key)?;
    let Json(request) = body?;
    state
        .game_settings
        .save(&key, &actor, &request)
        .await
        .map(Json)
        .map_err(settings_error)
}

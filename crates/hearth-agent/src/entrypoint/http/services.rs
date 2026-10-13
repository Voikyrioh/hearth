//! `GET /services`, `POST /engine/recheck` et `GET /services/{key}/env` (HRT-61, HRT-63,
//! BR-ENGINE-016, BR-ENGINE-020, BR-SVC-009, BR-SVC-020, BR-SVC-024).
//!
//! `GET /services` et `POST /engine/recheck` sont des consultations ouvertes aux deux rôles : aucune
//! entrée au journal. `POST /engine/recheck` est un `POST` qui ne modifie rien (il relance une
//! détection, sans effet sur le moteur) : il est dans `READ_ONLY_POSTS`, ni acte d'administration ni
//! exception nommée. Aucune route n'écrit dans le moteur ni dans les services.
//!
//! Ce que ces routes publient est filtré en amont, par le registre (HRT-79, ADR-0043) : seuls les
//! services créés par Hearth et les services adoptés en sortent, les autres conteneurs ne sont qu'un
//! nombre. Une clé qui n'est pas publiée reçoit EXACTEMENT la réponse d'une clé inconnue. Pour un service
//! adopté, la commande et les noms de variables ne sont donnés qu'aux comptes qui ont le droit
//! (`BR-ADOPT-015`) : `list` le décide d'après le compte de la requête.
//!
//! `GET /services/external`, `POST /services/adopt` et `POST /services/{key}/release` (HRT-80, ADR-0043) :
//! la liste des conteneurs non gérés, l'acte d'adopter et l'acte de relâcher. Les trois exigent le droit
//! « gérer les services » (niveau `Services`, jugé avant tout handler). Adopter exige en plus la
//! confirmation complète d'un acte d'administration, jamais couverte par l'élévation ; relâcher la
//! confirmation, couverte par l'élévation. La liste n'est jamais consignée quand elle réussit.
//!
//! `GET /services/{key}/env` révèle les VALEURS des variables d'un service : réservée aux comptes qui
//! ont le droit « gérer les services », quel que soit leur rôle (niveau `Services`, ADR-0041 : un
//! administrateur sans le droit est refusé, un compte en lecture seule qui l'a est servi ; la couche
//! d'accès refuse et consigne le refus), la réussite est consignée au journal par le cas d'usage sans nom ni valeur, la réponse n'est jamais
//! stockable. Aucune valeur n'entre dans une trace ni dans un message d'erreur.

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Extension, Path, State};
use axum::http::header::{CACHE_CONTROL, PRAGMA};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use hearth_proto::api::services::{
    AdoptRequest, AdoptResponse, AdoptionItem, AdoptionResult, EngineView, ExternalContainer,
    ExternalContainers, ImageInfo, ReleaseResponse, ServicesState,
};
use hearth_proto::error::{ErrorBody, ErrorCode};

use super::auth::{Caller, Requester};
use super::{ApiError, AppState};
use crate::application::service_adoption::{AdoptionError, Invalid, KeyOutcome};
use crate::application::services::RevealError;
use crate::application::sessions::Reauthenticated;
use crate::domain::audit::{AuditAction, Outcome, Target};
use crate::domain::services::ServiceKey;
use crate::entrypoint::services_wire::{self, Reader};

/// `GET /api/v1/services` : le même contenu que `services_snapshot` du flux (repli, tests).
pub async fn list(State(state): State<AppState>, Caller(session): Caller) -> Json<ServicesState> {
    let reader = Reader {
        may_manage_services: session.account.may_manage_services(),
    };
    Json(services_wire::snapshot(&state.services.snapshot(), reader))
}

/// `POST /api/v1/engine/recheck` : relance la détection du moteur tout de suite et rend son état.
pub async fn recheck(State(state): State<AppState>) -> Json<EngineView> {
    let status = state.services.recheck().await;
    Json(services_wire::engine_view(&status, state.services.host()))
}

/// `GET /api/v1/services/{key}/env` : les variables du service avec leurs valeurs (comptes qui ont le
/// droit « gérer les services » seulement, quel que soit le rôle ; BR-SVC-009, BR-GRANT-003). La clé du chemin n'est jamais répétée dans une réponse d'erreur.
pub async fn env(
    State(state): State<AppState>,
    Requester(actor): Requester,
    Path(key): Path<String>,
) -> Result<Response, ApiError> {
    let key = ServiceKey::parse(&key)
        .ok_or_else(|| ApiError::new(ErrorCode::NotFound, "Ce service n'existe pas."))?;
    let entries = state
        .services
        .reveal_env(&key, actor, state.sink.as_ref())
        .await
        .map_err(|error| match error {
            RevealError::NotFound => ApiError::new(ErrorCode::NotFound, "Ce service n'existe pas."),
            RevealError::NoContainer => ApiError::new(
                ErrorCode::NotFound,
                "Les variables de ce service ne sont pas lisibles : son conteneur n'existe pas.",
            ),
            RevealError::EngineUnavailable => ApiError::new(
                ErrorCode::Busy,
                "Le moteur de conteneurs ne répond pas. Réessaie dans un instant.",
            ),
            RevealError::JournalUnavailable => ApiError::new(
                ErrorCode::Busy,
                "La révélation n'a pas pu être consignée au journal : aucune valeur n'est rendue. Réessaie dans un instant.",
            ),
        })?;
    let mut response = (StatusCode::OK, Json(services_wire::env_reveal(&entries))).into_response();
    // Des valeurs secrètes : aucun cache, ni du client ni d'un intermédiaire.
    let headers = response.headers_mut();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(PRAGMA, HeaderValue::from_static("no-cache"));
    Ok(response)
}

/// Le refus de l'adoption désactivée (`BR-ADOPT-020`), texte de la spec.
fn adoption_disabled() -> ApiError {
    ApiError(ErrorBody::with_details(
        ErrorCode::Conflict,
        "L'adoption de services est désactivée sur ce serveur. Un administrateur du serveur peut la réactiver dans la configuration de l'agent Hearth.",
        serde_json::json!({ "reason": "adoption_disabled" }),
    ))
}

fn adoption_error(error: AdoptionError) -> ApiError {
    match error {
        AdoptionError::Disabled => adoption_disabled(),
        AdoptionError::EngineNotReady(state) => ApiError(ErrorBody::with_details(
            ErrorCode::Conflict,
            "Le moteur de conteneurs n'est pas prêt : rien ne peut être adopté pour l'instant.",
            serde_json::json!({
                "reason": "engine_not_ready",
                "engine": services_wire::engine_state(state),
            }),
        )),
        AdoptionError::Invalid(Invalid::TooMany) => {
            ApiError::invalid("keys", "Tu peux adopter 50 services à la fois.")
        }
        AdoptionError::Invalid(Invalid::Empty) => {
            ApiError::invalid("keys", "Choisis au moins un service à adopter.")
        }
        AdoptionError::Invalid(Invalid::Duplicate) => {
            ApiError::invalid("keys", "Un service ne se choisit qu'une fois.")
        }
        AdoptionError::Store(error) => ApiError::internal(&error),
    }
}

/// `GET /api/v1/services/external` : les conteneurs que ce compte peut choisir d'adopter, lus à
/// l'ouverture de la fenêtre (`BR-ADOPT-005`). Le droit est jugé par la couche d'accès. Une lecture qui
/// aboutit est consignée (`service.external.read`, décision par défaut du propriétaire : gérer des
/// conteneurs déjà présents est sensible), sans aucun nom ni clé de conteneur : la cible est la route,
/// et les lectures répétées du même compte passent par le regroupement des répétitions du journal
/// (`RepeatFilter`, une entrée par fenêtre). Jamais en cache : le contenu n'est pas fait pour être gardé.
pub async fn external(
    State(state): State<AppState>,
    Requester(actor): Requester,
) -> Result<Response, ApiError> {
    let found = state
        .service_adoption
        .candidates()
        .map_err(adoption_error)?;
    state
        .sink
        .record(
            actor,
            AuditAction::ServiceExternalRead,
            Target::Route("/services/external"),
            Outcome::Succeeded,
        )
        .await;
    let body = ExternalContainers {
        engine: services_wire::engine_state(found.engine),
        containers: found
            .candidates
            .iter()
            .map(|candidate| ExternalContainer {
                key: candidate.key.as_str().to_owned(),
                name: candidate.name.clone(),
                image: ImageInfo {
                    reference: candidate.image.reference.clone(),
                    tag: candidate.image.tag.clone(),
                    short_id: candidate.image.short_id.clone(),
                },
                unit: candidate.unit.as_ref().map(|unit| unit.as_str().to_owned()),
                state: services_wire::service_state(candidate.state),
            })
            .collect(),
    };
    let mut response = (StatusCode::OK, Json(body)).into_response();
    let headers = response.headers_mut();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(PRAGMA, HeaderValue::from_static("no-cache"));
    Ok(response)
}

/// `POST /api/v1/services/adopt` : adopte les conteneurs choisis, une issue par clé (`BR-ADOPT-008`,
/// `BR-ADOPT-010`). La couche `reauth` a vérifié la preuve de clé (qui lie le nombre et l'empreinte des
/// clés envoyées), puis le mot de passe, jamais l'élévation.
pub async fn adopt(
    State(state): State<AppState>,
    _confirmed: Extension<Reauthenticated>,
    Requester(by): Requester,
    body: Result<Json<AdoptRequest>, JsonRejection>,
) -> Result<Json<AdoptResponse>, ApiError> {
    let Json(request) = body?;
    let report = state
        .service_adoption
        .adopt(&request.keys, &by)
        .await
        .map_err(adoption_error)?;
    Ok(Json(AdoptResponse {
        adopted: report.adopted,
        results: report
            .results
            .into_iter()
            .map(|(key, outcome)| match outcome {
                KeyOutcome::Adopted => AdoptionItem {
                    key,
                    result: AdoptionResult::Adopted,
                    by: None,
                },
                KeyOutcome::AlreadyAdopted { by } => AdoptionItem {
                    key,
                    result: AdoptionResult::AlreadyAdopted,
                    by: Some(by),
                },
                KeyOutcome::NotFound => AdoptionItem {
                    key,
                    result: AdoptionResult::NotFound,
                    by: None,
                },
                KeyOutcome::Failed => AdoptionItem {
                    key,
                    result: AdoptionResult::Failed,
                    by: None,
                },
            })
            .collect(),
    }))
}

/// `POST /api/v1/services/{key}/release` : « Ne plus gérer » (`BR-ADOPT-013`). Une clé mal formée, inconnue,
/// cachée ou déjà relâchée reçoit la même réponse (`released: false`), sans effet ni entrée de journal.
pub async fn release(
    State(state): State<AppState>,
    _confirmed: Extension<Reauthenticated>,
    Requester(by): Requester,
    Path(key): Path<String>,
) -> Result<Json<ReleaseResponse>, ApiError> {
    let released = match ServiceKey::parse(&key) {
        Some(key) => state
            .service_adoption
            .release(&key, &by)
            .await
            .map_err(adoption_error)?,
        None => false,
    };
    Ok(Json(ReleaseResponse { released }))
}

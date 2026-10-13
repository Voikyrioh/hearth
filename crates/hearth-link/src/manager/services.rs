//! Le moteur de conteneurs et les services d'un serveur (HRT-62, ADR-0038) : la dernière vue connue et
//! « Vérifier à nouveau ». Le sujet `services` du flux est tenu par la tâche du serveur
//! (`task.rs`, règles dans `domain/services.rs`) ; ici, les appels typés, chemins fixes construits par
//! le code, jamais fournis par l'appelant.
//!
//! `POST /engine/recheck` est une LECTURE : il ne modifie rien (route de `READ_ONLY_POSTS` côté agent), il
//! n'est ni un acte d'administration ni une action suivie (pas de clé d'opération, pas de « résultat
//! inconnu » : si le lien tombe, l'appel échoue et l'interface le refait). Même contrôle d'accès que
//! toute lecture : c'est celui de l'agent.

use hearth_proto::admin_act::is_service_key;
use hearth_proto::api::services::{
    AdoptRequest, AdoptResponse, EngineView, ExternalContainers, ReleaseResponse, ServicesState,
};
use serde::de::DeserializeOwned;

use super::{ActionOutcome, ActionRequest, LinkManager};
use crate::domain::pending_ops::OperationId;
use crate::domain::secret::Secret;
use crate::domain::server::ServerId;
use crate::domain::time::WallTime;
use crate::error::LinkError;
use crate::ports::transport::{ApiRequest, Method};

const RECHECK_PATH: &str = "/engine/recheck";
const EXTERNAL_PATH: &str = "/services/external";
const ADOPT_PATH: &str = "/services/adopt";

/// L'issue d'un acte de gestion des services (adopter, relâcher) : la réponse typée de l'agent, ou
/// l'inconnu quand le lien est tombé avant la réponse. Un refus de l'agent (droit, confirmation, moteur
/// pas prêt, coupe-circuit...) n'est pas ici : c'est une `LinkError` typée.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Acted<T> {
    Done(T),
    /// Le lien est tombé avant la réponse : on ne sait pas si l'acte a eu lieu, et il n'est jamais rejoué.
    /// L'issue arrive plus tard par `Event::Operation`.
    ResultUnknown {
        id: OperationId,
    },
}

/// La clé d'un service telle qu'elle voyage dans le chemin : la barre oblique inverse d'un nom d'unité
/// échappé par le système (`\x2d`) est encodée, tout le reste d'une clé de service passe tel quel.
fn key_segment(key: &str) -> String {
    key.replace('\\', "%5C")
}

fn acted<T: DeserializeOwned>(outcome: ActionOutcome, what: &str) -> Result<Acted<T>, LinkError> {
    match outcome {
        ActionOutcome::ResultUnknown { id } => Ok(Acted::ResultUnknown { id }),
        ActionOutcome::Completed { status, body, .. } => {
            if !(200..300).contains(&status) {
                return Err(super::audit::rejection(&body));
            }
            serde_json::from_value(body)
                .map(Acted::Done)
                .map_err(|e| LinkError::Protocol(format!("{what} illisible : {e}")))
        }
    }
}

/// La dernière liste de services connue d'un serveur, avec la date à laquelle la liaison en a reçu la vue du
/// moteur (heure de ce poste ; `None` pour un fichier écrit avant que ce repère existe : jamais inventée).
#[derive(Debug, Clone, PartialEq)]
pub struct KnownServices {
    pub state: ServicesState,
    pub engine_received_at: Option<WallTime>,
}

impl LinkManager {
    /// La dernière liste de services connue de ce serveur, même hors ligne (en mémoire, sinon relue du
    /// disque) ; `None` tant qu'aucune n'a été reçue.
    pub async fn services_state(&self, id: &ServerId) -> Result<Option<KnownServices>, LinkError> {
        Ok(self.last_known(id).await?.and_then(|view| {
            let engine_received_at = view.services_at;
            view.services.map(|state| KnownServices {
                state,
                engine_received_at,
            })
        }))
    }

    /// Les conteneurs que ce compte peut choisir d'adopter (`GET /services/external`, lecture typée). Il
    /// faut le droit « gérer les services » : sans lui `MissingRight`. Rien n'est gardé : la liste n'est
    /// faite ni pour le flux ni pour le disque (BR-ADOPT-005).
    pub async fn services_external(&self, id: &ServerId) -> Result<ExternalContainers, LinkError> {
        let (target, token) = self.credentials(id)?;
        let body = self
            .typed_get(&target, &token, EXTERNAL_PATH.to_owned())
            .await?;
        serde_json::from_value(body)
            .map_err(|e| LinkError::Protocol(format!("conteneurs candidats illisibles : {e}")))
    }

    /// Adopte ces clés (`POST /services/adopt`) : un acte d'administration, le mot de passe ET la preuve
    /// de la clé de ce poste, à chaque fois (BR-ADOPT-008). La liaison ne juge ni le nombre ni les doublons :
    /// c'est le refus de l'agent, rendu `Rejected(Some(code))`. Les erreurs de `execute_act` s'appliquent :
    /// `NotConnected`, `NoDeviceKey`, `Incompatible(UpdateAgent)`, `InvalidInput(Credentials)` sans mot de
    /// passe. Jamais rejoué.
    pub async fn services_adopt(
        &self,
        id: &ServerId,
        keys: &[String],
        password: &Secret,
    ) -> Result<Acted<AdoptResponse>, LinkError> {
        let body = serde_json::to_value(AdoptRequest {
            keys: keys.to_vec(),
        })
        .map_err(|e| LinkError::Protocol(format!("demande d'adoption : {e}")))?;
        let outcome = self
            .execute_act(
                id,
                ActionRequest {
                    method: Method::Post,
                    path: ADOPT_PATH.to_owned(),
                    body: Some(body),
                },
                Some(password),
            )
            .await?;
        acted(outcome, "réponse d'adoption")
    }

    /// Relâche ce service (« Ne plus gérer », `POST /services/{key}/release`). La clé est celle de la liste
    /// des services ; la barre oblique inverse d'un nom d'unité est encodée ici, l'appelant passe la clé en
    /// clair. Une clé qui n'a pas la forme d'une clé de service : `UnreadableAct`, rien n'est envoyé.
    pub async fn services_release(
        &self,
        id: &ServerId,
        key: &str,
        password: &Secret,
    ) -> Result<Acted<ReleaseResponse>, LinkError> {
        if !is_service_key(key) {
            return Err(LinkError::UnreadableAct);
        }
        let outcome = self
            .execute_act(
                id,
                ActionRequest {
                    method: Method::Post,
                    path: format!("/services/{}/release", key_segment(key)),
                    body: Some(serde_json::json!({})),
                },
                Some(password),
            )
            .await?;
        acted(outcome, "réponse de relâche")
    }

    /// « Vérifier à nouveau » : relance la détection du moteur tout de suite (`POST /engine/recheck`) et
    /// rend l'état du moteur. L'état publié par le flux arrive aussi par le sujet `services`.
    ///
    /// Hors « Connecté » : `NotConnected`, rien n'est envoyé. Sur un agent sans la capacité `services` la
    /// route n'existe pas : le client ne la demande donc pas (`NotFound` de l'agent serait rendu en
    /// `Rejected`), mais cette garde est dans l'interface, qui n'affiche pas le bouton.
    pub async fn engine_recheck(&self, id: &ServerId) -> Result<EngineView, LinkError> {
        let (target, token) = self.credentials(id)?;
        let request = ApiRequest {
            method: Method::Post,
            path: RECHECK_PATH.to_owned(),
            body: None,
            idempotency_key: None,
        };
        let deps = &self.inner.deps;
        let response = tokio::time::timeout(
            deps.config.request_timeout,
            deps.transport.request(&target, &token, &request),
        )
        .await
        .map_err(|_| LinkError::Timeout)??;
        if !(200..300).contains(&response.status) {
            return Err(super::audit::rejection(&response.body));
        }
        serde_json::from_value(response.body)
            .map_err(|e| LinkError::Protocol(format!("état du moteur illisible : {e}")))
    }
}

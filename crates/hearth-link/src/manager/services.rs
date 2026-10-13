//! Le moteur de conteneurs et les services d'un serveur (HRT-62, ADR-0038) : la dernière vue connue et
//! « Vérifier à nouveau ». Le sujet `services` du flux est tenu par la tâche du serveur
//! (`task.rs`, règles dans `domain/services.rs`) ; ici, les appels typés, chemins fixes construits par
//! le code, jamais fournis par l'appelant.
//!
//! `POST /engine/recheck` est une LECTURE : il ne modifie rien (route de `READ_ONLY_POSTS` côté agent), il
//! n'est ni un acte d'administration ni une action suivie (pas de clé d'opération, pas de « résultat
//! inconnu » : si le lien tombe, l'appel échoue et l'interface le refait). Même contrôle d'accès que
//! toute lecture : c'est celui de l'agent.

use hearth_proto::api::services::{EngineView, ServicesState};

use super::LinkManager;
use crate::domain::server::ServerId;
use crate::domain::time::WallTime;
use crate::error::LinkError;
use crate::ports::transport::{ApiRequest, Method};

const RECHECK_PATH: &str = "/engine/recheck";

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

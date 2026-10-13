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
use hearth_proto::api::services::{EngineView, EnvReveal, ServicesState};

use super::LinkManager;
use crate::domain::server::ServerId;
use crate::domain::time::WallTime;
use crate::error::{InputField, LinkError};
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

    /// Révéler les valeurs des variables d'un service (HRT-63, `BR-SVC-009`) : `GET /services/{key}/env`, une
    /// LECTURE que l'agent réserve aux comptes qui ont le droit de gérer les services et qu'il consigne au
    /// journal (sans nom ni valeur). La clé est vérifiée (forme d'une clé de service) puis encodée par le
    /// code : l'appelant ne fournit jamais un chemin. Hors « Connecté » : `NotConnected`, rien n'est envoyé.
    /// Rien n'est rejoué ni gardé : si le lien tombe, l'appel échoue et l'interface le refait. La réponse
    /// n'entre dans aucune trace, et une réponse illisible ne cite jamais son contenu.
    pub async fn reveal_service_env(
        &self,
        id: &ServerId,
        key: &str,
    ) -> Result<EnvReveal, LinkError> {
        if !is_service_key(key) {
            return Err(LinkError::InvalidInput(InputField::Filter));
        }
        let (target, token) = self.credentials(id)?;
        let body = self
            .typed_get(
                &target,
                &token,
                format!("/services/{}/env", encode_key(key)),
            )
            .await?;
        serde_json::from_value(body)
            .map_err(|_| LinkError::Protocol("variables du service illisibles".into()))
    }
}

/// Encode une clé de service pour un segment de chemin : tout octet hors lettres, chiffres, `-`, `_` et `.`
/// devient `%XX` (une clé `u_…` peut porter `\`, `:` ou `@`). L'agent décode le segment.
fn encode_key(key: &str) -> String {
    let mut out = String::with_capacity(key.len());
    for byte in key.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::encode_key;

    #[test]
    fn a_key_is_encoded_as_one_path_segment() {
        assert_eq!(encode_key("c_0123abcd"), "c_0123abcd");
        assert_eq!(
            encode_key(r"u_docker-my\x2dapp.service"),
            "u_docker-my%5Cx2dapp.service"
        );
        assert_eq!(encode_key("u_a@b:c.service"), "u_a%40b%3Ac.service");
    }
}

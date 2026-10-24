//! Le droit « gérer les services » du compte connecté (HRT-78, ADR-0041, BR-GRANT-011) : UNE lecture
//! typée, chemin fixe (`GET /me`), jamais une écriture. Le droit ne se donne PAS d'ici (BR-GRANT-005) :
//! la bibliothèque n'a aucune méthode qui l'accorde ni le retire, elle ne fait que le lire.
//!
//! Deux faits, relus ensemble à chaque appel (le client relit à l'ouverture de la section, à chaque
//! reconnexion et après tout refus pour absence du droit) :
//! - la capacité `service-rights` de `GET /hello` : l'agent connaît-il le droit (BR-GRANT-016) ? Un agent
//!   qui ne l'annonce pas n'a pas ce droit, et `rights` y est absent du fil : lu comme vide ;
//! - `rights` du compte courant (`GET /me`) : le droit est-il accordé ?
//!
//! Même garanties que les autres lectures : cible épinglée, jeton du coffre, rien hors « Connecté »,
//! contrôle d'accès fait par l'agent.

use hearth_proto::api::accounts::RIGHT_SERVICES;
use hearth_proto::api::accounts::has_right;
use hearth_proto::api::sessions::MeResponse;
use tokio::time::timeout;

use super::LinkManager;
use crate::domain::server::ServerId;
use crate::domain::services::{agent_knows_service_rights, agent_serves_game_settings};
use crate::error::LinkError;
use crate::ports::transport::Target;

const ME_PATH: &str = "/me";

/// Ce que le client sait du droit du compte connecté.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceRightsRead {
    /// L'agent annonce `service-rights` : sans cela, `granted` vaut toujours `false` (BR-GRANT-016).
    pub agent_knows: bool,
    /// Le compte connecté a le droit (toujours `false` si l'agent ne le connaît pas).
    pub granted: bool,
    /// L'agent annonce `game-settings` : l'onglet « Réglages » d'un service de fiche peut exister (HRT-109).
    pub game_settings: bool,
    /// L'identifiant du compte connecté, tel que l'agent le connaît : la commande à lancer sur le serveur
    /// le porte (BR-GRANT-014). Texte de l'agent, à valider avant tout affichage.
    pub account: String,
}

impl LinkManager {
    /// Les capacités que l'agent annonce MAINTENANT (`GET /hello`, public) : une mise à jour de l'agent
    /// les change, elles sont relues à chaque appel.
    pub(super) async fn agent_capabilities(
        &self,
        target: &Target,
    ) -> Result<Vec<String>, LinkError> {
        let deps = &self.inner.deps;
        let probed = timeout(deps.config.request_timeout, deps.transport.hello(target))
            .await
            .map_err(|_| LinkError::Timeout)??;
        Ok(probed.hello.capabilities)
    }

    /// Le droit du compte connecté (`GET /me` + capacités de `GET /hello`). Hors « Connecté » :
    /// `NotConnected`, rien n'est envoyé. Pas de clé d'opération ni de suivi : si le lien tombe, l'appel
    /// échoue et l'interface le refait.
    pub async fn service_rights(&self, id: &ServerId) -> Result<ServiceRightsRead, LinkError> {
        let (target, token) = self.credentials(id)?;
        let capabilities = self.agent_capabilities(&target).await?;
        let me = self.typed_get(&target, &token, ME_PATH.to_owned()).await?;
        let me: MeResponse = serde_json::from_value(me)
            .map_err(|e| LinkError::Protocol(format!("compte courant illisible : {e}")))?;
        let agent_knows = agent_knows_service_rights(&capabilities);
        Ok(ServiceRightsRead {
            agent_knows,
            granted: agent_knows && has_right(&me.account.rights, RIGHT_SERVICES),
            game_settings: agent_serves_game_settings(&capabilities),
            account: me.account.username,
        })
    }
}

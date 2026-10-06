//! Lecture de l'état de la mise à jour de l'agent (HRT-17) : DEUX méthodes typées, chemins fixes
//! construits ici, méthode toujours `GET`, aucune requête quelconque. Mêmes garanties que le
//! journal et les comptes (cible épinglée, jeton du coffre, rien hors « Connecté », erreurs
//! typées) ; le contrôle d'accès est celui de l'agent (la lecture est ouverte à tout compte).
//!
//! Une lecture n'est pas une action : pas de clé d'opération, pas de suivi, jamais d'issue
//! « résultat inconnu » ; si le lien tombe elle échoue et l'interface la refait, au retour du lien
//! (BR-UPDATE-017). La demande de mise à jour, elle, est une ACTION (`LinkManager::execute`) : sa
//! requête est construite par la coquille (`agent_update/wire.rs`), jamais par l'interface.

use hearth_proto::api::update::{AgentUpdateStatus, LastUpdateResponse, UpdateResult};

use super::LinkManager;
use crate::domain::server::ServerId;
use crate::error::LinkError;

const STATUS_PATH: &str = "/agent/update";
const LAST_PATH: &str = "/agent/update/last";

impl LinkManager {
    /// `GET /agent/update` : version de l'agent, installation gérée ou non, mise à jour en cours,
    /// dernier résultat.
    pub async fn agent_update_status(&self, id: &ServerId) -> Result<AgentUpdateStatus, LinkError> {
        let (target, token) = self.credentials(id)?;
        let body = self
            .typed_get(&target, &token, STATUS_PATH.to_owned())
            .await?;
        serde_json::from_value(body)
            .map_err(|e| LinkError::Protocol(format!("état de la mise à jour illisible : {e}")))
    }

    /// `GET /agent/update/last` : le dernier résultat (`None` si aucune mise à jour n'a jamais eu
    /// lieu), pour le client qui revient après une coupure (BR-UPDATE-017).
    pub async fn agent_update_last(
        &self,
        id: &ServerId,
    ) -> Result<Option<UpdateResult>, LinkError> {
        let (target, token) = self.credentials(id)?;
        let body = self
            .typed_get(&target, &token, LAST_PATH.to_owned())
            .await?;
        let last: LastUpdateResponse = serde_json::from_value(body)
            .map_err(|e| LinkError::Protocol(format!("dernier résultat illisible : {e}")))?;
        Ok(last.last)
    }
}

//! Démarrer, arrêter, redémarrer un service (HRT-65, BR-LIFE-016) : trois actes d'administration confirmés
//! (`ServiceStart`, `ServiceStop`, `ServiceRestart`, couverts par l'élévation de cinq minutes). Le chemin est
//! construit ici (`/services/{key}/start|stop|restart`), la clé est vérifiée puis encodée par le code, jamais
//! fournie comme chemin. L'acte signé est reconstruit depuis cette requête par `domain/act.rs`, comme le fait
//! l'agent : ce que la clé signe est ce qui part.
//!
//! L'agent répond `202` avec l'identifiant de l'opération et le délai d'arrêt ; l'issue ne se lit jamais dans
//! cette réponse, elle arrive par le suivi des opérations (HRT-84). Si le lien tombe avant la réponse, le
//! résultat est inconnu et l'acte n'est JAMAIS rejoué (BR-LIFE-020, BR-RESIL-009).

use hearth_proto::admin_act::is_service_key;
use serde_json::json;

use super::services::key_segment;
use super::{ActionOutcome, ActionRequest, LinkManager};
use crate::domain::secret::Secret;
use crate::domain::server::ServerId;
use crate::domain::service_lifecycle::Gesture;
use crate::error::LinkError;
use crate::ports::transport::Method;

impl LinkManager {
    /// Envoie le geste de pilotage `gesture` pour le service `key` (clé de la liste des services, en clair).
    /// Une clé qui n'a pas la forme d'une clé de service : `UnreadableAct`, rien n'est envoyé. Les erreurs de
    /// `execute_act` s'appliquent (`NotConnected`, `NoDeviceKey`, `Incompatible(UpdateAgent)`,
    /// `InvalidInput(Credentials)` sans mot de passe quand la grille de l'état de confiance annoncé l'exige à chaque
    /// fois, HRT-194). La réponse
    /// de l'agent (`202` ou refus typé) est rendue telle quelle ; `domain::service_lifecycle::refusal_of` la lit.
    pub async fn service_gesture(
        &self,
        id: &ServerId,
        key: &str,
        gesture: Gesture,
        password: Option<&Secret>,
    ) -> Result<ActionOutcome, LinkError> {
        if !is_service_key(key) {
            return Err(LinkError::UnreadableAct);
        }
        let action = ActionRequest {
            method: Method::Post,
            path: format!("/services/{}/{}", key_segment(key), gesture.segment()),
            body: Some(json!({})),
        };
        self.execute_act(id, action, password).await
    }
}

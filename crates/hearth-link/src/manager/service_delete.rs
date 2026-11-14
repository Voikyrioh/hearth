//! Supprimer un service depuis le client (HRT-86, HRT-85, `BR-DEL-*`) : la consultation qui bâtit la fenêtre
//! (`GET /services/{key}/delete-preview`) et l'acte `ServiceDelete` (`0x14`, `DELETE /services/{key}`). Chemins
//! construits ici : la clé du service est vérifiée puis encodée par le code, jamais fournie comme chemin.
//!
//! - **Consulter** est une LECTURE : rien n'est écrit ni journalisé côté agent. L'agent mesure les dossiers
//!   gérés pendant 10 secondes au plus (`BR-DEL-009`) : le délai de la requête est allongé de cette durée.
//! - **Supprimer** est un ACTE d'administration confirmé, JAMAIS couvert par l'élévation (`BR-DEL-012`). L'acte
//!   signé est reconstruit depuis cette requête par `domain/act.rs` (la clé, les deux cases ; jamais le nom
//!   retapé). L'agent répond `202` : l'issue arrive par le suivi des opérations (HRT-84). Jamais rejoué : un
//!   lien tombé avant la réponse donne `ResultUnknown` (`BR-DEL-023`).

use std::time::Duration;

use hearth_proto::admin_act::is_service_key;
use hearth_proto::api::service_delete::{DeletePreview, DeleteRequest, MEASURE_BUDGET_S};

use super::services::key_segment;
use super::{ActionOutcome, ActionRequest, LinkManager};
use crate::domain::secret::Secret;
use crate::domain::server::ServerId;
use crate::error::{InputField, LinkError};
use crate::ports::transport::Method;

/// Le chemin de la suppression d'un service, ou `None` si `key` n'a pas la forme d'une clé de service.
pub fn delete_path(key: &str) -> Option<String> {
    is_service_key(key).then(|| format!("/services/{}", key_segment(key)))
}

/// Le chemin de la consultation.
pub fn preview_path(key: &str) -> Option<String> {
    delete_path(key).map(|path| format!("{path}/delete-preview"))
}

impl LinkManager {
    /// De quoi bâtir la fenêtre « Supprimer {nom} ? » (`GET /services/{key}/delete-preview`). Un refus de l'agent
    /// (droit, service inconnu ou adopté, moteur pas prêt) est `Rejected(Some(code))` ou `MissingRight`. Hors
    /// « Connecté » : `NotConnected`. Une clé qui n'en est pas une : `InvalidInput`, rien ne part.
    pub async fn delete_preview(
        &self,
        id: &ServerId,
        key: &str,
    ) -> Result<DeletePreview, LinkError> {
        let path = preview_path(key).ok_or(LinkError::InvalidInput(InputField::Filter))?;
        let (target, token) = self.credentials(id)?;
        let body = self
            .typed_get_slow(&target, &token, path, Duration::from_secs(MEASURE_BUDGET_S))
            .await?;
        serde_json::from_value(body)
            .map_err(|_| LinkError::Protocol("prévisualisation de la suppression illisible".into()))
    }

    /// Supprime le service `key` (`DELETE /services/{key}`, acte `ServiceDelete`). `password` : le mot de passe de
    /// confirmation, exigé à chaque fois (jamais gardé). La réponse de l'agent (`202` ou refus typé) est rendue
    /// telle quelle. Les erreurs de `execute_act` s'appliquent (`NotConnected`, `NoDeviceKey`,
    /// `Incompatible(UpdateAgent)`, `InvalidInput(Credentials)` sans mot de passe).
    pub async fn delete_service(
        &self,
        id: &ServerId,
        key: &str,
        request: &DeleteRequest,
        password: Option<&Secret>,
    ) -> Result<ActionOutcome, LinkError> {
        let path = delete_path(key).ok_or(LinkError::UnreadableAct)?;
        let body = serde_json::to_value(request)
            .map_err(|_| LinkError::Protocol("demande de suppression illisible".into()))?;
        let action = ActionRequest {
            method: Method::Delete,
            path,
            body: Some(body),
        };
        self.execute_act(id, action, password).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_service_key_becomes_one_encoded_segment_and_anything_else_never_leaves() {
        assert_eq!(
            delete_path("c_0123abcd").as_deref(),
            Some("/services/c_0123abcd")
        );
        assert_eq!(
            preview_path("h_0123abcd").as_deref(),
            Some("/services/h_0123abcd/delete-preview")
        );
        for bad in ["", "../x", "a/b", "c_ab/../x", "c_ab?x=1"] {
            assert!(delete_path(bad).is_none(), "{bad}");
            assert!(preview_path(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn the_signed_act_is_rebuilt_from_the_request_the_link_sends() {
        use crate::domain::act::{Route, classify};
        let key = "h_01J9ZY0G3Q8M2K6W4T7V5N1B9D";
        let path = delete_path(key).expect("chemin");
        let body = serde_json::to_value(DeleteRequest {
            erase_data: true,
            erase_backups: false,
            confirm_name: Some("terraria".into()),
        })
        .expect("json");
        let Ok(Route::Act(act)) = classify("DELETE", &path, Some(&body)) else {
            panic!("acte attendu");
        };
        assert_eq!(act.target(), key);
        assert_eq!(act.params(), vec![b"1".to_vec(), b"0".to_vec()]);
    }
}

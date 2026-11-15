//! L'adresse que le propriétaire donne à ses joueurs, depuis le client (HRT-215, `BR-JEU-022`) : la description d'un
//! service (`GET /services/{key}/spec`, une LECTURE qui donne sa révision) puis la modification qui ne change que cette
//! adresse (`PUT /services/{key}/spec`, acte `ServiceUpdate` `0x13`, JAMAIS couvert par l'élévation : le mot de passe
//! est demandé à chaque fois, `BR-EDIT-017`). Chemins construits ici : la clé du service est vérifiée puis encodée par
//! le code. L'acte signé est reconstruit depuis le corps par `domain/act.rs` : jamais l'adresse ni un nom. Jamais
//! rejoué : un lien tombé avant la réponse donne `ResultUnknown`, et l'interface relit la vue du jeu.

use hearth_proto::admin_act::is_service_key;
use hearth_proto::api::service_update::{ServiceSpecView, SpecAccept, SpecChange};

use super::parse::parse;
use super::services::key_segment;
use super::{ActionOutcome, ActionRequest, LinkManager};
use crate::domain::secret::Secret;
use crate::domain::server::ServerId;
use crate::error::{InputField, LinkError};
use crate::ports::transport::Method;

/// Le chemin de la description d'un service, ou `None` si `key` n'a pas la forme d'une clé de service.
pub fn spec_path(key: &str) -> Option<String> {
    is_service_key(key).then(|| format!("/services/{}/spec", key_segment(key)))
}

impl LinkManager {
    /// La description d'un service (`GET /services/{key}/spec`) : sa révision, que la modification renvoie. Une
    /// clé qui n'en est pas une : `InvalidInput`, rien ne part. Hors « Connecté » : `NotConnected`.
    pub async fn read_service_spec(
        &self,
        id: &ServerId,
        key: &str,
    ) -> Result<ServiceSpecView, LinkError> {
        let path = spec_path(key).ok_or(LinkError::InvalidInput(InputField::Filter))?;
        let (target, token) = self.credentials(id)?;
        let body = self.typed_get(&target, &token, path).await?;
        parse(body, "description du service")
    }

    /// Change l'adresse des joueurs de `key` (`PUT /services/{key}/spec`, acte `ServiceUpdate`) : lit la révision
    /// courante, puis n'envoie que `public_address` (`""` l'efface). `password` : le mot de passe de confirmation,
    /// exigé à chaque fois (jamais gardé). La réponse de l'agent (`202`, `200` quand rien ne change, ou un refus
    /// typé) est rendue telle quelle. Les erreurs de `execute_act` s'appliquent.
    pub async fn set_public_address(
        &self,
        id: &ServerId,
        key: &str,
        address: &str,
        password: Option<&Secret>,
    ) -> Result<ActionOutcome, LinkError> {
        let path = spec_path(key).ok_or(LinkError::UnreadableAct)?;
        let spec = self.read_service_spec(id, key).await?;
        let change = SpecChange {
            base_rev: spec.rev,
            seen_drift: None,
            tag: None,
            repull: false,
            env: Vec::new(),
            ports: None,
            mounts: None,
            command: None,
            answers: None,
            public_address: Some(address.to_owned()),
            accept: SpecAccept::default(),
            extra: std::collections::BTreeMap::new(),
        };
        let body = serde_json::to_value(&change)
            .map_err(|_| LinkError::Protocol("modification illisible".into()))?;
        let action = ActionRequest {
            method: Method::Put,
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
        assert!(
            spec_path("h_01J9ZY0G3Q8M2K6W4T7V5N1B9D")
                .is_some_and(|path| path.ends_with("/spec") && path.starts_with("/services/"))
        );
        for bad in ["", "../x", "a/b", "h_x?y=1"] {
            assert!(spec_path(bad).is_none(), "{bad}");
        }
    }
}

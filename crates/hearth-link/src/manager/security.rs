//! L'état de sécurité d'un serveur et le mode attaque (HRT-26, ADR-0025, BR-TRUST-010, 018) :
//! lecture typée de `GET /security`, activation et désactivation par `PUT /security/attack-mode`.
//!
//! Activer comme désactiver est UN ACTE D'ADMINISTRATION (Q14 point 3, Q16) : le mot de passe actuel
//! de l'administrateur ET la preuve de la clé de CE poste (acte `AttackMode`, usage `0x05`, liée au jeton de
//! la session et au geste demandé). La clé ne sort pas d'ici : aucune fonction publique ne la rend, ne rend le
//! défi ni la signature.

use hearth_proto::api::security::SecurityResponse;
use serde_json::json;

use super::device::{KeyState, load_key};
use super::{ActionOutcome, ActionRequest, LinkManager};
use crate::domain::compat::Compatibility;
use crate::domain::secret::Secret;
use crate::domain::server::ServerId;
use crate::error::{InputField, LinkError};
use crate::ports::transport::Method;

const SECURITY_PATH: &str = "/security";
const ATTACK_MODE_PATH: &str = "/security/attack-mode";

impl LinkManager {
    /// L'état de sécurité du compte de la session (`GET /security`, lecture typée, tout rôle) :
    /// alerte, mode attaque, et si la session a été prouvée par la clé d'un poste inscrit. Sur un
    /// agent d'avant la fonction : `Rejected(Some(NotFound))`.
    pub async fn security(&self, id: &ServerId) -> Result<SecurityResponse, LinkError> {
        let (target, token) = self.credentials(id)?;
        let body = self
            .typed_get(&target, &token, SECURITY_PATH.to_owned())
            .await?;
        serde_json::from_value(body)
            .map_err(|e| LinkError::Protocol(format!("état de sécurité illisible : {e}")))
    }

    /// Ce PC a-t-il une clé d'appareil au coffre pour ce serveur ? (un booléen : la clé elle-même ne
    /// sort pas). Sert à dire, sans rien envoyer, qu'un administrateur ne peut pas agir d'ici.
    pub fn has_device_key(&self, id: &ServerId) -> bool {
        matches!(load_key(&self.inner.deps, id), KeyState::Present(_))
    }

    /// Active (`active: true`) ou désactive le mode attaque : un acte d'administration, le mot de passe
    /// actuel ET la preuve de la clé de ce poste. Hors « Connecté » : `NotConnected` sans rien envoyer.
    /// Sans clé au coffre : `NoDeviceKey` sans AUCUN appel d'écriture (le défi n'est même pas demandé).
    /// Ensuite c'est une action ordinaire (clé d'opération, jamais rejouée, issue `ResultUnknown` si le
    /// lien tombe). Le mot de passe n'est jamais gardé.
    ///
    /// Le contrat commun des actes (`reauth`, usage `0x05`). Un agent qui n'annonce pas `admin_reauth`
    /// n'est pas un agent de cette famille : `Incompatible(UpdateAgent)`, rien n'est envoyé.
    pub async fn set_attack_mode(
        &self,
        id: &ServerId,
        active: bool,
        password: &Secret,
    ) -> Result<ActionOutcome, LinkError> {
        if password.is_empty() {
            return Err(LinkError::InvalidInput(InputField::Credentials));
        }
        let (target, token) = self.credentials(id)?;
        if self.read_admin_reauth(&target, &token).await?.is_none() {
            return Err(LinkError::Incompatible(Compatibility::UpdateAgent));
        }
        let action = ActionRequest {
            method: Method::Put,
            path: ATTACK_MODE_PATH.to_owned(),
            body: Some(json!({ "active": active })),
        };
        self.send_confirmed(id, &target, &token, action, Some(password))
            .await
    }
}

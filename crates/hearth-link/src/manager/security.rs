//! L'état de sécurité d'un serveur et le mode attaque (HRT-26, ADR-0025, BR-TRUST-010, 018) :
//! lecture typée de `GET /security`, activation et désactivation par `PUT /security/attack-mode`.
//!
//! Activer comme désactiver est UN ACTE D'ADMINISTRATION (Q14 point 3, Q16) : le mot de passe actuel
//! de l'administrateur ET la preuve de la clé de CE poste (usage `0x03`, liée au jeton de la session
//! et au geste demandé). La clé ne sort pas d'ici : aucune fonction publique ne la rend, ne rend le
//! défi ni la signature.

use hearth_proto::api::security::SecurityResponse;
use hearth_proto::api::sessions::ChallengePurpose;
use hearth_proto::device_proof::Binding;
use hearth_proto::error::ErrorCode;
use serde_json::json;

use super::device::{ChallengeAnswer, KeyState, ask_challenge, load_key, pinned, token_hash};
use super::{ActionOutcome, ActionRequest, LinkManager};
use crate::domain::secret::Secret;
use crate::domain::server::ServerId;
use crate::error::{InputField, LinkError};
use crate::ports::transport::{Method, Target};
use hearth_proto::fingerprint::Fingerprint;

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
    /// Face à un agent qui annonce `admin_reauth` : le contrat commun des actes (`reauth`, usage `0x05`).
    /// Face à un agent d'avant : la forme livrée (champs à plat, usage `0x03`), que l'agent accepte
    /// toujours pendant la transition (ADR-0033).
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
        if self.read_admin_reauth(&target, &token).await?.is_some() {
            let action = ActionRequest {
                method: Method::Put,
                path: ATTACK_MODE_PATH.to_owned(),
                body: Some(json!({ "active": active })),
            };
            return self
                .send_confirmed(id, &target, &token, action, Some(password))
                .await;
        }
        self.set_attack_mode_flat(id, &target, &token, active, password)
            .await
    }

    /// La forme livrée (HRT-26) pour un agent d'avant la confirmation des actes : mot de passe et preuve
    /// d'usage `0x03` à plat.
    async fn set_attack_mode_flat(
        &self,
        id: &ServerId,
        target: &Target,
        token: &Secret,
        active: bool,
        password: &Secret,
    ) -> Result<ActionOutcome, LinkError> {
        let deps = &self.inner.deps;
        let KeyState::Present(key) = load_key(deps, id) else {
            return Err(LinkError::NoDeviceKey);
        };
        let (_, shared) = self.handle(id)?;
        let username = shared.record().username;
        let fingerprint = fingerprint_of(target)?;
        let hash = token_hash(token.expose())
            .ok_or_else(|| LinkError::Protocol("jeton illisible".into()))?;
        // Un défi indisponible (coupure du seul défi, délai, réponse illisible) n'est pas « serveur
        // injoignable » : rien n'est parti, ni mot de passe ni preuve, et on le dit.
        let challenge =
            match ask_challenge(deps, target, &username, ChallengePurpose::AttackMode).await {
                ChallengeAnswer::Issued(challenge) => challenge,
                ChallengeAnswer::Unsupported => {
                    return Err(LinkError::Rejected(Some(ErrorCode::NotFound)));
                }
                ChallengeAnswer::Unavailable => return Err(LinkError::DeviceChallengeUnavailable),
                ChallengeAnswer::Mismatch(error) => return Err(error.into()),
            };
        let proof = key
            .prove(
                Binding::AttackMode {
                    token_hash: &hash,
                    activate: active,
                },
                &fingerprint,
                &username,
                &challenge,
            )
            .ok_or(LinkError::DeviceChallengeUnavailable)?;
        let body = json!({
            "active": active,
            "password": password.expose(),
            "device": proof,
        });
        self.execute_unchecked(
            id,
            ActionRequest {
                method: Method::Put,
                path: ATTACK_MODE_PATH.to_owned(),
                body: Some(body),
            },
        )
        .await
    }
}

/// L'empreinte épinglée du serveur. Un serveur du carnet est toujours épinglé ; sans empreinte, ce n'est
/// PAS « pas de clé » (l'utilisateur lirait « ce poste n'est pas enregistré ») mais une réponse qui n'est
/// pas celle d'un agent confirmé.
fn fingerprint_of(target: &Target) -> Result<Fingerprint, LinkError> {
    pinned(target).ok_or_else(|| LinkError::Protocol("empreinte du serveur non confirmée".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::transport::Pin;

    #[test]
    fn a_target_without_a_pinned_fingerprint_is_not_a_missing_key() {
        let probe = Target {
            host: "127.0.0.1".into(),
            port: 7341,
            pin: Pin::Probe,
        };
        assert!(matches!(
            fingerprint_of(&probe),
            Err(LinkError::Protocol(_))
        ));
    }
}

//! Déployer une image depuis le client (HRT-69, ADR-0039) : l'acte `ServiceDeploy` (`0x0F`), la consultation
//! du formulaire et les bornes de la machine. Chemins fixes construits ici, jamais fournis par l'appelant.
//!
//! - **Déployer** (`POST /deployments`) est un ACTE d'administration confirmé, JAMAIS couvert par l'élévation
//!   (le mot de passe à chaque fois, BR-DEPLOY-015). L'acte signé est reconstruit depuis le corps par
//!   `domain/act.rs` (`AdminAct::service_deploy`, source unique partagée avec l'agent) : ni valeur, ni nom de
//!   variable, ni commande dans la signature. L'agent répond `202` ; l'issue arrive par le suivi des
//!   opérations. Jamais rejoué : un lien tombé avant la réponse donne `ResultUnknown` (BR-DEPLOY-028).
//! - **Consulter** (`POST /deployments/check`) est une LECTURE : un `POST` qui ne modifie rien, ni clé
//!   d'opération ni journal (BR-DEPLOY-033). Elle ne reçoit JAMAIS un secret (`DeployCheckRequest` n'a ni
//!   variables ni commande) : si le lien tombe elle échoue et l'interface la refait.
//! - **Capacité** (`GET /engine/capacity`) : une lecture, ouverte à tout compte.
//!
//! Annuler l'opération d'un déploiement est `cancel_operation` (HRT-144), un autre acte.

use hearth_proto::api::deploy::{DeployCheck, DeployCheckRequest, DeploySpec, EngineCapacity};
use serde::de::DeserializeOwned;

use super::{ActionOutcome, ActionRequest, LinkManager};
use crate::domain::secret::Secret;
use crate::domain::server::ServerId;
use crate::error::LinkError;
use crate::ports::transport::Method;

const DEPLOYMENTS_PATH: &str = "/deployments";
const CHECK_PATH: &str = "/deployments/check";
const CAPACITY_PATH: &str = "/engine/capacity";

/// Lit une réponse typée. L'erreur de lecture ne recopie JAMAIS celle de `serde` : elle citerait la valeur fautive.
fn parse<T: DeserializeOwned>(body: serde_json::Value, what: &str) -> Result<T, LinkError> {
    serde_json::from_value(body).map_err(|_| LinkError::Protocol(format!("{what} illisible")))
}

impl LinkManager {
    /// Déploie une image (`POST /deployments`, acte `ServiceDeploy`). `password` : le mot de passe de
    /// confirmation, exigé à chaque fois (jamais gardé). La réponse de l'agent (`202` ou refus typé) est rendue
    /// telle quelle. Les erreurs de `execute_act` s'appliquent (`NotConnected`, `NoDeviceKey`,
    /// `Incompatible(UpdateAgent)`, `InvalidInput(Credentials)` sans mot de passe).
    pub async fn deploy_service(
        &self,
        id: &ServerId,
        spec: &DeploySpec,
        password: Option<&Secret>,
    ) -> Result<ActionOutcome, LinkError> {
        let body = serde_json::to_value(spec)
            .map_err(|_| LinkError::Protocol("demande de déploiement illisible".into()))?;
        let action = ActionRequest {
            method: Method::Post,
            path: DEPLOYMENTS_PATH.to_owned(),
            body: Some(body),
        };
        self.execute_act(id, action, password).await
    }

    /// Les constats du formulaire (`POST /deployments/check`) : une consultation, sans secret. Hors « Connecté » :
    /// `NotConnected`. Un refus de l'agent (droit, moteur) est `Rejected(Some(code))`.
    pub async fn deploy_check(
        &self,
        id: &ServerId,
        request: &DeployCheckRequest,
    ) -> Result<DeployCheck, LinkError> {
        let (target, token) = self.credentials(id)?;
        let body = serde_json::to_value(request)
            .map_err(|_| LinkError::Protocol("consultation illisible".into()))?;
        let answer = self
            .typed_request(
                &target,
                &token,
                Method::Post,
                CHECK_PATH.to_owned(),
                Some(body),
            )
            .await?;
        parse(answer, "constats du déploiement")
    }

    /// Les bornes de la machine et la part déjà engagée (`GET /engine/capacity`, BR-DEPLOY-038). Moteur pas
    /// prêt : `Rejected(Some(EngineUnavailable))`.
    pub async fn engine_capacity(&self, id: &ServerId) -> Result<EngineCapacity, LinkError> {
        let (target, token) = self.credentials(id)?;
        let body = self
            .typed_get(&target, &token, CAPACITY_PATH.to_owned())
            .await?;
        parse(body, "capacité du moteur")
    }
}

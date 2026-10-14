//! Les opérations de service d'un serveur (HRT-144, ADR-0045) : LIRE une opération, la DÉCIDER, l'ANNULER.
//! Chemins fixes construits ici, jamais fournis par l'appelant ; l'identifiant d'une opération est
//! contrôlé (lettres et chiffres) avant de partir, il ne peut pas devenir un autre chemin.
//!
//! - **Lire** (`GET /operations/services/{id}`) est une LECTURE : pas de clé d'opération, pas de « résultat
//!   inconnu » ; si le lien tombe elle échoue et l'interface la refait. C'est ainsi qu'un client qui revient
//!   après une coupure relit l'issue d'une opération qu'il suivait (BR-LIFE-020, BR-DEPLOY-028).
//! - **Décider** et **annuler** sont des ACTES d'administration confirmés (`OperationDecide`,
//!   `OperationCancel`) : preuve de clé du poste et mot de passe (ou élévation) par `execute_act`, comme
//!   tout acte ; l'issue est une action ordinaire (`ResultUnknown` si le lien tombe).
//!
//! Le suivi en direct arrive par `EventSink::service_operation` (message `operation` du sujet `services`).
//! Les refus de l'agent restent typés par leur code : `OperationNotFound`, `TooLateToCancel`,
//! `DecisionNotOffered`, `ForbiddenRole` + `details.missing` (`MissingRight`), `PostNotRecognized`…

use hearth_proto::api::service_ops::{OperationChoice, OperationView};
use serde_json::json;

use super::{ActionOutcome, ActionRequest, LinkManager};
use crate::domain::pending_ops::OperationId;
use crate::domain::secret::Secret;
use crate::domain::server::ServerId;
use crate::domain::service_ops::{OperationRefusal, refusal_of};
use crate::error::LinkError;
use crate::ports::transport::Method;

/// L'issue d'un acte sur une opération (annuler, décider).
#[derive(Debug, Clone, PartialEq)]
pub enum OperationActResult {
    /// L'agent a fait ce qui est demandé : l'opération telle qu'elle est maintenant.
    Done(OperationView),
    /// L'agent refuse, avec sa raison.
    Refused(OperationRefusal),
    /// Le lien est tombé avant la réponse : on ne sait pas. L'issue se relit par `service_operation`, et
    /// arrive aussi par le message `operation` du flux. L'acte n'est jamais rejoué.
    Unknown { id: OperationId },
}

fn result_of(outcome: ActionOutcome) -> Result<OperationActResult, LinkError> {
    match outcome {
        ActionOutcome::Completed { status, body, .. } if (200..300).contains(&status) => {
            serde_json::from_value(body)
                .map(OperationActResult::Done)
                .map_err(|e| LinkError::Protocol(format!("opération illisible : {e}")))
        }
        ActionOutcome::Completed { status, body, .. } => {
            Ok(OperationActResult::Refused(refusal_of(status, &body)))
        }
        ActionOutcome::ResultUnknown { id } => Ok(OperationActResult::Unknown { id }),
    }
}

/// Un identifiant d'opération de l'agent : un ULID, lettres et chiffres. Même borne que l'agent côté acte.
const ID_MAX_LEN: usize = 64;

fn operation_path(op_id: &str) -> Result<String, LinkError> {
    let valid = !op_id.is_empty()
        && op_id.len() <= ID_MAX_LEN
        && op_id.bytes().all(|b| b.is_ascii_alphanumeric());
    if valid {
        Ok(format!("/operations/services/{op_id}"))
    } else {
        Err(LinkError::UnreadableAct)
    }
}

impl LinkManager {
    /// `GET /operations/services/{id}` : l'état d'une opération (24 heures après sa fin). Une opération
    /// inconnue, purgée, ou sur un service que ce compte ne voit pas : `Rejected(Some(OperationNotFound))`,
    /// sans distinction.
    pub async fn service_operation(
        &self,
        id: &ServerId,
        op_id: &str,
    ) -> Result<OperationView, LinkError> {
        let path = operation_path(op_id)?;
        let (target, token) = self.credentials(id)?;
        let body = self.typed_get(&target, &token, path).await?;
        serde_json::from_value(body)
            .map_err(|e| LinkError::Protocol(format!("opération illisible : {e}")))
    }

    /// `DELETE /operations/services/{id}` : annule l'opération (acte `OperationCancel`, confirmé). Idempotent
    /// côté agent : annuler deux fois rend deux `200`. Après le point de non-retour du genre :
    /// `Refused(TooLate)`. Les erreurs de la confirmation elle-même (hors « Connecté », sans clé, agent
    /// trop ancien…) restent des `LinkError`, comme pour tout acte.
    pub async fn cancel_operation(
        &self,
        id: &ServerId,
        op_id: &str,
        password: Option<&Secret>,
    ) -> Result<OperationActResult, LinkError> {
        let action = ActionRequest {
            method: Method::Delete,
            path: operation_path(op_id)?,
            body: Some(json!({})),
        };
        result_of(self.execute_act(id, action, password).await?)
    }

    /// `POST /operations/services/{id}/decision` : le choix du compte parmi ceux que l'opération offre (acte
    /// `OperationDecide`, confirmé ; le choix est signé). Un choix inconnu de ce client ne part pas.
    pub async fn decide_operation(
        &self,
        id: &ServerId,
        op_id: &str,
        choice: OperationChoice,
        password: Option<&Secret>,
    ) -> Result<OperationActResult, LinkError> {
        if choice == OperationChoice::Unknown {
            return Err(LinkError::UnreadableAct);
        }
        let action = ActionRequest {
            method: Method::Post,
            path: format!("{}/decision", operation_path(op_id)?),
            body: Some(json!({ "choice": choice.as_str() })),
        };
        result_of(self.execute_act(id, action, password).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_operation_identifier_never_becomes_another_path() {
        assert_eq!(
            operation_path("01J9ZY0G3Q8M2K6W4T7V5N1B9D").as_deref(),
            Ok("/operations/services/01J9ZY0G3Q8M2K6W4T7V5N1B9D")
        );
        for bad in ["", "../x", "a/b", "a b", "a%2Fb", "a?x=1", &"A".repeat(65)] {
            assert!(operation_path(bad).is_err(), "{bad}");
        }
    }
}

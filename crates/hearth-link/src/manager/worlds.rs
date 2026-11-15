//! Reprendre un monde existant depuis le client (HRT-106, ADR-0059, `BR-MONDE-002` à `016`, `039`) : `GET
//! /catalog/{id}/worlds` (les mondes que la fiche reconnaît dans les dossiers ouverts aux services) et `POST
//! /catalog/{id}/worlds/inspect` (ce qu'une reprise copierait). Deux LECTURES : ni acte, ni élévation, ni journal,
//! rien n'est écrit. Chemins construits ici : l'identifiant d'une fiche ne peut pas devenir un autre chemin. Un monde
//! est désigné par un dossier ouvert et un chemin relatif, jamais un chemin absolu ; ni l'un ni l'autre n'entre dans
//! une trace. Installer avec un monde est le déploiement ordinaire (`DeploySource::Fiche.world`).

use hearth_proto::api::worlds::{WorldInspection, WorldRef, WorldsView};
use hearth_proto::error::{ErrorBody, ErrorCode};
use serde_json::Value;

use super::LinkManager;
use super::audit::rejection_at;
use super::catalog::is_stable_id;
use super::parse::parse;
use crate::domain::server::ServerId;
use crate::error::{InputField, LinkError};
use crate::ports::transport::{ApiRequest, Method};

/// Ce que la liste des mondes a donné.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorldsOutcome {
    Ready(WorldsView),
    /// `422` `details.reason = "not_supported"` : la fiche ne dit pas ce qu'est un monde (`BR-MONDE-039`).
    NotSupported,
}

/// Ce que l'inspection d'un monde a donné.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InspectOutcome {
    Ready(WorldInspection),
    /// `422` : le monde est refusé. `reason` est le code court de l'agent (`unknown_folder`, `bad_path`,
    /// `not_a_world`, `not_found`, `too_big`, `name_unusable`, `unreadable`), jamais un texte reçu recopié ailleurs.
    Refused {
        reason: Option<String>,
    },
}

fn worlds_path(fiche: &str) -> Option<String> {
    is_stable_id(fiche).then(|| format!("/catalog/{fiche}/worlds"))
}

/// Le `details.reason` d'un refus `422`, s'il y en a un.
fn reason_of(body: &Value) -> Option<String> {
    serde_json::from_value::<ErrorBody>(body.clone())
        .ok()
        .and_then(|parsed| {
            parsed
                .error
                .details
                .get("reason")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
}

fn is_validation(body: &Value, status: u16) -> bool {
    serde_json::from_value::<ErrorBody>(body.clone())
        .ok()
        .is_some_and(|parsed| {
            parsed.error.code.or_from_status(status) == ErrorCode::ValidationError
        })
}

impl LinkManager {
    /// Les mondes que la fiche reconnaît dans les dossiers ouverts aux services (`GET /catalog/{id}/worlds`). Hors
    /// « Connecté » : `NotConnected`. Un identifiant qui n'en est pas un : `InvalidInput`, rien ne part. Fiche
    /// inconnue de cet agent : `Rejected(Some(FicheUnknown))`.
    pub async fn read_worlds(
        &self,
        id: &ServerId,
        fiche: &str,
    ) -> Result<WorldsOutcome, LinkError> {
        let path = worlds_path(fiche).ok_or(LinkError::InvalidInput(InputField::Fiche))?;
        let (target, token) = self.credentials(id)?;
        let request = ApiRequest {
            method: Method::Get,
            path,
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
        if (200..300).contains(&response.status) {
            return parse(response.body, "mondes existants").map(WorldsOutcome::Ready);
        }
        if is_validation(&response.body, response.status)
            && reason_of(&response.body).as_deref() == Some("not_supported")
        {
            return Ok(WorldsOutcome::NotSupported);
        }
        Err(rejection_at(&response.body, response.status))
    }

    /// Inspecte un monde (`POST /catalog/{id}/worlds/inspect`) : une consultation qui ne modifie rien, la mesure peut
    /// durer jusqu'à une minute (le délai de la requête est allongé, voir `timeout_for`). Un monde refusé n'est pas
    /// une panne : c'est `InspectOutcome::Refused`.
    pub async fn inspect_world(
        &self,
        id: &ServerId,
        fiche: &str,
        world: &WorldRef,
    ) -> Result<InspectOutcome, LinkError> {
        let path = worlds_path(fiche)
            .map(|path| format!("{path}/inspect"))
            .ok_or(LinkError::InvalidInput(InputField::Fiche))?;
        let (target, token) = self.credentials(id)?;
        let body = serde_json::to_value(world)
            .map_err(|_| LinkError::Protocol("monde illisible".into()))?;
        let request = ApiRequest {
            method: Method::Post,
            path,
            body: Some(body),
            idempotency_key: None,
        };
        let deps = &self.inner.deps;
        let response = tokio::time::timeout(
            deps.config.request_timeout
                + std::time::Duration::from_secs(hearth_proto::api::backups::ESTIMATE_BUDGET_S),
            deps.transport.request(&target, &token, &request),
        )
        .await
        .map_err(|_| LinkError::Timeout)??;
        if (200..300).contains(&response.status) {
            return parse(response.body, "inspection du monde").map(InspectOutcome::Ready);
        }
        if is_validation(&response.body, response.status) {
            return Ok(InspectOutcome::Refused {
                reason: reason_of(&response.body),
            });
        }
        Err(rejection_at(&response.body, response.status))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fiche_identifier_never_becomes_another_path() {
        assert_eq!(
            worlds_path("terraria").as_deref(),
            Some("/catalog/terraria/worlds")
        );
        for bad in ["", "../x", "a/b", "A", "a?x=1"] {
            assert!(worlds_path(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn the_reason_of_a_refusal_is_read_from_its_details() {
        let body = serde_json::json!({"error": {"code": "VALIDATION_ERROR", "message": "m",
            "details": {"field": "world", "reason": "not_a_world"}}});
        assert_eq!(reason_of(&body).as_deref(), Some("not_a_world"));
        assert!(is_validation(&body, 422));
        assert_eq!(reason_of(&serde_json::json!({})), None);
    }
}

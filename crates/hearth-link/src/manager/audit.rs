//! Lecture du journal d'activité d'un serveur (HRT-14) : UNE méthode typée par lecture, aucune
//! requête quelconque. Le chemin est construit par `domain::audit_query` à partir d'un filtre
//! validé ; la méthode HTTP est toujours `GET`.
//!
//! Une lecture n'est pas une action : pas de clé d'opération, pas de suivi, jamais d'issue
//! « résultat inconnu » ; si le lien tombe, elle échoue et l'interface la refait (BR-AUDIT-020).
//! Hors « Connecté », rien n'est envoyé (BR-RESIL-008).

use futures_util::future::join_all;
use hearth_proto::api::audit::{AuditEventItem, AuditQuery, AuditResponse};
use hearth_proto::api::audit_csv;
use hearth_proto::error::ErrorBody;
use serde_json::Value;
use tokio::time::timeout;

use super::LinkManager;
use crate::domain::audit_query::{self, AuditFilter, EXPORT_CAP, FilterError, PAGE_SIZE};
use crate::domain::server::ServerId;
use crate::domain::state::LinkState;
use crate::error::{InputField, LinkError, RefusalReason};
use crate::ports::transport::{ApiRequest, Method, Pin, Target};
use crate::ports::vault::SecretKind;

/// Le fichier d'export : octets UTF-8 avec marque d'ordre des octets, prêts à écrire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditExportFile {
    pub bytes: Vec<u8>,
    /// Seules les 10 000 entrées les plus récentes du résultat y sont.
    pub truncated: bool,
}

impl From<FilterError> for LinkError {
    fn from(_: FilterError) -> Self {
        Self::InvalidInput(InputField::Filter)
    }
}

/// Un refus de l'agent, lu dans son enveloppe d'erreur.
#[cfg(test)]
pub(super) fn rejection(body: &Value) -> LinkError {
    rejection_at(body, 0)
}

/// Comme [`rejection`], avec le statut HTTP de la réponse : un code d'erreur inconnu de cette version
/// retombe sur le sens du statut (`ErrorCode::or_from_status`, BR-COMPAT-001 point 8), jamais sur un refus
/// passe-partout.
pub(super) fn rejection_at(body: &Value, status: u16) -> LinkError {
    let Some(body) = serde_json::from_value::<ErrorBody>(body.clone()).ok() else {
        return LinkError::Rejected(None);
    };
    // Le refus d'un droit (`403 FORBIDDEN_ROLE` + `details.missing`, BR-GRANT-004) n'est pas un refus de
    // rôle : il suit jusqu'au client, qui ne dit pas « administrateur » (BR-GRANT-014).
    if let Some(right) = body.error.missing_right() {
        return LinkError::MissingRight(right.to_owned());
    }
    LinkError::Rejected(Some(body.error.code.or_from_status(status)))
}

/// Comme [`rejection`], mais un refus qui porte `details.reason` le dit (`RejectedBecause`) : adopter et
/// relâcher, dont l'interface doit distinguer les refus que le seul code ne sépare pas. Un refus sans raison
/// reste `Rejected`. Comme [`rejection_at`], un code inconnu de cette version retombe sur le sens du statut HTTP
/// (BR-COMPAT-001 point 8) : FIX:01M4VR6Y4Q4AVQVT9FCME7Z2SB.
pub(super) fn rejection_with_reason(body: &Value, status: u16) -> LinkError {
    let Some(parsed) = serde_json::from_value::<ErrorBody>(body.clone()).ok() else {
        return LinkError::Rejected(None);
    };
    if parsed.error.missing_right().is_some() {
        return rejection_at(body, status);
    }
    let code = parsed.error.code.or_from_status(status);
    match parsed.error.details.get("reason").and_then(Value::as_str) {
        Some(reason) => LinkError::RejectedBecause {
            code,
            reason: RefusalReason::from_wire(reason),
        },
        None => LinkError::Rejected(Some(code)),
    }
}

impl LinkManager {
    /// Hors « Connecté » : `NotConnected`. Sinon, la cible et le jeton de ce serveur.
    pub(super) fn credentials(
        &self,
        id: &ServerId,
    ) -> Result<(Target, crate::domain::secret::Secret), LinkError> {
        let (_, shared) = self.handle(id)?;
        if shared.state.borrow().state != LinkState::Connected {
            return Err(LinkError::NotConnected);
        }
        let token = self
            .inner
            .deps
            .vault
            .get(id, SecretKind::Token)
            .map_err(|e| LinkError::Vault(e.to_string()))?
            .ok_or(LinkError::NotConnected)?;
        let target = shared.target();
        debug_assert!(matches!(target.pin, Pin::Pinned(_)));
        Ok((target, token))
    }

    /// Un GET authentifié sur un chemin CONSTRUIT PAR UN CODE TYPÉ (jamais fourni par l'appelant de
    /// la bibliothèque) : délai borné, tout refus (hors 2xx) rendu en erreur typée. Partagé par les
    /// lectures du journal et des comptes.
    pub(super) async fn typed_get(
        &self,
        target: &Target,
        token: &crate::domain::secret::Secret,
        path: String,
    ) -> Result<Value, LinkError> {
        let request = ApiRequest {
            method: Method::Get,
            path,
            body: None,
            idempotency_key: None,
        };
        let deps = &self.inner.deps;
        let response = timeout(
            deps.config.request_timeout,
            deps.transport.request(target, token, &request),
        )
        .await
        .map_err(|_| LinkError::Timeout)??;
        if !(200..300).contains(&response.status) {
            return Err(rejection_at(&response.body, response.status));
        }
        Ok(response.body)
    }

    async fn audit_get(
        &self,
        target: &Target,
        token: &crate::domain::secret::Secret,
        query: &AuditQuery,
        before: Option<i64>,
        limit: u8,
    ) -> Result<AuditResponse, LinkError> {
        let path = audit_query::path(query, Some((before, limit)))?;
        let body = self.typed_get(target, token, path).await?;
        serde_json::from_value(body)
            .map_err(|e| LinkError::Protocol(format!("page du journal illisible : {e}")))
    }

    /// Une page du journal filtré, de la plus récente à la plus ancienne : `limit` entrées au plus
    /// plus anciennes que `before`. `next_before` (absent à la fin) est le curseur de la suite.
    pub async fn audit_page(
        &self,
        id: &ServerId,
        filter: &AuditFilter,
        before: Option<i64>,
        limit: u8,
    ) -> Result<AuditResponse, LinkError> {
        let (target, token) = self.credentials(id)?;
        let limit = limit.clamp(1, PAGE_SIZE);
        let plan = filter.plan();
        self.audit_page_with(&target, &token, &plan, before, limit)
            .await
    }

    /// Le résultat filtré en CSV (BR-AUDIT-017). Une seule requête quand le filtre le permet : le
    /// fichier est celui de l'agent. Sinon les pages sont recollées (jusqu'au plafond de
    /// 10 000 entrées) et le fichier est écrit avec le MÊME rendu (`hearth_proto::api::audit_csv`),
    /// donc la même neutralisation des formules.
    pub async fn audit_export(
        &self,
        id: &ServerId,
        filter: &AuditFilter,
    ) -> Result<AuditExportFile, LinkError> {
        let (target, token) = self.credentials(id)?;
        let plan = filter.plan();
        let deps = &self.inner.deps;
        match plan.as_slice() {
            [] => Ok(AuditExportFile {
                bytes: audit_csv::render_items(&[]).into_bytes(),
                truncated: false,
            }),
            [query] => {
                let path = audit_query::path(query, None)?;
                let export = timeout(
                    deps.config.request_timeout,
                    deps.transport.export_audit(&target, &token, &path),
                )
                .await
                .map_err(|_| LinkError::Timeout)??;
                Ok(AuditExportFile {
                    bytes: export.body,
                    truncated: export.truncated,
                })
            }
            _ => {
                let mut items: Vec<AuditEventItem> = Vec::new();
                let mut before = None;
                let mut truncated = false;
                loop {
                    let page = self
                        .audit_page_with(&target, &token, &plan, before, PAGE_SIZE)
                        .await?;
                    items.extend(page.events);
                    match page.next_before {
                        Some(next) if items.len() < EXPORT_CAP => before = Some(next),
                        Some(_) => {
                            truncated = true;
                            break;
                        }
                        None => break,
                    }
                }
                if items.len() > EXPORT_CAP {
                    items.truncate(EXPORT_CAP);
                    truncated = true;
                }
                Ok(AuditExportFile {
                    bytes: audit_csv::render_items(&items).into_bytes(),
                    truncated,
                })
            }
        }
    }

    async fn audit_page_with(
        &self,
        target: &Target,
        token: &crate::domain::secret::Secret,
        plan: &[AuditQuery],
        before: Option<i64>,
        limit: u8,
    ) -> Result<AuditResponse, LinkError> {
        let calls = plan
            .iter()
            .map(|query| self.audit_get(target, token, query, before, limit));
        join_pages(join_all(calls).await, usize::from(limit))
    }
}

/// Recolle les réponses des requêtes d'un même plan : TOUT OU RIEN. Une seule qui échoue fait échouer
/// la page entière, jamais un résultat partiel présenté comme complet.
fn join_pages(
    results: Vec<Result<AuditResponse, LinkError>>,
    limit: usize,
) -> Result<AuditResponse, LinkError> {
    let mut pages = Vec::with_capacity(results.len());
    for page in results {
        pages.push(page?);
    }
    Ok(audit_query::merge_pages(pages, limit))
}

#[cfg(test)]
mod tests {
    use hearth_proto::api::audit::{AuditOrigin, OriginKindName, OutcomeName};

    use super::*;
    use hearth_proto::error::ErrorCode;
    use serde_json::json;

    #[test]
    fn a_refusal_for_a_missing_right_keeps_the_right_that_is_missing() {
        let body =
            serde_json::to_value(ErrorBody::missing_right("services", "pas le droit")).unwrap();
        assert_eq!(
            rejection(&body),
            LinkError::MissingRight("services".to_owned())
        );
    }

    #[test]
    fn an_unknown_error_code_keeps_the_meaning_of_its_http_status() {
        let future = json!({ "error": { "code": "CODE_DU_FUTUR", "message": "m", "details": {} } });
        assert_eq!(
            rejection_at(&future, 403),
            LinkError::Rejected(Some(ErrorCode::ForbiddenRole))
        );
        assert_eq!(
            rejection_at(&future, 404),
            LinkError::Rejected(Some(ErrorCode::NotFound))
        );
        assert_eq!(
            rejection_at(&future, 418),
            LinkError::Rejected(Some(ErrorCode::Unknown))
        );
    }

    #[test]
    fn a_plain_role_refusal_and_an_unreadable_body_stay_what_they_were() {
        let role = serde_json::to_value(ErrorBody::new(ErrorCode::ForbiddenRole, "rôle")).unwrap();
        assert_eq!(
            rejection(&role),
            LinkError::Rejected(Some(ErrorCode::ForbiddenRole))
        );
        assert_eq!(
            rejection(&json!("n'importe quoi")),
            LinkError::Rejected(None)
        );
    }

    #[test]
    fn a_refusal_of_an_adoption_or_a_release_gives_its_reason_and_tolerates_an_unknown_one() {
        use crate::error::RefusalReason as R;
        let cases = [
            (
                ErrorCode::Conflict,
                "adoption_disabled",
                R::AdoptionDisabled,
            ),
            (ErrorCode::Conflict, "engine_not_ready", R::EngineNotReady),
            (ErrorCode::ValidationError, "keys_empty", R::KeysEmpty),
            (ErrorCode::ValidationError, "keys_too_many", R::KeysTooMany),
            (
                ErrorCode::ValidationError,
                "keys_duplicate",
                R::KeysDuplicate,
            ),
            (
                ErrorCode::PostNotRecognized,
                "proof_missing",
                R::ProofMissing,
            ),
            (
                ErrorCode::PostNotRecognized,
                "proof_invalid",
                R::ProofInvalid,
            ),
            (
                ErrorCode::PostNotRecognized,
                "password_required",
                R::PasswordRequired,
            ),
            (
                ErrorCode::IncompatibleVersion,
                "reauth_required",
                R::ReauthRequired,
            ),
            // Une raison d'une version plus récente de l'agent : inconnue, jamais recopiée.
            (ErrorCode::Conflict, "une_raison_future", R::Unknown),
        ];
        for (code, reason, expected) in cases {
            let body = ErrorBody::with_details(code, "texte", json!({ "reason": reason }));
            assert_eq!(
                rejection_with_reason(&serde_json::to_value(body).unwrap(), 0),
                LinkError::RejectedBecause {
                    code,
                    reason: expected
                },
                "{reason}"
            );
        }
        // Une raison qui n'est pas un texte, ou aucune raison : le refus ordinaire.
        let odd = ErrorBody::with_details(ErrorCode::Conflict, "x", json!({ "reason": 3 }));
        assert_eq!(
            rejection_with_reason(&serde_json::to_value(odd).unwrap(), 0),
            LinkError::Rejected(Some(ErrorCode::Conflict))
        );
        assert_eq!(
            rejection_with_reason(&json!("n'importe quoi"), 409),
            LinkError::Rejected(None)
        );
        // Le droit qui manque garde la priorité.
        let right =
            serde_json::to_value(ErrorBody::missing_right("services", "pas le droit")).unwrap();
        assert_eq!(
            rejection_with_reason(&right, 403),
            LinkError::MissingRight("services".to_owned())
        );
    }

    /// FIX:01M4VR6Y4Q4AVQVT9FCME7Z2SB : un code inconnu retombe sur le sens du statut HTTP aussi pour les refus
    /// d'adoption et de relâche, avec ou sans raison ; un `2xx` ou un statut hors table reste `Unknown`.
    #[test]
    fn a_reasoned_refusal_with_an_unknown_code_falls_back_on_the_http_status() {
        let unknown = |details| ErrorBody::with_details(ErrorCode::Unknown, "texte", details);
        let without = serde_json::to_value(unknown(json!({}))).unwrap();
        let with = serde_json::to_value(unknown(json!({ "reason": "reauth_required" }))).unwrap();
        assert_eq!(
            rejection_with_reason(&without, 409),
            LinkError::Rejected(Some(ErrorCode::Conflict))
        );
        assert_eq!(
            rejection_with_reason(&without, 403),
            LinkError::Rejected(Some(ErrorCode::ForbiddenRole))
        );
        assert_eq!(
            rejection_with_reason(&with, 409),
            LinkError::RejectedBecause {
                code: ErrorCode::Conflict,
                reason: RefusalReason::ReauthRequired
            }
        );
        // Aucun repli à inventer : un statut hors table (ou un `2xx`) reste inconnu.
        for status in [0, 200, 418] {
            assert_eq!(
                rejection_with_reason(&without, status),
                LinkError::Rejected(Some(ErrorCode::Unknown)),
                "{status}"
            );
        }
    }

    #[test]
    fn a_missing_right_is_only_read_on_a_role_refusal_and_stays_short() {
        let other =
            ErrorBody::with_details(ErrorCode::NotFound, "x", json!({ "missing": "services" }));
        assert_eq!(
            rejection(&serde_json::to_value(other).unwrap()),
            LinkError::Rejected(Some(ErrorCode::NotFound))
        );
        for odd in [json!(""), json!(3), json!("x".repeat(33))] {
            let body =
                ErrorBody::with_details(ErrorCode::ForbiddenRole, "x", json!({ "missing": odd }));
            assert_eq!(
                rejection(&serde_json::to_value(body).unwrap()),
                LinkError::Rejected(Some(ErrorCode::ForbiddenRole))
            );
        }
    }

    fn item(id: i64) -> AuditEventItem {
        AuditEventItem {
            id,
            at: "2026-10-04T10:30:15Z".into(),
            account: None,
            origin: AuditOrigin {
                kind: OriginKindName::Cli,
                name: None,
                addr: None,
                text: "ligne de commande du serveur".into(),
            },
            action: "login".into(),
            action_label: "Connexion".into(),
            target: None,
            outcome: OutcomeName::Ok,
            reason: None,
            repeat_count: 0,
        }
    }

    #[test]
    fn one_failing_request_of_several_fails_the_whole_page() {
        let ok = AuditResponse {
            events: vec![item(9), item(7)].into(),
            next_before: None,
        };
        let results = vec![Ok(ok.clone()), Err(LinkError::Timeout), Ok(ok.clone())];
        assert_eq!(join_pages(results, 100), Err(LinkError::Timeout));
        let all_ok = join_pages(vec![Ok(ok.clone()), Ok(ok)], 100).unwrap();
        assert_eq!(all_ok.events.len(), 2, "doublons retirés");
    }
}

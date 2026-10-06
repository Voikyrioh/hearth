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
use crate::error::{InputField, LinkError};
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
fn rejection(body: &Value) -> LinkError {
    let code = serde_json::from_value::<ErrorBody>(body.clone())
        .ok()
        .map(|body| body.error.code);
    LinkError::Rejected(code)
}

impl LinkManager {
    /// Hors « Connecté » : `NotConnected`. Sinon, la cible et le jeton de ce serveur.
    fn audit_credentials(
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

    async fn audit_get(
        &self,
        target: &Target,
        token: &crate::domain::secret::Secret,
        query: &AuditQuery,
        before: Option<i64>,
        limit: u8,
    ) -> Result<AuditResponse, LinkError> {
        let path = audit_query::path(query, Some((before, limit)))?;
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
            return Err(rejection(&response.body));
        }
        serde_json::from_value(response.body)
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
        let (target, token) = self.audit_credentials(id)?;
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
        let (target, token) = self.audit_credentials(id)?;
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
            events: vec![item(9), item(7)],
            next_before: None,
        };
        let results = vec![Ok(ok.clone()), Err(LinkError::Timeout), Ok(ok.clone())];
        assert_eq!(join_pages(results, 100), Err(LinkError::Timeout));
        let all_ok = join_pages(vec![Ok(ok.clone()), Ok(ok)], 100).unwrap();
        assert_eq!(all_ok.events.len(), 2, "doublons retirés");
    }
}

//! `GET /audit` et `GET /audit/export` (administrateurs seulement : niveau `Admin` de `ENDPOINTS`,
//! BR-AUDIT-001). Consulter le journal ne s'y consigne pas (BR-AUDIT-004) ; un refus faute de
//! droits, si (BR-AUDIT-021) : c'est la couche d'accès qui l'écrit.
//!
//! Les paramètres arrivent en texte (`hearth_proto::api::audit::AuditQuery`) ; leur contrôle est
//! celui du domaine (`AuditFilter`). La recherche `q` est du texte, jamais une requête.

use axum::Json;
use axum::extract::{RawQuery, State};
use axum::http::header::{CONTENT_DISPOSITION, CONTENT_TYPE};
use axum::http::{HeaderName, HeaderValue};
use axum::response::{IntoResponse, Response};
use hearth_proto::api::audit::{AuditQuery, AuditResponse, EXPORT_TRUNCATED_HEADER};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::auth::Caller;
use super::{ApiError, AppState, wire};
use crate::domain::audit::{AuditFilter, FilterError, RawFilter};

fn list_of(value: Option<&str>) -> Vec<String> {
    value
        .map(|value| value.split(',').map(str::to_owned).collect())
        .unwrap_or_default()
}

fn date(field: &str, value: Option<&str>) -> Result<Option<OffsetDateTime>, ApiError> {
    value
        .filter(|value| !value.is_empty())
        .map(|value| {
            OffsetDateTime::parse(value, &Rfc3339)
                .map_err(|_| ApiError::invalid(field, "Date attendue au format RFC 3339"))
        })
        .transpose()
}

fn number<T: std::str::FromStr>(field: &str, value: Option<&str>) -> Result<Option<T>, ApiError> {
    value
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .parse::<T>()
                .map_err(|_| ApiError::invalid(field, "Nombre entier attendu"))
        })
        .transpose()
}

/// Lit et contrôle les paramètres de requête.
fn filter_of(raw_query: Option<String>) -> Result<AuditFilter, ApiError> {
    let query: AuditQuery = serde_urlencoded::from_str(raw_query.as_deref().unwrap_or(""))
        .map_err(|_| ApiError::invalid("query", "Paramètres de requête illisibles"))?;
    let raw = RawFilter {
        accounts: list_of(query.account.as_deref()),
        actions: list_of(query.action.as_deref()),
        outcomes: list_of(query.outcome.as_deref()),
        from: date("from", query.from.as_deref())?,
        to: date("to", query.to.as_deref())?,
        text: query.q,
        before: number("before", query.before.as_deref())?,
        limit: number("limit", query.limit.as_deref())?,
    };
    AuditFilter::new(raw).map_err(|error| {
        let field = match error {
            FilterError::UnknownAction(_) => "action",
            FilterError::UnknownOutcome(_) => "outcome",
            FilterError::PeriodReversed => "to",
            FilterError::InvalidCursor => "before",
            FilterError::TooManyValues => "query",
        };
        ApiError::invalid(field, error.to_string())
    })
}

/// `GET /api/v1/audit` : une page du journal, de la plus récente à la plus ancienne.
pub async fn list(
    State(state): State<AppState>,
    Caller(caller): Caller,
    RawQuery(query): RawQuery,
) -> Result<Json<AuditResponse>, ApiError> {
    let filter = filter_of(query)?;
    let page = state.audit.search(caller.account.role, &filter).await?;
    let events = page
        .records
        .iter()
        .map(wire::audit_item)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(AuditResponse {
        events,
        next_before: page.next_before,
    }))
}

/// `GET /api/v1/audit/export` : le résultat filtré en CSV (UTF-8 avec marque d'ordre des octets,
/// séparateur `;`), au plus les entrées les plus récentes jusqu'au plafond de l'export.
pub async fn export(
    State(state): State<AppState>,
    Caller(caller): Caller,
    RawQuery(query): RawQuery,
) -> Result<Response, ApiError> {
    let filter = filter_of(query)?;
    let export = state.audit.export(caller.account.role, &filter).await?;
    let mut response = export.csv.into_response();
    let headers = response.headers_mut();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    headers.insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"journal-hearth.csv\""),
    );
    if export.truncated {
        headers.insert(
            HeaderName::from_static(EXPORT_TRUNCATED_HEADER),
            HeaderValue::from_static("true"),
        );
    }
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::audit::{AuditAction, OutcomeKind};

    fn parse(query: &str) -> Result<AuditFilter, ApiError> {
        filter_of(Some(query.to_owned()))
    }

    #[test]
    fn no_parameter_means_the_whole_journal_one_page() {
        let filter = filter_of(None).unwrap();
        assert!(filter.accounts.is_empty() && filter.search.is_none());
        assert_eq!(filter.limit, 100);
    }

    #[test]
    fn every_parameter_is_read() {
        let filter = parse(
            "account=marie%2Cpaul&action=login,account.create&outcome=denied&from=2026-10-01T00:00:00Z&to=2026-10-04T00:00:00Z&q=10.0.0.7+poste&before=42&limit=20",
        )
        .unwrap();
        assert_eq!(filter.accounts, ["marie", "paul"]);
        assert_eq!(
            filter.actions,
            [AuditAction::Login, AuditAction::AccountCreate]
        );
        assert_eq!(filter.outcomes, [OutcomeKind::Denied]);
        assert!(filter.from.is_some() && filter.to.is_some());
        assert_eq!(
            filter.search.unwrap().expression(),
            "\"10.0.0.7\"* \"poste\"*"
        );
        assert_eq!((filter.before, filter.limit), (Some(42), 20));
    }

    #[test]
    fn empty_parameters_are_the_same_as_absent_ones() {
        let filter = parse("account=&action=&outcome=&from=&to=&q=&before=&limit=").unwrap();
        assert!(filter.accounts.is_empty() && filter.actions.is_empty());
        assert_eq!((filter.from, filter.to, filter.before), (None, None, None));
        assert!(filter.search.is_none());
    }

    #[test]
    fn a_bad_value_is_a_validation_error_naming_the_field() {
        for (query, field) in [
            ("from=hier", "from"),
            ("to=2026-13-45", "to"),
            ("before=abc", "before"),
            ("limit=-1", "limit"),
            ("action=nope", "action"),
            ("outcome=maybe", "outcome"),
            ("before=0", "before"),
            ("from=2026-10-04T00:00:00Z&to=2026-10-03T00:00:00Z", "to"),
        ] {
            let error = parse(query).unwrap_err();
            assert_eq!(
                error.0.error.details["field"], field,
                "{query} -> {:?}",
                error.0
            );
        }
    }

    #[test]
    fn engine_syntax_in_q_is_only_text() {
        let filter = parse("q=%22+OR+NEAR%28a+b%29+-x+col%3Av").unwrap();
        assert!(filter.search.is_some());
    }
}

//! Journal d'activité en SQLite (BR-AUDIT-*). Une entrée n'est jamais modifiée : on écrit, on lit
//! et, pour la conservation, on supprime les plus anciennes.
//!
//! La lecture est **une seule requête vérifiée à la compilation** : les filtres absents valent
//! `NULL` et les listes (comptes, actions, résultats) passent en tableau JSON lu par `json_each`.
//! Aucun SQL n'est assemblé à partir de la saisie ; la recherche plein texte reçoit une expression
//! déjà échappée par le domaine (`SearchQuery`), liée comme paramètre.

use async_trait::async_trait;
use sqlx::SqlitePool;
use time::OffsetDateTime;

use super::convert::{format_date, parse_date, storage};
use super::store::SqliteUnitOfWork;
use crate::application::ports::{AuditRepo, AuditTx, StoreError};
use crate::domain::audit::{AuditEvent, AuditFilter, AuditRecord, OriginKind, OutcomeKind};

const RESOURCE: &str = "audit_events";

pub struct SqliteAuditRepo {
    pool: SqlitePool,
}

impl SqliteAuditRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

struct AuditRow {
    id: i64,
    at: String,
    account: Option<String>,
    origin_kind: String,
    origin_name: Option<String>,
    origin_addr: Option<String>,
    action: String,
    action_label: String,
    target: Option<String>,
    outcome: String,
    reason: Option<String>,
    repeat_count: i64,
}

fn into_record(row: AuditRow) -> Result<AuditRecord, StoreError> {
    let unreadable = |what: &str| StoreError::Unavailable {
        resource: RESOURCE,
        message: format!("{what} illisible pour l'entrée {}", row.id),
    };
    Ok(AuditRecord {
        id: row.id,
        at: parse_date(RESOURCE, &row.at)?,
        account: row.account.clone(),
        origin_kind: OriginKind::from_code(&row.origin_kind)
            .ok_or_else(|| unreadable("origine"))?,
        origin_name: row.origin_name.clone(),
        origin_addr: row.origin_addr.clone(),
        action: row.action.clone(),
        action_label: row.action_label.clone(),
        target: row.target.clone(),
        outcome: OutcomeKind::from_code(&row.outcome).ok_or_else(|| unreadable("résultat"))?,
        reason: row.reason.clone(),
        repeat_count: u32::try_from(row.repeat_count).unwrap_or(u32::MAX),
    })
}

/// Un filtre multi-valeurs en tableau JSON ; `None` (pas de filtre) quand la liste est vide.
fn json_list(values: impl IntoIterator<Item = String>) -> Result<Option<String>, StoreError> {
    let values: Vec<String> = values.into_iter().collect();
    if values.is_empty() {
        return Ok(None);
    }
    serde_json::to_string(&values)
        .map(Some)
        .map_err(|error| StoreError::Unavailable {
            resource: RESOURCE,
            message: format!("filtre illisible : {error}"),
        })
}

#[async_trait]
impl AuditRepo for SqliteAuditRepo {
    async fn search(
        &self,
        filter: &AuditFilter,
        limit: usize,
    ) -> Result<Vec<AuditRecord>, StoreError> {
        let accounts = json_list(filter.accounts.iter().cloned())?;
        let actions = json_list(filter.actions.iter().map(|action| action.code().to_owned()))?;
        let outcomes = json_list(filter.outcomes.iter().map(|kind| kind.code().to_owned()))?;
        let from = filter
            .from
            .map(|date| format_date(RESOURCE, date))
            .transpose()?;
        let to = filter
            .to
            .map(|date| format_date(RESOURCE, date))
            .transpose()?;
        let text = filter.search.as_ref().map(|query| query.expression());
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let rows = sqlx::query_as!(
            AuditRow,
            r#"SELECT e.id AS "id!: i64", e.at, e.account, e.origin_kind, e.origin_name,
                      e.origin_addr, e.action, e.action_label, e.target, e.outcome, e.reason,
                      e.repeat_count AS "repeat_count!: i64"
               FROM audit_events e
               WHERE (?1 IS NULL OR e.account IN (SELECT value FROM json_each(?1)))
                 AND (?2 IS NULL OR e.action IN (SELECT value FROM json_each(?2)))
                 AND (?3 IS NULL OR e.outcome IN (SELECT value FROM json_each(?3)))
                 AND (?4 IS NULL OR e.at >= ?4)
                 AND (?5 IS NULL OR e.at <= ?5)
                 AND (?6 IS NULL OR e.id < ?6)
                 AND (?7 IS NULL OR e.id IN (SELECT rowid FROM audit_fts WHERE audit_fts MATCH ?7))
               ORDER BY e.id DESC
               LIMIT ?8"#,
            accounts,
            actions,
            outcomes,
            from,
            to,
            filter.before,
            text,
            limit
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage(RESOURCE))?;
        rows.into_iter().map(into_record).collect()
    }
}

#[async_trait]
impl AuditTx for SqliteUnitOfWork {
    async fn record(&mut self, event: &AuditEvent) -> Result<AuditRecord, StoreError> {
        let at = format_date(RESOURCE, event.at)?;
        let account = event.actor.account.as_ref().map(ToString::to_string);
        let origin_kind = event.actor.origin.kind().code();
        let origin_name = event.actor.origin.name();
        let origin_addr = event.actor.origin.addr();
        let action = event.action.code();
        let action_label = event.action.label();
        let target = event.target.text();
        let outcome = event.outcome.kind().code();
        let reason = event.clone().into_record(0).reason;
        let repeat_count = i64::from(event.repeat_count);
        let result = sqlx::query!(
            "INSERT INTO audit_events
                 (at, account, origin_kind, origin_name, origin_addr, action, action_label, target, outcome, reason, repeat_count)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            at,
            account,
            origin_kind,
            origin_name,
            origin_addr,
            action,
            action_label,
            target,
            outcome,
            reason,
            repeat_count
        )
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(event.clone().into_record(result.last_insert_rowid()))
    }

    async fn count(&mut self) -> Result<u64, StoreError> {
        let count = sqlx::query_scalar!(r#"SELECT COUNT(*) AS "count!: i64" FROM audit_events"#)
            .fetch_one(&mut *self.tx)
            .await
            .map_err(storage(RESOURCE))?;
        Ok(u64::try_from(count).unwrap_or(0))
    }

    async fn purge_before(&mut self, before: OffsetDateTime) -> Result<u64, StoreError> {
        let before = format_date(RESOURCE, before)?;
        let result = sqlx::query!("DELETE FROM audit_events WHERE at < ?", before)
            .execute(&mut *self.tx)
            .await
            .map_err(storage(RESOURCE))?;
        Ok(result.rows_affected())
    }

    async fn purge_oldest(&mut self, count: u64) -> Result<u64, StoreError> {
        let count = i64::try_from(count).unwrap_or(i64::MAX);
        let result = sqlx::query!(
            "DELETE FROM audit_events
             WHERE id IN (SELECT id FROM audit_events ORDER BY id ASC LIMIT ?)",
            count
        )
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(result.rows_affected())
    }
}

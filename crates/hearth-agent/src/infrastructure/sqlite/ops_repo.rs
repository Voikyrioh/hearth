//! Le journal des opérations de service, en SQLite (`service_ops`, `service_op_steps`, HRT-83,
//! ADR-0045).
//!
//! **L'`intent` est durable.** La base tourne en WAL avec `synchronous = NORMAL` (un commit n'attend pas le
//! disque). Pour une ligne `intent`, la connexion passe en `synchronous = FULL` le temps de la
//! transaction : quand `append_step` rend la main, la ligne est sur le disque, et le geste destructif peut
//! commencer. Les autres lignes (`done`, progression) gardent le régime normal.
//!
//! Aucun secret ici : un plan, des étapes (codes), des causes en liste fermée. `undo_json` ne nomme que
//! des objets créés par l'opération, relus avec les mêmes règles qu'à l'écriture.

use async_trait::async_trait;
use sqlx::{Acquire, SqlitePool};
use time::OffsetDateTime;

use super::convert::{format_date, is_unique_violation, parse_date, storage};
use crate::application::ports::{
    DecisionOffer, JournalError, NewOp, OpJournal, OpRecord, Progress, StoreError,
};
use crate::domain::ops::{
    Cause, Choice, OpKind, OpPlan, OpStatus, StepName, StepRecord, StepState, UndoInfo,
};
use crate::domain::services::key::ServiceKey;

const RESOURCE: &str = "service_ops";

pub struct SqliteOpJournal {
    pool: SqlitePool,
}

impl SqliteOpJournal {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

struct OpRow {
    id: String,
    op_key: String,
    service_key: String,
    service_name: String,
    kind: String,
    account: String,
    plan_json: String,
    status: String,
    step: Option<String>,
    decision_json: Option<String>,
    decision_at: Option<String>,
    progress_json: String,
    cause: String,
    forced: i64,
    started_at: String,
    updated_at: String,
    finished_at: Option<String>,
}

fn unreadable(what: &str, id: &str) -> StoreError {
    StoreError::Unavailable {
        resource: RESOURCE,
        message: format!("{what} illisible pour l'opération {id}"),
    }
}

fn into_record(row: OpRow) -> Result<OpRecord, StoreError> {
    let id = row.id.as_str();
    let decision = match (&row.decision_json, &row.decision_at) {
        (Some(json), Some(at)) => {
            let names: Vec<String> =
                serde_json::from_str(json).map_err(|_| unreadable("decision_json", id))?;
            let choices = names
                .iter()
                .map(|name| Choice::parse(name))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| unreadable("decision_json", id))?;
            Some(DecisionOffer {
                choices,
                since: parse_date(RESOURCE, at)?,
            })
        }
        (None, None) => None,
        _ => return Err(unreadable("decision", id)),
    };
    let cause = if row.cause.is_empty() {
        None
    } else {
        Some(Cause::parse(&row.cause).ok_or_else(|| unreadable("cause", id))?)
    };
    Ok(OpRecord {
        op_key: row.op_key,
        service: ServiceKey::parse(&row.service_key)
            .ok_or_else(|| unreadable("service_key", id))?,
        service_name: row.service_name,
        kind: OpKind::parse(&row.kind).ok_or_else(|| unreadable("kind", id))?,
        account: row.account,
        plan: OpPlan::from_json(&row.plan_json).map_err(|_| unreadable("plan_json", id))?,
        status: OpStatus::parse(&row.status).ok_or_else(|| unreadable("status", id))?,
        step: row
            .step
            .as_deref()
            .map(|step| StepName::parse(step).ok_or_else(|| unreadable("step", id)))
            .transpose()?,
        decision,
        progress: serde_json::from_str(&row.progress_json)
            .map_err(|_| unreadable("progress_json", id))?,
        cause,
        forced: row.forced == 1,
        started_at: parse_date(RESOURCE, &row.started_at)?,
        updated_at: parse_date(RESOURCE, &row.updated_at)?,
        finished_at: row
            .finished_at
            .as_deref()
            .map(|value| parse_date(RESOURCE, value))
            .transpose()?,
        id: row.id,
    })
}

fn decision_columns(
    decision: Option<&DecisionOffer>,
) -> Result<(Option<String>, Option<String>), StoreError> {
    let Some(offer) = decision else {
        return Ok((None, None));
    };
    let names: Vec<&str> = offer.choices.iter().map(|choice| choice.as_str()).collect();
    let json = serde_json::to_string(&names).map_err(|_| unreadable("decision_json", ""))?;
    Ok((Some(json), Some(format_date(RESOURCE, offer.since)?)))
}

/// Ramène au régime normal une connexion passée en `FULL`, quoi qu'il soit arrivé.
async fn restore_normal(conn: &mut sqlx::SqliteConnection) {
    if let Err(error) = sqlx::query("PRAGMA synchronous = NORMAL")
        .execute(conn)
        .await
    {
        tracing::warn!(%error, "retour au régime d'écriture normal de la base impossible");
    }
}

#[async_trait]
impl OpJournal for SqliteOpJournal {
    async fn insert(&self, op: &NewOp) -> Result<(), JournalError> {
        let plan_json = op.plan.to_json();
        let at = format_date(RESOURCE, op.at)?;
        let kind = op.kind.as_str();
        let status = OpStatus::Accepted.as_str();
        let service = op.service.as_str();
        let forced = i64::from(op.forced);
        let inserted = sqlx::query!(
            "INSERT INTO service_ops
                 (id, op_key, service_key, service_name, kind, account, plan_json, status, forced,
                  started_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            op.id,
            op.op_key,
            service,
            op.service_name,
            kind,
            op.account,
            plan_json,
            status,
            forced,
            at,
            at
        )
        .execute(&self.pool)
        .await;
        match inserted {
            Ok(_) => Ok(()),
            Err(error) if is_unique_violation(&error) => Err(JournalError::ServiceBusy),
            Err(error) => Err(storage(RESOURCE)(error).into()),
        }
    }

    async fn get(&self, id: &str) -> Result<Option<OpRecord>, StoreError> {
        sqlx::query_as!(
            OpRow,
            "SELECT id, op_key, service_key, service_name, kind, account, plan_json, status, step,
                    decision_json, decision_at, progress_json, cause, forced, started_at, updated_at,
                    finished_at
             FROM service_ops WHERE id = ?",
            id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(storage(RESOURCE))?
        .map(into_record)
        .transpose()
    }

    async fn unfinished(&self) -> Result<Vec<OpRecord>, StoreError> {
        let rows = sqlx::query_as!(
            OpRow,
            "SELECT id, op_key, service_key, service_name, kind, account, plan_json, status, step,
                    decision_json, decision_at, progress_json, cause, forced, started_at, updated_at,
                    finished_at
             FROM service_ops
             WHERE status IN ('accepted', 'running', 'awaiting_decision', 'cancelling',
                              'rolling_back', 'needs_attention')
             ORDER BY started_at, id"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage(RESOURCE))?;
        // Une ligne illisible est dite, pas fatale : l'agent doit démarrer. Son index unique partiel
        // continue de protéger le service.
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let id = row.id.clone();
                match into_record(row) {
                    Ok(record) => Some(record),
                    Err(error) => {
                        tracing::error!(op = %id, %error, "opération non terminée illisible : ignorée");
                        None
                    }
                }
            })
            .collect())
    }

    async fn steps(&self, id: &str) -> Result<Vec<StepRecord>, StoreError> {
        let rows = sqlx::query!(
            "SELECT seq, step, state, undo_json, at FROM service_op_steps WHERE op_id = ? ORDER BY seq",
            id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage(RESOURCE))?;
        rows.into_iter()
            .map(|row| {
                Ok(StepRecord {
                    seq: u32::try_from(row.seq).map_err(|_| unreadable("seq", id))?,
                    step: StepName::parse(&row.step).ok_or_else(|| unreadable("step", id))?,
                    state: StepState::parse(&row.state).ok_or_else(|| unreadable("state", id))?,
                    undo: UndoInfo::from_json(&row.undo_json)
                        .ok_or_else(|| unreadable("undo_json", id))?,
                    at: parse_date(RESOURCE, &row.at)?,
                })
            })
            .collect()
    }

    async fn append_step(
        &self,
        id: &str,
        step: &StepName,
        state: StepState,
        undo: &UndoInfo,
        at: OffsetDateTime,
        durable: bool,
    ) -> Result<u32, JournalError> {
        let at = format_date(RESOURCE, at)?;
        let undo_json = undo.to_json();
        let step_name = step.as_str();
        let state_name = state.as_str();
        let mut conn = self.pool.acquire().await.map_err(storage(RESOURCE))?;
        if durable {
            sqlx::query("PRAGMA synchronous = FULL")
                .execute(&mut *conn)
                .await
                .map_err(storage(RESOURCE))?;
        }
        let written = async {
            let mut tx = conn.begin().await?;
            let last: Option<i64> =
                sqlx::query_scalar!("SELECT MAX(seq) FROM service_op_steps WHERE op_id = ?", id)
                    .fetch_one(&mut *tx)
                    .await?;
            let seq = last.unwrap_or(0) + 1;
            sqlx::query!(
                "INSERT INTO service_op_steps (op_id, seq, step, state, undo_json, at)
                 VALUES (?, ?, ?, ?, ?, ?)",
                id,
                seq,
                step_name,
                state_name,
                undo_json,
                at
            )
            .execute(&mut *tx)
            .await?;
            sqlx::query!(
                "UPDATE service_ops SET step = ?, updated_at = ? WHERE id = ?",
                step_name,
                at,
                id
            )
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            Ok::<i64, sqlx::Error>(seq)
        }
        .await;
        if durable {
            restore_normal(&mut conn).await;
        }
        match written {
            Ok(seq) => u32::try_from(seq).map_err(|_| unreadable("seq", id).into()),
            // Clé étrangère : l'opération n'existe pas.
            Err(error)
                if error
                    .as_database_error()
                    .is_some_and(|database| database.is_foreign_key_violation()) =>
            {
                Err(JournalError::Unknown)
            }
            Err(error) => Err(storage(RESOURCE)(error).into()),
        }
    }

    async fn transition(
        &self,
        id: &str,
        from: OpStatus,
        to: OpStatus,
        cause: Option<Cause>,
        decision: Option<&DecisionOffer>,
        at: OffsetDateTime,
    ) -> Result<(), JournalError> {
        let at = format_date(RESOURCE, at)?;
        let (decision_json, decision_at) = decision_columns(decision)?;
        let finished_at = to.is_terminal().then(|| at.clone());
        let cause = cause.map_or("", Cause::as_str);
        let from = from.as_str();
        let to = to.as_str();
        let done = sqlx::query!(
            "UPDATE service_ops
             SET status = ?, cause = ?, decision_json = ?, decision_at = ?, updated_at = ?,
                 finished_at = ?
             WHERE id = ? AND status = ?",
            to,
            cause,
            decision_json,
            decision_at,
            at,
            finished_at,
            id,
            from
        )
        .execute(&self.pool)
        .await
        .map_err(storage(RESOURCE))?;
        if done.rows_affected() == 1 {
            return Ok(());
        }
        // Rien n'a bougé : l'opération n'existe pas, ou elle n'était plus dans le statut attendu.
        let exists = sqlx::query_scalar!("SELECT id FROM service_ops WHERE id = ?", id)
            .fetch_optional(&self.pool)
            .await
            .map_err(storage(RESOURCE))?;
        Err(if exists.is_some() {
            JournalError::Stale
        } else {
            JournalError::Unknown
        })
    }

    async fn set_progress(
        &self,
        id: &str,
        progress: &Progress,
        at: OffsetDateTime,
    ) -> Result<(), JournalError> {
        let at = format_date(RESOURCE, at)?;
        let json = serde_json::to_string(progress).map_err(|_| unreadable("progress", id))?;
        let done = sqlx::query!(
            "UPDATE service_ops SET progress_json = ?, updated_at = ? WHERE id = ?",
            json,
            at,
            id
        )
        .execute(&self.pool)
        .await
        .map_err(storage(RESOURCE))?;
        if done.rows_affected() == 1 {
            Ok(())
        } else {
            Err(JournalError::Unknown)
        }
    }

    async fn expired_decisions(
        &self,
        older_than: OffsetDateTime,
    ) -> Result<Vec<OpRecord>, StoreError> {
        let limit = format_date(RESOURCE, older_than)?;
        sqlx::query_as!(
            OpRow,
            "SELECT id, op_key, service_key, service_name, kind, account, plan_json, status, step,
                    decision_json, decision_at, progress_json, cause, forced, started_at, updated_at,
                    finished_at
             FROM service_ops
             WHERE status = 'awaiting_decision' AND decision_at IS NOT NULL AND decision_at <= ?
             ORDER BY decision_at",
            limit
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage(RESOURCE))?
        .into_iter()
        .map(into_record)
        .collect()
    }

    async fn purge_finished(&self, before: OffsetDateTime) -> Result<u64, StoreError> {
        let limit = format_date(RESOURCE, before)?;
        let mut tx = self.pool.begin().await.map_err(storage(RESOURCE))?;
        sqlx::query!(
            "DELETE FROM service_op_steps WHERE op_id IN
                 (SELECT id FROM service_ops WHERE finished_at IS NOT NULL AND finished_at < ?)",
            limit
        )
        .execute(&mut *tx)
        .await
        .map_err(storage(RESOURCE))?;
        let done = sqlx::query!(
            "DELETE FROM service_ops WHERE finished_at IS NOT NULL AND finished_at < ?",
            limit
        )
        .execute(&mut *tx)
        .await
        .map_err(storage(RESOURCE))?;
        tx.commit().await.map_err(storage(RESOURCE))?;
        Ok(done.rows_affected())
    }
}

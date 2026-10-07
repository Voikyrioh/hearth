//! Mode attaque en SQLite : la ligne unique `attack_mode` et les essais uniques `attack_trials`
//! (HRT-25, ADR-0025, BR-TRUST-011 à 021).

use async_trait::async_trait;
use sqlx::{SqliteConnection, SqlitePool};
use time::OffsetDateTime;

use super::convert::{format_date, parse_date, storage};
use super::store::SqliteUnitOfWork;
use crate::application::ports::{AttackModeRepo, AttackModeTx, StoreError};
use crate::domain::accounts::AccountId;
use crate::domain::trust::TrialKind;
use crate::domain::trust::attack_mode::{EndHow, Stored};

const RESOURCE: &str = "attack_mode";

pub struct SqliteAttackModeRepo {
    pool: SqlitePool,
}

impl SqliteAttackModeRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

struct Row {
    active: i64,
    activation_id: Option<String>,
    activated_at: Option<String>,
    activated_by: Option<String>,
    ended_at: Option<String>,
    ended_how: Option<String>,
    ended_boot_id: Option<String>,
    ended_uptime_s: Option<i64>,
    last_boot_id: Option<String>,
    window_boot_id: Option<String>,
    remote_reboot_boot_id: Option<String>,
}

fn into_stored(row: Row) -> Result<Stored, StoreError> {
    Ok(Stored {
        active: row.active == 1,
        activation_id: row.activation_id,
        activated_at: row
            .activated_at
            .as_deref()
            .map(|date| parse_date(RESOURCE, date))
            .transpose()?,
        activated_by: row.activated_by,
        ended_at: row
            .ended_at
            .as_deref()
            .map(|date| parse_date(RESOURCE, date))
            .transpose()?,
        // Une manière de finir inconnue (écrite par un agent plus récent) n'est pas une erreur.
        ended_how: row.ended_how.as_deref().and_then(EndHow::from_code),
        ended_boot_id: row.ended_boot_id,
        ended_uptime_s: row.ended_uptime_s.and_then(|s| u64::try_from(s).ok()),
        last_boot_id: row.last_boot_id,
        window_boot_id: row.window_boot_id,
        remote_reboot_boot_id: row.remote_reboot_boot_id,
    })
}

async fn load(conn: &mut SqliteConnection) -> Result<Stored, StoreError> {
    let row = sqlx::query_as!(
        Row,
        r#"SELECT active AS "active!: i64", activation_id, activated_at, activated_by, ended_at,
                  ended_how, ended_boot_id, ended_uptime_s, last_boot_id, window_boot_id,
                  remote_reboot_boot_id
           FROM attack_mode WHERE id = 1"#
    )
    .fetch_optional(&mut *conn)
    .await
    .map_err(storage(RESOURCE))?;
    // La ligne unique est créée par la migration ; si elle manquait, le mode est éteint.
    row.map(into_stored)
        .transpose()
        .map(Option::unwrap_or_default)
}

#[async_trait]
impl AttackModeRepo for SqliteAttackModeRepo {
    async fn load(&self) -> Result<Stored, StoreError> {
        let mut conn = self.pool.acquire().await.map_err(storage(RESOURCE))?;
        load(&mut conn).await
    }
}

fn kind_code(kind: TrialKind) -> &'static str {
    match kind {
        TrialKind::Address => "address",
        TrialKind::Key => "key",
    }
}

#[async_trait]
impl AttackModeTx for SqliteUnitOfWork {
    async fn load(&mut self) -> Result<Stored, StoreError> {
        load(&mut self.tx).await
    }

    async fn activate_fresh(
        &mut self,
        activation_id: &str,
        at: OffsetDateTime,
        by: &str,
    ) -> Result<(), StoreError> {
        let at = format_date(RESOURCE, at)?;
        sqlx::query!(
            "UPDATE attack_mode
             SET active = 1, activation_id = ?, activated_at = ?, activated_by = ?,
                 ended_at = NULL, ended_how = NULL, ended_boot_id = NULL, ended_uptime_s = NULL,
                 window_boot_id = NULL
             WHERE id = 1",
            activation_id,
            at,
            by
        )
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        // Les essais des activations précédentes ne servent plus à rien : rendus à la nouvelle.
        sqlx::query!(
            "DELETE FROM attack_trials WHERE activation_id <> ?",
            activation_id
        )
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(())
    }

    async fn reactivate(&mut self) -> Result<(), StoreError> {
        sqlx::query!(
            "UPDATE attack_mode
             SET active = 1, ended_at = NULL, ended_how = NULL, ended_boot_id = NULL,
                 ended_uptime_s = NULL, window_boot_id = NULL
             WHERE id = 1"
        )
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(())
    }

    async fn deactivate(
        &mut self,
        at: OffsetDateTime,
        how: EndHow,
        boot_id: Option<&str>,
        uptime_s: Option<u64>,
    ) -> Result<(), StoreError> {
        let at = format_date(RESOURCE, at)?;
        let how = how.code();
        let uptime_s = uptime_s.and_then(|s| i64::try_from(s).ok());
        sqlx::query!(
            "UPDATE attack_mode
             SET active = 0, ended_at = ?, ended_how = ?, ended_boot_id = ?, ended_uptime_s = ?,
                 window_boot_id = NULL
             WHERE id = 1",
            at,
            how,
            boot_id,
            uptime_s
        )
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(())
    }

    async fn save_boot(
        &mut self,
        last_boot_id: Option<&str>,
        window_boot_id: Option<&str>,
        remote_reboot_boot_id: Option<&str>,
    ) -> Result<(), StoreError> {
        sqlx::query!(
            "UPDATE attack_mode
             SET last_boot_id = ?, window_boot_id = ?, remote_reboot_boot_id = ?
             WHERE id = 1",
            last_boot_id,
            window_boot_id,
            remote_reboot_boot_id
        )
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(())
    }

    async fn clear_window(&mut self) -> Result<(), StoreError> {
        sqlx::query!("UPDATE attack_mode SET window_boot_id = NULL WHERE id = 1")
            .execute(&mut *self.tx)
            .await
            .map_err(storage(RESOURCE))?;
        Ok(())
    }

    async fn trial_used(
        &mut self,
        activation_id: &str,
        account: &AccountId,
        kind: TrialKind,
        subject: &str,
    ) -> Result<bool, StoreError> {
        let kind = kind_code(kind);
        let found = sqlx::query_scalar!(
            r#"SELECT COUNT(*) AS "count!: i64" FROM attack_trials
               WHERE activation_id = ? AND account_id = ? AND kind = ? AND subject = ?"#,
            activation_id,
            account.as_str(),
            kind,
            subject
        )
        .fetch_one(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(found > 0)
    }

    async fn record_trial(
        &mut self,
        activation_id: &str,
        account: &AccountId,
        kind: TrialKind,
        subject: &str,
        at: OffsetDateTime,
        succeeded: bool,
    ) -> Result<(), StoreError> {
        let kind = kind_code(kind);
        let at = format_date(RESOURCE, at)?;
        let outcome = if succeeded { "succeeded" } else { "failed" };
        sqlx::query!(
            "INSERT INTO attack_trials (activation_id, account_id, kind, subject, used_at, outcome)
             VALUES (?, ?, ?, ?, ?, ?)",
            activation_id,
            account.as_str(),
            kind,
            subject,
            at,
            outcome
        )
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(())
    }

    async fn purge_stale_trials(&mut self) -> Result<u64, StoreError> {
        let result = sqlx::query!(
            "DELETE FROM attack_trials
             WHERE activation_id IS NOT (SELECT activation_id FROM attack_mode WHERE id = 1)"
        )
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(result.rows_affected())
    }
}

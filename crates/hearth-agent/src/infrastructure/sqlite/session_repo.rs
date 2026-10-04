//! Sessions en SQLite : lectures sur le pool (`SessionRepo`) et écritures de l'unité de travail
//! (`SessionTx`, implémentée sur `SqliteUnitOfWork`) : fermer des sessions et changer un compte
//! sont une seule transaction.

use async_trait::async_trait;
use sqlx::SqlitePool;
use time::OffsetDateTime;

use super::convert::{format_date, is_unique_violation, parse_date, storage};
use super::store::SqliteUnitOfWork;
use crate::application::ports::{SessionRepo, SessionTx, StoreError};
use crate::domain::accounts::AccountId;
use crate::domain::session_token::TokenHash;
use crate::domain::sessions::{Session, SessionClosure, SessionId};

const RESOURCE: &str = "sessions";

pub struct SqliteSessionRepo {
    pool: SqlitePool,
}

impl SqliteSessionRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

struct SessionRow {
    id: String,
    account_id: String,
    token_hash: String,
    client_name: String,
    client_addr: String,
    created_at: String,
    last_seen_at: String,
    expires_at: String,
}

fn into_session(row: SessionRow) -> Result<Session, StoreError> {
    let token_hash = TokenHash::from_hex(&row.token_hash).map_err(|_| StoreError::Unavailable {
        resource: RESOURCE,
        message: format!("empreinte de jeton illisible pour la session {}", row.id),
    })?;
    Ok(Session {
        created_at: parse_date(RESOURCE, &row.created_at)?,
        last_seen_at: parse_date(RESOURCE, &row.last_seen_at)?,
        expires_at: parse_date(RESOURCE, &row.expires_at)?,
        id: SessionId::new(row.id),
        account: AccountId::new(row.account_id),
        token_hash,
        client_name: row.client_name,
        client_addr: row.client_addr,
    })
}

#[async_trait]
impl SessionRepo for SqliteSessionRepo {
    async fn expiries_of(&self, account: &AccountId) -> Result<Vec<OffsetDateTime>, StoreError> {
        let mut conn = self.pool.acquire().await.map_err(storage(RESOURCE))?;
        let rows = sqlx::query_scalar!(
            "SELECT expires_at FROM sessions WHERE account_id = ?",
            account.as_str()
        )
        .fetch_all(&mut *conn)
        .await
        .map_err(storage(RESOURCE))?;
        rows.iter()
            .map(|value| parse_date(RESOURCE, value))
            .collect()
    }

    async fn find_by_token_hash(&self, hash: &TokenHash) -> Result<Option<Session>, StoreError> {
        let mut conn = self.pool.acquire().await.map_err(storage(RESOURCE))?;
        let hash = hash.to_hex();
        sqlx::query_as!(
            SessionRow,
            "SELECT id, account_id, token_hash, client_name, client_addr, created_at, last_seen_at, expires_at
             FROM sessions WHERE token_hash = ?",
            hash
        )
        .fetch_optional(&mut *conn)
        .await
        .map_err(storage(RESOURCE))?
        .map(into_session)
        .transpose()
    }

    async fn is_revoked(&self, hash: &TokenHash) -> Result<bool, StoreError> {
        let mut conn = self.pool.acquire().await.map_err(storage(RESOURCE))?;
        let hash = hash.to_hex();
        let found = sqlx::query_scalar!(
            "SELECT token_hash FROM revoked_sessions WHERE token_hash = ?",
            hash
        )
        .fetch_optional(&mut *conn)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(found.is_some())
    }
}

#[async_trait]
impl SessionTx for SqliteUnitOfWork {
    async fn insert(&mut self, session: &Session) -> Result<(), StoreError> {
        let token_hash = session.token_hash.to_hex();
        let created_at = format_date(RESOURCE, session.created_at)?;
        let last_seen_at = format_date(RESOURCE, session.last_seen_at)?;
        let expires_at = format_date(RESOURCE, session.expires_at)?;
        sqlx::query!(
            "INSERT INTO sessions (id, account_id, token_hash, client_name, client_addr, created_at, last_seen_at, expires_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            session.id.as_str(),
            session.account.as_str(),
            token_hash,
            session.client_name,
            session.client_addr,
            created_at,
            last_seen_at,
            expires_at
        )
        .execute(&mut *self.tx)
        .await
        .map_err(|error| {
            if is_unique_violation(&error) {
                StoreError::Duplicate { resource: RESOURCE }
            } else {
                storage(RESOURCE)(error)
            }
        })?;
        Ok(())
    }

    async fn renew(
        &mut self,
        id: &SessionId,
        last_seen_at: OffsetDateTime,
        expires_at: OffsetDateTime,
    ) -> Result<(), StoreError> {
        let last_seen_at = format_date(RESOURCE, last_seen_at)?;
        let expires_at = format_date(RESOURCE, expires_at)?;
        sqlx::query!(
            "UPDATE sessions SET last_seen_at = ?, expires_at = ? WHERE id = ?",
            last_seen_at,
            expires_at,
            id.as_str()
        )
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(())
    }

    async fn delete(&mut self, id: &SessionId) -> Result<(), StoreError> {
        sqlx::query!("DELETE FROM sessions WHERE id = ?", id.as_str())
            .execute(&mut *self.tx)
            .await
            .map_err(storage(RESOURCE))?;
        Ok(())
    }

    async fn close(
        &mut self,
        account: &AccountId,
        closure: &SessionClosure,
        at: OffsetDateTime,
    ) -> Result<u64, StoreError> {
        let at = format_date(RESOURCE, at)?;
        let result = match closure {
            SessionClosure::All => {
                sqlx::query!(
                    "INSERT OR IGNORE INTO revoked_sessions (token_hash, revoked_at)
                     SELECT token_hash, ? FROM sessions WHERE account_id = ?",
                    at,
                    account.as_str()
                )
                .execute(&mut *self.tx)
                .await
                .map_err(storage(RESOURCE))?;
                sqlx::query!(
                    "DELETE FROM sessions WHERE account_id = ?",
                    account.as_str()
                )
                .execute(&mut *self.tx)
                .await
            }
            SessionClosure::AllExcept(kept) => {
                sqlx::query!(
                    "INSERT OR IGNORE INTO revoked_sessions (token_hash, revoked_at)
                     SELECT token_hash, ? FROM sessions WHERE account_id = ? AND id <> ?",
                    at,
                    account.as_str(),
                    kept.as_str()
                )
                .execute(&mut *self.tx)
                .await
                .map_err(storage(RESOURCE))?;
                sqlx::query!(
                    "DELETE FROM sessions WHERE account_id = ? AND id <> ?",
                    account.as_str(),
                    kept.as_str()
                )
                .execute(&mut *self.tx)
                .await
            }
        };
        Ok(result.map_err(storage(RESOURCE))?.rows_affected())
    }

    async fn purge_expired(&mut self, now: OffsetDateTime) -> Result<u64, StoreError> {
        let now = format_date(RESOURCE, now)?;
        let result = sqlx::query!("DELETE FROM sessions WHERE expires_at <= ?", now)
            .execute(&mut *self.tx)
            .await
            .map_err(storage(RESOURCE))?;
        Ok(result.rows_affected())
    }

    async fn purge_revocations(&mut self, before: OffsetDateTime) -> Result<u64, StoreError> {
        let before = format_date(RESOURCE, before)?;
        let result = sqlx::query!("DELETE FROM revoked_sessions WHERE revoked_at < ?", before)
            .execute(&mut *self.tx)
            .await
            .map_err(storage(RESOURCE))?;
        Ok(result.rows_affected())
    }
}

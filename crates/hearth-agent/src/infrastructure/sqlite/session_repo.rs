//! Sessions en SQLite.

use async_trait::async_trait;
use sqlx::{SqliteConnection, SqlitePool};
use time::OffsetDateTime;

use super::convert::{parse_date, storage};
use crate::application::ports::{SessionRepo, StoreError};
use crate::domain::accounts::AccountId;
use crate::domain::sessions::SessionClosure;

const RESOURCE: &str = "sessions";

pub struct SqliteSessionRepo {
    pool: SqlitePool,
}

impl SqliteSessionRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
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
}

/// Supprime les sessions du compte désignées par `closure`. Partagé avec la transaction des
/// comptes pour que la fermeture et le changement de compte soient atomiques.
pub(super) async fn close_sessions(
    conn: &mut SqliteConnection,
    account: &AccountId,
    closure: &SessionClosure,
) -> Result<u64, StoreError> {
    let result = match closure {
        SessionClosure::All => {
            sqlx::query!(
                "DELETE FROM sessions WHERE account_id = ?",
                account.as_str()
            )
            .execute(&mut *conn)
            .await
        }
        SessionClosure::AllExcept(kept) => {
            sqlx::query!(
                "DELETE FROM sessions WHERE account_id = ? AND id <> ?",
                account.as_str(),
                kept.as_str()
            )
            .execute(&mut *conn)
            .await
        }
    };
    Ok(result.map_err(storage(RESOURCE))?.rows_affected())
}

//! Magasin SQLite : unités de travail (écritures) dans une seule transaction `BEGIN IMMEDIATE`
//! (un seul écrivain à la fois : ce que l'unité a observé ne change plus avant sa validation,
//! ce qui rend atomique la garde du dernier administrateur ou le compteur de connexions).
//!
//! `SqliteUnitOfWork` implémente les ports d'écriture de chaque sujet ; chacun vit dans le
//! fichier de son sujet (`account_repo.rs`, `session_repo.rs`, `login_attempt_repo.rs`,
//! `operation_repo.rs`, `audit_repo.rs`). Ce fichier ne fait qu'ouvrir et valider la transaction.

use async_trait::async_trait;
use sqlx::{Sqlite, SqlitePool, Transaction};

use super::convert::storage;
use crate::application::ports::{
    AccountTx, AuditTx, LoginAttemptTx, OperationTx, SessionTx, Store, StoreError, UnitOfWork,
};

/// Ressource nommée par les erreurs d'ouverture et de validation d'une unité de travail : le
/// magasin lui-même, pas un sujet (comptes, sessions…).
const STORE: &str = "store";

pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl Store for SqliteStore {
    async fn begin(&self) -> Result<Box<dyn UnitOfWork>, StoreError> {
        let tx = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(storage(STORE))?;
        Ok(Box::new(SqliteUnitOfWork { tx }))
    }
}

/// Unité de travail SQLite. Abandonnée sans `commit`, SQLx l'annule.
pub struct SqliteUnitOfWork {
    pub(super) tx: Transaction<'static, Sqlite>,
}

#[async_trait]
impl UnitOfWork for SqliteUnitOfWork {
    fn accounts(&mut self) -> &mut dyn AccountTx {
        self
    }

    fn sessions(&mut self) -> &mut dyn SessionTx {
        self
    }

    fn login_attempts(&mut self) -> &mut dyn LoginAttemptTx {
        self
    }

    fn operations(&mut self) -> &mut dyn OperationTx {
        self
    }

    fn audit(&mut self) -> &mut dyn AuditTx {
        self
    }

    async fn commit(self: Box<Self>) -> Result<(), StoreError> {
        self.tx.commit().await.map_err(storage(STORE))
    }
}

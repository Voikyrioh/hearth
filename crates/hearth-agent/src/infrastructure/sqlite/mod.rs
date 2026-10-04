//! Base SQLite de l'agent (`hearth.db` dans le dossier de données) : pool, migrations embarquées,
//! dépôts SQLx. Aucune règle métier : le domaine décide, ici on observe et on exécute.
//!
//! Les requêtes sont vérifiées à la compilation (`query!`, mode hors ligne : dossier `.sqlx`
//! versionné). Après toute modification d'une requête ou d'une migration, voir « SQLx » dans
//! `CLAUDE.md` pour régénérer.

mod account_repo;
mod convert;
mod session_repo;

use std::path::{Path, PathBuf};
use std::time::Duration;

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use thiserror::Error;

pub use account_repo::SqliteAccountRepo;
pub use session_repo::SqliteSessionRepo;

pub const DATABASE_FILE: &str = "hearth.db";

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("dossier de données {path} inaccessible : {source}")]
    DataDir {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("ouverture de la base {path} impossible : {message}")]
    Open { path: PathBuf, message: String },
    #[error("migration de la base {path} impossible : {message}")]
    Migrate { path: PathBuf, message: String },
}

/// Base ouverte et à jour (migrations appliquées).
#[derive(Debug, Clone)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    /// Ouvre `hearth.db` dans `data_dir` (créé au besoin) : mode WAL, clés étrangères actives,
    /// puis applique les migrations embarquées. Plusieurs processus peuvent ouvrir la même base :
    /// un écrivain attend l'autre (délai de 5 s).
    pub async fn open(data_dir: &Path) -> Result<Self, DatabaseError> {
        std::fs::create_dir_all(data_dir).map_err(|source| DatabaseError::DataDir {
            path: data_dir.to_owned(),
            source,
        })?;
        let path = data_dir.join(DATABASE_FILE);
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await
            .map_err(|error| DatabaseError::Open {
                path: path.clone(),
                message: error.to_string(),
            })?;
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(|error| DatabaseError::Migrate {
                path,
                message: error.to_string(),
            })?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn opening_creates_the_file_in_wal_mode_with_foreign_keys() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("nested").join("data"))
            .await
            .unwrap();
        assert!(
            dir.path()
                .join("nested")
                .join("data")
                .join(DATABASE_FILE)
                .exists()
        );

        let mode: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(mode, "wal");
        let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(foreign_keys, 1);
    }

    #[tokio::test]
    async fn migrations_create_the_three_tables_and_are_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        Database::open(dir.path()).await.unwrap();
        let db = Database::open(dir.path()).await.unwrap();
        let tables: Vec<String> = sqlx::query_scalar(
            "SELECT name FROM sqlite_master WHERE type = 'table'              AND name IN ('accounts', 'sessions', 'meta') ORDER BY name",
        )
        .fetch_all(db.pool())
        .await
        .unwrap();
        assert_eq!(tables, ["accounts", "meta", "sessions"]);
    }
}

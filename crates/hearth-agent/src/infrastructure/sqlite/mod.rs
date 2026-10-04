//! Base SQLite de l'agent (`hearth.db` dans le dossier de données) : pool, migrations embarquées,
//! dépôts SQLx. Aucune règle métier : le domaine décide, ici on observe et on exécute.
//!
//! Les requêtes sont vérifiées à la compilation (`query!`, mode hors ligne : dossier `.sqlx`
//! versionné). Après toute modification d'une requête ou d'une migration, voir « SQLx » dans
//! `CLAUDE.md` pour régénérer.

mod account_repo;
mod convert;
mod session_repo;
mod store;

use std::path::{Path, PathBuf};
use std::time::Duration;

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use thiserror::Error;

use super::data_dir;

pub use account_repo::SqliteAccountRepo;
pub use session_repo::SqliteSessionRepo;
pub use store::SqliteStore;

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
        data_dir::ensure(data_dir).map_err(|source| DatabaseError::DataDir {
            path: data_dir.to_owned(),
            source,
        })?;
        let path = data_dir.join(DATABASE_FILE);
        // Le fichier de base (et donc ses compagnons -wal et -shm) n'est lisible que par nous.
        data_dir::ensure_private_file(&path).map_err(|source| DatabaseError::DataDir {
            path: path.clone(),
            source,
        })?;
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

    #[cfg(unix)]
    fn mode(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn the_directory_created_by_the_database_alone_is_private() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("data");
        Database::open(&dir).await.unwrap();
        assert_eq!(mode(&dir), 0o700);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn the_database_and_its_companions_are_private() {
        let root = tempfile::tempdir().unwrap();
        let db = Database::open(root.path()).await.unwrap();
        sqlx::query("CREATE TABLE IF NOT EXISTS probe (x INTEGER)")
            .execute(db.pool())
            .await
            .unwrap();
        for name in [DATABASE_FILE, "hearth.db-wal", "hearth.db-shm"] {
            let path = root.path().join(name);
            assert!(path.exists(), "{name}");
            assert_eq!(mode(&path), 0o600, "{name}");
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn an_existing_wide_directory_and_database_are_tightened() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        Database::open(root.path())
            .await
            .unwrap()
            .pool()
            .close()
            .await;
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        let file = root.path().join(DATABASE_FILE);
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        Database::open(root.path()).await.unwrap();
        assert_eq!(mode(root.path()), 0o700);
        assert_eq!(mode(&file), 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn the_identity_store_after_the_database_keeps_the_directory_private() {
        use crate::application::ports::IdentityStore;
        use crate::infrastructure::tls::FileIdentityStore;
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("data");
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(Database::open(&dir)).unwrap();
        FileIdentityStore::new(&dir).load_or_create().unwrap();
        assert_eq!(mode(&dir), 0o700);
    }

    #[cfg(unix)]
    #[test]
    fn the_database_after_the_identity_store_keeps_the_directory_private() {
        use crate::application::ports::IdentityStore;
        use crate::infrastructure::tls::FileIdentityStore;
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("data");
        FileIdentityStore::new(&dir).load_or_create().unwrap();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(Database::open(&dir)).unwrap();
        assert_eq!(mode(&dir), 0o700);
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

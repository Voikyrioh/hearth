//! Base SQLite de l'agent (`hearth.db` dans le dossier de données) : pool, migrations embarquées,
//! dépôts SQLx. Aucune règle métier : le domaine décide, ici on observe et on exécute.
//!
//! Les requêtes sont vérifiées à la compilation (`query!`, mode hors ligne : dossier `.sqlx`
//! versionné). Après toute modification d'une requête ou d'une migration, voir « SQLx » dans
//! `CLAUDE.md` pour régénérer.

mod account_repo;
mod audit_repo;
mod convert;
mod device_repo;
mod known_address_repo;
mod login_attempt_repo;
mod operation_repo;
mod session_repo;
mod store;

use std::path::{Path, PathBuf};
use std::time::Duration;

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use thiserror::Error;

use super::data_dir;

pub use account_repo::SqliteAccountRepo;
pub use audit_repo::SqliteAuditRepo;
pub use device_repo::SqliteDeviceRepo;
pub use known_address_repo::SqliteKnownAddressRepo;
pub use login_attempt_repo::SqliteLoginAttemptRepo;
pub use operation_repo::SqliteOperationRepo;
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
    #[error("fichier de base {path} inaccessible : {source}")]
    File {
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
        data_dir::ensure_private_file(&path).map_err(|source| DatabaseError::File {
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

/// Nombre d'administrateurs, lu en **lecture seule** et sans migration : l'installation observe
/// la base d'un service qui tourne sans la modifier. Base absente, ou pas encore migrée : 0.
/// Requête non vérifiée à la compilation (`query_scalar` dynamique) : elle ne peut pas l'être
/// sans créer ni migrer la base, et elle ne lit qu'un compte.
pub async fn count_admins_read_only(data_dir: &Path) -> Result<u64, DatabaseError> {
    let path = data_dir.join(DATABASE_FILE);
    if !path.is_file() {
        return Ok(0);
    }
    let options = SqliteConnectOptions::new()
        .filename(&path)
        .read_only(true)
        .create_if_missing(false)
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|error| DatabaseError::Open {
            path: path.clone(),
            message: error.to_string(),
        })?;
    let counted =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM accounts WHERE role = 'admin'")
            .fetch_one(&pool)
            .await;
    pool.close().await;
    // FIX:01M45V0PZB5TRE3A7KQHAHJNXD : seule la table absente (base jamais migrée) vaut « aucun
    // compte » ; toute autre erreur de lecture (base verrouillée, corrompue, droits) est rendue,
    // sinon l'installation croirait la machine sans administrateur et en créerait un autre.
    match counted {
        Ok(count) => Ok(u64::try_from(count).unwrap_or(0)),
        Err(error) if is_missing_table(&error) => Ok(0),
        Err(error) => Err(DatabaseError::Open {
            path,
            message: format!("lecture des comptes impossible : {error}"),
        }),
    }
}

/// L'erreur de SQLite « no such table » : la table n'existe pas.
fn is_missing_table(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Database(db) if db.message().starts_with("no such table"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_database_name_is_the_one_the_purge_knows() {
        assert_eq!(DATABASE_FILE, crate::domain::install::DATABASE_FILE);
    }

    // FIX:01M45V0PZB5TRE3A7KQHAHJNXD
    #[tokio::test]
    async fn counting_admins_tells_a_missing_table_from_an_unreadable_database() {
        let dir = crate::infrastructure::data_dir::private_tempdir();
        // Base jamais migrée (table absente) : aucun compte.
        let options = SqliteConnectOptions::new()
            .filename(dir.path().join(DATABASE_FILE))
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE unrelated (x INTEGER)")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
        assert_eq!(count_admins_read_only(dir.path()).await.unwrap(), 0);

        // Fichier qui n'est pas une base : une erreur, jamais « 0 administrateur ».
        let broken = crate::infrastructure::data_dir::private_tempdir();
        std::fs::write(broken.path().join(DATABASE_FILE), vec![7u8; 4096]).unwrap();
        assert!(count_admins_read_only(broken.path()).await.is_err());
    }

    #[tokio::test]
    async fn opening_creates_the_file_in_wal_mode_with_foreign_keys() {
        let dir = crate::infrastructure::data_dir::private_tempdir();
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
        let root = crate::infrastructure::data_dir::private_tempdir();
        let dir = root.path().join("data");
        Database::open(&dir).await.unwrap();
        assert_eq!(mode(&dir), 0o700);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn the_database_and_its_companions_are_private() {
        let root = crate::infrastructure::data_dir::private_tempdir();
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
    async fn an_existing_wide_database_file_is_tightened() {
        use std::os::unix::fs::PermissionsExt;
        let root = crate::infrastructure::data_dir::private_tempdir();
        Database::open(root.path())
            .await
            .unwrap()
            .pool()
            .close()
            .await;
        let file = root.path().join(DATABASE_FILE);
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        Database::open(root.path()).await.unwrap();
        assert_eq!(mode(&file), 0o600);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn an_existing_wide_directory_with_content_is_refused_not_tightened() {
        use std::os::unix::fs::PermissionsExt;
        let root = crate::infrastructure::data_dir::private_tempdir();
        Database::open(root.path())
            .await
            .unwrap()
            .pool()
            .close()
            .await;
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        let error = Database::open(root.path()).await.unwrap_err();
        assert!(matches!(error, DatabaseError::DataDir { .. }), "{error}");
        assert!(error.to_string().contains("chmod 700"), "{error}");
        assert_eq!(mode(root.path()), 0o755);
    }

    #[cfg(unix)]
    #[test]
    fn the_identity_store_after_the_database_keeps_the_directory_private() {
        use crate::application::ports::IdentityStore;
        use crate::infrastructure::tls::FileIdentityStore;
        let root = crate::infrastructure::data_dir::private_tempdir();
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
        let root = crate::infrastructure::data_dir::private_tempdir();
        let dir = root.path().join("data");
        FileIdentityStore::new(&dir).load_or_create().unwrap();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(Database::open(&dir)).unwrap();
        assert_eq!(mode(&dir), 0o700);
    }

    #[tokio::test]
    async fn a_problem_with_the_database_file_is_not_reported_as_the_data_directory() {
        let dir = crate::infrastructure::data_dir::private_tempdir();
        std::fs::create_dir(dir.path().join(DATABASE_FILE)).unwrap();
        let error = Database::open(dir.path()).await.unwrap_err();
        assert!(matches!(error, DatabaseError::File { .. }), "{error}");
        assert!(error.to_string().contains(DATABASE_FILE), "{error}");
    }

    #[tokio::test]
    async fn migrations_create_every_table_and_are_idempotent() {
        let dir = crate::infrastructure::data_dir::private_tempdir();
        Database::open(dir.path()).await.unwrap();
        let db = Database::open(dir.path()).await.unwrap();
        let tables: Vec<String> = sqlx::query_scalar(
            "SELECT name FROM sqlite_master WHERE type = 'table'              AND name IN ('accounts', 'sessions', 'meta', 'login_attempts', 'operations', 'revoked_sessions', 'audit_events', 'audit_fts', 'known_addresses', 'identifier_slowdowns') ORDER BY name",
        )
        .fetch_all(db.pool())
        .await
        .unwrap();
        assert_eq!(
            tables,
            [
                "accounts",
                "audit_events",
                "audit_fts",
                "identifier_slowdowns",
                "known_addresses",
                "login_attempts",
                "meta",
                "operations",
                "revoked_sessions",
                "sessions"
            ]
        );
    }
}

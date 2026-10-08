//! Base SQLite de l'agent (`hearth.db` dans le dossier de données) : pool, migrations embarquées,
//! dépôts SQLx. Aucune règle métier : le domaine décide, ici on observe et on exécute.
//!
//! Les requêtes sont vérifiées à la compilation (`query!`, mode hors ligne : dossier `.sqlx`
//! versionné). Après toute modification d'une requête ou d'une migration, voir « SQLx » dans
//! `CLAUDE.md` pour régénérer.

mod account_repo;
mod attack_mode_repo;
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
pub use attack_mode_repo::SqliteAttackModeRepo;
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

/// La base telle que le SERVICE l'obtient : ouverte par `Database::open_for_service`, qui a repris l'effacement
/// physique des anciennes empreintes. Le câblage du service (`app::start_*`) n'accepte que ce type : il ne peut
/// donc plus passer par `open` (l'effacement ne serait jamais repris et `erasure_pending` mentirait).
/// FIX:01M4D6KNHSZ39EY28XEFAV722X
#[derive(Debug, Clone)]
pub struct ServiceDatabase {
    database: Database,
    /// L'effacement a échoué à cette ouverture et sera retenté au prochain démarrage du service.
    erasure_pending: bool,
}

impl ServiceDatabase {
    pub fn database(&self) -> &Database {
        &self.database
    }

    pub fn pool(&self) -> &SqlitePool {
        self.database.pool()
    }

    /// L'effacement physique des anciennes empreintes est en attente (échec à l'ouverture du service).
    pub fn erasure_pending(&self) -> bool {
        self.erasure_pending
    }

    /// Pour les bancs d'essai qui ouvrent eux-mêmes leur base (`Database::open`) : rien n'est repris, rien
    /// n'est en attente. Jamais appelé par le code de production (`tests/service_database_guard.rs`).
    pub fn adopt(database: Database) -> Self {
        Self {
            database,
            erasure_pending: false,
        }
    }
}

impl Database {
    /// Ouvre `hearth.db` dans `data_dir` (créé au besoin) : mode WAL, clés étrangères actives,
    /// puis applique les migrations embarquées. Plusieurs processus peuvent ouvrir la même base :
    /// un écrivain attend l'autre (délai de 5 s).
    pub async fn open(data_dir: &Path) -> Result<Self, DatabaseError> {
        Ok(Self::open_with(data_dir, false).await?.0)
    }

    /// Comme `open`, pour le DÉMARRAGE DU SERVICE : seul lui reprend l'effacement physique des anciennes
    /// empreintes (`VACUUM`, point de contrôle). Les sous-commandes (`account revoke`, `attack-mode off`…)
    /// ouvrent la base sans le retenter à chaque lancement.
    // FIX:01M4D6KNHSZ39EY28XEFAV722X
    pub async fn open_for_service(data_dir: &Path) -> Result<ServiceDatabase, DatabaseError> {
        let database = Self::open_with(data_dir, true).await?;
        Ok(ServiceDatabase {
            erasure_pending: database.1,
            database: database.0,
        })
    }

    /// Ouvre la base ; rend aussi si l'effacement de reprise est resté en attente (jamais pour `scrub = false`).
    async fn open_with(data_dir: &Path, scrub: bool) -> Result<(Self, bool), DatabaseError> {
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
            // Une ligne supprimée ou vidée est réécrite avec des zéros dans la page (HRT-32) : une copie
            // de la base ne garde pas le contenu d'une ligne effacée (pages libres comprises).
            .pragma("secure_delete", "ON")
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
                path: path.clone(),
                message: error.to_string(),
            })?;
        // FIX:01M4BZN31A8Z8WN0WKNTCRTFFN : la migration 0008 vide la colonne, pas les octets ; l'effacement
        // physique se fait ici, hors de sa transaction (`VACUUM` n'y tourne pas).
        // Jamais bloquant : un point de contrôle retenu ou un `VACUUM` impossible (disque plein) ne
        // doivent pas mettre le serveur hors service. Avertissement, pas de marque, reprise au
        // démarrage suivant.
        let erasure_pending = scrub && !scrub_or_warn(&pool, &path).await;
        Ok((Self { pool }, erasure_pending))
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

/// Marque `PRAGMA user_version` : l'effacement physique des anciennes empreintes est fait.
///
/// **`user_version` sert de marque de l'agent** (SQLx tient la sienne dans `_sqlx_migrations` et ne
/// touche pas à `user_version`). Valeur 0 : rien de marqué ; 1 : effacement de la `0008` fait. Un
/// prochain besoin de marque prend la valeur suivante, jamais un autre mécanisme. Une base remise
/// par un retour arrière (BR-UPDATE-029) revient avec sa marque d'avant, sans la `0008` : tout
/// est rejoué à la mise à jour suivante.
const SCRUBBED_VERSION: i64 = 1;

#[derive(Debug, thiserror::Error)]
enum ScrubError {
    #[error(transparent)]
    Sql(#[from] sqlx::Error),
    /// Le point de contrôle n'a pas pu tronquer le journal (un autre lecteur le retient) : les
    /// anciennes trames y sont encore. Pas de marque, reprise au prochain démarrage.
    #[error("le journal de la base est retenu par un autre processus")]
    JournalBusy,
}

/// `PRAGMA wal_checkpoint(TRUNCATE)` : sa première colonne vaut 1 quand un lecteur l'a empêché de
/// finir. Ce n'est pas une erreur SQL, on la lit.
async fn truncate_journal(pool: &SqlitePool) -> Result<bool, sqlx::Error> {
    let (busy, _frames, _moved): (i64, i64, i64) =
        sqlx::query_as("PRAGMA wal_checkpoint(TRUNCATE)")
            .fetch_one(pool)
            .await?;
    Ok(busy == 0)
}

/// Lance l'effacement et ne propage jamais son échec : rend `false` (et avertit) si la marque n'a pas
/// été posée. Le message ne dit rien de sensible (le chemin du fichier et la cause technique seuls).
async fn scrub_or_warn(pool: &SqlitePool, path: &Path) -> bool {
    match scrub_after_0008(pool).await {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!(
                file = %path.display(),
                %error,
                "effacement physique des anciennes empreintes différé : l'agent démarre, l'effacement sera retenté au prochain démarrage"
            );
            false
        }
    }
}

/// Réécrit le fichier de la base et son journal pour que les empreintes effacées par la migration
/// `0008` ne restent pas dans les pages libres ni dans le journal : `VACUUM` (reconstruit la base,
/// le fichier rétrécit) puis point de contrôle qui tronque le journal à zéro octet. Fait une fois
/// (`user_version`) ; si l'effacement échoue ou reste partiel, l'agent démarre quand même (voir
/// `scrub_or_warn`) et le reprend
/// au démarrage suivant (la marque n'est posée qu'après un point de contrôle complet). Une base
/// neuve passe par là aussi, sans coût.
// FIX:01M4C9YKCVPDFSSJ2EYMZSRSPM : le résultat « occupé » du point de contrôle est lu avant la marque.
async fn scrub_after_0008(pool: &SqlitePool) -> Result<(), ScrubError> {
    let applied: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE version = 8 AND success")
            .fetch_one(pool)
            .await?;
    let done: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(pool)
        .await?;
    if applied == 0 || done >= SCRUBBED_VERSION {
        return Ok(());
    }
    sqlx::query("VACUUM").execute(pool).await?;
    if !truncate_journal(pool).await? {
        return Err(ScrubError::JournalBusy);
    }
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "PRAGMA user_version = {SCRUBBED_VERSION}"
    )))
    .execute(pool)
    .await?;
    // La marque elle-même passe au fichier ; qu'elle reste dans le journal ne coûte rien.
    truncate_journal(pool).await?;
    Ok(())
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

    async fn raw_pool(path: &Path) -> SqlitePool {
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(path)
                    .create_if_missing(true)
                    .journal_mode(SqliteJournalMode::Wal)
                    .busy_timeout(Duration::from_millis(200)),
            )
            .await
            .unwrap()
    }

    async fn user_version(pool: &SqlitePool) -> i64 {
        sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(pool)
            .await
            .unwrap()
    }

    // FIX:01M4C9YKCVPDFSSJ2EYMZSRSPM : un point de contrôle que retient un autre lecteur n'est pas
    // une erreur SQL ; la marque ne se pose pas, et se pose une fois le lecteur parti.
    #[tokio::test]
    async fn the_mark_is_not_set_while_another_reader_keeps_the_journal() {
        let dir = crate::infrastructure::data_dir::private_tempdir();
        let path = dir.path().join(DATABASE_FILE);
        let writer = raw_pool(&path).await;
        sqlx::migrate!("./migrations").run(&writer).await.unwrap();
        let reader = raw_pool(&path).await;
        let mut held = reader.acquire().await.unwrap();
        sqlx::query("BEGIN").execute(&mut *held).await.unwrap();
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM operations")
            .fetch_one(&mut *held)
            .await
            .unwrap();
        // Une écriture après l'instantané du lecteur : le journal a des trames qu'il retient.
        sqlx::query(
            "INSERT INTO operations (id, account_id, kind, request_hash, status, created_at)
             VALUES ('K', 'A', 'x', '', 'succeeded', '2026-10-07T10:00:00.000Z')",
        )
        .execute(&writer)
        .await
        .unwrap();

        let refused = scrub_after_0008(&writer).await;
        assert!(
            matches!(refused, Err(ScrubError::JournalBusy)),
            "{refused:?}"
        );
        assert_eq!(user_version(&writer).await, 0, "pas de marque");

        sqlx::query("ROLLBACK").execute(&mut *held).await.unwrap();
        drop(held);
        scrub_after_0008(&writer).await.unwrap();
        assert_eq!(user_version(&writer).await, SCRUBBED_VERSION);
    }

    // Règle (ADR-0034) : un effacement qui échoue ne met jamais le serveur hors service.
    #[tokio::test]
    async fn a_busy_journal_never_stops_the_database_from_opening_and_the_next_open_retries() {
        let dir = crate::infrastructure::data_dir::private_tempdir();
        let path = dir.path().join(DATABASE_FILE);
        // Une base d'avant la 0008, avec un lecteur qui tient un instantané.
        let writer = raw_pool(&path).await;
        let mut migrator = sqlx::migrate!("./migrations");
        migrator.migrations = migrator
            .migrations
            .iter()
            .filter(|m| m.version <= 7)
            .cloned()
            .collect::<Vec<_>>()
            .into();
        migrator.run(&writer).await.unwrap();
        let reader = raw_pool(&path).await;
        let mut held = reader.acquire().await.unwrap();
        sqlx::query("BEGIN").execute(&mut *held).await.unwrap();
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM operations")
            .fetch_one(&mut *held)
            .await
            .unwrap();
        writer.close().await;

        let db = Database::open_for_service(dir.path())
            .await
            .expect("l'agent démarre");
        assert_eq!(user_version(db.pool()).await, 0, "pas de marque");
        assert!(db.erasure_pending(), "l'effacement différé est signalé");
        db.pool().close().await;

        sqlx::query("ROLLBACK").execute(&mut *held).await.unwrap();
        drop(held);
        reader.close().await;
        let db = Database::open_for_service(dir.path()).await.unwrap();
        assert_eq!(user_version(db.pool()).await, SCRUBBED_VERSION, "retenté");
        assert!(!db.erasure_pending());
    }

    // Même règle pour un `VACUUM` impossible (disque plein, base en lecture seule).
    #[tokio::test]
    async fn a_vacuum_that_cannot_run_is_a_warning_not_a_failure() {
        let dir = crate::infrastructure::data_dir::private_tempdir();
        let path = dir.path().join(DATABASE_FILE);
        let db = Database::open(dir.path()).await.unwrap();
        sqlx::query("PRAGMA user_version = 0")
            .execute(db.pool())
            .await
            .unwrap();
        db.pool().close().await;
        let read_only = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(SqliteConnectOptions::new().filename(&path).read_only(true))
            .await
            .unwrap();
        assert!(!scrub_or_warn(&read_only, &path).await);
        assert!(scrub_after_0008(&read_only).await.is_err());
    }

    // FIX:01M4D6KNHSZ39EY28XEFAV722X : seul le démarrage du service reprend l'effacement ; une sous-commande
    // (`account revoke`, `attack-mode off`…) ouvre la base sans `VACUUM`.
    #[tokio::test]
    async fn only_the_service_start_retries_the_erasure_not_a_command_line_open() {
        let dir = crate::infrastructure::data_dir::private_tempdir();
        let path = dir.path().join(DATABASE_FILE);
        let writer = raw_pool(&path).await;
        let mut migrator = sqlx::migrate!("./migrations");
        migrator.migrations = migrator
            .migrations
            .iter()
            .filter(|m| m.version <= 7)
            .cloned()
            .collect::<Vec<_>>()
            .into();
        migrator.run(&writer).await.unwrap();
        writer.close().await;

        let command_line = Database::open(dir.path()).await.unwrap();
        assert_eq!(
            user_version(command_line.pool()).await,
            0,
            "aucun VACUUM, aucune marque"
        );
        command_line.pool().close().await;

        let service = Database::open_for_service(dir.path()).await.unwrap();
        assert_eq!(user_version(service.pool()).await, SCRUBBED_VERSION);
        assert!(!service.erasure_pending());
    }
}

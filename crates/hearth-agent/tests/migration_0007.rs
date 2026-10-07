//! Migration `0007` (HRT-28) : le réglage de fréquence du mot de passe en administration, par compte
//! (BR-TRUST-042). Additive : les comptes existants prennent le défaut (5 minutes), aucune donnée n'est
//! réécrite, la valeur est bornée à 0 (à chaque action) ou 300.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

fn migrator_up_to(version: i64) -> sqlx::migrate::Migrator {
    let mut migrator = sqlx::migrate!("./migrations");
    migrator.migrations = migrator
        .migrations
        .iter()
        .filter(|migration| migration.version <= version)
        .cloned()
        .collect::<Vec<_>>()
        .into();
    migrator
}

async fn open(path: &Path) -> SqlitePool {
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(true)
                .foreign_keys(true),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn existing_accounts_get_the_default_window_and_the_value_is_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let pool = open(&dir.path().join("hearth.db")).await;
    migrator_up_to(6).run(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO accounts (id, username, password_hash, role, created_at, password_changed_at)
         VALUES ('A1', 'marie', 'x', 'admin', '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    migrator_up_to(7).run(&pool).await.unwrap();
    let window: i64 = sqlx::query_scalar("SELECT reauth_window_s FROM accounts WHERE id = 'A1'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(window, 300, "les comptes existants prennent le défaut");
    let kept: String = sqlx::query_scalar("SELECT password_hash FROM accounts WHERE id = 'A1'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(kept, "x", "aucune donnée n'est réécrite");
    // 0 (à chaque action) et 300 sont permis, rien d'autre.
    for allowed in [0_i64, 300] {
        sqlx::query("UPDATE accounts SET reauth_window_s = ? WHERE id = 'A1'")
            .bind(allowed)
            .execute(&pool)
            .await
            .unwrap();
    }
    for refused in [1_i64, 60, 301, -1, 86_400] {
        assert!(
            sqlx::query("UPDATE accounts SET reauth_window_s = ? WHERE id = 'A1'")
                .bind(refused)
                .execute(&pool)
                .await
                .is_err(),
            "{refused}"
        );
    }
    // Rejouable : la base est à jour, rien ne se passe.
    migrator_up_to(7).run(&pool).await.unwrap();
}

#[tokio::test]
async fn a_database_opened_by_the_agent_is_at_least_at_0007() {
    use hearth_agent::infrastructure::sqlite::Database;
    let dir = tempfile::tempdir().unwrap();
    let fresh = Database::open(dir.path()).await.unwrap();
    let version: i64 = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations")
        .fetch_one(fresh.pool())
        .await
        .unwrap();
    assert!(version >= 7, "{version}");
    let window: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('accounts') WHERE name = 'reauth_window_s' AND \"notnull\" = 1",
    )
    .fetch_one(fresh.pool())
    .await
    .unwrap();
    assert_eq!(window, 1);
}

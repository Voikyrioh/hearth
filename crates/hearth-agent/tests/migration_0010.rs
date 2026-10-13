//! Migration `0010` (HRT-76, ADR-0041) : la colonne du droit « gérer les services ». Additive, valeur
//! par défaut `none` : aucun compte existant n'a le droit après la migration, pas même le premier
//! administrateur (BR-GRANT-002), et rien d'autre n'est modifié.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "support/tmp.rs"]
mod tmp;

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

async fn insert_account(pool: &SqlitePool, id: &str, username: &str, role: &str, hash: &str) {
    sqlx::query(
        "INSERT INTO accounts (id, username, password_hash, role, created_at, password_changed_at)
         VALUES (?, ?, ?, ?, '2026-10-01T10:00:00.000Z', '2026-10-01T10:00:00.000Z')",
    )
    .bind(id)
    .bind(username)
    .bind(hash)
    .bind(role)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn no_existing_account_gets_the_right_not_even_the_first_administrator() {
    let dir = tmp::tempdir().unwrap();
    let pool = open(&dir.path().join("hearth.db")).await;
    // Une base d'avant la migration, peuplée : le premier administrateur, un second, des comptes en
    // lecture seule, des sessions et un réglage de confirmation propre à un compte.
    migrator_up_to(9).run(&pool).await.unwrap();
    insert_account(&pool, "01A", "premier", "admin", "$argon2id$premier").await;
    insert_account(&pool, "01B", "second", "admin", "$argon2id$second").await;
    insert_account(&pool, "01C", "lucas", "readonly", "$argon2id$lucas").await;
    insert_account(&pool, "01D", "marie", "readonly", "$argon2id$marie").await;
    sqlx::query("UPDATE accounts SET reauth_window_s = 0 WHERE id = '01B'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO sessions (id, account_id, token_hash, client_name, client_addr, created_at,
                               last_seen_at, expires_at)
         VALUES ('S1', '01A', 'h1', 'test', '127.0.0.1', '2026-10-01T10:00:00.000Z',
                 '2026-10-01T10:00:00.000Z', '2026-11-01T10:00:00.000Z')",
    )
    .execute(&pool)
    .await
    .unwrap();

    migrator_up_to(10).run(&pool).await.unwrap();

    let rows: Vec<(String, String, String, String, i64)> = sqlx::query_as(
        "SELECT id, role, password_hash, services_right, reauth_window_s FROM accounts ORDER BY id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        rows,
        vec![
            (
                "01A".into(),
                "admin".into(),
                "$argon2id$premier".into(),
                "none".into(),
                300
            ),
            (
                "01B".into(),
                "admin".into(),
                "$argon2id$second".into(),
                "none".into(),
                0
            ),
            (
                "01C".into(),
                "readonly".into(),
                "$argon2id$lucas".into(),
                "none".into(),
                300
            ),
            (
                "01D".into(),
                "readonly".into(),
                "$argon2id$marie".into(),
                "none".into(),
                300
            ),
        ],
        "tous les comptes d'avant restent intacts et sans le droit"
    );
    let granted: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM accounts WHERE services_right <> 'none'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(granted, 0, "la migration n'accorde le droit à personne");
    let sessions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(sessions, 1, "aucune session n'est fermée");
}

#[tokio::test]
async fn an_account_inserted_without_naming_the_column_is_born_without_the_right() {
    let dir = tmp::tempdir().unwrap();
    let pool = open(&dir.path().join("hearth.db")).await;
    migrator_up_to(10).run(&pool).await.unwrap();
    // La requête d'insertion de l'agent ne nomme pas la colonne : le défaut fait foi, quel que soit le rôle.
    insert_account(&pool, "01A", "admin-neuf", "admin", "h").await;
    insert_account(&pool, "01B", "lecteur-neuf", "readonly", "h").await;
    let rights: Vec<String> = sqlx::query_scalar("SELECT services_right FROM accounts ORDER BY id")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rights, ["none", "none"]);
}

#[tokio::test]
async fn the_column_only_holds_the_two_known_values() {
    let dir = tmp::tempdir().unwrap();
    let pool = open(&dir.path().join("hearth.db")).await;
    migrator_up_to(10).run(&pool).await.unwrap();
    insert_account(&pool, "01A", "marie", "readonly", "h").await;
    for bad in ["root", "", "MANAGE", "all"] {
        let result = sqlx::query("UPDATE accounts SET services_right = ? WHERE id = '01A'")
            .bind(bad)
            .execute(&pool)
            .await;
        assert!(result.is_err(), "{bad:?} doit être refusé par la base");
    }
    // NULL aussi : la colonne est obligatoire.
    let null = sqlx::query("UPDATE accounts SET services_right = NULL WHERE id = '01A'")
        .execute(&pool)
        .await;
    assert!(null.is_err());
    // Les deux valeurs de la spec passent, le rôle n'y est pour rien.
    for good in ["manage", "none"] {
        sqlx::query("UPDATE accounts SET services_right = ? WHERE id = '01A'")
            .bind(good)
            .execute(&pool)
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn running_all_migrations_twice_changes_nothing() {
    let dir = tmp::tempdir().unwrap();
    let pool = open(&dir.path().join("hearth.db")).await;
    migrator_up_to(10).run(&pool).await.unwrap();
    insert_account(&pool, "01A", "marie", "admin", "h").await;
    sqlx::query("UPDATE accounts SET services_right = 'manage' WHERE id = '01A'")
        .execute(&pool)
        .await
        .unwrap();
    migrator_up_to(10).run(&pool).await.unwrap();
    let applied: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(applied, 10);
    // Rejouer la migration ne retire pas un droit accordé depuis.
    let right: String = sqlx::query_scalar("SELECT services_right FROM accounts WHERE id = '01A'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(right, "manage");
}

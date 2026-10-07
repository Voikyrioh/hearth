//! Migration `0008` (HRT-32, FIX-01M4BZN31A8Z8WN0WKNTCRTFFN) : les empreintes des requêtes suivies d'avant
//! la clé (SHA-256 sans clé, devinables hors ligne pour un corps qui contient un mot de passe) sont
//! effacées. Les lignes restent : le résultat d'une opération reste lisible, et une clé dont
//! l'empreinte est effacée n'est jamais ré-exécutée.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "support/tmp.rs"]
mod tmp;

use std::path::Path;

use sha2::{Digest, Sha256};
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

/// L'ancien calcul : SHA-256 nu de `méthode \0 chemin \0 corps`.
fn legacy_hash(method: &str, path: &str, body: &[u8]) -> String {
    let mut data = Vec::new();
    data.extend_from_slice(method.as_bytes());
    data.push(0);
    data.extend_from_slice(path.as_bytes());
    data.push(0);
    data.extend_from_slice(body);
    Sha256::digest(&data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[tokio::test]
async fn old_fingerprints_are_erased_and_nothing_else_is_touched() {
    let dir = tmp::tempdir().unwrap();
    let pool = open(&dir.path().join("hearth.db")).await;
    migrator_up_to(7).run(&pool).await.unwrap();
    let body = br#"{"username":"paul","password":"Un-bon-mot-de-passe-1"}"#;
    let leaked = legacy_hash("POST", "/accounts", body);
    assert_eq!(leaked.len(), 64);
    for (id, status, result) in [
        ("K-RUNNING", "running", None),
        ("K-DONE", "succeeded", Some(r#"{"status":201,"body":{}}"#)),
        ("K-FAILED", "failed", Some(r#"{"status":422,"body":{}}"#)),
        ("K-CUT", "interrupted", None),
    ] {
        sqlx::query(
            "INSERT INTO operations (id, account_id, kind, request_hash, status, result_json, created_at)
             VALUES (?, 'A1', 'POST /accounts', ?, ?, ?, '2026-10-07T10:00:00.000Z')",
        )
        .bind(id)
        .bind(&leaked)
        .bind(status)
        .bind(result)
        .execute(&pool)
        .await
        .unwrap();
    }

    migrator_up_to(8).run(&pool).await.unwrap();

    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM operations WHERE request_hash <> ''")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0, "aucune ancienne empreinte ne reste");
    // Les lignes, leurs états et leurs résultats restent lisibles.
    let rows: Vec<(String, String, Option<String>)> =
        sqlx::query_as("SELECT id, status, result_json FROM operations ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        rows,
        [
            ("K-CUT".to_owned(), "interrupted".to_owned(), None),
            (
                "K-DONE".to_owned(),
                "succeeded".to_owned(),
                Some(r#"{"status":201,"body":{}}"#.to_owned())
            ),
            (
                "K-FAILED".to_owned(),
                "failed".to_owned(),
                Some(r#"{"status":422,"body":{}}"#.to_owned())
            ),
            ("K-RUNNING".to_owned(), "running".to_owned(), None),
        ]
    );
}

#[tokio::test]
async fn an_empty_operations_table_migrates_and_the_column_stays_mandatory() {
    let dir = tmp::tempdir().unwrap();
    let pool = open(&dir.path().join("hearth.db")).await;
    migrator_up_to(8).run(&pool).await.unwrap();
    // La colonne reste NOT NULL : une empreinte effacée est la chaîne vide, jamais l'absence.
    let refused = sqlx::query(
        "INSERT INTO operations (id, account_id, kind, request_hash, status, created_at)
         VALUES ('K', 'A', 'x', NULL, 'running', '2026-10-07T10:00:00.000Z')",
    )
    .execute(&pool)
    .await;
    assert!(refused.is_err());
}

//! Migration `0015` (HRT-68, ADR-0037, ADR-0039) : les conteneurs que l'agent a CRÉÉS pour ses services. Migration
//! additive : une table neuve, rien d'existant ne bouge. La base elle-même refuse une clé qui n'est pas celle d'un
//! service créé par Hearth (`h_`), et un identifiant de conteneur ne se consigne qu'une fois.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "support/tmp.rs"]
mod tmp;

use std::path::Path;

use hearth_agent::application::ports::ServiceContainers;
use hearth_agent::domain::services::ServiceKey;
use hearth_agent::infrastructure::sqlite::SqliteServiceRepo;
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use time::OffsetDateTime;

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

const KEY: &str = "h_01JZ0000000000000000000001";

#[tokio::test]
async fn the_table_is_added_empty_and_replaying_changes_nothing() {
    let dir = tmp::tempdir().unwrap();
    let pool = open(&dir.path().join("hearth.db")).await;
    migrator_up_to(14).run(&pool).await.unwrap();
    migrator_up_to(15).run(&pool).await.unwrap();
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM service_containers")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
    let applied: i64 = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(applied, 15);
    migrator_up_to(15).run(&pool).await.unwrap();
}

#[tokio::test]
async fn the_base_refuses_a_key_that_is_not_a_service_created_by_hearth() {
    let dir = tmp::tempdir().unwrap();
    let pool = open(&dir.path().join("hearth.db")).await;
    migrator_up_to(15).run(&pool).await.unwrap();
    let insert = |key: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query(
                "INSERT INTO service_containers (container_id, service_key, recorded_at)
                 VALUES ('abc', ?, '2026-10-13T10:00:00.000Z')",
            )
            .bind(key)
            .execute(&pool)
            .await
        }
    };
    assert!(
        insert("c_0123abcd").await.is_err(),
        "un conteneur hors Hearth"
    );
    assert!(insert("u_docker-x.service").await.is_err(), "une unité");
    assert!(insert(KEY).await.is_ok());
}

#[tokio::test]
async fn the_repository_records_reads_and_forgets_without_duplicating() {
    let dir = tmp::tempdir().unwrap();
    let pool = open(&dir.path().join("hearth.db")).await;
    migrator_up_to(15).run(&pool).await.unwrap();
    let repo = SqliteServiceRepo::new(pool.clone());
    let key = ServiceKey::parse(KEY).unwrap();
    let other = ServiceKey::parse("h_01JZ0000000000000000000002").unwrap();
    let now = OffsetDateTime::UNIX_EPOCH + time::Duration::days(20_000);
    repo.record(&key, "id-1", now).await.unwrap();
    // Rejouer le même couple est sans effet ; le même identifiant pour une autre clé ne l'écrase pas.
    repo.record(&key, "id-1", now).await.unwrap();
    repo.record(&other, "id-1", now).await.unwrap();
    repo.record(&key, "id-2", now).await.unwrap();
    repo.record(&other, "id-3", now).await.unwrap();
    let mut mine = repo.of(&key).await.unwrap();
    mine.sort();
    assert_eq!(mine, ["id-1", "id-2"]);
    let mut all = repo.all().await.unwrap();
    all.sort();
    assert_eq!(all.len(), 3);
    assert!(all.contains(&("id-1".to_owned(), key.clone())));
    repo.forget_container("id-1").await.unwrap();
    assert_eq!(repo.of(&key).await.unwrap(), ["id-2"]);
    repo.forget_service(&key).await.unwrap();
    assert!(repo.of(&key).await.unwrap().is_empty());
    assert_eq!(repo.of(&other).await.unwrap(), ["id-3"]);
}

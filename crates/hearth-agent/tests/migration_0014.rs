//! Migration `0014` (HRT-80, BR-ADOPT-027, revue de la PR 89) : l'état « à confirmer » d'une adoption par
//! unité devient durable (`adopted_services.unconfirmed_since`). Migration additive : les adoptions
//! existantes restent telles quelles (colonne nulle), et la base elle-même refuse un état « à confirmer »
//! sur autre chose qu'une clé `u_` ou avec une date mal formée.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "support/tmp.rs"]
mod tmp;

use std::path::Path;

use hearth_agent::application::ports::ServiceRepo;
use hearth_agent::domain::services::ServiceKey;
use hearth_agent::infrastructure::sqlite::SqliteServiceRepo;
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

fn at(text: &str) -> OffsetDateTime {
    OffsetDateTime::parse(text, &Rfc3339).unwrap()
}

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

async fn insert_adopted(pool: &SqlitePool, key: &str, at: &str) {
    sqlx::query(
        "INSERT INTO adopted_services (service_key, name, unit, adopted_by, adopted_at)
         VALUES (?, 'service', ?, '01A', ?)",
    )
    .bind(key)
    .bind(key.strip_prefix("u_"))
    .bind(at)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn existing_adoptions_are_kept_as_they_are_and_none_is_to_confirm() {
    let dir = tmp::tempdir().unwrap();
    let pool = open(&dir.path().join("hearth.db")).await;
    migrator_up_to(13).run(&pool).await.unwrap();
    insert_adopted(
        &pool,
        "u_docker-terraria.service",
        "2026-10-13T10:00:00.000Z",
    )
    .await;
    insert_adopted(&pool, "c_0123abcd", "2026-10-13T10:00:00.000Z").await;
    migrator_up_to(14).run(&pool).await.unwrap();
    let untouched: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM adopted_services WHERE unconfirmed_since IS NULL")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(untouched, 2);
    let applied: i64 = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(applied, 14);
    // Rejouer ne change rien.
    migrator_up_to(14).run(&pool).await.unwrap();
}

#[tokio::test]
async fn the_base_refuses_a_to_confirm_state_on_a_container_key_or_with_a_malformed_date() {
    let dir = tmp::tempdir().unwrap();
    let pool = open(&dir.path().join("hearth.db")).await;
    migrator_up_to(14).run(&pool).await.unwrap();
    insert_adopted(
        &pool,
        "u_docker-terraria.service",
        "2026-10-13T10:00:00.000Z",
    )
    .await;
    insert_adopted(&pool, "c_0123abcd", "2026-10-13T10:00:00.000Z").await;
    let set = |key: &'static str, since: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query("UPDATE adopted_services SET unconfirmed_since = ? WHERE service_key = ?")
                .bind(since)
                .bind(key)
                .execute(&pool)
                .await
        }
    };
    assert!(
        set("c_0123abcd", "2026-10-14T10:00:00.000Z").await.is_err(),
        "seule une clé u_ peut être à confirmer"
    );
    assert!(
        set("u_docker-terraria.service", "hier").await.is_err(),
        "une date à largeur fixe"
    );
    assert!(
        set("u_docker-terraria.service", "2026-10-14T10:00:00.000Z")
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn the_repository_writes_the_state_all_or_nothing_keeps_the_original_date_and_ignores_another_adoption()
 {
    let dir = tmp::tempdir().unwrap();
    let pool = open(&dir.path().join("hearth.db")).await;
    migrator_up_to(14).run(&pool).await.unwrap();
    let repo = SqliteServiceRepo::new(pool.clone());
    let adopted_at = at("2026-10-13T10:00:00Z");
    insert_adopted(&pool, "u_docker-a.service", "2026-10-13T10:00:00.000Z").await;
    insert_adopted(&pool, "u_docker-b.service", "2026-10-13T10:00:00.000Z").await;
    let a = ServiceKey::parse("u_docker-a.service").unwrap();
    let b = ServiceKey::parse("u_docker-b.service").unwrap();
    let gone = ServiceKey::parse("u_docker-gone.service").unwrap();
    let first = at("2026-10-14T08:00:00Z");
    let written = repo
        .mark_unconfirmed(
            &[
                (a.clone(), adopted_at),
                // Autre date d'adoption : relâchée puis adoptée de nouveau, pas la même.
                (b.clone(), at("2026-10-13T11:00:00Z")),
                // Plus de ligne.
                (gone, adopted_at),
            ],
            first,
        )
        .await
        .unwrap();
    assert_eq!(written, vec![(a.clone(), first)]);
    // Une seconde écriture plus tard garde la date d'origine : le délai ne repart jamais.
    let later = at("2026-10-14T20:00:00Z");
    let again = repo
        .mark_unconfirmed(&[(a.clone(), adopted_at)], later)
        .await
        .unwrap();
    assert_eq!(again, vec![(a.clone(), first)]);
    let loaded = repo.load_adopted().await.unwrap();
    let state = |key: &ServiceKey| {
        loaded
            .iter()
            .find(|record| &record.key == key)
            .map(|record| record.unconfirmed_since)
    };
    assert_eq!(state(&a), Some(Some(first)));
    assert_eq!(state(&b), Some(None));
}

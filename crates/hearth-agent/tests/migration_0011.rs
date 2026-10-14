//! Migration `0011` (HRT-79, ADR-0043, BR-ADOPT-017) : la table des services adoptés, et
//! l'effacement de ce que `supervised_services` retenait d'un service qui n'est pas adopté.
//!
//! Sur une base existante, rien n'est encore adopté : la migration efface TOUTES les lignes de
//! `supervised_services` (c'est le but : elles décrivaient des conteneurs que personne n'a choisi de
//! faire gérer). Tout le reste de la base est intact.

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

async fn insert_supervised(pool: &SqlitePool, unit: &str, name: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO supervised_services (service_key, unit, name, last_view_json, last_seen_at)
         VALUES (?, ?, ?, '{}', '2026-10-13T10:00:00.000Z')",
    )
    .bind(format!("u_{unit}"))
    .bind(unit)
    .bind(name)
    .execute(pool)
    .await
    .map(|_| ())
}

async fn insert_adopted(
    pool: &SqlitePool,
    key: &str,
    unit: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO adopted_services (service_key, name, unit, adopted_by, adopted_at)
         VALUES (?, 'service', ?, '01A', '2026-10-13T10:00:00.000Z')",
    )
    .bind(key)
    .bind(unit)
    .execute(pool)
    .await
    .map(|_| ())
}

async fn count(pool: &SqlitePool, table: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn on_a_populated_base_every_remembered_service_is_erased_because_none_is_adopted_yet() {
    let dir = tmp::tempdir().unwrap();
    let pool = open(&dir.path().join("hearth.db")).await;
    migrator_up_to(10).run(&pool).await.unwrap();
    // Une base d'avant : un compte, une session, des entrées du journal, trois services lancés par
    // une unité (que la liste d'avant publiait tous), un déploiement et une action.
    sqlx::query(
        "INSERT INTO accounts (id, username, password_hash, role, created_at, password_changed_at)
         VALUES ('01A', 'marie', '$argon2id$marie', 'admin', '2026-10-01T10:00:00.000Z', '2026-10-01T10:00:00.000Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO audit_events (at, account, origin_kind, action, action_label, target, outcome)
         VALUES ('2026-10-12T10:00:00.000Z', 'marie', 'client', 'login', 'Connexion', NULL, 'ok')",
    )
    .execute(&pool)
    .await
    .unwrap();
    for (unit, name) in [
        ("docker-terraria.service", "terraria"),
        ("docker-7dtd.service", "7dtd"),
        ("docker-forge-studio.service", "forge-studio"),
    ] {
        insert_supervised(&pool, unit, name).await.unwrap();
    }
    sqlx::query(
        "INSERT INTO deployments (id, op_key, service_key, name, name_folded, image, tag, account,
                                  step, status, created_at)
         VALUES ('D1', 'K', 'h_X', 'web', 'web', 'busybox', '1.36', '01A', 'pull', 'running',
                 '2026-10-13T10:00:00.000Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(count(&pool, "supervised_services").await, 3);

    migrator_up_to(11).run(&pool).await.unwrap();

    assert_eq!(
        count(&pool, "supervised_services").await,
        0,
        "rien n'est adopté : rien ne reste"
    );
    assert_eq!(count(&pool, "adopted_services").await, 0);
    // Le reste de la base est intact.
    assert_eq!(count(&pool, "accounts").await, 1);
    assert_eq!(count(&pool, "audit_events").await, 1);
    assert_eq!(count(&pool, "deployments").await, 1);
    let applied: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(applied, 11);
}

#[tokio::test]
async fn running_the_migration_again_changes_nothing() {
    let dir = tmp::tempdir().unwrap();
    let pool = open(&dir.path().join("hearth.db")).await;
    migrator_up_to(11).run(&pool).await.unwrap();
    insert_adopted(
        &pool,
        "u_docker-terraria.service",
        Some("docker-terraria.service"),
    )
    .await
    .unwrap();
    migrator_up_to(11).run(&pool).await.unwrap();
    assert_eq!(count(&pool, "adopted_services").await, 1);
}

#[tokio::test]
async fn an_adoption_is_by_unit_or_by_container_never_by_name_and_never_for_a_service_made_by_hearth()
 {
    let dir = tmp::tempdir().unwrap();
    let pool = open(&dir.path().join("hearth.db")).await;
    migrator_up_to(11).run(&pool).await.unwrap();
    insert_adopted(
        &pool,
        "u_docker-terraria.service",
        Some("docker-terraria.service"),
    )
    .await
    .unwrap();
    insert_adopted(&pool, "c_0123abcd", None).await.unwrap();
    // Un service créé par Hearth est publié de droit : il ne s'adopte pas.
    assert!(
        insert_adopted(&pool, "h_01ARZ3NDEKTSV4RRFFQ69G5FAV", None)
            .await
            .is_err()
    );
    // Une adoption par nom n'existe pas.
    assert!(insert_adopted(&pool, "terraria", None).await.is_err());
    // L'unité va avec la clé `u_`, et seulement avec elle.
    assert!(
        insert_adopted(&pool, "u_docker-7dtd.service", None)
            .await
            .is_err()
    );
    assert!(
        insert_adopted(&pool, "u_docker-7dtd.service", Some("docker-other.service"))
            .await
            .is_err()
    );
    assert!(
        insert_adopted(&pool, "c_ffff", Some("docker-7dtd.service"))
            .await
            .is_err()
    );
    // Une même clé ne s'adopte qu'une fois.
    assert!(insert_adopted(&pool, "c_0123abcd", None).await.is_err());
    assert_eq!(count(&pool, "adopted_services").await, 2);
}

#[tokio::test]
async fn the_base_itself_keeps_only_adopted_services_in_supervised_services() {
    let dir = tmp::tempdir().unwrap();
    let pool = open(&dir.path().join("hearth.db")).await;
    migrator_up_to(11).run(&pool).await.unwrap();
    // Retenir un service qui n'est pas adopté : refusé par la base.
    assert!(
        insert_supervised(&pool, "docker-terraria.service", "terraria")
            .await
            .is_err()
    );
    assert_eq!(count(&pool, "supervised_services").await, 0);
    // Adopté, il se retient.
    insert_adopted(
        &pool,
        "u_docker-terraria.service",
        Some("docker-terraria.service"),
    )
    .await
    .unwrap();
    insert_supervised(&pool, "docker-terraria.service", "terraria")
        .await
        .unwrap();
    insert_adopted(&pool, "u_docker-7dtd.service", Some("docker-7dtd.service"))
        .await
        .unwrap();
    insert_supervised(&pool, "docker-7dtd.service", "7dtd")
        .await
        .unwrap();
    // Relâcher un service oublie ce qu'on en retenait, et seulement lui.
    sqlx::query("DELETE FROM adopted_services WHERE service_key = 'u_docker-terraria.service'")
        .execute(&pool)
        .await
        .unwrap();
    let kept: Vec<String> =
        sqlx::query_scalar("SELECT service_key FROM supervised_services ORDER BY service_key")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(kept, vec!["u_docker-7dtd.service".to_owned()]);
}

#[tokio::test]
async fn the_repository_reads_the_adopted_services_skips_an_unreadable_row_and_forgets_one() {
    use hearth_agent::application::ports::ServiceRepo;
    use hearth_agent::domain::services::ServiceKey;
    use hearth_agent::infrastructure::sqlite::SqliteServiceRepo;

    let dir = tmp::tempdir().unwrap();
    let pool = open(&dir.path().join("hearth.db")).await;
    // Le dépôt lit aussi la colonne de la migration 0014 : la base est à jour.
    migrator_up_to(14).run(&pool).await.unwrap();
    let repo = SqliteServiceRepo::new(pool.clone());
    assert!(repo.load_adopted().await.unwrap().is_empty());
    insert_adopted(
        &pool,
        "u_docker-terraria.service",
        Some("docker-terraria.service"),
    )
    .await
    .unwrap();
    insert_adopted(&pool, "c_0123abcd", None).await.unwrap();
    // Une clé que l'agent ne saurait pas lire : omise, jamais publiée.
    insert_adopted(&pool, "c_not a key!", None).await.unwrap();
    insert_supervised(&pool, "docker-terraria.service", "terraria")
        .await
        .unwrap();
    let loaded = repo.load_adopted().await.unwrap();
    let mut keys: Vec<&str> = loaded.iter().map(|record| record.key.as_str()).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["c_0123abcd", "u_docker-terraria.service"]);
    assert!(loaded.iter().all(|record| record.by == "01A"));
    // Oublier l'adoption d'un conteneur sans unité n'efface que celle-là.
    repo.forget_adopted(&ServiceKey::parse("c_0123abcd").unwrap())
        .await
        .unwrap();
    assert_eq!(repo.load_adopted().await.unwrap().len(), 1);
    // Oublier celle d'une unité oublie aussi ce qu'on en retenait.
    repo.forget_adopted(&ServiceKey::parse("u_docker-terraria.service").unwrap())
        .await
        .unwrap();
    assert!(repo.load_adopted().await.unwrap().is_empty());
    assert_eq!(count(&pool, "supervised_services").await, 0);
}

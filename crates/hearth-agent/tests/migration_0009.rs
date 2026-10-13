//! Migration `0009` (HRT-63, ADR-0037) : les quatre tables du lot « déploiements » et la colonne
//! `audit_events.details`. Additive : rien de ce qui existait n'est modifié, une base d'avant la
//! migration se relit à l'identique.

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

async fn migrated(dir: &Path) -> SqlitePool {
    let pool = open(&dir.join("hearth.db")).await;
    migrator_up_to(9).run(&pool).await.unwrap();
    pool
}

async fn insert_deployment(
    pool: &SqlitePool,
    id: &str,
    name_folded: &str,
    status: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO deployments (id, op_key, service_key, name, name_folded, image, tag, account,
                                  step, status, created_at)
         VALUES (?, 'K', 'h_X', ?, ?, 'busybox', '1.36', 'A1', 'pull', ?, '2026-10-13T10:00:00.000Z')",
    )
    .bind(id)
    .bind(name_folded)
    .bind(name_folded)
    .bind(status)
    .execute(pool)
    .await
    .map(|_| ())
}

#[tokio::test]
async fn the_four_tables_and_the_details_column_exist_and_old_journal_entries_are_unchanged() {
    let dir = tmp::tempdir().unwrap();
    let pool = open(&dir.path().join("hearth.db")).await;
    migrator_up_to(8).run(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO audit_events (at, account, origin_kind, action, action_label, target, outcome)
         VALUES ('2026-10-12T10:00:00.000Z', 'marie', 'client', 'login', 'Connexion', NULL, 'ok')",
    )
    .execute(&pool)
    .await
    .unwrap();

    migrator_up_to(9).run(&pool).await.unwrap();

    for (table, count_rows) in [
        ("service_intents", "SELECT COUNT(*) FROM service_intents"),
        ("service_actions", "SELECT COUNT(*) FROM service_actions"),
        ("deployments", "SELECT COUNT(*) FROM deployments"),
        (
            "supervised_services",
            "SELECT COUNT(*) FROM supervised_services",
        ),
    ] {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE name = ?")
            .bind(table)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1, "{table}");
        let rows: i64 = sqlx::query_scalar(count_rows)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(rows, 0, "{table} commence vide");
    }
    // L'entrée d'avant est intacte, sans détail.
    let row: (String, String, Option<String>) =
        sqlx::query_as("SELECT action, action_label, details FROM audit_events")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row, ("login".to_owned(), "Connexion".to_owned(), None));
    // La colonne s'écrit à l'insertion ; le journal reste immuable et la recherche plein texte vit.
    sqlx::query(
        "INSERT INTO audit_events (at, account, origin_kind, action, action_label, target, outcome, details)
         VALUES ('2026-10-13T10:00:00.000Z', 'marie', 'client', 'service.env.reveal',
                 'Valeurs des variables révélées', 'web', 'ok', '{\"kind\":\"none\"}')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let details: Option<String> =
        sqlx::query_scalar("SELECT details FROM audit_events WHERE action = 'service.env.reveal'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(details.as_deref(), Some("{\"kind\":\"none\"}"));
    let updated = sqlx::query("UPDATE audit_events SET details = 'x'")
        .execute(&pool)
        .await;
    assert!(updated.is_err(), "une entrée du journal ne se modifie pas");
    let found: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM audit_fts WHERE audit_fts MATCH 'variables'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(found, 1);
}

#[tokio::test]
async fn running_all_migrations_twice_changes_nothing() {
    let dir = tmp::tempdir().unwrap();
    let pool = migrated(dir.path()).await;
    migrator_up_to(9).run(&pool).await.unwrap();
    let applied: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(applied, 9);
}

#[tokio::test]
async fn two_running_deployments_cannot_share_a_name_but_finished_ones_can() {
    let dir = tmp::tempdir().unwrap();
    let pool = migrated(dir.path()).await;
    insert_deployment(&pool, "D1", "web", "running")
        .await
        .unwrap();
    // Même nom tant que le premier tourne : refusé par la base elle-même.
    assert!(
        insert_deployment(&pool, "D2", "web", "running")
            .await
            .is_err()
    );
    // Un autre nom, ou un déploiement fini du même nom : permis.
    insert_deployment(&pool, "D3", "api", "running")
        .await
        .unwrap();
    insert_deployment(&pool, "D4", "web", "failed")
        .await
        .unwrap();
    insert_deployment(&pool, "D5", "web", "succeeded")
        .await
        .unwrap();
    // Le premier fini, un nouveau déploiement du même nom est accepté.
    sqlx::query("UPDATE deployments SET status = 'succeeded' WHERE id = 'D1'")
        .execute(&pool)
        .await
        .unwrap();
    insert_deployment(&pool, "D6", "web", "running")
        .await
        .unwrap();
}

#[tokio::test]
async fn the_closed_lists_are_enforced_by_the_database() {
    let dir = tmp::tempdir().unwrap();
    let pool = migrated(dir.path()).await;
    let action = |kind: &'static str, status: &'static str, forced: i64| {
        let pool = pool.clone();
        async move {
            sqlx::query(
                "INSERT INTO service_actions (id, op_key, service_key, service_name, origin, kind,
                                              account, status, forced, started_at)
                 VALUES ('A', 'K', 'c_1', 'web', 'external', ?, 'marie', ?, ?, '2026-10-13T10:00:00.000Z')",
            )
            .bind(kind)
            .bind(status)
            .bind(forced)
            .execute(&pool)
            .await
        }
    };
    assert!(action("stop", "running", 0).await.is_ok());
    sqlx::query("DELETE FROM service_actions")
        .execute(&pool)
        .await
        .unwrap();
    assert!(action("explode", "running", 0).await.is_err());
    assert!(action("stop", "done", 0).await.is_err());
    assert!(action("stop", "running", 2).await.is_err());
    // Un arrêt voulu ne peut être que « stopped ».
    let bad_intent = sqlx::query(
        "INSERT INTO service_intents (service_key, container_id, intent, account, at)
         VALUES ('c_1', 'x', 'paused', 'marie', '2026-10-13T10:00:00.000Z')",
    )
    .execute(&pool)
    .await;
    assert!(bad_intent.is_err());
}

#[tokio::test]
async fn a_supervised_service_is_keyed_by_its_unit_and_nothing_else() {
    let dir = tmp::tempdir().unwrap();
    let pool = migrated(dir.path()).await;
    let insert = |key: &'static str, unit: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query(
                "INSERT INTO supervised_services (service_key, unit, name, last_view_json, last_seen_at)
                 VALUES (?, ?, 'terraria', '{}', '2026-10-13T10:00:00.000Z')",
            )
            .bind(key)
            .bind(unit)
            .execute(&pool)
            .await
        }
    };
    assert!(
        insert("u_docker-terraria.service", "docker-terraria.service")
            .await
            .is_ok()
    );
    // Une clé qui n'est pas celle de son unité, ou qui n'est pas une clé `u_` : refusée.
    assert!(insert("u_other.service", "docker-x.service").await.is_err());
    assert!(
        insert("c_0123", "c_0123").await.is_err(),
        "pas une clé d'unité"
    );
    // La même clé deux fois : refusée (une unité, un service).
    assert!(
        insert("u_docker-terraria.service", "docker-terraria.service")
            .await
            .is_err()
    );
}

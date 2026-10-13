//! Migration `0012` (HRT-83, ADR-0044 et ADR-0045) : la description durable des services et le journal
//! des opérations. Elle REMPLACE `service_actions` et `deployments` (créées par `0009`, jamais
//! écrites), garde `service_intents` et `supervised_services`, et ne détruit jamais de données.

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

/// La plus haute migration appliquée à la base.
async fn applied_version(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations")
        .fetch_one(pool)
        .await
        .unwrap()
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

async fn tables(pool: &SqlitePool) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

/// Nombre de lignes d'une table connue du test (jamais une chaîne construite).
async fn count(pool: &SqlitePool, table: &str) -> i64 {
    let sql = match table {
        "service_actions" => "SELECT COUNT(*) FROM service_actions",
        "deployments" => "SELECT COUNT(*) FROM deployments",
        "service_specs" => "SELECT COUNT(*) FROM service_specs",
        "service_spec_secrets" => "SELECT COUNT(*) FROM service_spec_secrets",
        "service_ops" => "SELECT COUNT(*) FROM service_ops",
        "service_op_steps" => "SELECT COUNT(*) FROM service_op_steps",
        other => panic!("table inconnue du test : {other}"),
    };
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

/// Une base du schéma `0010`, peuplée comme un serveur en service : comptes, session, journal d'activité,
/// un arrêt voulu, un service supervisé retenu.
async fn populated_0010(pool: &SqlitePool) {
    migrator_up_to(10).run(pool).await.unwrap();
    for (id, name, role) in [("01A", "premier", "admin"), ("01B", "lucas", "readonly")] {
        sqlx::query(
            "INSERT INTO accounts (id, username, password_hash, role, created_at, password_changed_at)
             VALUES (?, ?, 'hash', ?, '2026-10-01T10:00:00.000Z', '2026-10-01T10:00:00.000Z')",
        )
        .bind(id)
        .bind(name)
        .bind(role)
        .execute(pool)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO sessions (id, account_id, token_hash, client_name, client_addr, created_at,
                               last_seen_at, expires_at)
         VALUES ('S1', '01A', 'h1', 'test', '127.0.0.1', '2026-10-01T10:00:00.000Z',
                 '2026-10-01T10:00:00.000Z', '2026-11-01T10:00:00.000Z')",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO service_intents (service_key, container_id, intent, account, at)
         VALUES ('c_abc', 'abc', 'stopped', '01A', '2026-10-02T10:00:00.000Z')",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO supervised_services (service_key, unit, name, last_view_json, last_seen_at)
         VALUES ('u_docker-terraria.service', 'docker-terraria.service', 'terraria', '{}',
                 '2026-10-02T10:00:00.000Z')",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO meta (key, value) VALUES ('probe', 'kept')")
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn a_populated_0010_database_keeps_everything_and_swaps_the_two_empty_tables() {
    let dir = tmp::tempdir().unwrap();
    let pool = open(&dir.path().join("hearth.db")).await;
    populated_0010(&pool).await;
    let before = tables(&pool).await;
    assert!(before.contains(&"service_actions".to_owned()));
    assert!(before.contains(&"deployments".to_owned()));

    // `0011` oublie les services supervisés non adoptés (BR-ADOPT) : on en adopte un avant `0012`, qui
    // doit le garder tel quel.
    migrator_up_to(11).run(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO adopted_services (service_key, name, unit, adopted_by, adopted_at)
         VALUES ('u_docker-terraria.service', 'terraria', 'docker-terraria.service', '01A',
                 '2026-10-02T10:00:00.000Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO supervised_services (service_key, unit, name, last_view_json, last_seen_at)
         VALUES ('u_docker-terraria.service', 'docker-terraria.service', 'terraria', '{}',
                 '2026-10-02T10:00:00.000Z')",
    )
    .execute(&pool)
    .await
    .unwrap();

    migrator_up_to(12).run(&pool).await.unwrap();

    let after = tables(&pool).await;
    for gone in ["service_actions", "deployments", "migration_0012_guard"] {
        assert!(
            !after.contains(&gone.to_owned()),
            "{gone} doit avoir disparu"
        );
    }
    for born in [
        "service_specs",
        "service_spec_secrets",
        "service_ops",
        "service_op_steps",
    ] {
        assert!(after.contains(&born.to_owned()), "{born} doit exister");
    }
    // Ce qui existait est intact.
    let accounts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM accounts")
        .fetch_one(&pool)
        .await
        .unwrap();
    let sessions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    let intents: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM service_intents")
        .fetch_one(&pool)
        .await
        .unwrap();
    let supervised: String = sqlx::query_scalar("SELECT name FROM supervised_services")
        .fetch_one(&pool)
        .await
        .unwrap();
    let meta: String = sqlx::query_scalar("SELECT value FROM meta WHERE key = 'probe'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!((accounts, sessions, intents), (2, 1, 1));
    assert_eq!(supervised, "terraria");
    assert_eq!(meta, "kept");
    // La table de `0011` (services adoptés) n'est ni touchée ni requise par `0012`.
    assert!(after.contains(&"adopted_services".to_owned()));
    let adopted: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM adopted_services")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(adopted, 1);
    // Les nouvelles tables sont vides : aucun service n'a de description.
    for table in [
        "service_specs",
        "service_spec_secrets",
        "service_ops",
        "service_op_steps",
    ] {
        assert_eq!(count(&pool, table).await, 0, "{table}");
    }
}

#[tokio::test]
async fn a_non_empty_old_table_stops_the_migration_and_leaves_the_database_at_0010() {
    for (table, insert) in [
        (
            "service_actions",
            "INSERT INTO service_actions (id, op_key, service_key, service_name, origin, kind, account,
                                          status, started_at)
             VALUES ('A1', 'k', 'c_x', 'x', 'hearth', 'start', '01A', 'running', '2026-10-02T10:00:00.000Z')",
        ),
        (
            "deployments",
            "INSERT INTO deployments (id, op_key, service_key, name, name_folded, image, tag, account, step,
                                      status, created_at)
             VALUES ('D1', 'k', 'h_x', 'X', 'x', 'img', 'latest', '01A', 'pull', 'running',
                     '2026-10-02T10:00:00.000Z')",
        ),
    ] {
        let dir = tmp::tempdir().unwrap();
        let pool = open(&dir.path().join("hearth.db")).await;
        populated_0010(&pool).await;
        // L'état juste avant `0012`, quelles que soient les migrations intercalées depuis `0010`.
        migrator_up_to(11).run(&pool).await.unwrap();
        let before = applied_version(&pool).await;
        sqlx::query(insert).execute(&pool).await.unwrap();

        let result = migrator_up_to(12).run(&pool).await;
        assert!(result.is_err(), "{table} non vide : la migration doit refuser");

        // Rien n'a bougé : les anciennes tables, leur ligne, aucune nouvelle table, version appliquée inchangée.
        let names = tables(&pool).await;
        assert!(names.contains(&"service_actions".to_owned()), "{table}");
        assert!(names.contains(&"deployments".to_owned()), "{table}");
        assert!(!names.contains(&"service_specs".to_owned()), "{table}");
        assert!(!names.contains(&"migration_0012_guard".to_owned()), "{table}");
        assert_eq!(count(&pool, table).await, 1, "{table}");
        assert_eq!(applied_version(&pool).await, before, "{table}");
    }
}

#[tokio::test]
async fn running_the_migrations_twice_changes_nothing() {
    let dir = tmp::tempdir().unwrap();
    let pool = open(&dir.path().join("hearth.db")).await;
    populated_0010(&pool).await;
    migrator_up_to(12).run(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO service_specs (service_key, rev, name, name_folded, spec_json, updated_by, created_at, current)
         VALUES ('h_A', 1, 'A', 'a', '{}', '01A', '2026-10-02T10:00:00.000Z', 1)",
    )
    .execute(&pool)
    .await
    .unwrap();
    migrator_up_to(12).run(&pool).await.unwrap();
    let applied: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(applied, 12, "douze migrations, de `0001` à `0012`");
    let specs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM service_specs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(specs, 1);
}

async fn migrated(dir: &tmp::TestDir) -> SqlitePool {
    let pool = open(&dir.path().join("hearth.db")).await;
    migrator_up_to(12).run(&pool).await.unwrap();
    pool
}

async fn insert_spec(
    pool: &SqlitePool,
    key: &str,
    rev: i64,
    name: &str,
    current: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO service_specs (service_key, rev, name, name_folded, spec_json, updated_by, created_at, current)
         VALUES (?, ?, ?, ?, '{}', '01A', '2026-10-02T10:00:00.000Z', ?)",
    )
    .bind(key)
    .bind(rev)
    .bind(name)
    .bind(name.to_lowercase())
    .bind(current)
    .execute(pool)
    .await
    .map(|_| ())
}

#[tokio::test]
async fn one_current_revision_per_service_and_one_current_service_per_name() {
    let dir = tmp::tempdir().unwrap();
    let pool = migrated(&dir).await;
    insert_spec(&pool, "h_A", 1, "Terraria", 0).await.unwrap();
    insert_spec(&pool, "h_A", 2, "Terraria", 1).await.unwrap();
    // Une seconde révision courante du même service est refusée.
    assert!(insert_spec(&pool, "h_A", 3, "Terraria", 1).await.is_err());
    // Un autre service du même nom, à la casse près, aussi.
    assert!(insert_spec(&pool, "h_B", 1, "TERRARIA", 1).await.is_err());
    // Mais un autre service de ce nom redevient possible quand le premier n'est plus courant.
    insert_spec(&pool, "h_B", 1, "Autre", 1).await.unwrap();
    // Une révision numérotée 0 n'existe pas.
    assert!(insert_spec(&pool, "h_C", 0, "Zero", 1).await.is_err());
}

#[tokio::test]
async fn a_nonce_has_twelve_bytes_is_never_reused_and_secrets_follow_their_revision() {
    let dir = tmp::tempdir().unwrap();
    let pool = migrated(&dir).await;
    insert_spec(&pool, "h_A", 1, "A", 1).await.unwrap();
    let put = |field: &'static str, nonce: Vec<u8>, sealed: Vec<u8>| {
        let pool = pool.clone();
        async move {
            sqlx::query(
                "INSERT INTO service_spec_secrets (service_key, rev, field, nonce, sealed)
                 VALUES ('h_A', 1, ?, ?, ?)",
            )
            .bind(field)
            .bind(nonce)
            .bind(sealed)
            .execute(&pool)
            .await
        }
    };
    put("env:0", vec![1; 12], vec![9; 20]).await.unwrap();
    assert!(
        put("env:1", vec![1; 12], vec![9; 20]).await.is_err(),
        "nonce réutilisé"
    );
    assert!(
        put("env:2", vec![2; 11], vec![9; 20]).await.is_err(),
        "nonce trop court"
    );
    assert!(
        put("env:3", vec![3; 12], vec![9; 5]).await.is_err(),
        "sans étiquette"
    );
    // Une révision qui n'existe pas ne peut pas porter de secret.
    let orphan = sqlx::query(
        "INSERT INTO service_spec_secrets (service_key, rev, field, nonce, sealed)
         VALUES ('h_Z', 1, 'env:0', ?, ?)",
    )
    .bind(vec![4_u8; 12])
    .bind(vec![9_u8; 20])
    .execute(&pool)
    .await;
    assert!(orphan.is_err());
    // Supprimer la révision emporte ses secrets.
    sqlx::query("DELETE FROM service_specs WHERE service_key = 'h_A'")
        .execute(&pool)
        .await
        .unwrap();
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM service_spec_secrets")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
}

async fn insert_op(
    pool: &SqlitePool,
    id: &str,
    service: &str,
    status: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO service_ops (id, op_key, service_key, service_name, kind, account, plan_json, status,
                                  started_at, updated_at)
         VALUES (?, 'k', ?, 'x', 'update', '01A', '{}', ?, '2026-10-02T10:00:00.000Z',
                 '2026-10-02T10:00:00.000Z')",
    )
    .bind(id)
    .bind(service)
    .bind(status)
    .execute(pool)
    .await
    .map(|_| ())
}

#[tokio::test]
async fn two_unfinished_operations_never_share_a_service() {
    let dir = tmp::tempdir().unwrap();
    let pool = migrated(&dir).await;
    insert_op(&pool, "O1", "h_A", "running").await.unwrap();
    for open_status in [
        "accepted",
        "running",
        "awaiting_decision",
        "cancelling",
        "rolling_back",
        "needs_attention",
    ] {
        assert!(
            insert_op(&pool, &format!("X-{open_status}"), "h_A", open_status)
                .await
                .is_err(),
            "{open_status}"
        );
    }
    // Un autre service n'est pas gêné ; une opération terminée non plus.
    insert_op(&pool, "O2", "h_B", "running").await.unwrap();
    for done in [
        "cancelled",
        "rolled_back",
        "succeeded",
        "failed",
        "interrupted",
    ] {
        insert_op(&pool, &format!("D-{done}"), "h_A", done)
            .await
            .unwrap();
    }
    // Un statut ou un genre hors liste est refusé par la base.
    assert!(insert_op(&pool, "O3", "h_C", "paused").await.is_err());
    let bad_kind = sqlx::query(
        "INSERT INTO service_ops (id, op_key, service_key, service_name, kind, account, plan_json, status,
                                  started_at, updated_at)
         VALUES ('O4', 'k', 'h_D', 'x', 'teleport', '01A', '{}', 'running', 't', 't')",
    )
    .execute(&pool)
    .await;
    assert!(bad_kind.is_err());
}

#[tokio::test]
async fn the_journal_follows_its_operation_and_only_knows_four_states() {
    let dir = tmp::tempdir().unwrap();
    let pool = migrated(&dir).await;
    insert_op(&pool, "O1", "h_A", "running").await.unwrap();
    for (seq, state) in [(1, "intent"), (2, "done"), (3, "failed"), (4, "undone")] {
        sqlx::query(
            "INSERT INTO service_op_steps (op_id, seq, step, state, at) VALUES ('O1', ?, 'rename_old', ?, 't')",
        )
        .bind(seq)
        .bind(state)
        .execute(&pool)
        .await
        .unwrap();
    }
    let bad = sqlx::query(
        "INSERT INTO service_op_steps (op_id, seq, step, state, at) VALUES ('O1', 5, 's', 'maybe', 't')",
    )
    .execute(&pool)
    .await;
    assert!(bad.is_err());
    let orphan = sqlx::query(
        "INSERT INTO service_op_steps (op_id, seq, step, state, at) VALUES ('NOPE', 1, 's', 'intent', 't')",
    )
    .execute(&pool)
    .await;
    assert!(orphan.is_err(), "une étape sans opération");
    sqlx::query("DELETE FROM service_ops WHERE id = 'O1'")
        .execute(&pool)
        .await
        .unwrap();
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM service_op_steps")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
}

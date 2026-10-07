//! Migration `0006` (HRT-25) : origine « système » et nombre d'adresses d'une synthèse dans le journal,
//! fin d'activation mesurée sur le démarrage du noyau. Une seule migration, rejouable, testée sur une
//! base issue de la `0005` avec des données ; l'agent précédent redémarre sur la copie d'avant
//! l'échange (BR-UPDATE-029).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::path::Path;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

/// Le migrateur de l'agent tel qu'il était à cette version.
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

/// Une base telle que l'agent précédent (0005) la laisse, avec des données : un compte, une session,
/// un mode attaque qui a déjà servi, et un journal avec des entrées de toutes les origines, dont
/// des identifiants qui ont des trous (entrées purgées).
async fn database_at_0005(path: &Path) -> SqlitePool {
    let pool = open(path).await;
    migrator_up_to(5).run(&pool).await.unwrap();
    for sql in [
        "INSERT INTO accounts (id, username, password_hash, role, created_at, password_changed_at)
         VALUES ('A1', 'marie', 'x', 'admin', '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
        "INSERT INTO audit_events (at, account, origin_kind, origin_name, origin_addr, action, action_label, target, outcome, reason, repeat_count)
         VALUES ('2026-10-06T10:00:00.000Z', 'marie', 'client', 'poste', '10.0.0.7', 'login', 'Connexion', NULL, 'ok', NULL, 0),
                ('2026-10-06T10:01:00.000Z', NULL, 'cli', NULL, NULL, 'account.create', 'Création de compte', 'paul', 'ok', NULL, 0),
                ('2026-10-06T10:02:00.000Z', NULL, 'assistant', NULL, NULL, 'agent.update', 'Mise à jour de l''agent', NULL, 'failed', 'signature invalide', 0),
                ('2026-10-06T10:03:00.000Z', NULL, 'client', NULL, '', 'login', 'Connexion', NULL, 'denied', 'trop de tentatives (7 tentatives depuis 3 adresses)', 7)",
        "INSERT INTO audit_events (id, at, origin_kind, action, action_label, outcome)
         VALUES (900, '2026-10-06T10:04:00.000Z', 'cli', 'logout', 'Déconnexion', 'ok')",
        "DELETE FROM audit_events WHERE id = 900",
        "UPDATE attack_mode SET active = 0, activation_id = 'ACT0', activated_at = '2026-10-05T10:00:00.000Z',
                activated_by = 'marie', ended_at = '2026-10-05T11:00:00.000Z', ended_how = 'manual',
                last_boot_id = 'boot-0' WHERE id = 1",
        "INSERT INTO attack_trials (activation_id, account_id, kind, subject, used_at, outcome)
         VALUES ('ACT0', 'A1', 'address', '10.0.0.7', '2026-10-05T10:30:00.000Z', 'failed')",
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    pool
}

async fn count(pool: &SqlitePool, sql: &'static str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

type Column = (String, String, i64, i64);

async fn columns(pool: &SqlitePool, table: &str) -> BTreeSet<Column> {
    sqlx::query(r#"SELECT name, type, "notnull", pk FROM pragma_table_info(?)"#)
        .bind(table)
        .fetch_all(pool)
        .await
        .unwrap()
        .into_iter()
        .map(|row| {
            (
                row.get("name"),
                row.get("type"),
                row.get("notnull"),
                row.get("pk"),
            )
        })
        .collect()
}

#[tokio::test]
async fn the_migration_keeps_every_row_and_every_column_of_a_0005_database_and_only_adds() {
    let dir = tempfile::tempdir().unwrap();
    let pool = database_at_0005(&dir.path().join("hearth.db")).await;
    let audit_before = columns(&pool, "audit_events").await;
    let attack_before = columns(&pool, "attack_mode").await;
    let other_tables = [
        "accounts",
        "sessions",
        "known_addresses",
        "identifier_slowdowns",
        "trusted_devices",
        "attack_trials",
        "login_attempts",
        "operations",
    ];
    let mut others_before = Vec::new();
    for table in other_tables {
        others_before.push(columns(&pool, table).await);
    }

    migrator_up_to(6).run(&pool).await.unwrap();

    // Additive : rien de ce qui existait n'a disparu ni changé de type.
    let audit_after = columns(&pool, "audit_events").await;
    let attack_after = columns(&pool, "attack_mode").await;
    assert!(audit_before.is_subset(&audit_after));
    assert!(attack_before.is_subset(&attack_after));
    let added: Vec<_> = audit_after
        .difference(&audit_before)
        .map(|column| column.0.as_str())
        .collect();
    assert_eq!(added, vec!["repeat_addresses"]);
    let mut added: Vec<_> = attack_after
        .difference(&attack_before)
        .map(|column| column.0.clone())
        .collect();
    added.sort();
    assert_eq!(added, vec!["ended_boot_id", "ended_uptime_s"]);
    for (table, before) in other_tables.iter().zip(&others_before) {
        assert_eq!(
            &columns(&pool, table).await,
            before,
            "{table} est inchangée"
        );
    }

    // Toutes les données sont là, au même identifiant.
    assert_eq!(count(&pool, "SELECT COUNT(*) FROM audit_events").await, 4);
    let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM audit_events ORDER BY id")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(ids, vec![1, 2, 3, 4]);
    let synthesis: String = sqlx::query_scalar("SELECT reason FROM audit_events WHERE id = 4")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(synthesis.contains("7 tentatives depuis 3 adresses"));
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM audit_events WHERE repeat_addresses = 0"
        )
        .await,
        4,
        "les entrées déjà écrites n'ont pas de M typé"
    );
    assert_eq!(count(&pool, "SELECT COUNT(*) FROM accounts").await, 1);
    assert_eq!(count(&pool, "SELECT COUNT(*) FROM attack_trials").await, 1);
    let attack = sqlx::query(
        "SELECT active, activation_id, ended_how, last_boot_id, ended_boot_id, ended_uptime_s
         FROM attack_mode",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(attack.get::<i64, _>("active"), 0);
    assert_eq!(attack.get::<String, _>("activation_id"), "ACT0");
    assert_eq!(attack.get::<String, _>("ended_how"), "manual");
    assert_eq!(attack.get::<String, _>("last_boot_id"), "boot-0");
    assert_eq!(attack.get::<Option<String>, _>("ended_boot_id"), None);
    assert_eq!(attack.get::<Option<i64>, _>("ended_uptime_s"), None);
}

#[tokio::test]
async fn the_system_origin_is_accepted_and_the_journal_keeps_its_guarantees() {
    let dir = tempfile::tempdir().unwrap();
    let pool = database_at_0005(&dir.path().join("hearth.db")).await;
    // Avant : l'origine « système » est refusée par la contrainte.
    assert!(
        sqlx::query(
            "INSERT INTO audit_events (at, origin_kind, action, action_label, outcome)
             VALUES ('2026-10-07T00:00:00.000Z', 'system', 'security.alert', 'x', 'ok')"
        )
        .execute(&pool)
        .await
        .is_err()
    );
    migrator_up_to(6).run(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO audit_events (at, origin_kind, action, action_label, outcome, repeat_addresses)
         VALUES ('2026-10-07T00:00:00.000Z', 'system', 'attack_mode.auto_disable', 'x', 'ok', 3)",
    )
    .execute(&pool)
    .await
    .unwrap();
    // Les autres origines et les contraintes qui restent.
    for bad in [
        "INSERT INTO audit_events (at, origin_kind, action, action_label, outcome)
         VALUES ('2026-10-07T00:00:00.000Z', 'ailleurs', 'a', 'a', 'ok')",
        "INSERT INTO audit_events (at, origin_kind, action, action_label, outcome)
         VALUES ('2026-10-07T00:00:00.000Z', 'client', 'a', 'a', 'peut-etre')",
    ] {
        assert!(sqlx::query(bad).execute(&pool).await.is_err(), "{bad}");
    }
    // La suite des identifiants continue : jamais réutilisés, même ceux d'entrées purgées avant.
    let id: i64 = sqlx::query_scalar("SELECT MAX(id) FROM audit_events")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(
        id >= 901,
        "l'identifiant 900 purgé n'est pas réutilisé : {id}"
    );
    // Une entrée ne se modifie toujours pas.
    assert!(
        sqlx::query("UPDATE audit_events SET reason = 'x' WHERE id = 1")
            .execute(&pool)
            .await
            .is_err()
    );
    // La recherche plein texte voit les entrées d'avant ET celles d'après.
    let before: i64 = count(
        &pool,
        "SELECT COUNT(*) FROM audit_events WHERE id IN (SELECT rowid FROM audit_fts WHERE audit_fts MATCH 'signature')",
    )
    .await;
    assert_eq!(before, 1);
    sqlx::query(
        "INSERT INTO audit_events (at, origin_kind, action, action_label, outcome, reason)
         VALUES ('2026-10-07T00:00:01.000Z', 'system', 'attack_mode.resume', 'Mode attaque repris', 'ok', 'reprise')",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM audit_events WHERE id IN (SELECT rowid FROM audit_fts WHERE audit_fts MATCH 'reprise')",
        )
        .await,
        1
    );
    sqlx::query("DELETE FROM audit_events WHERE action = 'attack_mode.resume'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM audit_events WHERE id IN (SELECT rowid FROM audit_fts WHERE audit_fts MATCH 'reprise')",
        )
        .await,
        0,
        "le déclencheur de suppression tient l'index à jour"
    );
}

#[tokio::test]
async fn the_migration_can_be_replayed_and_the_previous_agent_still_writes_on_the_migrated_schema()
{
    let dir = tempfile::tempdir().unwrap();
    let pool = database_at_0005(&dir.path().join("hearth.db")).await;
    migrator_up_to(6).run(&pool).await.unwrap();
    // Rejouée : rien ne change, rien ne casse.
    let rows = count(&pool, "SELECT COUNT(*) FROM audit_events").await;
    migrator_up_to(6).run(&pool).await.unwrap();
    assert_eq!(
        count(&pool, "SELECT COUNT(*) FROM audit_events").await,
        rows
    );
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    // L'agent d'avant (0005) écrit une entrée comme il l'a toujours fait (sans la colonne ajoutée) : la
    // valeur par défaut tient.
    sqlx::query(
        "INSERT INTO audit_events (at, account, origin_kind, origin_name, origin_addr, action, action_label, target, outcome, reason, repeat_count)
         VALUES ('2026-10-07T00:00:00.000Z', 'marie', 'client', 'poste', '10.0.0.7', 'logout', 'Déconnexion', NULL, 'ok', NULL, 0)",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        count(
            &pool,
            "SELECT repeat_addresses FROM audit_events ORDER BY id DESC LIMIT 1"
        )
        .await,
        0
    );
    // Et le mode attaque : la ligne unique est intacte.
    assert_eq!(count(&pool, "SELECT COUNT(*) FROM attack_mode").await, 1);
    assert!(
        sqlx::query("INSERT INTO attack_mode (id) VALUES (2)")
            .execute(&pool)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn a_database_opened_by_the_agent_is_at_0006_and_a_fresh_one_matches_the_migrated_one() {
    use hearth_agent::infrastructure::sqlite::Database;
    let dir = tempfile::tempdir().unwrap();
    let migrated = database_at_0005(&dir.path().join("old.db")).await;
    migrator_up_to(6).run(&migrated).await.unwrap();
    let fresh_dir = tempfile::tempdir().unwrap();
    let fresh = Database::open(fresh_dir.path()).await.unwrap();
    for table in ["audit_events", "attack_mode", "attack_trials", "sessions"] {
        assert_eq!(
            columns(&migrated, table).await,
            columns(fresh.pool(), table).await,
            "{table} : une base neuve et une base migrée ont le même schéma"
        );
    }
    let version: i64 = count(fresh.pool(), "SELECT MAX(version) FROM _sqlx_migrations").await;
    assert_eq!(version, 6);
}

#[tokio::test]
async fn the_copy_taken_before_the_swap_comes_back_intact_after_a_rollback_and_migrates_again() {
    // Le superviseur de la mise à jour remet la copie de la base d'avant l'échange quand le nouvel agent
    // ne répond pas (BR-UPDATE-029) : l'agent précédent (0005) la retrouve telle quelle, et un nouvel
    // essai de mise à jour applique de nouveau la 0006.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hearth.db");
    let backup = dir.path().join("hearth.db.avant-echange");
    let pool = database_at_0005(&path).await;
    pool.close().await;
    std::fs::copy(&path, &backup).unwrap();

    let pool = open(&path).await;
    migrator_up_to(6).run(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO audit_events (at, origin_kind, action, action_label, outcome)
         VALUES ('2026-10-07T00:00:00.000Z', 'system', 'attack_mode.suspend', 'x', 'ok')",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;
    for suffix in ["-wal", "-shm"] {
        let _ = std::fs::remove_file(dir.path().join(format!("hearth.db{suffix}")));
    }
    std::fs::copy(&backup, &path).unwrap();

    let pool = open(&path).await;
    assert!(
        columns(&pool, "audit_events")
            .await
            .iter()
            .all(|column| column.0 != "repeat_addresses"),
        "l'agent précédent retrouve le schéma de la 0005"
    );
    assert_eq!(count(&pool, "SELECT COUNT(*) FROM audit_events").await, 4);
    assert_eq!(count(&pool, "SELECT COUNT(*) FROM accounts").await, 1);
    migrator_up_to(6).run(&pool).await.unwrap();
    assert_eq!(count(&pool, "SELECT COUNT(*) FROM audit_events").await, 4);
    assert_eq!(
        count(&pool, "SELECT MAX(version) FROM _sqlx_migrations").await,
        6
    );
}

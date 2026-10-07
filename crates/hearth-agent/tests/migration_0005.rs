//! Migration `0005` (HRT-22) : additive sur la `0004`, rejouable, testée sur une base issue de la
//! `0004` avec des données ; l'agent précédent redémarre sur la copie d'avant l'échange
//! (BR-UPDATE-029).

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "support/tmp.rs"]
mod tmp;
use std::collections::BTreeSet;
use std::path::Path;

use hearth_agent::infrastructure::sqlite::Database;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

/// Le migrateur de l'agent tel qu'il était à la `0004` (l'agent d'avant HRT-22).
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

/// Une base telle que l'agent précédent (0004) la laisse, avec des données.
async fn database_at_0004(path: &Path) -> SqlitePool {
    let pool = open(path).await;
    migrator_up_to(4).run(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO accounts (id, username, password_hash, role, created_at, password_changed_at)
         VALUES ('A1', 'marie', 'x', 'admin', '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z'),
                ('A2', 'paul', 'y', 'readonly', '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO sessions (id, account_id, token_hash, client_name, client_addr, created_at,
                               last_seen_at, expires_at)
         VALUES ('S1', 'A1', 'h1', 'poste', '10.0.0.7', '2026-10-06T10:00:00.000Z',
                 '2026-10-06T10:00:00.000Z', '2999-01-01T00:00:00.000Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO known_addresses (account_id, address, last_success_at)
         VALUES ('A1', '10.0.0.8', '2026-10-06T10:00:00.000Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO identifier_slowdowns (key, failures, wait_until, last_failure_at)
         VALUES ('ident:abc', 12, '2026-10-06T10:05:00.000Z', '2026-10-06T10:04:00.000Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool
}

type Column = (String, String, i64, i64);

/// Toutes les colonnes de toutes les tables : (table, nom, type, non nul, clé primaire).
async fn schema(pool: &SqlitePool) -> BTreeSet<(String, Column)> {
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
         AND name NOT LIKE '_sqlx_%' AND name NOT LIKE 'audit_fts%'",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    let mut columns = BTreeSet::new();
    for table in tables {
        for row in sqlx::query(r#"SELECT name, type, "notnull", pk FROM pragma_table_info(?)"#)
            .bind(&table)
            .fetch_all(pool)
            .await
            .unwrap()
        {
            columns.insert((
                table.clone(),
                (
                    row.get::<String, _>("name"),
                    row.get::<String, _>("type"),
                    row.get::<i64, _>("notnull"),
                    row.get::<i64, _>("pk"),
                ),
            ));
        }
    }
    columns
}

#[tokio::test]
async fn the_migration_only_adds_and_keeps_every_row_of_a_0004_database() {
    let dir = tmp::tempdir().unwrap();
    let pool = database_at_0004(&dir.path().join("hearth.db")).await;
    let before = schema(&pool).await;

    // La 0005 seule : les migrations suivantes (0006, HRT-25) ont leur propre test.
    migrator_up_to(5).run(&pool).await.unwrap();
    let after = schema(&pool).await;

    // Additive : rien de ce que la 0004 contenait n'a disparu ni changé de type.
    let lost: Vec<_> = before.difference(&after).collect();
    assert!(lost.is_empty(), "colonnes perdues ou modifiées : {lost:?}");
    let added: BTreeSet<&str> = after
        .difference(&before)
        .map(|(table, _)| table.as_str())
        .collect();
    assert_eq!(
        added,
        BTreeSet::from([
            "attack_mode",
            "attack_trials",
            "identifier_slowdowns",
            "known_addresses",
            "sessions",
            "trusted_devices",
        ])
    );
    for (table, column) in [
        ("known_addresses", "device_id"),
        ("known_addresses", "last_used_at"),
        ("identifier_slowdowns", "alerted_at"),
        ("sessions", "device_id"),
    ] {
        assert!(
            after.iter().any(|(t, c)| t == table && c.0 == column),
            "{table}.{column}"
        );
    }

    // Toutes les données sont là, et les colonnes ajoutées sont nulles.
    let counts = |sql: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>(sql)
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };
    assert_eq!(counts("SELECT COUNT(*) FROM accounts").await, 2);
    assert_eq!(
        counts("SELECT COUNT(*) FROM sessions WHERE device_id IS NULL").await,
        1
    );
    assert_eq!(
        counts(
            "SELECT COUNT(*) FROM known_addresses WHERE device_id IS NULL AND last_used_at IS NULL"
        )
        .await,
        1,
        "l'adresse retenue par l'agent d'avant"
    );
    assert_eq!(
        counts(
            "SELECT COUNT(*) FROM identifier_slowdowns WHERE failures = 12 AND alerted_at IS NULL"
        )
        .await,
        1
    );
    // Le mode attaque existe, inactif, en une seule ligne.
    let attack = sqlx::query("SELECT id, active, activation_id FROM attack_mode")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(attack.len(), 1);
    assert_eq!(attack[0].get::<i64, _>("id"), 1);
    assert_eq!(attack[0].get::<i64, _>("active"), 0);
    assert!(
        attack[0]
            .get::<Option<String>, _>("activation_id")
            .is_none()
    );
    assert_eq!(counts("SELECT COUNT(*) FROM trusted_devices").await, 0);
    assert_eq!(counts("SELECT COUNT(*) FROM attack_trials").await, 0);
}

#[tokio::test]
async fn what_the_previous_agent_writes_is_still_valid_on_the_migrated_schema() {
    let dir = tmp::tempdir().unwrap();
    let pool = database_at_0004(&dir.path().join("hearth.db")).await;
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    // Les écritures de l'agent d'avant (colonnes de la 0004 seulement) passent telles quelles.
    sqlx::query(
        "INSERT INTO sessions (id, account_id, token_hash, client_name, client_addr, created_at,
                               last_seen_at, expires_at)
         VALUES ('S9', 'A2', 'h9', 'poste', '10.0.0.9', '2026-10-06T10:00:00.000Z',
                 '2026-10-06T10:00:00.000Z', '2999-01-01T00:00:00.000Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO known_addresses (account_id, address, last_success_at)
         VALUES ('A2', '10.0.0.9', '2026-10-06T10:00:00.000Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO identifier_slowdowns (key, failures, wait_until, last_failure_at)
         VALUES ('ident:def', 1, NULL, '2026-10-06T10:04:00.000Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn the_constraints_of_the_new_tables_hold() {
    let dir = tmp::tempdir().unwrap();
    let pool = database_at_0004(&dir.path().join("hearth.db")).await;
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let device = |id: &'static str, account: &'static str, key: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query(
                "INSERT INTO trusted_devices (id, account_id, key_id, algorithm, public_key, name,
                                              created_at, last_proved_at, last_addr)
                 VALUES (?, ?, ?, 'ed25519', x'00', 'poste', '2026-10-07T00:00:00.000Z',
                         '2026-10-07T00:00:00.000Z', '10.0.0.7')",
            )
            .bind(id)
            .bind(account)
            .bind(key)
            .execute(&pool)
            .await
        }
    };
    device("D1", "A1", "k1").await.unwrap();
    assert!(
        device("D2", "A1", "k1").await.is_err(),
        "une clé par compte une fois"
    );
    device("D3", "A2", "k1")
        .await
        .expect("la contrainte est par compte");
    assert!(
        device("D4", "NOACCOUNT", "k4").await.is_err(),
        "compte inconnu"
    );
    // Le mode attaque : une seule ligne, valeurs bornées.
    assert!(
        sqlx::query("INSERT INTO attack_mode (id) VALUES (2)")
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE attack_mode SET active = 5")
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE attack_mode SET ended_how = 'ailleurs'")
            .execute(&pool)
            .await
            .is_err()
    );

    // Le poste emporte son adresse (cascade), détache sa session (set null) ; le compte emporte
    // ses postes.
    sqlx::query("UPDATE known_addresses SET device_id = 'D1' WHERE address = '10.0.0.8'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE sessions SET device_id = 'D1' WHERE id = 'S1'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM trusted_devices WHERE id = 'D1'")
        .execute(&pool)
        .await
        .unwrap();
    let remaining: Vec<String> = sqlx::query_scalar("SELECT address FROM known_addresses")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(!remaining.contains(&"10.0.0.8".to_owned()), "{remaining:?}");
    let session_device: Option<String> =
        sqlx::query_scalar("SELECT device_id FROM sessions WHERE id = 'S1'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(session_device, None);
    sqlx::query("DELETE FROM accounts WHERE id = 'A2'")
        .execute(&pool)
        .await
        .unwrap();
    let devices: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM trusted_devices")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(devices, 0);
}

#[tokio::test]
async fn the_migration_replays_and_the_previous_agent_restarts_on_the_copy_taken_before_the_swap() {
    let root = tmp::tempdir().unwrap();
    // Un sous-dossier : l'agent refuse un dossier de données ouvert aux autres (0755) et crée le sien en 0700.
    let dir = root.path().join("data");
    hearth_agent::infrastructure::data_dir::ensure(&dir).unwrap();
    let path = dir.join("hearth.db");
    let pool = database_at_0004(&path).await;
    pool.close().await;
    // Le superviseur copie la base service arrêté, avant l'échange (BR-UPDATE-029).
    let copy = root.path().join("hearth.db.before-swap");
    std::fs::copy(&path, &copy).unwrap();

    // Le nouvel agent démarre : 0005 s'applique, les données restent. Redémarrer ne rejoue rien.
    let database = Database::open(dir.as_path()).await.unwrap();
    sqlx::query(
        "INSERT INTO trusted_devices (id, account_id, key_id, algorithm, public_key, name,
                                      created_at, last_proved_at, last_addr)
         VALUES ('D1', 'A1', 'k1', 'ed25519', x'00', 'poste', '2026-10-07T00:00:00.000Z',
                 '2026-10-07T00:00:00.000Z', '10.0.0.7')",
    )
    .execute(database.pool())
    .await
    .unwrap();
    database.pool().close().await;
    let again = Database::open(dir.as_path()).await.unwrap();
    let kept: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM trusted_devices")
        .fetch_one(again.pool())
        .await
        .unwrap();
    assert_eq!(kept, 1, "redémarrer sur la base migrée ne rejoue rien");
    again.pool().close().await;

    // L'ancien binaire devant la base DÉJÀ migrée refuse de démarrer (migration inconnue de lui) :
    // c'est pourquoi le retour arrière remet la copie d'avant l'échange, jamais la base migrée.
    let migrated = open(&path).await;
    assert!(migrator_up_to(4).run(&migrated).await.is_err());
    migrated.close().await;

    // Retour arrière : la copie est remise (base d'abord) ; l'ancien agent redémarre dessus avec
    // toutes ses données ...
    std::fs::copy(&copy, &path).unwrap();
    for suffix in ["-wal", "-shm"] {
        let _ = std::fs::remove_file(dir.join(format!("hearth.db{suffix}")));
    }
    let restored = open(&path).await;
    migrator_up_to(4)
        .run(&restored)
        .await
        .expect("l'ancien agent redémarre");
    let accounts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM accounts")
        .fetch_one(&restored)
        .await
        .unwrap();
    assert_eq!(accounts, 2);
    restored.close().await;
    // ... et le nouvel agent, à la mise à jour suivante, rejoue 0005 sans erreur.
    let replayed = Database::open(dir.as_path()).await.unwrap();
    let attack: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM attack_mode")
        .fetch_one(replayed.pool())
        .await
        .unwrap();
    assert_eq!(attack, 1);
    let devices: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM trusted_devices")
        .fetch_one(replayed.pool())
        .await
        .unwrap();
    assert_eq!(
        devices, 0,
        "les postes inscrits après la copie sont perdus, comme le dit la règle"
    );
}

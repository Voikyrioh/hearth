//! Sous-commande `attack-mode status|off` : le vrai binaire, lancé en processus sur un dossier
//! temporaire, directement sur la base, sans réseau (HRT-25, BR-TRUST-027 voie c). Il n'existe pas de
//! sous-commande pour ACTIVER : activer exige un poste avec sa clé (Q14 point 3).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::Command;

use hearth_agent::infrastructure::sqlite::Database;
use sqlx::Row;
use tempfile::TempDir;

struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

fn hearth(dir: &Path, args: &[&str]) -> Run {
    let output = Command::new(env!("CARGO_BIN_EXE_hearth-agent"))
        .arg("--data-dir")
        .arg(dir)
        .arg("attack-mode")
        .args(args)
        .env_remove("HEARTH_CONFIG")
        .env_remove("HEARTH_DATA_DIR")
        .output()
        .expect("lancement de hearth-agent");
    Run {
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

async fn database() -> (TempDir, Database) {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path()).await.unwrap();
    (dir, db)
}

async fn activate(db: &Database) {
    sqlx::query(
        "UPDATE attack_mode SET active = 1, activation_id = 'ACT1',
                activated_at = '2026-10-07T01:00:00.000Z', activated_by = 'marie' WHERE id = 1",
    )
    .execute(db.pool())
    .await
    .unwrap();
}

#[tokio::test]
async fn status_tells_the_mode_and_the_activation_and_off_ends_it_with_a_journal_entry() {
    let (dir, db) = database().await;
    let off = hearth(dir.path(), &["status"]);
    assert_eq!(off.code, Some(0), "{}{}", off.stdout, off.stderr);
    assert!(off.stdout.contains("mode : off"), "{}", off.stdout);
    assert!(!off.stdout.contains("activation_id"));

    activate(&db).await;
    let active = hearth(dir.path(), &["status"]);
    assert_eq!(active.code, Some(0));
    assert!(active.stdout.contains("mode : active"), "{}", active.stdout);
    assert!(
        active.stdout.contains("activation_id : ACT1"),
        "{}",
        active.stdout
    );

    let done = hearth(dir.path(), &["off"]);
    assert_eq!(done.code, Some(0), "{}{}", done.stdout, done.stderr);
    assert_eq!(done.stdout.trim(), "Le mode attaque est désactivé.");
    let row = sqlx::query("SELECT active, ended_how FROM attack_mode WHERE id = 1")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(row.get::<i64, _>("active"), 0);
    assert_eq!(
        row.get::<Option<String>, _>("ended_how").as_deref(),
        Some("cli")
    );
    let entry = sqlx::query(
        "SELECT origin_kind, account, outcome FROM audit_events WHERE action = 'attack_mode.disable'",
    )
    .fetch_all(db.pool())
    .await
    .unwrap();
    assert_eq!(entry.len(), 1);
    assert_eq!(entry[0].get::<String, _>("origin_kind"), "cli");
    assert_eq!(entry[0].get::<Option<String>, _>("account"), None);
    assert_eq!(entry[0].get::<String, _>("outcome"), "ok");

    // Sans effet, sans écriture, quand il n'est pas actif.
    let again = hearth(dir.path(), &["off"]);
    assert_eq!(again.code, Some(0));
    assert_eq!(again.stdout.trim(), "Le mode attaque n'était pas actif.");
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_events WHERE action = 'attack_mode.disable'",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(count, 1);
    let status = hearth(dir.path(), &["status"]);
    assert!(status.stdout.contains("mode : off"));
}

#[test]
fn there_is_no_subcommand_to_enable_the_mode() {
    let dir = tempfile::tempdir().unwrap();
    for args in [&["on"][..], &["enable"][..], &["activate"][..]] {
        let run = hearth(dir.path(), args);
        assert_ne!(run.code, Some(0), "{args:?}");
    }
}

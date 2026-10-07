//! Sous-commandes `account …` : le vrai binaire, lancé en processus sur un dossier temporaire.
#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "support/tmp.rs"]
mod tmp;
use std::path::Path;
use std::process::{Command, Output};

use hearth_agent::infrastructure::sqlite::{DATABASE_FILE, Database};
use tmp::TestDir;

const PASSWORD: &str = "Correct-Horse-9";

struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Run {
    fn ok(&self) -> bool {
        self.code == Some(0)
    }

    fn all(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }
}

fn hearth(dir: &Path, args: &[&str], password: Option<&str>) -> Run {
    let mut command = Command::new(env!("CARGO_BIN_EXE_hearth-agent"));
    command
        .arg("--data-dir")
        .arg(dir)
        .arg("account")
        .args(args)
        .env_remove("HEARTH_CONFIG")
        .env_remove("HEARTH_DATA_DIR")
        .env_remove("HEARTH_ACCOUNT_PASSWORD");
    if let Some(password) = password {
        command.env("HEARTH_ACCOUNT_PASSWORD", password);
    }
    let Output {
        status,
        stdout,
        stderr,
    } = command.output().expect("lancement de hearth-agent");
    Run {
        code: status.code(),
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
    }
}

fn data_dir() -> TestDir {
    tmp::tempdir().expect("dossier temporaire")
}

fn add(dir: &Path, username: &str, role: &str) -> Run {
    hearth(dir, &["add", username, "--role", role], Some(PASSWORD))
}

#[test]
fn add_then_list() {
    let dir = data_dir();

    let created = add(dir.path(), "marie", "admin");
    assert!(created.ok(), "{}", created.all());
    assert_eq!(created.stdout.trim(), "Compte marie créé");
    assert!(dir.path().join(DATABASE_FILE).exists());

    assert!(add(dir.path(), "paul", "readonly").ok());
    let listed = hearth(dir.path(), &["list"], None);
    assert!(listed.ok(), "{}", listed.all());
    let lines: Vec<&str> = listed.stdout.lines().collect();
    assert_eq!(lines.len(), 3, "{}", listed.stdout);
    assert!(lines[0].starts_with("Identifiant"));
    assert!(lines[0].contains("Rôle"));
    assert!(lines[0].contains("Créé le"));
    assert!(lines[0].contains("Dernière connexion"));
    assert!(lines[0].contains("Sessions ouvertes"));
    assert!(lines[1].starts_with("marie") && lines[1].contains("Administrateur"));
    assert!(lines[2].starts_with("paul") && lines[2].contains("Lecture seule"));
    assert!(lines[1].contains("jamais") && lines[1].trim_end().ends_with('0'));
}

#[test]
fn list_without_account_invites_to_create_one() {
    let dir = data_dir();
    let listed = hearth(dir.path(), &["list"], None);
    assert!(listed.ok(), "{}", listed.all());
    assert_eq!(
        listed.stdout.trim(),
        "Aucun compte n'existe pour le moment. Crée-en un pour commencer."
    );
}

#[test]
fn a_weak_password_is_refused_with_every_unmet_rule_and_nothing_is_created() {
    let dir = data_dir();
    let run = hearth(
        dir.path(),
        &["add", "marie", "--role", "admin"],
        Some("abc"),
    );
    assert_ne!(run.code, Some(0));
    assert!(run.stdout.is_empty());
    assert!(
        run.stderr
            .contains("Le mot de passe doit contenir au moins 12 caractères")
    );
    assert!(
        run.stderr
            .contains("Le mot de passe doit contenir au moins un chiffre")
    );
    assert!(
        run.stderr
            .contains("Le mot de passe doit contenir au moins une majuscule")
    );
    assert!(
        hearth(dir.path(), &["list"], None)
            .stdout
            .contains("Aucun compte")
    );
}

#[test]
fn a_password_containing_the_username_is_refused() {
    let dir = data_dir();
    let run = hearth(
        dir.path(),
        &["add", "marie", "--role", "admin"],
        Some("Hello-Marie-123"),
    );
    assert_ne!(run.code, Some(0));
    assert!(
        run.stderr
            .contains("Le mot de passe ne doit pas contenir l'identifiant")
    );
}

#[test]
fn invalid_and_duplicate_usernames_are_refused_with_the_specified_messages() {
    let dir = data_dir();
    let invalid = add(dir.path(), "a b", "admin");
    assert_ne!(invalid.code, Some(0));
    assert!(
        invalid
            .stderr
            .contains("L'identifiant contient des caractères non autorisés")
    );
    let short = add(dir.path(), "ab", "admin");
    assert!(
        short
            .stderr
            .contains("L'identifiant doit contenir au moins 3 caractères")
    );

    assert!(add(dir.path(), "marie", "admin").ok());
    let duplicate = add(dir.path(), "MARIE", "readonly");
    assert_ne!(duplicate.code, Some(0));
    assert!(
        duplicate
            .stderr
            .contains("Cet identifiant est déjà utilisé")
    );
}

#[test]
fn the_password_never_appears_in_the_output() {
    let dir = data_dir();
    let created = add(dir.path(), "marie", "admin");
    assert!(!created.all().contains(PASSWORD));
    let weak = hearth(
        dir.path(),
        &["add", "paul", "--role", "admin"],
        Some("Short-1"),
    );
    assert!(!weak.all().contains("Short-1"));
    assert!(!hearth(dir.path(), &["list"], None).all().contains(PASSWORD));
}

#[test]
fn removing_the_last_administrator_is_refused() {
    let dir = data_dir();
    assert!(add(dir.path(), "marie", "admin").ok());
    assert!(add(dir.path(), "paul", "readonly").ok());

    let removal = hearth(dir.path(), &["remove", "marie"], None);
    assert_ne!(removal.code, Some(0));
    assert!(removal.stdout.is_empty());
    assert_eq!(
        removal.stderr.trim(),
        "Il doit toujours rester au moins un administrateur"
    );

    let demotion = hearth(dir.path(), &["role", "marie", "readonly"], None);
    assert_ne!(demotion.code, Some(0));
    assert!(
        demotion
            .stderr
            .contains("Il doit toujours rester au moins un administrateur")
    );

    let listed = hearth(dir.path(), &["list"], None);
    assert!(listed.stdout.contains("marie") && listed.stdout.contains("Administrateur"));
}

#[test]
fn role_and_remove_work_when_another_administrator_remains() {
    let dir = data_dir();
    assert!(add(dir.path(), "marie", "admin").ok());
    assert!(add(dir.path(), "paul", "readonly").ok());

    let promoted = hearth(dir.path(), &["role", "paul", "admin"], None);
    assert!(promoted.ok(), "{}", promoted.all());
    assert_eq!(promoted.stdout.trim(), "paul est maintenant Administrateur");

    let demoted = hearth(dir.path(), &["role", "marie", "readonly"], None);
    assert_eq!(demoted.stdout.trim(), "marie est maintenant Lecture seule");

    let removed = hearth(dir.path(), &["remove", "marie"], None);
    assert_eq!(removed.stdout.trim(), "Compte marie supprimé");
    let listed = hearth(dir.path(), &["list"], None);
    assert!(!listed.stdout.contains("marie") && listed.stdout.contains("paul"));
}

#[test]
fn an_unknown_account_is_an_error() {
    let dir = data_dir();
    assert!(add(dir.path(), "marie", "admin").ok());
    for args in [
        vec!["remove", "personne"],
        vec!["revoke", "personne"],
        vec!["role", "personne", "admin"],
    ] {
        let run = hearth(dir.path(), &args, None);
        assert_ne!(run.code, Some(0), "{args:?}");
        assert_eq!(run.stderr.trim(), "Ce compte n'existe pas", "{args:?}");
    }
    let run = hearth(dir.path(), &["passwd", "personne"], Some(PASSWORD));
    assert_ne!(run.code, Some(0));
    assert_eq!(run.stderr.trim(), "Ce compte n'existe pas");
}

#[test]
fn an_unknown_role_is_refused_before_anything_happens() {
    let dir = data_dir();
    let run = hearth(
        dir.path(),
        &["add", "marie", "--role", "root"],
        Some(PASSWORD),
    );
    assert_ne!(run.code, Some(0));
    assert!(!dir.path().join(DATABASE_FILE).exists());
}

async fn insert_session(dir: &Path, username: &str, id: &str) {
    let db = Database::open(dir).await.unwrap();
    sqlx::query(
        "INSERT INTO sessions (id, account_id, token_hash, client_name, client_addr, \
         created_at, last_seen_at, expires_at) \
         SELECT ?, id, ?, 'test', '127.0.0.1', '2026-10-04T10:00:00Z', '2026-10-04T10:00:00Z', \
         '2099-01-01T00:00:00Z' FROM accounts WHERE username = ?",
    )
    .bind(id)
    .bind(format!("hash-{id}"))
    .bind(username)
    .execute(db.pool())
    .await
    .unwrap();
    db.pool().close().await;
}

#[tokio::test]
async fn passwd_changes_the_password_and_closes_the_sessions() {
    let dir = data_dir();
    assert!(add(dir.path(), "marie", "admin").ok());
    insert_session(dir.path(), "marie", "S1").await;
    let listed = hearth(dir.path(), &["list"], None);
    assert!(
        listed
            .stdout
            .lines()
            .nth(1)
            .unwrap()
            .trim_end()
            .ends_with('1')
    );

    let changed = hearth(dir.path(), &["passwd", "marie"], Some("Brand-New-Pass-7"));
    assert!(changed.ok(), "{}", changed.all());
    assert_eq!(changed.stdout.trim(), "Mot de passe changé");
    assert!(!changed.all().contains("Brand-New-Pass-7"));
    let listed = hearth(dir.path(), &["list"], None);
    assert!(
        listed
            .stdout
            .lines()
            .nth(1)
            .unwrap()
            .trim_end()
            .ends_with('0')
    );

    // Les règles s'appliquent aussi au changement.
    let weak = hearth(dir.path(), &["passwd", "marie"], Some("trop-court"));
    assert_ne!(weak.code, Some(0));
    assert!(weak.stderr.contains("au moins 12 caractères"));
}

#[tokio::test]
async fn revoke_closes_the_sessions_without_touching_the_password() {
    let dir = data_dir();
    assert!(add(dir.path(), "marie", "admin").ok());
    insert_session(dir.path(), "marie", "S1").await;
    insert_session(dir.path(), "marie", "S2").await;
    let listed = hearth(dir.path(), &["list"], None);
    assert!(
        listed
            .stdout
            .lines()
            .nth(1)
            .unwrap()
            .trim_end()
            .ends_with('2')
    );

    let hash_before = stored_hash(dir.path()).await;
    let revoked = hearth(dir.path(), &["revoke", "marie"], None);
    assert!(revoked.ok(), "{}", revoked.all());
    assert_eq!(revoked.stdout.trim(), "Sessions de marie fermées");
    assert!(
        hearth(dir.path(), &["list"], None)
            .stdout
            .lines()
            .nth(1)
            .unwrap()
            .trim_end()
            .ends_with('0')
    );
    assert_eq!(stored_hash(dir.path()).await, hash_before);
}

/// action, compte, origine, adresse, cible, résultat, raison.
type AuditRow = (
    String,
    Option<String>,
    String,
    Option<String>,
    Option<String>,
    String,
    Option<String>,
);

async fn stored_hash(dir: &Path) -> String {
    let db = Database::open(dir).await.unwrap();
    let hash = sqlx::query_scalar("SELECT password_hash FROM accounts")
        .fetch_one(db.pool())
        .await
        .unwrap();
    db.pool().close().await;
    hash
}

#[tokio::test]
async fn every_command_is_journaled_with_the_command_line_origin_and_no_secret() {
    let dir = data_dir();
    assert!(add(dir.path(), "marie", "admin").ok());
    assert!(add(dir.path(), "paul", "readonly").ok());
    assert!(hearth(dir.path(), &["role", "paul", "admin"], None).ok());
    assert!(hearth(dir.path(), &["passwd", "paul"], Some("Another-Pass-77")).ok());
    assert!(hearth(dir.path(), &["revoke", "paul"], None).ok());
    assert!(hearth(dir.path(), &["remove", "paul"], None).ok());
    // Une commande qui échoue n'écrit rien.
    assert!(!add(dir.path(), "marie", "admin").ok());
    // Une consultation n'est pas journalisée.
    assert!(hearth(dir.path(), &["list"], None).ok());

    let db = Database::open(dir.path()).await.unwrap();
    let rows: Vec<AuditRow> = sqlx::query_as(
        "SELECT action, account, origin_kind, origin_addr, target, outcome, reason
             FROM audit_events ORDER BY id",
    )
    .fetch_all(db.pool())
    .await
    .unwrap();
    db.pool().close().await;
    let actions: Vec<(&str, Option<&str>)> = rows
        .iter()
        .map(|row| (row.0.as_str(), row.4.as_deref()))
        .collect();
    assert_eq!(
        actions,
        [
            ("account.create", Some("marie")),
            ("account.create", Some("paul")),
            ("account.role", Some("paul (Administrateur)")),
            ("account.password", Some("paul")),
            ("sessions.revoke", Some("paul")),
            ("account.delete", Some("paul")),
        ]
    );
    for row in &rows {
        assert_eq!(row.1, None, "pas de compte pour la ligne de commande");
        assert_eq!(row.2, "cli");
        assert_eq!(row.3, None, "pas d'adresse");
        assert_eq!(row.5, "ok");
        assert_eq!(row.6, None);
    }
    let dump = format!("{rows:?}");
    assert!(!dump.contains(PASSWORD) && !dump.contains("Another-Pass-77"));
}

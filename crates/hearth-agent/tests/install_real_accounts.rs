//! HRT-77, BR-GRANT-006, BR-GRANT-007, BR-GRANT-009 : l'octroi du droit « gérer les services » PENDANT
//! l'installation, par le VRAI chemin (`app::local_admins`, l'adaptateur que `install` assemble) sur une
//! vraie base temporaire, sans Docker, sans service système, sans écrire hors du dossier de test.
//!
//! `install_flow.rs` et `install_fs.rs` passent par un faux `AdminAccounts` qui note un booléen : ils
//! prouvent que la réponse arrive jusqu'au port, pas que le port écrit la bonne chose. Ici on lit la base.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "support/tmp.rs"]
mod tmp;

use std::path::Path;

use hearth_agent::app::local_admins;
use hearth_agent::application::ports::{AdminAccounts, AdminCredential};
use hearth_agent::domain::accounts::Username;
use hearth_agent::domain::secret::Secret;
use hearth_agent::infrastructure::sqlite::Database;

const PASSWORD: &str = "Correct-Horse-9";

fn name(text: &str) -> Username {
    Username::parse(text).unwrap()
}

fn password() -> AdminCredential {
    AdminCredential::Password(Secret::from(PASSWORD))
}

/// Le droit stocké du compte, lu par une connexion à part (la base de l'installation est fermée).
async fn right_of(dir: &Path, username: &str) -> Option<String> {
    let database = Database::open(dir).await.unwrap();
    let right = sqlx::query_scalar("SELECT services_right FROM accounts WHERE username = ?")
        .bind(username)
        .fetch_optional(database.pool())
        .await
        .unwrap();
    database.pool().close().await;
    right
}

/// Les entrées du journal qui parlent du droit : (action, origine, cible, résultat).
async fn grant_entries(dir: &Path) -> Vec<(String, String, Option<String>, String)> {
    let database = Database::open(dir).await.unwrap();
    let rows = sqlx::query_as(
        "SELECT action, origin_kind, target, outcome FROM audit_events
         WHERE action LIKE 'account.services.%' ORDER BY id",
    )
    .fetch_all(database.pool())
    .await
    .unwrap();
    database.pool().close().await;
    rows
}

#[tokio::test]
async fn the_first_account_gets_the_right_when_the_installation_asks_for_it_and_it_is_journaled() {
    let dir = tmp::tempdir().unwrap();
    let admins = local_admins(dir.path().to_path_buf()).unwrap();
    admins
        .create_admin(&name("marie"), password(), true)
        .await
        .unwrap();
    assert_eq!(
        right_of(dir.path(), "marie").await.as_deref(),
        Some("manage")
    );
    // Une entrée, de l'origine « ligne de commande du serveur » (le même chemin que la commande).
    assert_eq!(
        grant_entries(dir.path()).await,
        vec![(
            "account.services.grant".to_owned(),
            "cli".to_owned(),
            Some("marie".to_owned()),
            "ok".to_owned()
        )]
    );
}

#[tokio::test]
async fn the_first_account_gets_no_right_when_the_installation_does_not_ask_for_it() {
    let dir = tmp::tempdir().unwrap();
    let admins = local_admins(dir.path().to_path_buf()).unwrap();
    admins
        .create_admin(&name("marie"), password(), false)
        .await
        .unwrap();
    assert_eq!(right_of(dir.path(), "marie").await.as_deref(), Some("none"));
    assert!(grant_entries(dir.path()).await.is_empty());
    // Le compte existe bel et bien (administrateur), le chemin n'a pas été sauté.
    assert_eq!(admins.admin_count().await.unwrap(), 1);
}

/// L'octroi échoue (ici une règle de la base qui refuse toute écriture de la colonne) : l'installation
/// remonte l'erreur, le compte qui vient d'être créé n'est PAS laissé derrière, et aucune entrée
/// « droit accordé » ne subsiste pour un droit qui n'existe pas.
#[tokio::test]
async fn a_failed_grant_leaves_neither_the_account_nor_a_grant_entry() {
    let dir = tmp::tempdir().unwrap();
    let database = Database::open(dir.path()).await.unwrap();
    sqlx::query(
        "CREATE TRIGGER no_right_write BEFORE UPDATE OF services_right ON accounts
         BEGIN SELECT RAISE(ABORT, 'écriture refusée'); END",
    )
    .execute(database.pool())
    .await
    .unwrap();
    database.pool().close().await;

    let admins = local_admins(dir.path().to_path_buf()).unwrap();
    let result = admins.create_admin(&name("marie"), password(), true).await;
    assert!(result.is_err(), "l'installation doit échouer");
    assert_eq!(
        right_of(dir.path(), "marie").await,
        None,
        "compte laissé derrière"
    );
    assert!(grant_entries(dir.path()).await.is_empty());
    assert_eq!(admins.admin_count().await.unwrap(), 0);
}

//! Dépôts SQLite des comptes et des sessions, sur une base temporaire.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use hearth_agent::application::ports::{AccountRepo, SessionRepo, StoreError};
use hearth_agent::domain::accounts::{Account, AccountId, Role, Username};
use hearth_agent::domain::secret::Secret;
use hearth_agent::domain::sessions::{SessionClosure, SessionId};
use hearth_agent::infrastructure::sqlite::{Database, SqliteAccountRepo, SqliteSessionRepo};
use support::{env, start_time};
use time::Duration;

fn account(id: &str, username: &str, role: Role) -> Account {
    Account {
        id: AccountId::new(id),
        username: Username::parse(username).unwrap(),
        role,
        password_hash: Secret::from("$argon2id$fake"),
        created_at: start_time(),
        password_changed_at: start_time(),
        last_login_at: None,
    }
}

async fn insert(repo: &SqliteAccountRepo, account: &Account) {
    let mut tx = repo.begin().await.unwrap();
    tx.insert(account).await.unwrap();
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn an_account_round_trips_through_the_database() {
    let env = env().await;
    let repo = SqliteAccountRepo::new(env.db.pool().clone());
    let mut original = account("A1", "marie", Role::Admin);
    original.last_login_at = Some(start_time() + Duration::hours(3));
    insert(&repo, &original).await;

    let found = repo
        .find_by_id(&AccountId::new("A1"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.username, original.username);
    assert_eq!(found.role, Role::Admin);
    assert_eq!(found.password_hash.expose(), "$argon2id$fake");
    assert_eq!(found.created_at, start_time());
    assert_eq!(found.last_login_at, original.last_login_at);

    let by_name = repo
        .find_by_username(&Username::parse("MARIE").unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(by_name.id.as_str(), "A1");
    assert!(
        repo.find_by_id(&AccountId::new("A2"))
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn list_is_ordered_by_creation() {
    let env = env().await;
    let repo = SqliteAccountRepo::new(env.db.pool().clone());
    let mut late = account("A1", "zoe", Role::Admin);
    late.created_at = start_time() + Duration::hours(1);
    insert(&repo, &late).await;
    insert(&repo, &account("A2", "adam", Role::ReadOnly)).await;

    let names: Vec<_> = repo
        .list()
        .await
        .unwrap()
        .into_iter()
        .map(|a| a.username.to_string())
        .collect();
    assert_eq!(names, ["adam", "zoe"]);
}

#[tokio::test]
async fn the_database_refuses_two_usernames_differing_by_case() {
    let env = env().await;
    let repo = SqliteAccountRepo::new(env.db.pool().clone());
    insert(&repo, &account("A1", "marie", Role::Admin)).await;

    // Même en contournant le domaine : la contrainte de la base tient.
    let direct = sqlx::query(
        "INSERT INTO accounts (id, username, password_hash, role, created_at, password_changed_at) \
         VALUES ('A9', 'MARIE', 'x', 'admin', 'x', 'x')",
    )
    .execute(env.db.pool())
    .await;
    assert!(direct.is_err());

    let mut tx = repo.begin().await.unwrap();
    let duplicate = tx.insert(&account("A2", "marie", Role::ReadOnly)).await;
    assert!(matches!(
        duplicate,
        Err(StoreError::Duplicate {
            resource: "accounts"
        })
    ));
}

#[tokio::test]
async fn a_transaction_dropped_without_commit_changes_nothing() {
    let env = env().await;
    let repo = SqliteAccountRepo::new(env.db.pool().clone());
    insert(&repo, &account("A1", "marie", Role::Admin)).await;

    {
        let mut tx = repo.begin().await.unwrap();
        tx.insert(&account("A2", "paul", Role::Admin))
            .await
            .unwrap();
        tx.set_role(&AccountId::new("A1"), Role::ReadOnly)
            .await
            .unwrap();
        tx.delete(&AccountId::new("A1")).await.unwrap();
        // Abandon : pas de commit.
    }

    let accounts = repo.list().await.unwrap();
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].role, Role::Admin);
    // La base reste utilisable pour l'écrivain suivant.
    let mut tx = repo.begin().await.unwrap();
    assert_eq!(tx.count_admins().await.unwrap(), 1);
}

#[tokio::test]
async fn count_admins_counts_only_administrators() {
    let env = env().await;
    let repo = SqliteAccountRepo::new(env.db.pool().clone());
    insert(&repo, &account("A1", "marie", Role::Admin)).await;
    insert(&repo, &account("A2", "paul", Role::ReadOnly)).await;
    insert(&repo, &account("A3", "zoe", Role::Admin)).await;

    let mut tx = repo.begin().await.unwrap();
    assert_eq!(tx.count_admins().await.unwrap(), 2);
}

#[tokio::test]
async fn set_role_and_set_password_are_visible_after_commit() {
    let env = env().await;
    let repo = SqliteAccountRepo::new(env.db.pool().clone());
    insert(&repo, &account("A1", "marie", Role::ReadOnly)).await;

    let later = start_time() + Duration::days(2);
    let mut tx = repo.begin().await.unwrap();
    tx.set_role(&AccountId::new("A1"), Role::Admin)
        .await
        .unwrap();
    tx.set_password(&AccountId::new("A1"), &Secret::from("$argon2id$new"), later)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let found = repo
        .find_by_id(&AccountId::new("A1"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.role, Role::Admin);
    assert_eq!(found.password_hash.expose(), "$argon2id$new");
    assert_eq!(found.password_changed_at, later);
}

#[tokio::test]
async fn deleting_an_account_deletes_its_sessions_by_cascade() {
    let env = env().await;
    let repo = SqliteAccountRepo::new(env.db.pool().clone());
    let marie = account("A1", "marie", Role::Admin);
    insert(&repo, &marie).await;
    env.insert_session(&marie.id, "S1", Duration::hours(1))
        .await;

    let mut tx = repo.begin().await.unwrap();
    tx.delete(&marie.id).await.unwrap();
    tx.commit().await.unwrap();

    assert!(env.session_ids(&marie.id).await.is_empty());
}

#[tokio::test]
async fn a_session_cannot_reference_an_unknown_account() {
    let env = env().await;
    let result = sqlx::query(
        "INSERT INTO sessions (id, account_id, token_hash, client_name, client_addr, created_at, \
         last_seen_at, expires_at) VALUES ('S1', 'FANTOME', 'h', 'c', 'a', 'x', 'x', 'x')",
    )
    .execute(env.db.pool())
    .await;
    assert!(result.is_err(), "les clés étrangères doivent être actives");
}

#[tokio::test]
async fn session_repo_lists_expiries_and_closes_by_scope() {
    let env = env().await;
    let accounts = SqliteAccountRepo::new(env.db.pool().clone());
    let sessions = SqliteSessionRepo::new(env.db.pool().clone());
    let marie = account("A1", "marie", Role::Admin);
    let paul = account("A2", "paul", Role::ReadOnly);
    insert(&accounts, &marie).await;
    insert(&accounts, &paul).await;
    for id in ["S1", "S2", "S3"] {
        env.insert_session(&marie.id, id, Duration::hours(1)).await;
    }
    env.insert_session(&paul.id, "S4", Duration::hours(1)).await;

    let expiries = sessions.expiries_of(&marie.id).await.unwrap();
    assert_eq!(expiries.len(), 3);
    assert!(
        expiries
            .iter()
            .all(|&e| e == start_time() + Duration::hours(1))
    );

    let closed = sessions
        .close(&marie.id, &SessionClosure::AllExcept(SessionId::new("S2")))
        .await
        .unwrap();
    assert_eq!(closed, 2);
    assert_eq!(env.session_ids(&marie.id).await, ["S2"]);
    assert_eq!(env.session_ids(&paul.id).await, ["S4"]);

    assert_eq!(
        sessions
            .close(&marie.id, &SessionClosure::All)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sessions
            .close(&marie.id, &SessionClosure::All)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn a_corrupt_row_is_reported_with_the_resource_not_hidden() {
    let env = env().await;
    let repo = SqliteAccountRepo::new(env.db.pool().clone());
    sqlx::query(
        "INSERT INTO accounts (id, username, password_hash, role, created_at, password_changed_at) \
         VALUES ('A1', 'marie', 'x', 'admin', 'pas une date', 'pas une date')",
    )
    .execute(env.db.pool())
    .await
    .unwrap();

    let error = repo.list().await.unwrap_err();
    assert!(error.to_string().contains("accounts"), "{error}");
}

#[tokio::test]
async fn the_database_survives_being_reopened() {
    let env = env().await;
    let repo = SqliteAccountRepo::new(env.db.pool().clone());
    insert(&repo, &account("A1", "marie", Role::Admin)).await;
    env.db.pool().close().await;

    let reopened = Database::open(env.dir.path()).await.unwrap();
    let repo = SqliteAccountRepo::new(reopened.pool().clone());
    assert_eq!(repo.list().await.unwrap().len(), 1);
}

#[tokio::test]
async fn list_orders_accounts_created_within_the_same_second() {
    let env = env().await;
    let repo = SqliteAccountRepo::new(env.db.pool().clone());
    let mut half = account("A1", "later", Role::Admin);
    half.created_at = start_time() + Duration::milliseconds(500);
    insert(&repo, &half).await;
    insert(&repo, &account("A2", "first", Role::Admin)).await;

    let names: Vec<_> = repo
        .list()
        .await
        .unwrap()
        .into_iter()
        .map(|a| a.username.to_string())
        .collect();
    assert_eq!(names, ["first", "later"]);
}

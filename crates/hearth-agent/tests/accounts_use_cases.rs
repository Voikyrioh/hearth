//! Cas d'usage des comptes, de bout en bout sur une vraie base SQLite temporaire.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use hearth_agent::application::accounts::AccountError;
use hearth_agent::application::ports::AccountRepo;
use hearth_agent::domain::accounts::{PasswordRule, Role, Username};
use hearth_agent::domain::sessions::SessionId;
use hearth_agent::infrastructure::sqlite::SqliteAccountRepo;
use support::{PASSWORD, env, secret};
use time::Duration;

#[tokio::test]
async fn create_stores_a_hash_and_never_the_password() {
    let env = env().await;
    let account = env.create("marie", Role::Admin).await;
    assert_eq!(account.username.as_str(), "marie");
    assert_eq!(account.role, Role::Admin);
    assert_eq!(account.created_at, support::start_time());
    assert_eq!(account.last_login_at, None);

    let stored: String = sqlx::query_scalar("SELECT password_hash FROM accounts")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert!(stored.starts_with("$argon2id$"));
    assert!(!stored.contains(PASSWORD));
}

#[tokio::test]
async fn create_refuses_an_invalid_username() {
    let env = env().await;
    for bad in ["", "ab", "a b", &"x".repeat(33)] {
        let error = env
            .service
            .create(bad, secret(PASSWORD), Role::Admin)
            .await
            .unwrap_err();
        assert!(matches!(error, AccountError::Username(_)), "{bad:?}");
    }
    assert!(env.service.list().await.unwrap().is_empty());
}

#[tokio::test]
async fn create_lists_every_unmet_password_rule() {
    let env = env().await;
    let error = env
        .service
        .create("marie", secret("abc"), Role::ReadOnly)
        .await
        .unwrap_err();
    let AccountError::WeakPassword(rejected) = error else {
        panic!("attendu : mot de passe refusé");
    };
    assert_eq!(
        rejected.rules,
        vec![
            PasswordRule::MinLength,
            PasswordRule::Digit,
            PasswordRule::Uppercase
        ]
    );
    assert!(env.service.list().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_password_containing_the_username_is_refused() {
    let env = env().await;
    let error = env
        .service
        .create("marie", secret("Hello-Marie-123"), Role::Admin)
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        AccountError::WeakPassword(ref r) if r.rules == vec![PasswordRule::ContainsUsername]
    ));
}

#[tokio::test]
async fn usernames_are_unique_whatever_the_case() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    for duplicate in ["marie", "MARIE", "Marie"] {
        let error = env
            .service
            .create(duplicate, secret(PASSWORD), Role::ReadOnly)
            .await
            .unwrap_err();
        assert!(matches!(error, AccountError::UsernameTaken), "{duplicate}");
        assert_eq!(error.to_string(), "Cet identifiant est déjà utilisé");
    }
    assert_eq!(env.service.list().await.unwrap().len(), 1);
}

#[tokio::test]
async fn two_simultaneous_creations_of_the_same_username_give_one_account() {
    let env = env().await;
    let first = env.service.create("marie", secret(PASSWORD), Role::Admin);
    let second = env.service.create("MARIE", secret(PASSWORD), Role::Admin);
    let (first, second) = tokio::join!(first, second);
    assert_eq!(first.is_ok() as u8 + second.is_ok() as u8, 1);
    assert_eq!(env.service.list().await.unwrap().len(), 1);
}

#[tokio::test]
async fn find_ignores_the_case_and_reports_unknown_accounts() {
    let env = env().await;
    let created = env.create("marie", Role::Admin).await;
    assert_eq!(env.service.find("MARIE").await.unwrap().id, created.id);
    assert!(matches!(
        env.service.find("personne").await,
        Err(AccountError::NotFound)
    ));
}

#[tokio::test]
async fn list_counts_open_sessions_and_ignores_expired_ones() {
    let env = env().await;
    let marie = env.create("marie", Role::Admin).await;
    let paul = env.create("paul", Role::ReadOnly).await;
    env.insert_session(&marie.id, "S1", Duration::hours(1))
        .await;
    env.insert_session(&marie.id, "S2", Duration::hours(2))
        .await;
    env.insert_session(&marie.id, "S3", Duration::hours(-1))
        .await;

    let summaries = env.service.list().await.unwrap();
    assert_eq!(summaries.len(), 2);
    assert_eq!(summaries[0].account.username.as_str(), "marie");
    assert_eq!(summaries[0].sessions_open, 2);
    assert_eq!(summaries[1].account.id, paul.id);
    assert_eq!(summaries[1].sessions_open, 0);

    env.clock.advance(Duration::minutes(90));
    let summaries = env.service.list().await.unwrap();
    assert_eq!(summaries[0].sessions_open, 1);
}

#[tokio::test]
async fn change_role_promotes_and_demotes() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let paul = env.create("paul", Role::ReadOnly).await;

    env.service
        .change_role(&paul.id, Role::Admin)
        .await
        .unwrap();
    assert_eq!(env.service.find("paul").await.unwrap().role, Role::Admin);
    env.service
        .change_role(&paul.id, Role::ReadOnly)
        .await
        .unwrap();
    assert_eq!(env.service.find("paul").await.unwrap().role, Role::ReadOnly);
}

#[tokio::test]
async fn the_last_administrator_cannot_be_removed_nor_demoted() {
    let env = env().await;
    let marie = env.create("marie", Role::Admin).await;
    let paul = env.create("paul", Role::ReadOnly).await;
    env.insert_session(&marie.id, "S1", Duration::hours(1))
        .await;

    let removal = env.service.delete(&marie.id, None, None).await.unwrap_err();
    assert!(matches!(removal, AccountError::LastAdmin(_)));
    assert_eq!(
        removal.to_string(),
        "Il doit toujours rester au moins un administrateur"
    );
    let demotion = env
        .service
        .change_role(&marie.id, Role::ReadOnly)
        .await
        .unwrap_err();
    assert!(matches!(demotion, AccountError::LastAdmin(_)));

    // Rien n'a bougé : compte, rôle et session.
    assert_eq!(env.service.find("marie").await.unwrap().role, Role::Admin);
    assert_eq!(env.session_ids(&marie.id).await, ["S1"]);

    // Avec un second administrateur, la rétrogradation puis la suppression de l'autre passent.
    env.service
        .change_role(&paul.id, Role::Admin)
        .await
        .unwrap();
    env.service
        .change_role(&marie.id, Role::ReadOnly)
        .await
        .unwrap();
    assert!(matches!(
        env.service.delete(&paul.id, None, None).await,
        Err(AccountError::LastAdmin(_))
    ));
    env.service.delete(&marie.id, None, None).await.unwrap();
}

#[tokio::test]
async fn two_simultaneous_removals_of_the_last_two_administrators_leave_one() {
    let env = env().await;
    let marie = env.create("marie", Role::Admin).await;
    let paul = env.create("paul", Role::Admin).await;

    let first = env.service.delete(&marie.id, None, None);
    let second = env.service.delete(&paul.id, None, None);
    let (first, second) = tokio::join!(first, second);

    let refused = [&first, &second]
        .iter()
        .filter(|result| matches!(result, Err(AccountError::LastAdmin(_))))
        .count();
    assert_eq!(refused, 1, "{first:?} {second:?}");
    assert_eq!(first.is_ok() as u8 + second.is_ok() as u8, 1);
    assert_eq!(env.service.list().await.unwrap().len(), 1);
}

#[tokio::test]
async fn an_administrator_password_change_closes_every_session() {
    let env = env().await;
    let marie = env.create("marie", Role::ReadOnly).await;
    let other = env.create("paul", Role::ReadOnly).await;
    env.insert_session(&marie.id, "S1", Duration::hours(1))
        .await;
    env.insert_session(&marie.id, "S2", Duration::hours(1))
        .await;
    env.insert_session(&other.id, "S3", Duration::hours(1))
        .await;
    env.clock.advance(Duration::minutes(5));
    let before = env.hash_of(&marie.id).await;

    let closed = env
        .service
        .set_password(&marie.id, secret("Brand-New-Pass-7"))
        .await
        .unwrap();
    assert_eq!(closed, 2);
    assert!(env.session_ids(&marie.id).await.is_empty());
    assert_eq!(env.session_ids(&other.id).await, ["S3"]);

    let updated = env.service.find("marie").await.unwrap();
    assert_eq!(
        updated.password_changed_at,
        support::start_time() + Duration::minutes(5)
    );
    assert_ne!(env.hash_of(&marie.id).await, before);
}

#[tokio::test]
async fn set_password_applies_the_rules_and_changes_nothing_when_refused() {
    let env = env().await;
    let marie = env.create("marie", Role::Admin).await;
    env.insert_session(&marie.id, "S1", Duration::hours(1))
        .await;

    let before = env.hash_of(&marie.id).await;
    let error = env
        .service
        .set_password(&marie.id, secret("short"))
        .await
        .unwrap_err();
    assert!(matches!(error, AccountError::WeakPassword(_)));
    assert_eq!(env.session_ids(&marie.id).await, ["S1"]);
    assert_eq!(env.hash_of(&marie.id).await, before);
}

#[tokio::test]
async fn changing_your_own_password_keeps_only_the_current_session() {
    let env = env().await;
    let marie = env.create("marie", Role::ReadOnly).await;
    env.insert_session(&marie.id, "S1", Duration::hours(1))
        .await;
    env.insert_session(&marie.id, "S2", Duration::hours(1))
        .await;
    env.insert_session(&marie.id, "S3", Duration::hours(1))
        .await;

    let closed = env
        .service
        .change_own_password(
            &marie.id,
            secret(PASSWORD),
            secret("Brand-New-Pass-7"),
            Some(SessionId::new("S2")),
        )
        .await
        .unwrap();
    assert_eq!(closed, 2);
    assert_eq!(env.session_ids(&marie.id).await, ["S2"]);

    // Le nouveau mot de passe est celui qui sert pour la suite.
    let again = env
        .service
        .change_own_password(
            &marie.id,
            secret("Brand-New-Pass-7"),
            secret("Another-Pass-88"),
            None,
        )
        .await
        .unwrap();
    assert_eq!(again, 1);
}

#[tokio::test]
async fn a_wrong_old_password_changes_nothing() {
    let env = env().await;
    let marie = env.create("marie", Role::ReadOnly).await;
    env.insert_session(&marie.id, "S1", Duration::hours(1))
        .await;
    env.insert_session(&marie.id, "S2", Duration::hours(1))
        .await;

    let before = env.hash_of(&marie.id).await;
    let error = env
        .service
        .change_own_password(
            &marie.id,
            secret("Not-The-Password-1"),
            secret("Brand-New-Pass-7"),
            Some(SessionId::new("S1")),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, AccountError::OldPasswordIncorrect));
    assert_eq!(error.to_string(), "L'ancien mot de passe est incorrect");
    assert_eq!(env.session_ids(&marie.id).await, ["S1", "S2"]);
    assert_eq!(env.hash_of(&marie.id).await, before);
}

#[tokio::test]
async fn the_old_password_is_not_subject_to_the_complexity_rules() {
    use argon2::PasswordHasher as _;

    let env = env().await;
    let marie = env.create("marie", Role::ReadOnly).await;
    // Un mot de passe défini sous d'anciennes règles : son haché est placé directement en base.
    let weak_hash = argon2::Argon2::default()
        .hash_password(b"weak")
        .unwrap()
        .to_string();
    sqlx::query("UPDATE accounts SET password_hash = ? WHERE id = ?")
        .bind(&weak_hash)
        .bind(marie.id.as_str())
        .execute(env.db.pool())
        .await
        .unwrap();

    env.service
        .change_own_password(&marie.id, secret("weak"), secret("Brand-New-Pass-7"), None)
        .await
        .unwrap();
}

#[tokio::test]
async fn removing_an_account_closes_its_sessions() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let paul = env.create("paul", Role::ReadOnly).await;
    env.insert_session(&paul.id, "S1", Duration::hours(1)).await;
    env.insert_session(&paul.id, "S2", Duration::hours(1)).await;

    let closed = env.service.delete(&paul.id, None, None).await.unwrap();
    assert_eq!(closed, 2);
    assert!(env.session_ids(&paul.id).await.is_empty());
    assert!(matches!(
        env.service.find("paul").await,
        Err(AccountError::NotFound)
    ));
}

#[tokio::test]
async fn removing_an_unknown_account_is_not_found() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let ghost = hearth_agent::domain::accounts::AccountId::new("INCONNU");
    assert!(matches!(
        env.service.delete(&ghost, None, None).await,
        Err(AccountError::NotFound)
    ));
    assert!(matches!(
        env.service.set_password(&ghost, secret(PASSWORD)).await,
        Err(AccountError::NotFound)
    ));
    assert!(matches!(
        env.service.revoke_sessions(&ghost).await,
        Err(AccountError::NotFound)
    ));
    assert!(matches!(
        env.service.change_role(&ghost, Role::Admin).await,
        Err(AccountError::NotFound)
    ));
}

#[tokio::test]
async fn revoking_closes_sessions_and_keeps_the_password() {
    let env = env().await;
    let marie = env.create("marie", Role::Admin).await;
    env.insert_session(&marie.id, "S1", Duration::hours(1))
        .await;
    env.insert_session(&marie.id, "S2", Duration::hours(1))
        .await;

    let before = env.hash_of(&marie.id).await;
    assert_eq!(env.service.revoke_sessions(&marie.id).await.unwrap(), 2);
    assert!(env.session_ids(&marie.id).await.is_empty());
    assert_eq!(env.service.revoke_sessions(&marie.id).await.unwrap(), 0);

    let after = env.service.find("marie").await.unwrap();
    assert_eq!(env.hash_of(&marie.id).await, before);
    assert_eq!(after.password_changed_at, marie.password_changed_at);
}

#[tokio::test]
async fn deleting_your_own_account_requires_your_username() {
    let env = env().await;
    let marie = env.create("marie", Role::Admin).await;
    env.create("paul", Role::Admin).await;

    for wrong in [None, Some(""), Some("paul"), Some("mari")] {
        let error = env
            .service
            .delete(&marie.id, Some(&marie.id), wrong)
            .await
            .unwrap_err();
        assert!(matches!(error, AccountError::SelfDeletion(_)), "{wrong:?}");
        assert_eq!(
            error.to_string(),
            "L'identifiant ne correspond pas, réessaye"
        );
    }
    assert_eq!(env.service.list().await.unwrap().len(), 2);

    env.service
        .delete(&marie.id, Some(&marie.id), Some("MARIE"))
        .await
        .unwrap();
    assert_eq!(env.service.list().await.unwrap().len(), 1);
}

#[tokio::test]
async fn deleting_someone_else_needs_no_confirmation() {
    let env = env().await;
    let marie = env.create("marie", Role::Admin).await;
    let paul = env.create("paul", Role::ReadOnly).await;
    env.service
        .delete(&paul.id, Some(&marie.id), None)
        .await
        .unwrap();
}

#[tokio::test]
async fn errors_never_contain_the_password() {
    let env = env().await;
    let marie = env.create("marie", Role::Admin).await;
    let attempts = ["Zz9-secret-marie-Zz9", "tooshort", "nodigitsHEREmarie"];

    let mut messages = Vec::new();
    for attempt in attempts {
        let error = env
            .service
            .set_password(&marie.id, secret(attempt))
            .await
            .unwrap_err();
        messages.push(format!("{error} {error:?}"));
        let error = env
            .service
            .change_own_password(&marie.id, secret(attempt), secret(attempt), None)
            .await
            .unwrap_err();
        messages.push(format!("{error} {error:?}"));
    }
    for message in &messages {
        for attempt in attempts {
            assert!(!message.contains(attempt), "{message}");
        }
        assert!(!message.contains(PASSWORD), "{message}");
    }
}

#[tokio::test]
async fn the_repository_reads_what_the_service_wrote() {
    let env = env().await;
    let created = env.create("marie", Role::Admin).await;
    let repo = SqliteAccountRepo::new(env.db.pool().clone());
    let found = repo
        .find_by_username(&Username::parse("marie").unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.id, created.id);
    assert_eq!(found.password_hash.expose(), env.hash_of(&created.id).await);
}

/// Hacheur qui simule un changement concurrent du mot de passe, juste après la vérification de
/// l'ancien : le haché en base n'est plus celui qui a été vérifié.
struct RacingHasher {
    inner: hearth_agent::infrastructure::argon2::Argon2Hasher,
    pool: sqlx::SqlitePool,
}

#[async_trait::async_trait]
impl hearth_agent::application::ports::PasswordHasher for RacingHasher {
    async fn hash(
        &self,
        password: &hearth_agent::domain::accounts::PlainPassword,
    ) -> Result<hearth_agent::domain::secret::Secret, hearth_agent::application::ports::HashError>
    {
        self.inner.hash(password).await
    }

    async fn verify(
        &self,
        password: &hearth_agent::domain::secret::Secret,
        hash: &hearth_agent::domain::secret::Secret,
    ) -> Result<bool, hearth_agent::application::ports::HashError> {
        let verified = self.inner.verify(password, hash).await?;
        sqlx::query("UPDATE accounts SET password_hash = 'concurrent-change'")
            .execute(&self.pool)
            .await
            .unwrap();
        Ok(verified)
    }
}

#[tokio::test]
async fn a_password_changed_between_verification_and_write_is_not_overwritten() {
    use std::sync::Arc;

    use hearth_agent::application::accounts::AccountService;
    use hearth_agent::infrastructure::sqlite::{SqliteSessionRepo, SqliteStore};

    let env = env().await;
    let marie = env.create("marie", Role::ReadOnly).await;
    env.insert_session(&marie.id, "S1", Duration::hours(1))
        .await;
    let pool = env.db.pool().clone();
    let racing = AccountService::new(
        Arc::new(SqliteAccountRepo::new(pool.clone())),
        Arc::new(SqliteSessionRepo::new(pool.clone())),
        Arc::new(SqliteStore::new(pool.clone())),
        Arc::new(RacingHasher {
            inner: hearth_agent::infrastructure::argon2::Argon2Hasher::with_cost(8, 1, 1).unwrap(),
            pool: pool.clone(),
        }),
        env.clock.clone(),
        Arc::new(support::SequentialIds::starting_at(100)),
    );

    let error = racing
        .change_own_password(
            &marie.id,
            secret(PASSWORD),
            secret("Brand-New-Pass-7"),
            None,
        )
        .await
        .unwrap_err();
    assert!(matches!(error, AccountError::PasswordChangedMeanwhile));
    assert_eq!(
        error.to_string(),
        "Le mot de passe a été modifié entre-temps, réessaie"
    );
    let stored: String = sqlx::query_scalar("SELECT password_hash FROM accounts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored, "concurrent-change");
    assert_eq!(env.session_ids(&marie.id).await, ["S1"]);
}

#[test]
fn an_invalid_username_is_refused_without_touching_anything() {
    use hearth_agent::application::accounts::AccountService;
    assert!(AccountService::validate_username("marie").is_ok());
    assert!(AccountService::validate_username("a b").is_err());
    assert!(AccountService::validate_username("").is_err());
}

#[tokio::test]
async fn the_date_returned_by_create_is_the_one_read_back_by_find() {
    use std::sync::Arc;

    use hearth_agent::application::accounts::AccountService;
    use hearth_agent::infrastructure::argon2::Argon2Hasher;
    use hearth_agent::infrastructure::clock::SystemClock;
    use hearth_agent::infrastructure::ids::UlidGen;
    use hearth_agent::infrastructure::sqlite::{SqliteSessionRepo, SqliteStore};

    let env = env().await;
    let service = AccountService::new(
        Arc::new(SqliteAccountRepo::new(env.db.pool().clone())),
        Arc::new(SqliteSessionRepo::new(env.db.pool().clone())),
        Arc::new(SqliteStore::new(env.db.pool().clone())),
        Arc::new(Argon2Hasher::with_cost(8, 1, 1).unwrap()),
        Arc::new(SystemClock),
        Arc::new(UlidGen),
    );
    let created = service
        .create("marie", secret(PASSWORD), Role::Admin)
        .await
        .unwrap();
    let found = service.find("marie").await.unwrap();
    assert_eq!(created.created_at, found.created_at);
    assert_eq!(created.password_changed_at, found.password_changed_at);
}

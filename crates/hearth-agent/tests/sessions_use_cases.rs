//! Cas d'usage des sessions, de bout en bout sur une vraie base SQLite temporaire : connexion,
//! verrouillage, expiration glissante, révocation, purge, suivi des opérations.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use hearth_agent::application::operations::Begin;
use hearth_agent::application::ports::Clock;
use hearth_agent::application::sessions::{AuthError, LoginError};
use hearth_agent::domain::accounts::Role;
use hearth_agent::domain::operations::OperationKey;
use hearth_agent::domain::sessions::{LIFETIME, SessionEnd};
use support::{PASSWORD, client, client_at, env, secret};
use time::Duration;

const WRONG: &str = "Wrong-Horse-9999";

async fn login_ok(
    env: &support::Env,
    username: &str,
) -> hearth_agent::application::sessions::LoginOutcome {
    env.sessions
        .login(username, secret(PASSWORD), &client())
        .await
        .expect("connexion")
}

#[tokio::test]
async fn a_login_creates_a_session_and_records_the_login_in_one_transaction() {
    let env = env().await;
    let marie = env.create("marie", Role::Admin).await;
    env.clock.advance(Duration::hours(2));

    let outcome = login_ok(&env, "Marie").await;

    let now = env.clock.now();
    assert_eq!(outcome.expires_at, now + LIFETIME);
    assert_eq!(outcome.account.id, marie.id);
    assert_eq!(outcome.account.last_login_at, Some(now));
    let stored = env.service.find("marie").await.unwrap();
    assert_eq!(stored.last_login_at, Some(now), "relu en base");
    assert_eq!(
        env.session_ids(&marie.id).await,
        [outcome.session_id.as_str()]
    );
}

#[tokio::test]
async fn the_token_is_never_stored_only_its_hash() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let outcome = login_ok(&env, "marie").await;
    let token = outcome.token.encode();
    assert_eq!(token.len(), 64);

    let row: (String, String, String, String) =
        sqlx::query_as("SELECT id, token_hash, client_name, client_addr FROM sessions")
            .fetch_one(env.db.pool())
            .await
            .unwrap();
    assert_eq!(row.1.len(), 64);
    assert_ne!(row.1, token);
    for column in [&row.0, &row.1, &row.2, &row.3] {
        assert!(!column.contains(&token));
    }
    assert_eq!(row.2, "poste/1.0");
    assert_eq!(row.3, support::CLIENT_ADDR);
    assert_eq!(row.1, outcome.token.hash().to_hex());
}

#[tokio::test]
async fn two_logins_give_two_distinct_sessions_and_tokens() {
    let env = env().await;
    let marie = env.create("marie", Role::Admin).await;
    let first = login_ok(&env, "marie").await;
    let second = login_ok(&env, "marie").await;
    assert_ne!(first.token.encode(), second.token.encode());
    assert_eq!(env.session_ids(&marie.id).await.len(), 2);
}

#[tokio::test]
async fn unknown_username_and_wrong_password_take_the_same_path() {
    let env = env().await;
    env.create("marie", Role::Admin).await;

    let wrong = env
        .sessions
        .login("marie", secret(WRONG), &client())
        .await
        .unwrap_err();
    assert!(matches!(wrong, LoginError::InvalidCredentials), "{wrong:?}");
    assert_eq!(env.hasher.verifications(), 1);
    assert_eq!(env.hasher.against_decoy(), 0);

    for unknown in ["fantome", "x"] {
        let before = (env.hasher.verifications(), env.hasher.against_decoy());
        let error = env
            .sessions
            .login(unknown, secret(WRONG), &client())
            .await
            .unwrap_err();
        assert!(matches!(error, LoginError::InvalidCredentials), "{error:?}");
        assert_eq!(error.to_string(), wrong.to_string());
        // Une vérification de plus, contre le haché factice : même appel au hacheur.
        assert_eq!(env.hasher.verifications(), before.0 + 1, "{unknown}");
        assert_eq!(env.hasher.against_decoy(), before.1 + 1, "{unknown}");
    }

    // Même écriture : un compteur par couple, que l'identifiant existe ou non.
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM login_attempts")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert_eq!(rows, 3);
    let sessions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert_eq!(sessions, 0);
}

#[tokio::test]
async fn a_failed_login_leaves_no_session_and_no_last_login() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let _ = env.sessions.login("marie", secret(WRONG), &client()).await;
    assert_eq!(env.service.find("marie").await.unwrap().last_login_at, None);
}

#[tokio::test]
async fn five_failures_lock_for_one_minute_then_the_wait_doubles_up_to_fifteen() {
    let env = env().await;
    env.create("marie", Role::Admin).await;

    for _ in 0..4 {
        let error = env
            .sessions
            .login("marie", secret(WRONG), &client())
            .await
            .unwrap_err();
        assert!(matches!(error, LoginError::InvalidCredentials));
    }
    let fifth = env
        .sessions
        .login("marie", secret(WRONG), &client())
        .await
        .unwrap_err();
    assert!(
        matches!(fifth, LoginError::TooManyAttempts { retry_after } if retry_after == Duration::seconds(60)),
        "{fifth:?}"
    );

    // Pendant l'attente : refusé sans vérifier le mot de passe, même le bon.
    let verifications = env.hasher.verifications();
    env.clock.advance(Duration::seconds(10));
    let blocked = env
        .sessions
        .login("marie", secret(PASSWORD), &client())
        .await
        .unwrap_err();
    assert!(
        matches!(blocked, LoginError::TooManyAttempts { retry_after } if retry_after == Duration::seconds(50)),
        "{blocked:?}"
    );
    assert_eq!(env.hasher.verifications(), verifications);

    // Paliers suivants : 2, 4, 8, puis plafond 15 minutes.
    for expected in [120, 240, 480, 900, 900] {
        env.clock.advance(Duration::minutes(20));
        let error = env
            .sessions
            .login("marie", secret(WRONG), &client())
            .await
            .unwrap_err();
        assert!(
            matches!(error, LoginError::TooManyAttempts { retry_after } if retry_after == Duration::seconds(expected)),
            "{expected} : {error:?}"
        );
    }

    // Après l'attente, le bon mot de passe passe et remet tout à zéro.
    env.clock.advance(Duration::minutes(20));
    login_ok(&env, "marie").await;
    for _ in 0..4 {
        let error = env
            .sessions
            .login("marie", secret(WRONG), &client())
            .await
            .unwrap_err();
        assert!(matches!(error, LoginError::InvalidCredentials));
    }
}

#[tokio::test]
async fn the_lockout_is_per_username_and_address() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    env.create("paul", Role::Admin).await;
    for _ in 0..5 {
        let _ = env.sessions.login("marie", secret(WRONG), &client()).await;
    }
    let locked = env
        .sessions
        .login("marie", secret(PASSWORD), &client())
        .await
        .unwrap_err();
    assert!(matches!(locked, LoginError::TooManyAttempts { .. }));

    env.sessions
        .login("marie", secret(PASSWORD), &client_at("10.0.0.99"))
        .await
        .expect("autre adresse");
    env.sessions
        .login("paul", secret(PASSWORD), &client())
        .await
        .expect("autre identifiant");
}

#[tokio::test]
async fn an_unknown_username_locks_like_a_known_one() {
    let env = env().await;
    for _ in 0..5 {
        let _ = env
            .sessions
            .login("fantome", secret(WRONG), &client())
            .await;
    }
    let error = env
        .sessions
        .login("fantome", secret(WRONG), &client())
        .await
        .unwrap_err();
    assert!(
        matches!(error, LoginError::TooManyAttempts { .. }),
        "{error:?}"
    );
}

#[tokio::test]
async fn an_authenticated_request_finds_the_account_of_the_token() {
    let env = env().await;
    let marie = env.create("marie", Role::ReadOnly).await;
    let outcome = login_ok(&env, "marie").await;
    let current = env
        .sessions
        .authenticate(&outcome.token.encode())
        .await
        .unwrap();
    assert_eq!(current.account.id, marie.id);
    assert_eq!(current.account.role, Role::ReadOnly);
    assert_eq!(current.session_id, outcome.session_id);
}

#[tokio::test]
async fn a_missing_or_unreadable_token_is_malformed_and_an_unknown_one_is_expired() {
    let env = env().await;
    for bad in ["", "abc", &"z".repeat(64)] {
        let error = env.sessions.authenticate(bad).await.unwrap_err();
        assert!(matches!(error, AuthError::Malformed), "{bad:?}");
    }
    let unknown = env
        .sessions
        .authenticate(&"ab".repeat(32))
        .await
        .unwrap_err();
    assert!(matches!(unknown, AuthError::Ended(SessionEnd::Expired)));
}

#[tokio::test]
async fn a_session_expires_after_thirty_days_without_activity() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let token = login_ok(&env, "marie").await.token.encode();

    env.clock.advance(LIFETIME - Duration::seconds(1));
    env.sessions
        .authenticate(&token)
        .await
        .expect("encore ouverte");

    env.clock.advance(LIFETIME + Duration::seconds(1));
    let error = env.sessions.authenticate(&token).await.unwrap_err();
    assert!(
        matches!(error, AuthError::Ended(SessionEnd::Expired)),
        "{error:?}"
    );
}

#[tokio::test]
async fn activity_slides_the_expiry_forward() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let token = login_ok(&env, "marie").await.token.encode();

    // Une visite tous les 20 jours : la session ne s'éteint jamais.
    for _ in 0..5 {
        env.clock.advance(Duration::days(20));
        let current = env.sessions.authenticate(&token).await.expect("ouverte");
        assert_eq!(current.expires_at, env.clock.now() + LIFETIME);
    }
    // Puis 31 jours sans activité.
    env.clock.advance(Duration::days(31));
    assert!(env.sessions.authenticate(&token).await.is_err());
}

#[tokio::test]
async fn the_expiry_is_written_at_most_once_per_renewal_interval() {
    let env = env().await;
    let marie = env.create("marie", Role::Admin).await;
    let outcome = login_ok(&env, "marie").await;
    let token = outcome.token.encode();

    env.clock.advance(Duration::minutes(1));
    let current = env.sessions.authenticate(&token).await.unwrap();
    assert_eq!(current.expires_at, outcome.expires_at, "rien d'écrit");

    env.clock.advance(Duration::minutes(5));
    let current = env.sessions.authenticate(&token).await.unwrap();
    assert_eq!(current.expires_at, env.clock.now() + LIFETIME);
    let stored: String = sqlx::query_scalar("SELECT expires_at FROM sessions WHERE account_id = ?")
        .bind(marie.id.as_str())
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    let stored =
        time::OffsetDateTime::parse(&stored, &time::format_description::well_known::Rfc3339)
            .unwrap();
    assert_eq!(stored, env.clock.now() + LIFETIME);
}

#[tokio::test]
async fn logout_deletes_the_session_without_a_revocation_trace() {
    let env = env().await;
    let marie = env.create("marie", Role::Admin).await;
    let outcome = login_ok(&env, "marie").await;
    env.sessions.logout(&outcome.session_id).await.unwrap();
    assert!(env.session_ids(&marie.id).await.is_empty());
    let error = env
        .sessions
        .authenticate(&outcome.token.encode())
        .await
        .unwrap_err();
    assert!(matches!(error, AuthError::Ended(SessionEnd::Expired)));
}

#[tokio::test]
async fn closing_sessions_leaves_a_revocation_trace_whatever_the_cause() {
    let env = env().await;
    let marie = env.create("marie", Role::Admin).await;
    let paul = env.create("paul", Role::ReadOnly).await;
    let marie_token = login_ok(&env, "marie").await.token.encode();
    let paul_token = login_ok(&env, "paul").await.token.encode();

    // Mot de passe changé par un administrateur.
    env.service
        .set_password(&marie.id, secret("Another-Pass-77"))
        .await
        .unwrap();
    let error = env.sessions.authenticate(&marie_token).await.unwrap_err();
    assert!(
        matches!(error, AuthError::Ended(SessionEnd::Revoked)),
        "{error:?}"
    );

    // Suppression du compte : le jeton est révoqué, pas « expiré ».
    env.service.delete(&paul.id, None, None).await.unwrap();
    let error = env.sessions.authenticate(&paul_token).await.unwrap_err();
    assert!(
        matches!(error, AuthError::Ended(SessionEnd::Revoked)),
        "{error:?}"
    );
}

#[tokio::test]
async fn revoking_sessions_closes_them_and_changing_my_password_keeps_the_current_one() {
    let env = env().await;
    let marie = env.create("marie", Role::Admin).await;
    let kept = login_ok(&env, "marie").await;
    let other = login_ok(&env, "marie").await;

    env.service
        .change_own_password(
            &marie.id,
            secret(PASSWORD),
            secret("Another-Pass-77"),
            Some(kept.session_id.clone()),
        )
        .await
        .unwrap();
    env.sessions
        .authenticate(&kept.token.encode())
        .await
        .expect("gardée");
    let error = env
        .sessions
        .authenticate(&other.token.encode())
        .await
        .unwrap_err();
    assert!(matches!(error, AuthError::Ended(SessionEnd::Revoked)));

    env.service.revoke_sessions(&marie.id).await.unwrap();
    let error = env
        .sessions
        .authenticate(&kept.token.encode())
        .await
        .unwrap_err();
    assert!(matches!(error, AuthError::Ended(SessionEnd::Revoked)));
}

#[tokio::test]
async fn the_purge_removes_expired_sessions_old_revocations_idle_counters_and_old_operations() {
    let env = env().await;
    let marie = env.create("marie", Role::Admin).await;
    let paul = env.create("paul", Role::ReadOnly).await;
    let stays = login_ok(&env, "marie").await;
    env.insert_session(&paul.id, "OLD", Duration::hours(1))
        .await;
    let _ = env
        .sessions
        .login("marie", secret(WRONG), &client_at("10.9.9.9"))
        .await;
    let key = OperationKey::parse("OLDKEY").unwrap();
    assert!(matches!(
        env.operations
            .begin(&key, &marie.id, "PUT /x")
            .await
            .unwrap(),
        Begin::Execute
    ));
    env.service.revoke_sessions(&paul.id).await.unwrap();

    // 2 h plus tard : la session de test (1 h) est expirée, rien d'autre n'est périmé.
    env.clock.advance(Duration::hours(2));
    let report = env.maintenance.purge().await.unwrap();
    assert_eq!(
        report.sessions, 0,
        "paul n'a plus de session : elle a été révoquée"
    );
    assert_eq!(report.operations, 0);
    assert_eq!(report.login_attempts, 0);
    assert_eq!(report.revocations, 0);

    env.clock.advance(Duration::days(2));
    let report = env.maintenance.purge().await.unwrap();
    assert_eq!(
        report.login_attempts, 2,
        "compteurs inactifs depuis plus de 24 h"
    );
    assert_eq!(report.operations, 1);
    assert_eq!(report.sessions, 0);
    assert!(
        env.sessions
            .authenticate(&stays.token.encode())
            .await
            .is_ok()
    );

    env.clock.advance(Duration::days(91));
    let report = env.maintenance.purge().await.unwrap();
    assert_eq!(report.revocations, 1);
    assert_eq!(report.sessions, 1, "la session de marie, expirée depuis");
}

#[tokio::test]
async fn an_operation_key_runs_once_and_replays_its_result() {
    let env = env().await;
    let marie = env.create("marie", Role::Admin).await;
    let paul = env.create("paul", Role::Admin).await;
    let key = OperationKey::parse("01J9ZY0G3Q8M2K6W4T7V5N1B9D").unwrap();

    assert!(matches!(
        env.operations
            .begin(&key, &marie.id, "PUT /me/password")
            .await
            .unwrap(),
        Begin::Execute
    ));
    assert!(matches!(
        env.operations
            .begin(&key, &marie.id, "PUT /me/password")
            .await
            .unwrap(),
        Begin::InProgress
    ));
    assert!(matches!(
        env.operations
            .begin(&key, &paul.id, "PUT /me/password")
            .await
            .unwrap(),
        Begin::ForeignKey
    ));
    assert!(env.operations.find(&key, &paul.id).await.unwrap().is_none());

    env.operations
        .finish(&key, true, r#"{"status":200,"body":{}}"#)
        .await
        .unwrap();
    let Begin::Replay(replayed) = env
        .operations
        .begin(&key, &marie.id, "PUT /me/password")
        .await
        .unwrap()
    else {
        panic!("attendu : rejeu");
    };
    assert_eq!(
        replayed.result_json.as_deref(),
        Some(r#"{"status":200,"body":{}}"#)
    );
    let found = env.operations.find(&key, &marie.id).await.unwrap().unwrap();
    assert_eq!(found.kind, "PUT /me/password");

    env.operations.discard(&key).await.unwrap();
    assert!(
        env.operations
            .find(&key, &marie.id)
            .await
            .unwrap()
            .is_none()
    );
}

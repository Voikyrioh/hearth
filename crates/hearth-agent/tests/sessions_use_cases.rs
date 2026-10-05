//! Cas d'usage des sessions, de bout en bout sur une vraie base SQLite temporaire : connexion,
//! verrouillage, expiration glissante, révocation, purge, suivi des opérations.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use hearth_agent::application::ports::Clock;
use hearth_agent::application::sessions::{AuthError, LoginError};
use hearth_agent::domain::accounts::Role;
use hearth_agent::domain::operations::{OperationKey, OperationStatus, Replay, RequestFingerprint};
use hearth_agent::domain::sessions::{LIFETIME, SessionEnd};
use support::{PASSWORD, by, client, client_at, env, secret};
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
    assert_eq!(rows, 4, "trois couples et une adresse");
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
    env.sessions
        .logout(&outcome.session_id, by())
        .await
        .unwrap();
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
        .set_password(&marie.id, secret("Another-Pass-77"), by())
        .await
        .unwrap();
    let error = env.sessions.authenticate(&marie_token).await.unwrap_err();
    assert!(
        matches!(error, AuthError::Ended(SessionEnd::Revoked)),
        "{error:?}"
    );

    // Suppression du compte : le jeton est révoqué, pas « expiré ».
    env.service
        .delete(&paul.id, None, None, by())
        .await
        .unwrap();
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
            by(),
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

    env.service.revoke_sessions(&marie.id, by()).await.unwrap();
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
    let request = RequestFingerprint::of("PUT", "/x", b"");
    assert_eq!(
        env.operations
            .begin(&key, &marie.id, "PUT /x", &request)
            .await
            .unwrap(),
        Replay::Execute
    );
    env.service.revoke_sessions(&paul.id, by()).await.unwrap();

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
        report.login_attempts, 3,
        "deux couples et une adresse, inactifs depuis plus de 24 h"
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
    let key = OperationKey::parse("01J9ZY0G3Q8M2K6W4T7V5N1B9D").unwrap();
    let request = RequestFingerprint::of("PUT", "/me/password", b"{}");
    let begin = || {
        env.operations
            .begin(&key, &marie.id, "PUT /me/password", &request)
    };

    assert_eq!(begin().await.unwrap(), Replay::Execute);
    assert_eq!(begin().await.unwrap(), Replay::InProgress);

    env.operations
        .finish(&marie.id, &key, true, r#"{"status":200,"body":{}}"#)
        .await
        .unwrap();
    let Replay::Return(replayed) = begin().await.unwrap() else {
        panic!("attendu : rejeu");
    };
    assert_eq!(
        replayed.result_json.as_deref(),
        Some(r#"{"status":200,"body":{}}"#)
    );
    assert_eq!(replayed.status, OperationStatus::Succeeded);
    let found = env.operations.find(&key, &marie.id).await.unwrap().unwrap();
    assert_eq!(found.kind, "PUT /me/password");

    env.operations.discard(&marie.id, &key).await.unwrap();
    assert!(
        env.operations
            .find(&key, &marie.id)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn a_key_is_bound_to_its_request_and_scoped_by_account() {
    let env = env().await;
    let marie = env.create("marie", Role::Admin).await;
    let paul = env.create("paul", Role::Admin).await;
    let key = OperationKey::parse("SHARED").unwrap();
    let put = RequestFingerprint::of("PUT", "/accounts/B/password", b"{}");
    let delete = RequestFingerprint::of("DELETE", "/accounts/B", b"");

    assert_eq!(
        env.operations
            .begin(&key, &marie.id, "PUT", &put)
            .await
            .unwrap(),
        Replay::Execute
    );
    env.operations
        .finish(&marie.id, &key, true, r#"{"status":200,"body":null}"#)
        .await
        .unwrap();
    // Même clé, autre requête : refus, rien n'est enregistré ni exécuté.
    assert_eq!(
        env.operations
            .begin(&key, &marie.id, "DELETE", &delete)
            .await
            .unwrap(),
        Replay::KeyReused
    );
    // Un autre compte peut choisir la même clé : elle est à lui, indépendante.
    assert_eq!(
        env.operations
            .begin(&key, &paul.id, "DELETE", &delete)
            .await
            .unwrap(),
        Replay::Execute
    );
    assert!(
        env.operations
            .find(&key, &marie.id)
            .await
            .unwrap()
            .is_some()
    );
    let own = env.operations.find(&key, &paul.id).await.unwrap().unwrap();
    assert_eq!(own.status, OperationStatus::Running);
}

#[tokio::test]
async fn running_operations_become_interrupted_at_startup_and_are_not_replayed() {
    let env = env().await;
    let marie = env.create("marie", Role::Admin).await;
    let running = OperationKey::parse("RUNNING").unwrap();
    let done = OperationKey::parse("DONE").unwrap();
    let request = RequestFingerprint::of("PUT", "/x", b"");
    env.operations
        .begin(&running, &marie.id, "PUT /x", &request)
        .await
        .unwrap();
    env.operations
        .begin(&done, &marie.id, "PUT /x", &request)
        .await
        .unwrap();
    env.operations
        .finish(&marie.id, &done, true, r#"{"status":200,"body":null}"#)
        .await
        .unwrap();

    assert_eq!(env.operations.interrupt_running().await.unwrap(), 1);
    let interrupted = env
        .operations
        .find(&running, &marie.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(interrupted.status, OperationStatus::Interrupted);
    assert!(interrupted.finished_at.is_some());
    assert_eq!(
        env.operations
            .find(&done, &marie.id)
            .await
            .unwrap()
            .unwrap()
            .status,
        OperationStatus::Succeeded
    );
    assert_eq!(
        env.operations
            .begin(&running, &marie.id, "PUT /x", &request)
            .await
            .unwrap(),
        Replay::Interrupted
    );
    assert_eq!(env.operations.interrupt_running().await.unwrap(), 0);
}

#[tokio::test]
async fn nine_simultaneous_wrong_logins_make_exactly_five_verifications() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let mut tasks = Vec::new();
    // Neuf : une traitée et huit en attente, le plafond d'une adresse.
    for _ in 0..9 {
        let sessions = env.sessions.clone();
        tasks.push(tokio::spawn(async move {
            sessions.login("marie", secret(WRONG), &client()).await
        }));
    }
    let mut invalid = 0;
    let mut locked = 0;
    for task in tasks {
        match task.await.unwrap().unwrap_err() {
            LoginError::InvalidCredentials => invalid += 1,
            LoginError::TooManyAttempts { .. } => locked += 1,
            other => panic!("{other:?}"),
        }
    }
    assert_eq!(
        env.hasher.verifications(),
        5,
        "les autres sont bloquées avant vérification"
    );
    assert_eq!((invalid, locked), (4, 5));
}

#[tokio::test]
async fn an_address_trying_many_usernames_is_blocked_after_twenty_failures() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    for index in 0..19 {
        let error = env
            .sessions
            .login(&format!("inconnu{index}"), secret(WRONG), &client())
            .await
            .unwrap_err();
        assert!(
            matches!(error, LoginError::InvalidCredentials),
            "{index} : {error:?}"
        );
    }
    let twentieth = env
        .sessions
        .login("inconnu19", secret(WRONG), &client())
        .await
        .unwrap_err();
    assert!(
        matches!(twentieth, LoginError::TooManyAttempts { retry_after } if retry_after == Duration::seconds(60)),
        "{twentieth:?}"
    );

    // L'adresse est bloquée pour tous les identifiants, même un compte réel avec le bon mot de passe.
    let verifications = env.hasher.verifications();
    let blocked = env
        .sessions
        .login("marie", secret(PASSWORD), &client())
        .await
        .unwrap_err();
    assert!(matches!(blocked, LoginError::TooManyAttempts { .. }));
    assert_eq!(env.hasher.verifications(), verifications);

    // Une autre adresse n'est pas touchée ; l'attente écoulée, celle-ci non plus.
    env.sessions
        .login("marie", secret(PASSWORD), &client_at("10.0.0.99"))
        .await
        .expect("autre adresse");
    env.clock.advance(Duration::seconds(61));
    env.sessions
        .login("marie", secret(PASSWORD), &client())
        .await
        .expect("attente écoulée");
}

/// Hacheur qui change le mot de passe du compte juste après la vérification.
struct ChangingHasher {
    inner: std::sync::Arc<support::CountingHasher>,
    pool: sqlx::SqlitePool,
}

#[async_trait::async_trait]
impl hearth_agent::application::ports::PasswordHasher for ChangingHasher {
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
        sqlx::query("UPDATE accounts SET password_hash = 'changed-meanwhile'")
            .execute(&self.pool)
            .await
            .unwrap();
        Ok(verified)
    }

    fn decoy_hash(&self) -> &hearth_agent::domain::secret::Secret {
        self.inner.decoy_hash()
    }
}

#[tokio::test]
async fn a_password_changed_between_verification_and_session_creation_does_not_log_in() {
    use std::sync::Arc;

    use hearth_agent::application::sessions::SessionService;
    use hearth_agent::infrastructure::random::OsTokenGen;
    use hearth_agent::infrastructure::sqlite::{
        SqliteAccountRepo, SqliteLoginAttemptRepo, SqliteSessionRepo, SqliteStore,
    };

    let env = env().await;
    let marie = env.create("marie", Role::Admin).await;
    let pool = env.db.pool().clone();
    let service = SessionService::new(
        Arc::new(SqliteAccountRepo::new(pool.clone())),
        Arc::new(SqliteSessionRepo::new(pool.clone())),
        Arc::new(SqliteLoginAttemptRepo::new(pool.clone())),
        Arc::new(SqliteStore::new(pool.clone())),
        Arc::new(ChangingHasher {
            inner: env.hasher.clone(),
            pool,
        }),
        env.clock.clone(),
        Arc::new(support::SequentialIds::starting_at(500)),
        Arc::new(OsTokenGen),
        env.trail.clone(),
        env.audit_sink.clone(),
    );
    let error = service
        .login("marie", secret(PASSWORD), &client())
        .await
        .unwrap_err();
    assert!(matches!(error, LoginError::InvalidCredentials), "{error:?}");
    assert!(env.session_ids(&marie.id).await.is_empty());
    assert_eq!(env.service.find("marie").await.unwrap().last_login_at, None);
}

#[tokio::test]
async fn the_ninth_waiting_connection_of_an_address_is_refused_busy_at_once() {
    let env = env().await;
    env.hasher
        .delay_ms
        .store(300, std::sync::atomic::Ordering::SeqCst);
    // Dix connexions simultanées de la même adresse : une est traitée, huit attendent, une est
    // refusée sans attendre (et sans garder son mot de passe).
    let mut attempts = Vec::new();
    for n in 0..10 {
        let sessions = env.sessions.clone();
        attempts.push(tokio::spawn(async move {
            sessions
                .login(&format!("user{n}"), secret(WRONG), &client())
                .await
        }));
    }
    let mut busy = 0;
    let mut refused = 0;
    for attempt in attempts {
        match attempt.await.unwrap().unwrap_err() {
            LoginError::Busy => busy += 1,
            LoginError::InvalidCredentials => refused += 1,
            other => panic!("{other:?}"),
        }
    }
    assert_eq!((busy, refused), (1, 9));

    // Une adresse est libérée quand sa file est vide : la suivante passe, comptée comme un échec.
    env.hasher
        .delay_ms
        .store(0, std::sync::atomic::Ordering::SeqCst);
    let error = env
        .sessions
        .login("later", secret(WRONG), &client())
        .await
        .unwrap_err();
    assert!(matches!(error, LoginError::InvalidCredentials), "{error:?}");
}

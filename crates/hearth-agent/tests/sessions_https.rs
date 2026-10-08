//! Sessions de bout en bout sur HTTPS : vrai agent, vrai TLS 1.3, vraie base. Connexion réussie
//! et refusée, verrouillage, expiration, révocation, version incompatible, clé d'opération
//! rejouée, adresse du client prise sur la connexion.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use hearth_agent::domain::accounts::Role;
use hearth_agent::domain::sessions::LIFETIME;
use serde_json::json;
use support::https::{self, Agent};
use support::{PASSWORD, env};
use time::Duration;

const KEY: &str = "01J9ZY0G3Q8M2K6W4T7V5N1B9D";
const OTHER_PASSWORD: &str = "Another-Pass-77";

async fn login(agent: &Agent, username: &str, password: &str) -> https::Wire {
    agent
        .request("POST", "/sessions")
        .header("x-hearth-client", "poste-de-test/1.0")
        .json(&json!({ "username": username, "password": password }))
        .send()
        .await
}

async fn token(agent: &Agent, username: &str) -> String {
    let reply = login(agent, username, PASSWORD).await;
    assert_eq!(reply.status, 201, "{:?}", reply.body);
    reply.body["token"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn a_successful_login_opens_a_session_usable_over_tls() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let agent = https::start(&env).await;

    let reply = login(&agent, "marie", PASSWORD).await;
    assert_eq!(reply.status, 201, "{:?}", reply.body);
    assert_eq!(reply.header("x-hearth-api-range"), Some("1-1"));
    let token = reply.body["token"].as_str().unwrap();
    assert_eq!(token.len(), 64);
    assert_eq!(reply.body["account"]["role"], "admin");

    let me = agent.request("GET", "/me").token(token).send().await;
    assert_eq!(me.status, 200);
    assert_eq!(me.body["account"]["username"], "marie");

    let out = agent
        .request("DELETE", "/sessions/current")
        .token(token)
        .send()
        .await;
    assert_eq!(out.status, 204);
    let again = agent.request("GET", "/me").token(token).send().await;
    assert_eq!(again.status, 401);

    agent.shutdown().await;
}

#[tokio::test]
async fn the_client_address_is_the_one_of_the_connection_never_a_proxy_header() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let agent = https::start(&env).await;

    let reply = agent
        .request("POST", "/sessions")
        .header("x-forwarded-for", "203.0.113.9")
        .header("forwarded", "for=203.0.113.9")
        .json(&json!({ "username": "marie", "password": PASSWORD }))
        .send()
        .await;
    assert_eq!(reply.status, 201);
    let addr: String = sqlx::query_scalar("SELECT client_addr FROM sessions")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert_eq!(addr, "127.0.0.1");
    agent.shutdown().await;
}

#[tokio::test]
async fn a_refused_login_answers_the_same_for_a_wrong_password_and_an_unknown_username() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let agent = https::start(&env).await;

    let wrong = login(&agent, "marie", "Wrong-Horse-9999").await;
    let unknown = login(&agent, "fantome", "Wrong-Horse-9999").await;
    assert_eq!((wrong.status, wrong.code()), (401, "INVALID_CREDENTIALS"));
    assert_eq!(wrong.status, unknown.status);
    assert_eq!(wrong.body, unknown.body);
    agent.shutdown().await;
}

#[tokio::test]
async fn the_fifth_failure_locks_with_the_wait_in_seconds_and_the_lock_expires() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let agent = https::start(&env).await;

    for _ in 0..4 {
        assert_eq!(login(&agent, "marie", "Wrong-Horse-9999").await.status, 401);
    }
    let fifth = login(&agent, "marie", "Wrong-Horse-9999").await;
    assert_eq!((fifth.status, fifth.code()), (429, "TOO_MANY_ATTEMPTS"));
    assert_eq!(fifth.body["error"]["details"]["retry_after_s"], 60);

    let blocked = login(&agent, "marie", PASSWORD).await;
    assert_eq!(blocked.status, 429, "même le bon mot de passe est refusé");

    env.clock.advance(Duration::seconds(61));
    assert_eq!(login(&agent, "marie", PASSWORD).await.status, 201);
    agent.shutdown().await;
}

#[tokio::test]
async fn a_session_expires_after_thirty_days_without_activity() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let agent = https::start(&env).await;
    let token = token(&agent, "marie").await;

    env.clock.advance(LIFETIME - Duration::days(1));
    assert_eq!(
        agent
            .request("GET", "/me")
            .token(&token)
            .send()
            .await
            .status,
        200
    );

    env.clock.advance(LIFETIME + Duration::seconds(1));
    let expired = agent.request("GET", "/me").token(&token).send().await;
    assert_eq!((expired.status, expired.code()), (401, "SESSION_EXPIRED"));
    agent.shutdown().await;
}

#[tokio::test]
async fn a_session_is_revoked_when_its_password_changes() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let lucas = env.create("lucas", Role::ReadOnly).await;
    let agent = https::start(&env).await;
    let admin = agent.login_actor("marie").await;
    let victim = token(&agent, "lucas").await;

    let changed = agent
        .confirmed(
            &admin,
            "PUT",
            &format!("/accounts/{}/password", lucas.id),
            json!({ "password": OTHER_PASSWORD }),
        )
        .await
        .send()
        .await;
    assert_eq!(changed.status, 200, "{:?}", changed.body);

    let revoked = agent.request("GET", "/me").token(&victim).send().await;
    assert_eq!((revoked.status, revoked.code()), (401, "SESSION_REVOKED"));
    // L'administrateur, lui, garde sa session ; l'ancien mot de passe ne connecte plus.
    assert_eq!(
        agent
            .request("GET", "/me")
            .token(&admin.token)
            .send()
            .await
            .status,
        200
    );
    assert_eq!(login(&agent, "lucas", PASSWORD).await.status, 401);
    assert_eq!(login(&agent, "lucas", OTHER_PASSWORD).await.status, 201);
    agent.shutdown().await;
}

#[tokio::test]
async fn an_incompatible_version_is_refused_with_who_must_upgrade() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let agent = https::start(&env).await;

    let too_new = agent
        .request("POST", "/sessions")
        .header("x-hearth-api", "2")
        .json(&json!({ "username": "marie", "password": PASSWORD }))
        .send()
        .await;
    assert_eq!(
        (too_new.status, too_new.code()),
        (426, "INCOMPATIBLE_VERSION")
    );
    assert_eq!(too_new.body["error"]["details"]["upgrade"], "agent");

    let too_old = agent
        .request("GET", "/me")
        .header("x-hearth-api", "0")
        .send()
        .await;
    assert_eq!(too_old.status, 426);
    assert_eq!(too_old.body["error"]["details"]["upgrade"], "client");
    assert_eq!(too_old.header("x-hearth-api-range"), Some("1-1"));

    // /hello reste lisible quelle que soit la version : c'est ainsi qu'on la découvre.
    let hello = agent
        .request("GET", "/hello")
        .header("x-hearth-api", "9")
        .send()
        .await;
    assert_eq!(hello.status, 200);
    agent.shutdown().await;
}

#[tokio::test]
async fn replaying_an_operation_key_returns_the_first_result_without_running_again() {
    let env = env().await;
    env.create("lucas", Role::ReadOnly).await;
    let agent = https::start(&env).await;
    let lucas = agent.login_actor("lucas").await;
    let token = lucas.token.clone();
    // Le corps CONFIRMÉ est envoyé tel quel les deux fois (même clé, même requête).
    let body = agent
        .confirm(
            &lucas,
            "PUT",
            "/me/password",
            json!({ "current": PASSWORD, "password": OTHER_PASSWORD }),
        )
        .await;

    let first = agent
        .request("PUT", "/me/password")
        .token(&token)
        .header("idempotency-key", KEY)
        .json(&body)
        .send()
        .await;
    assert_eq!(first.status, 200, "{:?}", first.body);
    assert_eq!(first.header("idempotent-replayed"), None);

    // Ré-exécutée, la requête échouerait : le mot de passe actuel n'est plus PASSWORD.
    let second = agent
        .request("PUT", "/me/password")
        .token(&token)
        .header("idempotency-key", KEY)
        .json(&body)
        .send()
        .await;
    assert_eq!(second.status, 200);
    assert_eq!(second.body, first.body);
    assert_eq!(second.header("idempotent-replayed"), Some("true"));

    let state = agent
        .request("GET", &format!("/operations/{KEY}"))
        .token(&token)
        .send()
        .await;
    assert_eq!(state.body["status"], "succeeded");

    // Sans la clé, la même requête est ré-exécutée et échoue.
    // Un défi neuf et l'ancien mot de passe, devenu faux : la requête est ré-exécutée et refusée.
    let fresh = agent
        .confirm(
            &lucas,
            "PUT",
            "/me/password",
            json!({ "current": PASSWORD, "password": OTHER_PASSWORD }),
        )
        .await;
    let again = agent
        .request("PUT", "/me/password")
        .token(&token)
        .json(&fresh)
        .send()
        .await;
    assert_eq!((again.status, again.code()), (422, "WRONG_PASSWORD"));
    agent.shutdown().await;
}

#[tokio::test]
async fn a_read_only_account_cannot_manage_accounts_over_tls() {
    let env = env().await;
    env.create("lucas", Role::ReadOnly).await;
    let agent = https::start(&env).await;
    let token = token(&agent, "lucas").await;

    let list = agent.request("GET", "/accounts").token(&token).send().await;
    assert_eq!((list.status, list.code()), (403, "FORBIDDEN_ROLE"));
    let create = agent
        .request("POST", "/accounts")
        .token(&token)
        .json(&json!({ "username": "intrus", "password": PASSWORD, "role": "admin" }))
        .send()
        .await;
    assert_eq!((create.status, create.code()), (403, "FORBIDDEN_ROLE"));
    assert_eq!(env.service.list().await.unwrap().len(), 1);
    agent.shutdown().await;
}

#[tokio::test]
async fn a_client_that_cuts_before_the_answer_still_gets_its_result_recorded() {
    let env = env().await;
    env.create("lucas", Role::ReadOnly).await;
    let agent = https::start(&env).await;
    let lucas = agent.login_actor("lucas").await;
    let token = lucas.token.clone();
    let body = agent
        .confirm(
            &lucas,
            "PUT",
            "/me/password",
            json!({ "current": PASSWORD, "password": OTHER_PASSWORD }),
        )
        .await;

    // Le calcul du mot de passe dure 300 ms : le client coupe pendant l'exécution.
    env.hasher
        .delay_ms
        .store(300, std::sync::atomic::Ordering::SeqCst);
    agent
        .request("PUT", "/me/password")
        .token(&token)
        .header("idempotency-key", KEY)
        .json(&body)
        .send_and_cut()
        .await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    env.hasher
        .delay_ms
        .store(0, std::sync::atomic::Ordering::SeqCst);

    // Le client revient : l'opération est terminée (jamais « en cours » pour toujours).
    let mut status = String::new();
    for _ in 0..200 {
        let state = agent
            .request("GET", &format!("/operations/{KEY}"))
            .token(&token)
            .send()
            .await;
        if state.status == 200 {
            status = state.body["status"].as_str().unwrap().to_owned();
            if status != "running" {
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(status, "succeeded");
    assert_eq!(
        login(&agent, "lucas", OTHER_PASSWORD).await.status,
        201,
        "le changement a eu lieu"
    );

    // Et la même clé rend ce résultat sans ré-exécuter.
    let replay = agent
        .request("PUT", "/me/password")
        .token(&token)
        .header("idempotency-key", KEY)
        .json(&body)
        .send()
        .await;
    assert_eq!(replay.status, 200);
    assert_eq!(replay.header("idempotent-replayed"), Some("true"));
    agent.shutdown().await;
}

#[tokio::test]
async fn an_operation_left_running_by_a_previous_run_is_interrupted_at_startup() {
    use hearth_agent::domain::operations::OperationKey;

    let env = env().await;
    let lucas = env.create("lucas", Role::ReadOnly).await;
    let body = json!({ "current": PASSWORD, "password": OTHER_PASSWORD });
    let key = OperationKey::parse(KEY).unwrap();
    let request = env
        .operations
        .fingerprint("PUT", "/me/password", body.to_string().as_bytes());
    env.operations
        .begin(&key, &lucas.id, "PUT /me/password", &request)
        .await
        .unwrap();

    let agent = https::start(&env).await;
    let token = token(&agent, "lucas").await;
    let state = agent
        .request("GET", &format!("/operations/{KEY}"))
        .token(&token)
        .send()
        .await;
    assert_eq!(state.body["status"], "interrupted");

    let retry = agent
        .request("PUT", "/me/password")
        .token(&token)
        .header("idempotency-key", KEY)
        .json(&body)
        .send()
        .await;
    assert_eq!(
        (retry.status, retry.code()),
        (409, "CONFLICT"),
        "résultat inconnu : pas de rejeu"
    );
    agent.shutdown().await;
}

#[tokio::test]
async fn a_login_cut_by_the_client_still_counts_its_failure() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let agent = https::start(&env).await;

    // La vérification dure 300 ms : le client coupe pendant la tentative.
    env.hasher
        .delay_ms
        .store(300, std::sync::atomic::Ordering::SeqCst);
    agent
        .request("POST", "/sessions")
        .json(&json!({ "username": "marie", "password": "Wrong-Horse-9999" }))
        .send_and_cut()
        .await;
    env.hasher
        .delay_ms
        .store(0, std::sync::atomic::Ordering::SeqCst);

    let mut failures = 0_i64;
    for _ in 0..100 {
        failures = sqlx::query_scalar(
            "SELECT COALESCE(SUM(failures), 0) FROM login_attempts WHERE key NOT LIKE 'addr:%'",
        )
        .fetch_one(env.db.pool())
        .await
        .unwrap();
        if failures > 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(failures, 1, "l'échec est compté malgré la coupure");
    agent.shutdown().await;
}

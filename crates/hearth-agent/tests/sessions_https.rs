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
    let admin = token(&agent, "marie").await;
    let victim = token(&agent, "lucas").await;

    let changed = agent
        .request("PUT", &format!("/accounts/{}/password", lucas.id))
        .token(&admin)
        .json(&json!({ "password": OTHER_PASSWORD }))
        .send()
        .await;
    assert_eq!(changed.status, 200, "{:?}", changed.body);

    let revoked = agent.request("GET", "/me").token(&victim).send().await;
    assert_eq!((revoked.status, revoked.code()), (401, "SESSION_REVOKED"));
    // L'administrateur, lui, garde sa session ; l'ancien mot de passe ne connecte plus.
    assert_eq!(
        agent
            .request("GET", "/me")
            .token(&admin)
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
    let token = token(&agent, "lucas").await;
    let body = json!({ "current": PASSWORD, "password": OTHER_PASSWORD });

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
    let again = agent
        .request("PUT", "/me/password")
        .token(&token)
        .json(&body)
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

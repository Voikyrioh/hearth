//! Routes `/api/v1` de bout en bout en processus (sans TLS) sur une vraie base : garde de rôle
//! sur toutes les routes (test de balayage), connexion, comptes, version d'interface, clés
//! d'opération.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use axum::http::{Method, StatusCode};
use hearth_agent::domain::accounts::Role;
use hearth_agent::entrypoint::http::{Access, ENDPOINTS};
use serde_json::json;
use support::api::Api;
use support::{PASSWORD, env};

const OTHER_PASSWORD: &str = "Another-Pass-77";
const KEY: &str = "01J9ZY0G3Q8M2K6W4T7V5N1B9D";

/// Remplace les paramètres de chemin par un identifiant quelconque : la garde refuse avant que
/// le handler ne s'en serve.
fn concrete(path: &str) -> String {
    path.replace("{id}", "01JNOSUCHACCOUNT0000000000")
}

const METHODS: [Method; 5] = [
    Method::GET,
    Method::POST,
    Method::PUT,
    Method::PATCH,
    Method::DELETE,
];

// ---------------------------------------------------------------------------------------------
// Balayage : toute route déclarée réservée refuse l'appelant sans droit (BR-ACCT-013/014)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn every_reserved_route_refuses_a_caller_without_a_session() {
    let env = env().await;
    let api = Api::new(&env);
    for endpoint in ENDPOINTS
        .iter()
        // Le flux s'authentifie par son premier message : voir `tests/stream_https.rs`.
        .filter(|endpoint| !matches!(endpoint.access, Access::Public | Access::FirstMessage))
    {
        let path = concrete(endpoint.path);
        let reply = api
            .call(endpoint.method.clone(), &path)
            .json(&json!({}))
            .send()
            .await;
        assert_eq!(
            (reply.status, reply.code()),
            (StatusCode::UNAUTHORIZED, "UNAUTHENTICATED"),
            "{} {} sans jeton",
            endpoint.method,
            endpoint.path
        );
        let reply = api
            .call(endpoint.method.clone(), &path)
            .token("pas-un-jeton")
            .json(&json!({}))
            .send()
            .await;
        assert_eq!(
            reply.status,
            StatusCode::UNAUTHORIZED,
            "{} {} avec un jeton illisible",
            endpoint.method,
            endpoint.path
        );
        let reply = api
            .call(endpoint.method.clone(), &path)
            .token(&"ab".repeat(32))
            .json(&json!({}))
            .send()
            .await;
        assert_eq!(
            (reply.status, reply.code()),
            (StatusCode::UNAUTHORIZED, "SESSION_EXPIRED"),
            "{} {} avec un jeton inconnu",
            endpoint.method,
            endpoint.path
        );
    }
}

#[tokio::test]
async fn every_admin_route_refuses_a_read_only_account_and_changes_nothing() {
    let env = env().await;
    let api = Api::new(&env);
    let admin_token = env.account_with_token(&api, "marie", Role::Admin).await;
    let readonly_token = env.account_with_token(&api, "lucas", Role::ReadOnly).await;
    let before = snapshot(&env).await;

    let mut swept = 0;
    for endpoint in ENDPOINTS
        .iter()
        .filter(|endpoint| endpoint.access == Access::Admin)
    {
        let path = concrete(endpoint.path);
        // Les refus identiques à moins d'une minute se regroupent : on espace les appels.
        env.clock.advance(time::Duration::seconds(61));
        // Un corps valide pour la route : si la garde laissait passer, la requête agirait.
        let body = json!({
            "username": "intrus", "password": OTHER_PASSWORD, "role": "admin",
            "confirmation": "lucas"
        });
        let reply = api
            .call(endpoint.method.clone(), &path)
            .token(&readonly_token)
            .json(&body)
            .send()
            .await;
        assert_eq!(
            (reply.status, reply.code()),
            (StatusCode::FORBIDDEN, "FORBIDDEN_ROLE"),
            "{} {} doit refuser un compte lecture seule",
            endpoint.method,
            endpoint.path
        );
        if endpoint.modifies() {
            swept += 1;
        }
    }
    assert!(
        swept >= 5,
        "le balayage doit couvrir les routes modifiantes"
    );
    assert_eq!(snapshot(&env).await, before, "rien n'a changé en base");

    // Chaque refus est consigné au journal (BR-AUDIT-003, BR-AUDIT-021), une entrée par route.
    let admin_routes = ENDPOINTS
        .iter()
        .filter(|endpoint| endpoint.access == Access::Admin)
        .count();
    let denials: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_events WHERE outcome = 'denied' AND account = 'lucas'",
    )
    .fetch_one(env.db.pool())
    .await
    .unwrap();
    assert_eq!(usize::try_from(denials).unwrap(), admin_routes);

    // Contrôle : le même appel passe pour l'administrateur (la garde ne refuse pas tout).
    let reply = api.get("/accounts").token(&admin_token).send().await;
    assert_eq!(reply.status, StatusCode::OK);
}

/// Ce que le balayage ne doit jamais modifier : comptes, rôles, hachés, sessions.
async fn snapshot(env: &support::Env) -> Vec<String> {
    let mut lines: Vec<String> = sqlx::query_scalar(
        "SELECT id || '|' || username || '|' || role || '|' || password_hash FROM accounts ORDER BY id",
    )
    .fetch_all(env.db.pool())
    .await
    .unwrap();
    let sessions: Vec<String> =
        sqlx::query_scalar("SELECT id || '|' || account_id FROM sessions ORDER BY id")
            .fetch_all(env.db.pool())
            .await
            .unwrap();
    lines.extend(sessions);
    lines
}

#[tokio::test]
async fn authenticated_routes_let_a_read_only_account_through_the_guard() {
    let env = env().await;
    let api = Api::new(&env);
    let token = env.account_with_token(&api, "lucas", Role::ReadOnly).await;
    for endpoint in ENDPOINTS
        .iter()
        .filter(|endpoint| endpoint.access == Access::Authenticated)
        // La déconnexion ferme la session : testée à part.
        .filter(|endpoint| endpoint.path != "/sessions/current")
    {
        let reply = api
            .call(endpoint.method.clone(), &concrete(endpoint.path))
            .token(&token)
            .json(&json!({}))
            .send()
            .await;
        assert!(
            reply.status != StatusCode::UNAUTHORIZED && reply.status != StatusCode::FORBIDDEN,
            "{} {} : {}",
            endpoint.method,
            endpoint.path,
            reply.status
        );
    }
}

#[tokio::test]
async fn no_route_exists_outside_the_endpoint_table() {
    let env = env().await;
    let api = Api::new(&env);
    let mut paths: Vec<&str> = ENDPOINTS.iter().map(|endpoint| endpoint.path).collect();
    paths.sort_unstable();
    paths.dedup();
    for path in paths {
        for method in &METHODS {
            let declared = ENDPOINTS
                .iter()
                .any(|endpoint| endpoint.path == path && endpoint.method == *method);
            let reply = api.call(method.clone(), &concrete(path)).send().await;
            if declared {
                assert_ne!(
                    reply.status,
                    StatusCode::METHOD_NOT_ALLOWED,
                    "{method} {path}"
                );
                assert_ne!(reply.status, StatusCode::NOT_FOUND, "{method} {path}");
            } else {
                assert_eq!(
                    (reply.status, reply.code()),
                    (StatusCode::METHOD_NOT_ALLOWED, "METHOD_NOT_ALLOWED"),
                    "{method} {path} n'est pas dans la table"
                );
            }
        }
    }
    let reply = api.get("/nope").send().await;
    assert_eq!(
        (reply.status, reply.code()),
        (StatusCode::NOT_FOUND, "NOT_FOUND")
    );
}

// ---------------------------------------------------------------------------------------------
// Connexion, session courante, déconnexion
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn hello_needs_neither_a_session_nor_a_version() {
    let env = env().await;
    let api = Api::new(&env);
    let reply = api.get("/hello").version(None).send().await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.body["product"], "hearth");
    let reply = api.post("/hello").send().await;
    assert_eq!(reply.status, StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn login_answers_201_with_the_token_the_expiry_and_the_account() {
    let env = env().await;
    let api = Api::new(&env);
    env.create("marie", Role::Admin).await;
    let reply = api
        .post("/sessions")
        .header("x-hearth-client", "poste-de-marie/0.1")
        .json(&json!({ "username": "Marie", "password": PASSWORD }))
        .send()
        .await;
    assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.body);
    let token = reply.body["token"].as_str().unwrap();
    assert_eq!(token.len(), 64);
    assert!(token.bytes().all(|b| b.is_ascii_hexdigit()));
    assert!(reply.body["expires_at"].as_str().unwrap().ends_with('Z'));
    assert_eq!(reply.body["account"]["username"], "marie");
    assert_eq!(reply.body["account"]["role"], "admin");
    assert!(reply.body["account"].get("password_hash").is_none());

    let client: String = sqlx::query_scalar("SELECT client_name FROM sessions")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert_eq!(client, "poste-de-marie/0.1");
    let addr: String = sqlx::query_scalar("SELECT client_addr FROM sessions")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert_eq!(addr, "10.0.0.7", "adresse de la connexion, sans port");
}

#[tokio::test]
async fn the_same_refusal_for_a_wrong_password_and_an_unknown_username() {
    let env = env().await;
    let api = Api::new(&env);
    env.create("marie", Role::Admin).await;
    let wrong = api.login("marie", "Wrong-Horse-9999").await;
    let unknown = api.login("fantome", "Wrong-Horse-9999").await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    assert_eq!(wrong.code(), "INVALID_CREDENTIALS");
    assert_eq!(wrong.status, unknown.status);
    assert_eq!(wrong.body, unknown.body);
}

#[tokio::test]
async fn a_malformed_login_body_is_a_validation_error() {
    let env = env().await;
    let api = Api::new(&env);
    let reply = api.post("/sessions").raw_body("pas du json").send().await;
    assert_eq!(
        (reply.status, reply.code()),
        (StatusCode::UNPROCESSABLE_ENTITY, "VALIDATION_ERROR")
    );
    let reply = api
        .post("/sessions")
        .json(&json!({ "username": "marie" }))
        .send()
        .await;
    assert_eq!(reply.code(), "VALIDATION_ERROR");
}

#[tokio::test]
async fn five_failures_answer_429_with_the_wait_in_seconds() {
    let env = env().await;
    let api = Api::new(&env);
    env.create("marie", Role::Admin).await;
    for _ in 0..4 {
        assert_eq!(
            api.login("marie", "Wrong-Horse-9999").await.status,
            StatusCode::UNAUTHORIZED
        );
    }
    let fifth = api.login("marie", "Wrong-Horse-9999").await;
    assert_eq!(fifth.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(fifth.code(), "TOO_MANY_ATTEMPTS");
    assert_eq!(fifth.body["error"]["details"]["retry_after_s"], 60);
    let blocked = api.login("marie", PASSWORD).await;
    assert_eq!(blocked.status, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn me_returns_the_current_account_and_logout_ends_the_session() {
    let env = env().await;
    let api = Api::new(&env);
    let token = env.account_with_token(&api, "lucas", Role::ReadOnly).await;
    let reply = api.get("/me").token(&token).send().await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.body["account"]["username"], "lucas");
    assert_eq!(reply.body["account"]["role"], "readonly");
    assert!(reply.body["session_expires_at"].is_string());

    let reply = api.delete("/sessions/current").token(&token).send().await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);
    let reply = api.get("/me").token(&token).send().await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn an_expired_session_answers_session_expired_and_a_closed_one_session_revoked() {
    use hearth_agent::domain::sessions::LIFETIME;
    let env = env().await;
    let api = Api::new(&env);
    let admin = env.account_with_token(&api, "marie", Role::Admin).await;
    let lucas = env.account_with_token(&api, "lucas", Role::ReadOnly).await;

    let reply = api
        .put(
            "/accounts/{id}/password"
                .replace("{id}", &lucas_id(&env).await)
                .as_str(),
        )
        .token(&admin)
        .json(&json!({ "password": OTHER_PASSWORD }))
        .send()
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    assert_eq!(reply.body["sessions_closed"], 1);
    let reply = api.get("/me").token(&lucas).send().await;
    assert_eq!(
        (reply.status, reply.code()),
        (StatusCode::UNAUTHORIZED, "SESSION_REVOKED")
    );

    env.clock.advance(LIFETIME + time::Duration::seconds(1));
    let reply = api.get("/me").token(&admin).send().await;
    assert_eq!(
        (reply.status, reply.code()),
        (StatusCode::UNAUTHORIZED, "SESSION_EXPIRED")
    );
}

async fn lucas_id(env: &support::Env) -> String {
    env.service.find("lucas").await.unwrap().id.to_string()
}

// ---------------------------------------------------------------------------------------------
// Version d'interface
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn an_incompatible_version_says_who_must_upgrade() {
    let env = env().await;
    let api = Api::new(&env);
    let token = env.account_with_token(&api, "marie", Role::Admin).await;

    let old = api.get("/me").token(&token).version(Some("0")).send().await;
    assert_eq!(
        (old.status, old.code()),
        (StatusCode::UPGRADE_REQUIRED, "INCOMPATIBLE_VERSION")
    );
    assert_eq!(old.body["error"]["details"]["upgrade"], "client");

    let new = api.get("/me").token(&token).version(Some("2")).send().await;
    assert_eq!(new.status, StatusCode::UPGRADE_REQUIRED);
    assert_eq!(new.body["error"]["details"]["upgrade"], "agent");
    assert_eq!(new.headers.get("x-hearth-api-range").unwrap(), "1-1");

    // Le contrôle précède l'authentification : même sans jeton.
    let no_token = api.get("/me").version(Some("2")).send().await;
    assert_eq!(no_token.status, StatusCode::UPGRADE_REQUIRED);

    let missing = api.get("/me").token(&token).version(None).send().await;
    assert_eq!(
        (missing.status, missing.code()),
        (StatusCode::UNPROCESSABLE_ENTITY, "VALIDATION_ERROR")
    );
}

#[tokio::test]
async fn the_version_is_checked_on_the_login_too() {
    let env = env().await;
    let api = Api::new(&env);
    env.create("marie", Role::Admin).await;
    let reply = api
        .post("/sessions")
        .version(Some("7"))
        .json(&json!({ "username": "marie", "password": PASSWORD }))
        .send()
        .await;
    assert_eq!(reply.status, StatusCode::UPGRADE_REQUIRED);
    let sessions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert_eq!(sessions, 0);
}

// ---------------------------------------------------------------------------------------------
// Comptes
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn an_administrator_manages_accounts() {
    let env = env().await;
    let api = Api::new(&env);
    let admin = env.account_with_token(&api, "marie", Role::Admin).await;

    let created = api
        .post("/accounts")
        .token(&admin)
        .json(&json!({ "username": "lucas", "password": PASSWORD, "role": "readonly" }))
        .send()
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.body);
    assert_eq!(created.body["username"], "lucas");
    assert_eq!(created.body["sessions_open"], 0);
    let lucas = created.body["id"].as_str().unwrap().to_owned();

    let taken = api
        .post("/accounts")
        .token(&admin)
        .json(&json!({ "username": "LUCAS", "password": PASSWORD, "role": "admin" }))
        .send()
        .await;
    assert_eq!(
        (taken.status, taken.code()),
        (StatusCode::CONFLICT, "USERNAME_TAKEN")
    );

    let weak = api
        .post("/accounts")
        .token(&admin)
        .json(&json!({ "username": "paul", "password": "abc", "role": "admin" }))
        .send()
        .await;
    assert_eq!(
        (weak.status, weak.code()),
        (StatusCode::UNPROCESSABLE_ENTITY, "WEAK_PASSWORD")
    );
    let rules = weak.body["error"]["details"]["rules"].as_array().unwrap();
    assert!(rules.contains(&json!("min_length")) && rules.contains(&json!("digit")));

    let list = api.get("/accounts").token(&admin).send().await;
    let accounts = list.body["accounts"].as_array().unwrap();
    assert_eq!(accounts.len(), 2);
    assert_eq!(accounts[0]["username"], "marie");
    assert_eq!(accounts[0]["sessions_open"], 1);
    assert!(accounts[0]["last_login_at"].is_string());
    assert!(accounts[1]["last_login_at"].is_null());

    let promoted = api
        .patch(&format!("/accounts/{lucas}"))
        .token(&admin)
        .json(&json!({ "role": "admin" }))
        .send()
        .await;
    assert_eq!(promoted.status, StatusCode::NO_CONTENT);
    assert_eq!(env.service.find("lucas").await.unwrap().role, Role::Admin);
}

#[tokio::test]
async fn the_last_administrator_cannot_be_demoted_or_deleted() {
    let env = env().await;
    let api = Api::new(&env);
    let admin = env.account_with_token(&api, "marie", Role::Admin).await;
    let marie = env.service.find("marie").await.unwrap().id.to_string();

    let demote = api
        .patch(&format!("/accounts/{marie}"))
        .token(&admin)
        .json(&json!({ "role": "readonly" }))
        .send()
        .await;
    assert_eq!(
        (demote.status, demote.code()),
        (StatusCode::CONFLICT, "LAST_ADMIN")
    );

    let delete = api
        .delete(&format!("/accounts/{marie}"))
        .token(&admin)
        .json(&json!({ "confirmation": "marie" }))
        .send()
        .await;
    assert_eq!(
        (delete.status, delete.code()),
        (StatusCode::CONFLICT, "LAST_ADMIN")
    );
}

#[tokio::test]
async fn deleting_your_own_account_needs_your_username_retyped() {
    let env = env().await;
    let api = Api::new(&env);
    let admin = env.account_with_token(&api, "marie", Role::Admin).await;
    env.create("paul", Role::Admin).await;
    let marie = env.service.find("marie").await.unwrap().id.to_string();

    let refused = api
        .delete(&format!("/accounts/{marie}"))
        .token(&admin)
        .send()
        .await;
    assert_eq!(
        (refused.status, refused.code()),
        (StatusCode::UNPROCESSABLE_ENTITY, "VALIDATION_ERROR")
    );
    assert_eq!(refused.body["error"]["details"]["field"], "confirmation");

    let done = api
        .delete(&format!("/accounts/{marie}"))
        .token(&admin)
        .json(&json!({ "confirmation": "Marie" }))
        .send()
        .await;
    assert_eq!(done.status, StatusCode::OK, "{:?}", done.body);
    assert_eq!(done.body["sessions_closed"], 1);
    let reply = api.get("/me").token(&admin).send().await;
    assert_eq!(
        (reply.status, reply.code()),
        (StatusCode::UNAUTHORIZED, "SESSION_REVOKED")
    );
}

#[tokio::test]
async fn revoking_sessions_and_changing_your_own_password_follow_the_rules() {
    let env = env().await;
    let api = Api::new(&env);
    let admin = env.account_with_token(&api, "marie", Role::Admin).await;
    let lucas_current = env.account_with_token(&api, "lucas", Role::ReadOnly).await;
    let lucas_other = api.token_of("lucas").await;
    let lucas = lucas_id(&env).await;

    // Un compte lecture seule change son propre mot de passe : l'autre session est fermée.
    let wrong = api
        .put("/me/password")
        .token(&lucas_current)
        .json(&json!({ "current": "Wrong-Horse-9999", "password": OTHER_PASSWORD }))
        .send()
        .await;
    assert_eq!(
        (wrong.status, wrong.code()),
        (StatusCode::UNPROCESSABLE_ENTITY, "WRONG_PASSWORD")
    );
    let changed = api
        .put("/me/password")
        .token(&lucas_current)
        .json(&json!({ "current": PASSWORD, "password": OTHER_PASSWORD }))
        .send()
        .await;
    assert_eq!(changed.status, StatusCode::OK, "{:?}", changed.body);
    assert_eq!(changed.body["sessions_closed"], 1);
    assert_eq!(
        api.get("/me").token(&lucas_current).send().await.status,
        StatusCode::OK
    );
    assert_eq!(
        api.get("/me").token(&lucas_other).send().await.code(),
        "SESSION_REVOKED"
    );

    // Un administrateur révoque les sessions de lucas.
    let revoked = api
        .delete(&format!("/accounts/{lucas}/sessions"))
        .token(&admin)
        .send()
        .await;
    assert_eq!(revoked.status, StatusCode::OK);
    assert_eq!(revoked.body["sessions_closed"], 1);
    assert_eq!(
        api.get("/me").token(&lucas_current).send().await.code(),
        "SESSION_REVOKED"
    );

    let unknown = api
        .delete("/accounts/01JNOSUCHACCOUNT0000000000/sessions")
        .token(&admin)
        .send()
        .await;
    assert_eq!(
        (unknown.status, unknown.code()),
        (StatusCode::NOT_FOUND, "NOT_FOUND")
    );
}

// ---------------------------------------------------------------------------------------------
// Clés d'opération
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn replaying_a_key_returns_the_first_result_without_running_again() {
    let env = env().await;
    let api = Api::new(&env);
    let token = env.account_with_token(&api, "lucas", Role::ReadOnly).await;
    let body = json!({ "current": PASSWORD, "password": OTHER_PASSWORD });

    let first = api
        .put("/me/password")
        .token(&token)
        .key(KEY)
        .json(&body)
        .send()
        .await;
    assert_eq!(first.status, StatusCode::OK, "{:?}", first.body);
    assert!(first.headers.get("idempotent-replayed").is_none());

    // Ré-exécutée, la requête échouerait (le mot de passe actuel n'est plus PASSWORD).
    let second = api
        .put("/me/password")
        .token(&token)
        .key(KEY)
        .json(&body)
        .send()
        .await;
    assert_eq!(second.status, StatusCode::OK);
    assert_eq!(second.body, first.body);
    assert_eq!(second.headers.get("idempotent-replayed").unwrap(), "true");

    let state = api
        .get(&format!("/operations/{KEY}"))
        .token(&token)
        .send()
        .await;
    assert_eq!(state.status, StatusCode::OK);
    assert_eq!(state.body["status"], "succeeded");
    assert_eq!(state.body["kind"], "PUT /me/password");
    assert_eq!(state.body["result"], first.body);
}

#[tokio::test]
async fn a_failed_result_is_replayed_too() {
    let env = env().await;
    let api = Api::new(&env);
    let token = env.account_with_token(&api, "lucas", Role::ReadOnly).await;
    let body = json!({ "current": "Wrong-Horse-9999", "password": OTHER_PASSWORD });
    let first = api
        .put("/me/password")
        .token(&token)
        .key(KEY)
        .json(&body)
        .send()
        .await;
    let second = api
        .put("/me/password")
        .token(&token)
        .key(KEY)
        .json(&body)
        .send()
        .await;
    assert_eq!(
        (first.status, first.code()),
        (StatusCode::UNPROCESSABLE_ENTITY, "WRONG_PASSWORD")
    );
    assert_eq!(
        (second.status, second.body.clone()),
        (first.status, first.body)
    );
    let state = api
        .get(&format!("/operations/{KEY}"))
        .token(&token)
        .send()
        .await;
    assert_eq!(state.body["status"], "failed");
}

#[tokio::test]
async fn a_key_still_running_answers_409_and_each_account_has_its_own_keys() {
    use hearth_agent::domain::operations::RequestFingerprint;
    let env = env().await;
    let api = Api::new(&env);
    let lucas = env.account_with_token(&api, "lucas", Role::ReadOnly).await;
    let paul = env.account_with_token(&api, "paul", Role::ReadOnly).await;
    let lucas_account = env.service.find("lucas").await.unwrap().id;
    let key = hearth_agent::domain::operations::OperationKey::parse(KEY).unwrap();
    let body = json!({ "current": PASSWORD, "password": OTHER_PASSWORD });
    let request = RequestFingerprint::of("PUT", "/me/password", body.to_string().as_bytes());
    env.operations
        .begin(&key, &lucas_account, "PUT /me/password", &request)
        .await
        .unwrap();

    let busy = api
        .put("/me/password")
        .token(&lucas)
        .key(KEY)
        .json(&body)
        .send()
        .await;
    assert_eq!(
        (busy.status, busy.code()),
        (StatusCode::CONFLICT, "OPERATION_IN_PROGRESS")
    );
    let state = api
        .get(&format!("/operations/{KEY}"))
        .token(&lucas)
        .send()
        .await;
    assert_eq!(state.body["status"], "running");
    assert!(state.body["result"].is_null());

    // La même clé chez un autre compte est la sienne : elle s'exécute, sans voir celle de lucas.
    let own = api
        .put("/me/password")
        .token(&paul)
        .key(KEY)
        .json(&body)
        .send()
        .await;
    assert_eq!(own.status, StatusCode::OK, "{:?}", own.body);
    let paul_state = api
        .get(&format!("/operations/{KEY}"))
        .token(&paul)
        .send()
        .await;
    assert_eq!(paul_state.body["status"], "succeeded");
}

#[tokio::test]
async fn a_key_reused_for_another_request_is_refused_without_running() {
    let env = env().await;
    let api = Api::new(&env);
    let admin = env.account_with_token(&api, "marie", Role::Admin).await;
    let lucas = env.create("lucas", Role::ReadOnly).await;
    let first = api
        .put(&format!("/accounts/{}/password", lucas.id))
        .token(&admin)
        .key(KEY)
        .json(&json!({ "password": OTHER_PASSWORD }))
        .send()
        .await;
    assert_eq!(first.status, StatusCode::OK, "{:?}", first.body);

    // Même clé, autre méthode et autre chemin : jamais le résultat du PUT, jamais exécutée.
    let other = api
        .delete(&format!("/accounts/{}", lucas.id))
        .token(&admin)
        .key(KEY)
        .send()
        .await;
    assert_eq!(
        (other.status, other.code()),
        (StatusCode::UNPROCESSABLE_ENTITY, "IDEMPOTENCY_KEY_REUSED")
    );
    assert!(
        env.service.find("lucas").await.is_ok(),
        "le compte existe toujours"
    );

    // Même clé, même requête mais un autre corps : refusée aussi.
    let changed_body = api
        .put(&format!("/accounts/{}/password", lucas.id))
        .token(&admin)
        .key(KEY)
        .json(&json!({ "password": "Third-Pass-9999" }))
        .send()
        .await;
    assert_eq!(changed_body.code(), "IDEMPOTENCY_KEY_REUSED");

    // La requête d'origine, rejouée à l'identique, rend son résultat.
    let replay = api
        .put(&format!("/accounts/{}/password", lucas.id))
        .token(&admin)
        .key(KEY)
        .json(&json!({ "password": OTHER_PASSWORD }))
        .send()
        .await;
    assert_eq!(replay.headers.get("idempotent-replayed").unwrap(), "true");
}

#[tokio::test]
async fn an_unknown_operation_is_a_404_and_an_invalid_key_a_validation_error() {
    let env = env().await;
    let api = Api::new(&env);
    let token = env.account_with_token(&api, "lucas", Role::ReadOnly).await;
    let reply = api
        .get("/operations/01JNEVERSEEN")
        .token(&token)
        .send()
        .await;
    assert_eq!(
        (reply.status, reply.code()),
        (StatusCode::NOT_FOUND, "NOT_FOUND")
    );

    let reply = api
        .put("/me/password")
        .token(&token)
        .key("pas valide !")
        .json(&json!({ "current": PASSWORD, "password": OTHER_PASSWORD }))
        .send()
        .await;
    assert_eq!(
        (reply.status, reply.code()),
        (StatusCode::UNPROCESSABLE_ENTITY, "VALIDATION_ERROR")
    );
}

#[tokio::test]
async fn the_login_ignores_the_key_so_no_token_is_ever_stored_in_an_operation() {
    let env = env().await;
    let api = Api::new(&env);
    env.create("marie", Role::Admin).await;
    let reply = api
        .post("/sessions")
        .key(KEY)
        .json(&json!({ "username": "marie", "password": PASSWORD }))
        .send()
        .await;
    assert_eq!(reply.status, StatusCode::CREATED);
    let operations: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM operations")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert_eq!(operations, 0);
}

#[tokio::test]
async fn a_replayed_creation_creates_the_account_once() {
    let env = env().await;
    let api = Api::new(&env);
    let admin = env.account_with_token(&api, "marie", Role::Admin).await;
    let body = json!({ "username": "lucas", "password": PASSWORD, "role": "readonly" });
    let first = api
        .post("/accounts")
        .token(&admin)
        .key(KEY)
        .json(&body)
        .send()
        .await;
    let second = api
        .post("/accounts")
        .token(&admin)
        .key(KEY)
        .json(&body)
        .send()
        .await;
    assert_eq!(first.status, StatusCode::CREATED);
    assert_eq!(
        (second.status, second.body),
        (StatusCode::CREATED, first.body)
    );
    assert_eq!(env.service.list().await.unwrap().len(), 2);
}

#[tokio::test]
async fn a_tracked_body_over_one_mebibyte_is_413_not_422() {
    let env = env().await;
    let api = Api::new(&env);
    let token = env.account_with_token(&api, "lucas", Role::ReadOnly).await;
    let huge = format!(r#"{{"current":"{}"}}"#, "x".repeat(1 << 20));
    let reply = api
        .put("/me/password")
        .token(&token)
        .key(KEY)
        .raw_body(&huge)
        .send()
        .await;
    assert_eq!(
        (reply.status, reply.code()),
        (StatusCode::PAYLOAD_TOO_LARGE, "PAYLOAD_TOO_LARGE")
    );
    // Un corps simplement illisible reste une erreur de validation.
    let reply = api
        .put("/me/password")
        .token(&token)
        .key("K2")
        .raw_body("pas du json")
        .send()
        .await;
    assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
}

// ---------------------------------------------------------------------------------------------
// Balayage des succès : chaque route qui modifie laisse exactement une entrée « réussi »
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn every_modifying_route_leaves_exactly_one_success_entry() {
    let env = env().await;
    let api = Api::new(&env);
    let admin = env.account_with_token(&api, "marie", Role::Admin).await;
    let own = env.account_with_token(&api, "carl", Role::ReadOnly).await;
    let role_victim = env.create("v-role", Role::ReadOnly).await;
    let password_victim = env.create("v-pass", Role::ReadOnly).await;
    let sessions_victim = env.create("v-sess", Role::ReadOnly).await;
    let delete_victim = env.create("v-del", Role::ReadOnly).await;
    // Un poste de confiance de `carl`, que la route de retrait peut retirer (HRT-22).
    let own_account = env.service.find("carl").await.unwrap().id;
    env.insert_device(
        &own_account,
        "01JDEVICEOFCARL0000000000",
        &"ab".repeat(16),
        "poste-de-carl",
    )
    .await;

    // `carl` ouvre aussi une session AVEC la clé d'un poste inscrit : retirer un poste exige le mot de passe
    // et la preuve de la clé du poste courant (Q16).
    let carl_key = support::device::DeviceKey::new();
    let carl_session = support::device::login_token(&api, &carl_key, "carl", PASSWORD).await;
    // `marie` ouvre une session AVEC la clé d'un poste inscrit : activer le mode attaque exige la preuve
    // d'une clé inscrite et le mot de passe (Q14 point 3, Q16).
    let marie_key = support::device::DeviceKey::new();
    let marie_session = support::device::login_token(&api, &marie_key, "marie", PASSWORD).await;

    let successes = |env: &support::Env| {
        let pool = env.db.pool().clone();
        async move {
            // L'ouverture du délai du mot de passe a sa propre entrée (BR-TRUST-053) : elle n'est pas
            // « l'entrée de la route ».
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM audit_events WHERE outcome = 'ok' AND action <> 'reauth.elevation'",
            )
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };

    let mut swept = 0;
    for endpoint in ENDPOINTS.iter().filter(|endpoint| endpoint.modifies()) {
        // La mise à jour de l'agent répond `202` et s'exécute côté serveur : son entrée réussie est
        // écrite par l'agent qui revient (BR-UPDATE-024), pas par la requête. Elle est couverte par
        // `update_use_cases.rs` et `update_http.rs`.
        if endpoint.path == "/agent/update" {
            continue;
        }
        // Une session fraîche pour la déconnexion (sa connexion s'écrit avant la mesure).
        let logout_token = if endpoint.path == "/sessions/current" {
            Some(api.token_of("marie").await)
        } else {
            None
        };
        // Le retrait d'un poste (route précédente) ferme les sessions SANS lien du compte (HRT-24,
        // suivi de la revue de la PR #25) : `carl` ouvre une session fraîche pour le changement de
        // son mot de passe (sa connexion s'écrit avant la mesure).
        let own_token = if endpoint.path == "/me/password" {
            Some(api.token_of("carl").await)
        } else {
            None
        };
        let before = successes(&env).await;
        let reply = match (endpoint.method.as_str(), endpoint.path) {
            ("POST", "/sessions") => api.login("marie", PASSWORD).await,
            ("DELETE", "/sessions/current") => {
                api.delete("/sessions/current")
                    .token(logout_token.as_deref().unwrap())
                    .send()
                    .await
            }
            ("DELETE", "/me/devices/{id}") => {
                let target = "01JDEVICEOFCARL0000000000";
                let body = support::device::removal_body(
                    &api,
                    &carl_key,
                    "carl",
                    &carl_session,
                    target,
                    PASSWORD,
                )
                .await;
                api.delete(&format!("/me/devices/{target}"))
                    .token(&carl_session)
                    .json(&body)
                    .send()
                    .await
            }
            ("PUT", "/me/password") => {
                api.put("/me/password")
                    .token(own_token.as_deref().unwrap_or(&own))
                    .json(&json!({ "current": PASSWORD, "password": OTHER_PASSWORD }))
                    .send()
                    .await
            }
            ("POST", "/accounts") => {
                api.post("/accounts")
                    .token(&admin)
                    .json(&json!({ "username": "nouveau", "password": OTHER_PASSWORD, "role": "readonly" }))
                    .send()
                    .await
            }
            ("PATCH", "/accounts/{id}") => {
                api.patch(&format!("/accounts/{}", role_victim.id))
                    .token(&admin)
                    .json(&json!({ "role": "admin" }))
                    .send()
                    .await
            }
            ("DELETE", "/accounts/{id}") => {
                api.delete(&format!("/accounts/{}", delete_victim.id))
                    .token(&admin)
                    .send()
                    .await
            }
            ("PUT", "/accounts/{id}/password") => {
                api.put(&format!("/accounts/{}/password", password_victim.id))
                    .token(&admin)
                    .json(&json!({ "password": OTHER_PASSWORD }))
                    .send()
                    .await
            }
            ("DELETE", "/accounts/{id}/sessions") => {
                api.delete(&format!("/accounts/{}/sessions", sessions_victim.id))
                    .token(&admin)
                    .send()
                    .await
            }
            ("PUT", "/security/attack-mode") => {
                let body = support::device::attack_mode_body(
                    &api,
                    &marie_key,
                    "marie",
                    &marie_session,
                    true,
                    PASSWORD,
                )
                .await;
                let reply = api
                    .put("/security/attack-mode")
                    .token(&marie_session)
                    .json(&body)
                    .send()
                    .await;
                // Le mode est éteint pour les routes suivantes du balayage.
                sqlx::query("UPDATE attack_mode SET active = 0 WHERE id = 1")
                    .execute(env.db.pool())
                    .await
                    .unwrap();
                reply
            }
            ("PUT", "/me/reauth") => {
                // Le réglage de fréquence du mot de passe (HRT-28) : toujours confirmé.
                let act = hearth_proto::admin_act::AdminAct::ReauthSetting {
                    mode: hearth_proto::api::reauth::ReauthMode::Each,
                };
                let body = support::device::with_reauth(
                    &api,
                    &marie_key,
                    "marie",
                    &marie_session,
                    &act,
                    PASSWORD,
                    json!({ "password": "each" }),
                )
                .await;
                api.put("/me/reauth")
                    .token(&marie_session)
                    .json(&body)
                    .send()
                    .await
            }
            (method, path) => panic!("route modifiante sans scénario de réussite : {method} {path}"),
        };
        assert!(
            reply.status.is_success(),
            "{} {} : {:?}",
            endpoint.method,
            endpoint.path,
            reply.body
        );
        assert_eq!(
            successes(&env).await - before,
            1,
            "{} {} doit laisser exactement une entrée réussie",
            endpoint.method,
            endpoint.path
        );
        swept += 1;
    }
    assert!(swept >= 8, "{swept} routes balayées");

    // Les routes de lecture (dont celles des mesures) ne laissent aucune entrée (BR-AUDIT-004).
    let entries = || async {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM audit_events")
            .fetch_one(env.db.pool())
            .await
            .unwrap()
    };
    let before = entries().await;
    let mut reads = 0;
    for endpoint in ENDPOINTS
        .iter()
        .filter(|endpoint| !endpoint.modifies() && endpoint.audit.is_none())
    {
        let path = match endpoint.path {
            "/hello" | "/me" | "/me/devices" | "/security" | "/machine" | "/metrics/history"
            | "/agent/update" | "/agent/update/last" => endpoint.path.to_owned(),
            "/operations/{id}" => "/operations/INCONNUE".to_owned(),
            "/stream" => continue, // le flux se teste en WebSocket (stream_https.rs)
            // Un POST sans effet : testé à part (device_proof.rs).
            "/sessions/challenge" => continue,
            other => panic!("route de lecture sans scénario : {other}"),
        };
        let reply = api.get(&path).token(&admin).send().await;
        assert!(
            reply.status.is_success() || reply.status == StatusCode::NOT_FOUND,
            "{path} : {:?}",
            reply.body
        );
        reads += 1;
    }
    assert!(reads >= 4, "{reads} routes de lecture balayées");
    assert_eq!(
        entries().await,
        before,
        "une lecture n'écrit rien au journal"
    );
}

// ---------------------------------------------------------------------------------------------
// Mesures : /machine, /metrics/history, /stream (BR-DASH-010, BR-DASH-013)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn both_roles_see_the_machine_and_its_history_identically() {
    let env = env().await;
    let api = Api::new(&env);
    let admin = env.account_with_token(&api, "marie", Role::Admin).await;
    let readonly = env.account_with_token(&api, "lucas", Role::ReadOnly).await;

    let for_admin = api.get("/machine").token(&admin).send().await;
    let for_readonly = api.get("/machine").token(&readonly).send().await;
    assert_eq!(for_admin.status, StatusCode::OK);
    assert_eq!(for_admin.body, for_readonly.body);
    assert_eq!(for_admin.body["name"], "forge-test");
    assert_eq!(for_admin.body["capabilities"]["gpu"], true);
    assert_eq!(for_admin.body["capabilities"]["temps"], false);
    assert_eq!(for_admin.body["gpus"][0]["name"], "Test GPU");

    for token in [&admin, &readonly] {
        let reply = api.get("/metrics/history").token(token).send().await;
        assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
        assert_eq!(reply.body["window"], "5m", "5 minutes par défaut");
        assert_eq!(reply.body["step_s"], 1);
        assert_eq!(reply.body["samples"], json!([]), "rien n'est encore mesuré");
    }
}

#[tokio::test]
async fn the_history_windows_are_one_five_and_sixty_minutes() {
    let env = env().await;
    let api = Api::new(&env);
    let token = env.account_with_token(&api, "lucas", Role::ReadOnly).await;
    for (window, step) in [("1m", 1), ("5m", 1), ("1h", 10)] {
        let reply = api
            .get(&format!("/metrics/history?window={window}"))
            .token(&token)
            .send()
            .await;
        assert_eq!(reply.status, StatusCode::OK, "{window}");
        assert_eq!(reply.body["window"], window);
        assert_eq!(reply.body["step_s"], step, "{window}");
    }
    for bad in ["2m", "", "1H", "60m"] {
        let reply = api
            .get(&format!("/metrics/history?window={bad}"))
            .token(&token)
            .send()
            .await;
        assert_eq!(
            (reply.status, reply.code()),
            (StatusCode::UNPROCESSABLE_ENTITY, "VALIDATION_ERROR"),
            "{bad:?}"
        );
    }
}

#[tokio::test]
async fn the_machine_routes_check_the_interface_version_like_the_others() {
    let env = env().await;
    let api = Api::new(&env);
    let token = env.account_with_token(&api, "lucas", Role::ReadOnly).await;
    for path in ["/machine", "/metrics/history", "/stream"] {
        let reply = api.get(path).token(&token).version(Some("2")).send().await;
        assert_eq!(
            (reply.status, reply.code()),
            (StatusCode::UPGRADE_REQUIRED, "INCOMPATIBLE_VERSION"),
            "{path}"
        );
        let reply = api.get(path).token(&token).version(None).send().await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY, "{path}");
    }
}

#[tokio::test]
async fn the_stream_route_without_an_upgrade_explains_itself() {
    let env = env().await;
    let api = Api::new(&env);
    let reply = api.get("/stream").send().await;
    assert_eq!(
        (reply.status, reply.code()),
        (StatusCode::UNPROCESSABLE_ENTITY, "VALIDATION_ERROR")
    );
}

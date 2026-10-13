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
        .replace("{key}", "c_nosuchservice")
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

/// Pose les déclencheurs qui voient toute écriture de la colonne du droit « gérer les services »
/// (BR-GRANT-005). `UPDATE OF services_right` se déclenche dès que la colonne figure dans la liste
/// `SET` d'une requête, même pour la valeur qu'elle avait déjà : une route qui réécrit le droit à
/// l'identique est vue aussi. Un compte inséré avec une valeur autre que `none` l'est également.
/// Les écritures vues s'accumulent dans `right_writes`.
async fn install_right_write_detector(env: &support::Env) {
    for statement in [
        "CREATE TABLE right_writes (what TEXT NOT NULL)",
        "CREATE TRIGGER right_updated AFTER UPDATE OF services_right ON accounts
         BEGIN INSERT INTO right_writes VALUES ('update ' || NEW.id); END",
        "CREATE TRIGGER right_born AFTER INSERT ON accounts WHEN NEW.services_right <> 'none'
         BEGIN INSERT INTO right_writes VALUES ('insert ' || NEW.id); END",
    ] {
        sqlx::query(statement).execute(env.db.pool()).await.unwrap();
    }
}

async fn right_writes(env: &support::Env) -> Vec<String> {
    sqlx::query_scalar("SELECT what FROM right_writes ORDER BY rowid")
        .fetch_all(env.db.pool())
        .await
        .unwrap()
}

/// Le témoin du détecteur : sans lui, un détecteur mal posé ne verrait rien et tous les balayages
/// passeraient. Une écriture directe (celle qu'une route ajoutée plus tard ferait) est vue, qu'elle
/// accorde, retire ou réécrive à l'identique, et qu'elle passe par `UPDATE` ou par un compte inséré
/// avec le droit.
#[tokio::test]
async fn the_write_detector_sees_every_kind_of_write_of_the_services_right() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    install_right_write_detector(&env).await;
    assert!(right_writes(&env).await.is_empty());
    // Le chemin de production qui crée un compte ne l'écrit pas.
    env.create("lucas", Role::ReadOnly).await;
    assert!(
        right_writes(&env).await.is_empty(),
        "{:?}",
        right_writes(&env).await
    );
    // Écritures directes, de toutes les formes.
    sqlx::query("UPDATE accounts SET services_right = 'manage' WHERE username = 'marie'")
        .execute(env.db.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE accounts SET services_right = 'manage' WHERE username = 'marie'")
        .execute(env.db.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE accounts SET services_right = 'none' WHERE username = 'lucas'")
        .execute(env.db.pool())
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO accounts (id, username, password_hash, role, services_right, created_at,
                               password_changed_at)
         VALUES ('01X', 'intrus', 'h', 'readonly', 'manage', '2026-10-01T10:00:00.000Z',
                 '2026-10-01T10:00:00.000Z')",
    )
    .execute(env.db.pool())
    .await
    .unwrap();
    assert_eq!(right_writes(&env).await.len(), 4);
}

/// Les quatre profils de la spec (BR-GRANT-001) : A admin avec le droit, B admin sans, C lecture seule
/// sans, D lecture seule avec. Rend les jetons dans l'ordre A, B, C, D.
async fn four_profiles(env: &support::Env, api: &Api) -> [String; 4] {
    let a = env.account_with_token(api, "ada", Role::Admin).await;
    let b = env.account_with_token(api, "bob", Role::Admin).await;
    let c = env.account_with_token(api, "cleo", Role::ReadOnly).await;
    let d = env.account_with_token(api, "dan", Role::ReadOnly).await;
    env.grant_services("ada").await;
    env.grant_services("dan").await;
    [a, b, c, d]
}

/// BR-GRANT-001, BR-GRANT-003, BR-GRANT-004 : sur CHAQUE route de niveau « services » (existante ou
/// ajoutée plus tard : la table fait foi), un administrateur sans le droit et un compte en lecture
/// seule sans le droit sont refusés, quel que soit le client, avec le droit manquant en détail et un
/// refus consigné ; les comptes qui ont le droit passent la garde, administrateur ou non.
#[tokio::test]
async fn every_services_route_refuses_an_account_without_the_right_even_an_administrator() {
    let env = env().await;
    let api = Api::new(&env);
    let [a, b, c, d] = four_profiles(&env, &api).await;
    install_right_write_detector(&env).await;
    let before = snapshot(&env).await;

    let mut swept = 0;
    for endpoint in ENDPOINTS
        .iter()
        .filter(|endpoint| endpoint.access == Access::Services)
    {
        let path = concrete(endpoint.path);
        // Les refus identiques à moins d'une minute se regroupent : on espace les appels.
        env.clock.advance(time::Duration::seconds(61));
        for (who, token) in [("bob", &b), ("cleo", &c)] {
            let reply = api
                .call(endpoint.method.clone(), &path)
                .token(token)
                .json(&json!({}))
                .send()
                .await;
            assert_eq!(
                (reply.status, reply.code()),
                (StatusCode::FORBIDDEN, "FORBIDDEN_ROLE"),
                "{} {} doit refuser {who}",
                endpoint.method,
                endpoint.path
            );
            assert_eq!(
                reply.body["error"]["details"],
                json!({ "missing": "services" }),
                "{} {}",
                endpoint.method,
                endpoint.path
            );
            assert_eq!(
                reply.body["error"]["message"],
                "Tu n'as pas le droit de gérer les services de ce serveur."
            );
            env.clock.advance(time::Duration::seconds(61));
        }
        for (who, token) in [("ada", &a), ("dan", &d)] {
            let reply = api
                .call(endpoint.method.clone(), &path)
                .token(token)
                .json(&json!({}))
                .send()
                .await;
            assert!(
                reply.status != StatusCode::UNAUTHORIZED && reply.status != StatusCode::FORBIDDEN,
                "{} {} doit laisser passer {who} : {}",
                endpoint.method,
                endpoint.path,
                reply.status
            );
        }
        // Le refus est consigné sous l'action de la route, avec la cause « droits insuffisants ».
        let action = endpoint
            .audit
            .expect("une route de service est consignée")
            .code();
        for who in ["bob", "cleo"] {
            let reasons: Vec<Option<String>> = sqlx::query_scalar(
                "SELECT reason FROM audit_events WHERE outcome = 'denied' AND action = ? AND account = ?",
            )
            .bind(action)
            .bind(who)
            .fetch_all(env.db.pool())
            .await
            .unwrap();
            assert!(
                !reasons.is_empty(),
                "{who} : refus non consigné pour {action}"
            );
            assert!(
                reasons
                    .iter()
                    .all(|reason| reason.as_deref() == Some("droits insuffisants")),
                "{reasons:?}"
            );
        }
        swept += 1;
    }
    assert!(swept >= 1, "au moins la révélation des variables");
    assert_eq!(snapshot(&env).await, before, "rien n'a changé en base");
    assert!(right_writes(&env).await.is_empty());
}

/// BR-GRANT-001 : le droit n'ouvre RIEN d'autre que les services. Un compte en lecture seule qui a le
/// droit (profil D) est refusé sur toutes les routes d'administration (comptes, journal d'activité,
/// mise à jour de l'agent, mode attaque) et rien ne change ; le rôle d'administrateur sans le droit
/// (profil B) garde ses routes.
#[tokio::test]
async fn a_read_only_account_with_the_right_reaches_nothing_but_the_services() {
    let env = env().await;
    let api = Api::new(&env);
    let [_a, b, _c, d] = four_profiles(&env, &api).await;
    install_right_write_detector(&env).await;
    let before = snapshot(&env).await;
    let journal_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_events")
        .fetch_one(env.db.pool())
        .await
        .unwrap();

    let mut admin_routes = 0;
    for endpoint in ENDPOINTS
        .iter()
        .filter(|endpoint| endpoint.access == Access::Admin)
    {
        env.clock.advance(time::Duration::seconds(61));
        let reply = api
            .call(endpoint.method.clone(), &concrete(endpoint.path))
            .token(&d)
            .json(&json!({
                "username": "intrus", "password": OTHER_PASSWORD, "role": "admin",
                "confirmation": "dan", "active": true,
                "rights": ["services"], "services_right": "manage"
            }))
            .send()
            .await;
        assert_eq!(
            (reply.status, reply.code()),
            (StatusCode::FORBIDDEN, "FORBIDDEN_ROLE"),
            "{} {} doit refuser un compte en lecture seule, même avec le droit",
            endpoint.method,
            endpoint.path
        );
        // Un refus de rôle : aucun droit manquant en détail, le client ne confond pas les deux.
        assert_eq!(reply.body["error"]["details"], json!({}));
        admin_routes += 1;
    }
    assert!(
        admin_routes >= 8,
        "{admin_routes} routes d'administration balayées"
    );
    assert_eq!(snapshot(&env).await, before, "rien n'a changé en base");
    // Les refus sont consignés au nom de `dan` (une entrée par route), pas de succès en plus.
    let denials: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_events WHERE outcome = 'denied' AND account = 'dan'",
    )
    .fetch_one(env.db.pool())
    .await
    .unwrap();
    assert_eq!(usize::try_from(denials).unwrap(), admin_routes);
    let journal_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_events")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert_eq!(journal_after - journal_before, denials);
    // Il lit toujours ce que tout compte lit, et le journal d'activité lui reste fermé.
    for path in ["/me", "/services", "/machine"] {
        let reply = api.get(path).token(&d).send().await;
        assert_eq!(reply.status, StatusCode::OK, "{path}");
    }
    // Contrôle inverse : l'administrateur sans le droit garde les comptes et le journal.
    for path in ["/accounts", "/audit"] {
        let reply = api.get(path).token(&b).send().await;
        assert_eq!(reply.status, StatusCode::OK, "{path}");
    }
    assert!(right_writes(&env).await.is_empty());
}

/// BR-GRANT-002 et BR-GRANT-016 : un compte créé par la route naît sans le droit, quel que soit son
/// rôle et ce que le corps de la requête essaie de lui donner ; chaque ligne de la liste des comptes et
/// la description du compte courant portent `rights`.
#[tokio::test]
async fn a_created_account_is_born_without_the_right_and_rights_are_described_on_the_wire() {
    let env = env().await;
    let api = Api::new(&env);
    let marie = env.actor(&api, "marie", Role::Admin).await;
    env.grant_services("marie").await;
    install_right_write_detector(&env).await;

    for (name, role) in [("nouvel-admin", "admin"), ("nouveau-lecteur", "readonly")] {
        // Un client modifié glisse le droit dans le corps : rien ne le lit.
        let reply = api
            .act(
                &marie,
                Method::POST,
                "/accounts",
                json!({
                    "username": name, "password": OTHER_PASSWORD, "role": role,
                    "rights": ["services"], "services_right": "manage"
                }),
            )
            .await;
        assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.body);
        assert_eq!(reply.body["rights"], json!([]), "{name}");
        assert_eq!(env.services_right_of(name).await, "none", "{name}");
    }
    // La liste des comptes : une ligne par compte, `rights` partout, `services` pour marie seule.
    let list = api.get("/accounts").token(&marie.token).send().await;
    assert_eq!(list.status, StatusCode::OK);
    let accounts = list.body["accounts"].as_array().unwrap();
    assert_eq!(accounts.len(), 3);
    for account in accounts {
        let expected = if account["username"] == "marie" {
            json!(["services"])
        } else {
            json!([])
        };
        assert_eq!(account["rights"], expected, "{account}");
    }
    // Le compte courant, par `GET /me` et par la connexion.
    let me = api.get("/me").token(&marie.token).send().await;
    assert_eq!(me.body["account"]["rights"], json!(["services"]));
    let login = api.login("marie", PASSWORD).await;
    assert_eq!(login.body["account"]["rights"], json!(["services"]));
    let lecteur = api.login("nouveau-lecteur", OTHER_PASSWORD).await;
    assert_eq!(lecteur.body["account"]["rights"], json!([]));
    // Changer le rôle ne touche pas au droit (BR-GRANT-001 : deux axes indépendants).
    let victim = env.service.find("nouveau-lecteur").await.unwrap().id;
    env.grant_services("nouveau-lecteur").await;
    sqlx::query("DELETE FROM right_writes")
        .execute(env.db.pool())
        .await
        .unwrap();
    let promoted = api
        .act(
            &marie,
            Method::PATCH,
            &format!("/accounts/{victim}"),
            json!({ "role": "admin", "rights": [], "services_right": "none" }),
        )
        .await;
    assert_eq!(
        promoted.status,
        StatusCode::NO_CONTENT,
        "{:?}",
        promoted.body
    );
    assert_eq!(env.services_right_of("nouveau-lecteur").await, "manage");
    assert!(right_writes(&env).await.is_empty());
}

/// Ce que le balayage ne doit jamais modifier : comptes, rôles, droits, hachés, sessions.
async fn snapshot(env: &support::Env) -> Vec<String> {
    let mut lines: Vec<String> = sqlx::query_scalar(
        "SELECT id || '|' || username || '|' || role || '|' || services_right || '|' || password_hash FROM accounts ORDER BY id",
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
    let admin = env.actor(&api, "marie", Role::Admin).await;
    let lucas = env.account_with_token(&api, "lucas", Role::ReadOnly).await;

    let reply = api
        .act(
            &admin,
            Method::PUT,
            &format!("/accounts/{}/password", lucas_id(&env).await),
            json!({ "password": OTHER_PASSWORD }),
        )
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    assert_eq!(reply.body["sessions_closed"], 1);
    let reply = api.get("/me").token(&lucas).send().await;
    assert_eq!(
        (reply.status, reply.code()),
        (StatusCode::UNAUTHORIZED, "SESSION_REVOKED")
    );

    env.clock.advance(LIFETIME + time::Duration::seconds(1));
    let reply = api.get("/me").token(&admin.token).send().await;
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
    let admin = env.actor(&api, "marie", Role::Admin).await;

    let created = api
        .act(
            &admin,
            Method::POST,
            "/accounts",
            json!({ "username": "lucas", "password": PASSWORD, "role": "readonly" }),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.body);
    assert_eq!(created.body["username"], "lucas");
    assert_eq!(created.body["sessions_open"], 0);
    let lucas = created.body["id"].as_str().unwrap().to_owned();

    let taken = api
        .act(
            &admin,
            Method::POST,
            "/accounts",
            json!({ "username": "LUCAS", "password": PASSWORD, "role": "admin" }),
        )
        .await;
    assert_eq!(
        (taken.status, taken.code()),
        (StatusCode::CONFLICT, "USERNAME_TAKEN")
    );

    let weak = api
        .act(
            &admin,
            Method::POST,
            "/accounts",
            json!({ "username": "paul", "password": "abc", "role": "admin" }),
        )
        .await;
    assert_eq!(
        (weak.status, weak.code()),
        (StatusCode::UNPROCESSABLE_ENTITY, "WEAK_PASSWORD")
    );
    let rules = weak.body["error"]["details"]["rules"].as_array().unwrap();
    assert!(rules.contains(&json!("min_length")) && rules.contains(&json!("digit")));

    let list = api.get("/accounts").token(&admin.token).send().await;
    let accounts = list.body["accounts"].as_array().unwrap();
    assert_eq!(accounts.len(), 2);
    assert_eq!(accounts[0]["username"], "marie");
    assert_eq!(accounts[0]["sessions_open"], 1);
    assert!(accounts[0]["last_login_at"].is_string());
    assert!(accounts[1]["last_login_at"].is_null());

    let promoted = api
        .act(
            &admin,
            Method::PATCH,
            &format!("/accounts/{lucas}"),
            json!({ "role": "admin" }),
        )
        .await;
    assert_eq!(promoted.status, StatusCode::NO_CONTENT);
    assert_eq!(env.service.find("lucas").await.unwrap().role, Role::Admin);
}

#[tokio::test]
async fn the_last_administrator_cannot_be_demoted_or_deleted() {
    let env = env().await;
    let api = Api::new(&env);
    let admin = env.actor(&api, "marie", Role::Admin).await;
    let marie = env.service.find("marie").await.unwrap().id.to_string();

    let demote = api
        .act(
            &admin,
            Method::PATCH,
            &format!("/accounts/{marie}"),
            json!({ "role": "readonly" }),
        )
        .await;
    assert_eq!(
        (demote.status, demote.code()),
        (StatusCode::CONFLICT, "LAST_ADMIN")
    );

    let delete = api
        .act(
            &admin,
            Method::DELETE,
            &format!("/accounts/{marie}"),
            json!({ "confirmation": "marie" }),
        )
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
    let admin = env.actor(&api, "marie", Role::Admin).await;
    env.create("paul", Role::Admin).await;
    let marie = env.service.find("marie").await.unwrap().id.to_string();

    let refused = api
        .act(
            &admin,
            Method::DELETE,
            &format!("/accounts/{marie}"),
            json!({}),
        )
        .await;
    assert_eq!(
        (refused.status, refused.code()),
        (StatusCode::UNPROCESSABLE_ENTITY, "VALIDATION_ERROR")
    );
    assert_eq!(refused.body["error"]["details"]["field"], "confirmation");

    let done = api
        .act(
            &admin,
            Method::DELETE,
            &format!("/accounts/{marie}"),
            json!({ "confirmation": "Marie" }),
        )
        .await;
    assert_eq!(done.status, StatusCode::OK, "{:?}", done.body);
    assert_eq!(done.body["sessions_closed"], 1);
    let reply = api.get("/me").token(&admin.token).send().await;
    assert_eq!(
        (reply.status, reply.code()),
        (StatusCode::UNAUTHORIZED, "SESSION_REVOKED")
    );
}

#[tokio::test]
async fn revoking_sessions_and_changing_your_own_password_follow_the_rules() {
    let env = env().await;
    let api = Api::new(&env);
    let admin = env.actor(&api, "marie", Role::Admin).await;
    let lucas_current = env.actor(&api, "lucas", Role::ReadOnly).await;
    let lucas_other = api.token_of("lucas").await;
    let lucas = lucas_id(&env).await;

    // Un compte lecture seule change son propre mot de passe : l'autre session est fermée.
    let wrong = {
        let body = lucas_current
            .confirm_with(
                &api,
                &Method::PUT,
                "/me/password",
                json!({ "current": "Wrong-Horse-9999", "password": OTHER_PASSWORD }),
                "Wrong-Horse-9999",
            )
            .await;
        api.put("/me/password")
            .token(&lucas_current.token)
            .json(&body)
            .send()
            .await
    };
    assert_eq!(
        (wrong.status, wrong.code()),
        (StatusCode::UNPROCESSABLE_ENTITY, "WRONG_PASSWORD")
    );
    let changed = api
        .act(
            &lucas_current,
            Method::PUT,
            "/me/password",
            json!({ "current": PASSWORD, "password": OTHER_PASSWORD }),
        )
        .await;
    assert_eq!(changed.status, StatusCode::OK, "{:?}", changed.body);
    assert_eq!(changed.body["sessions_closed"], 1);
    assert_eq!(
        api.get("/me")
            .token(&lucas_current.token)
            .send()
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(
        api.get("/me").token(&lucas_other).send().await.code(),
        "SESSION_REVOKED"
    );

    // Un administrateur révoque les sessions de lucas.
    let revoked = api
        .act(
            &admin,
            Method::DELETE,
            &format!("/accounts/{lucas}/sessions"),
            json!({}),
        )
        .await;
    assert_eq!(revoked.status, StatusCode::OK);
    assert_eq!(revoked.body["sessions_closed"], 1);
    assert_eq!(
        api.get("/me")
            .token(&lucas_current.token)
            .send()
            .await
            .code(),
        "SESSION_REVOKED"
    );

    let unknown = api
        .act(
            &admin,
            Method::DELETE,
            "/accounts/01JNOSUCHACCOUNT0000000000/sessions",
            json!({}),
        )
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
    let lucas = env.actor(&api, "lucas", Role::ReadOnly).await;
    let token = lucas.token.clone();
    // Le corps CONFIRMÉ est envoyé tel quel les deux fois : même clé, même requête (un rejeu à l'identique).
    let body = lucas
        .confirm(
            &api,
            &Method::PUT,
            "/me/password",
            json!({ "current": PASSWORD, "password": OTHER_PASSWORD }),
        )
        .await;

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
    let lucas = env.actor(&api, "lucas", Role::ReadOnly).await;
    let token = lucas.token.clone();
    let body = lucas
        .confirm_with(
            &api,
            &Method::PUT,
            "/me/password",
            json!({ "current": "Wrong-Horse-9999", "password": OTHER_PASSWORD }),
            "Wrong-Horse-9999",
        )
        .await;
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

/// FIX:01M4BZN31A8Z8WN0WKNTCRTFFN (HRT-32) : une clé dont l'empreinte a été effacée par la migration
/// 0008 n'est jamais ré-exécutée, quel que soit son état, et son résultat reste lisible.
#[tokio::test]
async fn a_key_whose_fingerprint_was_erased_is_never_executed_again_but_stays_readable() {
    let env = env().await;
    let api = Api::new(&env);
    let token = env.account_with_token(&api, "lucas", Role::ReadOnly).await;
    let account = env.service.find("lucas").await.unwrap().id;
    let hash_before = env.hash_of(&account).await;
    for (key, status, result) in [
        ("OLD-RUNNING", "running", None),
        (
            "OLD-DONE",
            "succeeded",
            Some(r#"{"status":200,"body":{"ok":true}}"#),
        ),
        ("OLD-CUT", "interrupted", None),
    ] {
        sqlx::query(
            "INSERT INTO operations (id, account_id, kind, request_hash, status, result_json, created_at)
             VALUES (?, ?, 'PUT /me/password', '', ?, ?, '2026-10-07T10:00:00.000Z')",
        )
        .bind(key)
        .bind(account.as_str())
        .bind(status)
        .bind(result)
        .execute(env.db.pool())
        .await
        .unwrap();
        let body = json!({ "current": PASSWORD, "password": OTHER_PASSWORD });
        let replay = api
            .put("/me/password")
            .token(&token)
            .key(key)
            .json(&body)
            .send()
            .await;
        assert_eq!(
            (replay.status, replay.code()),
            (StatusCode::CONFLICT, "CONFLICT"),
            "{key}"
        );
        assert!(replay.headers.get("idempotent-replayed").is_none(), "{key}");
        // Rien n'a été exécuté : le mot de passe est resté le même.
        assert_eq!(env.hash_of(&account).await, hash_before, "{key}");
        let state = api
            .get(&format!("/operations/{key}"))
            .token(&token)
            .send()
            .await;
        assert_eq!(state.status, StatusCode::OK, "{key}");
        assert_eq!(state.body["status"], status, "{key}");
    }
}

#[tokio::test]
async fn a_key_still_running_answers_409_and_each_account_has_its_own_keys() {
    let env = env().await;
    let api = Api::new(&env);
    let lucas = env.account_with_token(&api, "lucas", Role::ReadOnly).await;
    let paul = env.actor(&api, "paul", Role::ReadOnly).await;
    let lucas_account = env.service.find("lucas").await.unwrap().id;
    let key = hearth_agent::domain::operations::OperationKey::parse(KEY).unwrap();
    let body = json!({ "current": PASSWORD, "password": OTHER_PASSWORD });
    let request = env
        .operations
        .fingerprint("PUT", "/me/password", body.to_string().as_bytes());
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
    // Chez paul la même clé est la sienne : l'acte, confirmé, s'exécute.
    let confirmed = paul
        .confirm(&api, &Method::PUT, "/me/password", body.clone())
        .await;
    let own = api
        .put("/me/password")
        .token(&paul.token)
        .key(KEY)
        .json(&confirmed)
        .send()
        .await;
    assert_eq!(own.status, StatusCode::OK, "{:?}", own.body);
    let paul_state = api
        .get(&format!("/operations/{KEY}"))
        .token(&paul.token)
        .send()
        .await;
    assert_eq!(paul_state.body["status"], "succeeded");
}

#[tokio::test]
async fn a_key_reused_for_another_request_is_refused_without_running() {
    let env = env().await;
    let api = Api::new(&env);
    let admin = env.actor(&api, "marie", Role::Admin).await;
    let lucas = env.create("lucas", Role::ReadOnly).await;
    let path = format!("/accounts/{}/password", lucas.id);
    let original = admin
        .confirm(
            &api,
            &Method::PUT,
            &path,
            json!({ "password": OTHER_PASSWORD }),
        )
        .await;
    let first = api
        .put(&path)
        .token(&admin.token)
        .key(KEY)
        .json(&original)
        .send()
        .await;
    assert_eq!(first.status, StatusCode::OK, "{:?}", first.body);

    // Même clé, autre méthode et autre chemin : jamais le résultat du PUT, jamais exécutée.
    let other = api
        .delete(&format!("/accounts/{}", lucas.id))
        .token(&admin.token)
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
        .token(&admin.token)
        .key(KEY)
        .json(&json!({ "password": "Third-Pass-9999" }))
        .send()
        .await;
    assert_eq!(changed_body.code(), "IDEMPOTENCY_KEY_REUSED");

    // La requête d'origine, rejouée à l'identique, rend son résultat.
    let replay = api
        .put(&format!("/accounts/{}/password", lucas.id))
        .token(&admin.token)
        .key(KEY)
        .json(&original)
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
    let admin = env.actor(&api, "marie", Role::Admin).await;
    let body = admin
        .confirm(
            &api,
            &Method::POST,
            "/accounts",
            json!({ "username": "lucas", "password": PASSWORD, "role": "readonly" }),
        )
        .await;
    let first = api
        .post("/accounts")
        .token(&admin.token)
        .key(KEY)
        .json(&body)
        .send()
        .await;
    let second = api
        .post("/accounts")
        .token(&admin.token)
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
    let _ = env.account_with_token(&api, "carl", Role::ReadOnly).await;
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
    let marie = env.actor(&api, "marie", Role::Admin).await;
    // L'acteur a TOUS les pouvoirs (administrateur avec le droit) et le détecteur d'écriture du droit
    // est posé : aucune des routes balayées ci-dessous ne doit écrire la colonne (BR-GRANT-005).
    env.grant_services("marie").await;
    install_right_write_detector(&env).await;

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
        let own_actor = if endpoint.path == "/me/password" {
            Some(env.login_actor(&api, "carl").await)
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
                api.act(
                    own_actor.as_ref().unwrap(),
                    Method::PUT,
                    "/me/password",
                    json!({ "current": PASSWORD, "password": OTHER_PASSWORD }),
                )
                .await
            }
            ("POST", "/accounts") => {
                api.act(
                    &marie,
                    Method::POST,
                    "/accounts",
                    // Des champs de droit glissés dans le corps : ignorés, jamais lus.
                    json!({
                        "username": "nouveau", "password": OTHER_PASSWORD, "role": "readonly",
                        "rights": ["services"], "services_right": "manage"
                    }),
                )
                .await
            }
            ("PATCH", "/accounts/{id}") => {
                api.act(
                    &marie,
                    Method::PATCH,
                    &format!("/accounts/{}", role_victim.id),
                    json!({ "role": "admin", "rights": ["services"], "services_right": "manage" }),
                )
                .await
            }
            ("DELETE", "/accounts/{id}") => {
                api.act(
                    &marie,
                    Method::DELETE,
                    &format!("/accounts/{}", delete_victim.id),
                    json!({}),
                )
                .await
            }
            ("PUT", "/accounts/{id}/password") => {
                api.act(
                    &marie,
                    Method::PUT,
                    &format!("/accounts/{}/password", password_victim.id),
                    json!({ "password": OTHER_PASSWORD }),
                )
                .await
            }
            ("DELETE", "/accounts/{id}/sessions") => {
                api.act(
                    &marie,
                    Method::DELETE,
                    &format!("/accounts/{}/sessions", sessions_victim.id),
                    json!({}),
                )
                .await
            }
            ("PUT", "/security/attack-mode") => {
                let reply = api
                    .act(
                        &marie,
                        Method::PUT,
                        "/security/attack-mode",
                        json!({ "active": true }),
                    )
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
                api.act(
                    &marie,
                    Method::PUT,
                    "/me/reauth",
                    json!({ "password": "each" }),
                )
                .await
            }
            (method, path) => {
                panic!("route modifiante sans scénario de réussite : {method} {path}")
            }
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
    // BR-GRANT-005 : aucune de ces routes n'a écrit la colonne du droit. Ni le compte créé avec un droit
    // glissé dans son corps, ni le compte promu administrateur ne l'ont reçu.
    assert_eq!(right_writes(&env).await, Vec::<String>::new());
    assert_eq!(env.services_right_of("nouveau").await, "none");
    assert_eq!(env.services_right_of("v-role").await, "none");
    assert_eq!(env.services_right_of("marie").await, "manage");

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
            | "/agent/update" | "/agent/update/last" | "/services" => endpoint.path.to_owned(),
            "/operations/{id}" => "/operations/INCONNUE".to_owned(),
            // Un POST qui ne modifie rien : il relance la détection du moteur (HRT-61, lecture seule).
            "/engine/recheck" => {
                let reply = api
                    .call(Method::POST, "/engine/recheck")
                    .token(&marie.token)
                    .send()
                    .await;
                assert!(
                    reply.status.is_success(),
                    "/engine/recheck : {:?}",
                    reply.body
                );
                reads += 1;
                continue;
            }
            "/stream" => continue, // le flux se teste en WebSocket (stream_https.rs)
            // Un POST sans effet : testé à part (device_proof.rs).
            "/sessions/challenge" => continue,
            other => panic!("route de lecture sans scénario : {other}"),
        };
        let reply = api.get(&path).token(&marie.token).send().await;
        assert!(
            reply.status.is_success() || reply.status == StatusCode::NOT_FOUND,
            "{path} : {:?}",
            reply.body
        );
        reads += 1;
    }
    assert!(reads >= 6, "{reads} routes de lecture balayées");
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

/// FIX:01M4CY8BVM3MV9769QWNDNW7VT (suite, review r1 de la PR #48) : la ROUTE rend `peaks` sur l'heure, un
/// maximum par échantillon ; elle n'en rend pas sur les fenêtres à une seconde. Rouge si elle cesse d'en rendre :
/// le client retomberait en silence sur les moyennes.
#[tokio::test]
async fn the_hour_window_route_hands_back_one_peak_per_sample_and_short_windows_none() {
    use std::sync::Arc;

    use hearth_agent::application::metrics::MetricsService;
    use hearth_agent::application::ports::MonotonicClock as _;
    use support::probe::{FakeGpu, FakeSystem};
    use time::Duration;

    let env = env().await;
    let metrics = Arc::new(MetricsService::new(
        Arc::new(FakeSystem::default()),
        Arc::new(FakeGpu),
        env.clock.clone(),
        env.monotonic.clone(),
    ));
    // Deux pas de 10 s pleins, un échantillon par seconde, temps piloté (aucune attente).
    let start = env.monotonic.elapsed().whole_seconds();
    env.monotonic
        .advance(Duration::seconds(10 - start.rem_euclid(10)));
    for _ in 0..20 {
        metrics.sample_once().await.unwrap();
        env.clock.advance(Duration::seconds(1));
        env.monotonic.advance(Duration::seconds(1));
    }
    let mut state = support::api::state(&env);
    state.metrics = metrics;
    let api = Api::from_state(state);
    let token = env.account_with_token(&api, "lucas", Role::ReadOnly).await;

    let hour = api
        .get("/metrics/history?window=1h")
        .token(&token)
        .send()
        .await;
    assert_eq!(hour.status, StatusCode::OK, "{:?}", hour.body);
    let samples = hour.body["samples"].as_array().unwrap();
    let peaks = hour.body["peaks"]
        .as_array()
        .expect("la route rend `peaks`");
    assert_eq!(samples.len(), 2);
    assert_eq!(peaks.len(), samples.len(), "un maximum par échantillon");
    for (mean, peak) in samples.iter().zip(peaks) {
        assert!(peak["cpu"].as_f64().unwrap() >= mean["cpu"].as_f64().unwrap());
        assert!(
            peak["mem_used_bytes"].as_u64().unwrap() >= mean["mem"]["used_bytes"].as_u64().unwrap()
        );
    }
    for window in ["1m", "5m"] {
        let reply = api
            .get(&format!("/metrics/history?window={window}"))
            .token(&token)
            .send()
            .await;
        assert_eq!(reply.status, StatusCode::OK, "{window}");
        assert!(
            !reply.body["samples"].as_array().unwrap().is_empty(),
            "{window}"
        );
        assert!(
            reply.body.get("peaks").is_none(),
            "{window} : à une seconde, pas de maxima"
        );
    }
}

/// FIX:01M4D6KNQRS959ZRJ38TJBQE6N : l'effacement différé des anciennes empreintes se lit dans l'état de l'agent,
/// pour l'administrateur seulement ; absent quand rien n'est en attente.
#[tokio::test]
async fn a_deferred_erasure_is_told_to_administrators_only_and_absent_otherwise() {
    let env = env().await;
    let calm = Api::new(&env);
    let admin = env.account_with_token(&calm, "marie", Role::Admin).await;
    let reply = calm.get("/security").token(&admin).send().await;
    assert!(
        reply.body.get("erasure_pending").is_none(),
        "rien en attente : champ absent"
    );

    let mut state = support::api::state(&env);
    state.erasure_pending = true;
    let pending = Api::from_state(state);
    let readonly = env
        .account_with_token(&pending, "lucas", Role::ReadOnly)
        .await;
    let admin = env.account_with_token(&pending, "chef", Role::Admin).await;
    let seen = pending.get("/security").token(&admin).send().await;
    assert_eq!(seen.body["erasure_pending"], true);
    let hidden = pending.get("/security").token(&readonly).send().await;
    assert!(
        hidden.body.get("erasure_pending").is_none(),
        "jamais pour un compte lecture seule"
    );
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

// ---------------------------------------------------------------------------------------------
// BR-GRANT-005 : le droit ne se donne pas par le réseau, et ne se PREND pas non plus
// ---------------------------------------------------------------------------------------------

/// Le mot de passe qu'un administrateur sans le droit voudrait imposer à un compte qui l'a.
const TAKEOVER_PASSWORD: &str = "Taken-Over-55-Pass";

/// Le refus dédié (BR-GRANT-005), au mot près.
const HOLDER_PROTECTED: &str = "Ce compte peut gérer les services de ce serveur : seul un compte qui a ce droit, ou la ligne de commande du serveur, peut modifier ses moyens de connexion.";

async fn id_of(env: &support::Env, name: &str) -> String {
    env.service.find(name).await.unwrap().id.to_string()
}

fn password_body() -> serde_json::Value {
    json!({ "password": TAKEOVER_PASSWORD })
}

/// Les entrées du journal d'un acte : (résultat, raison), pour l'acteur et la cible donnés.
async fn journal_of(
    env: &support::Env,
    action: &str,
    actor: &str,
    target: &str,
) -> Vec<(String, Option<String>)> {
    sqlx::query_as(
        "SELECT outcome, reason FROM audit_events WHERE action = ? AND account = ? AND target = ? ORDER BY id",
    )
    .bind(action)
    .bind(actor)
    .bind(target)
    .fetch_all(env.db.pool())
    .await
    .unwrap()
}

/// B1 (bloquant de la review) : un administrateur SANS le droit ne définit pas le mot de passe d'un compte
/// qui l'a (administrateur ou lecture seule). Refus dédié, au mot près, droit manquant en détail, refus
/// consigné « droits insuffisants », ni haché ni sessions touchés. Non-régression : la même demande est
/// acceptée sur un compte qui n'a pas le droit, et un compte qui a le droit est accepté partout.
#[tokio::test]
async fn an_administrator_without_the_right_cannot_define_the_password_of_a_holder() {
    let env = env().await;
    let api = Api::new(&env);
    let ada = env.actor(&api, "ada", Role::Admin).await;
    let bob = env.actor(&api, "bob", Role::Admin).await;
    env.create("cleo", Role::ReadOnly).await;
    env.create("dan", Role::ReadOnly).await;
    env.grant_services("ada").await;
    env.grant_services("dan").await;
    install_right_write_detector(&env).await;
    let (ada_id, dan_id, cleo_id, bob_id) = (
        id_of(&env, "ada").await,
        id_of(&env, "dan").await,
        id_of(&env, "cleo").await,
        id_of(&env, "bob").await,
    );
    let ada_hash = env
        .hash_of(&env.service.find("ada").await.unwrap().id)
        .await;
    let ada_sessions = env
        .session_ids(&env.service.find("ada").await.unwrap().id)
        .await;

    // Refusé sur un titulaire administrateur (ada) et sur un titulaire en lecture seule (dan).
    for (name, id) in [("ada", &ada_id), ("dan", &dan_id)] {
        env.clock.advance(time::Duration::seconds(61));
        let reply = api
            .act(
                &bob,
                Method::PUT,
                &format!("/accounts/{id}/password"),
                password_body(),
            )
            .await;
        assert_eq!(
            (reply.status, reply.code()),
            (StatusCode::FORBIDDEN, "FORBIDDEN_ROLE"),
            "{name} : {:?}",
            reply.body
        );
        assert_eq!(reply.body["error"]["message"], HOLDER_PROTECTED, "{name}");
        assert_eq!(
            reply.body["error"]["details"],
            json!({ "missing": "services" })
        );
        assert_eq!(
            journal_of(&env, "account.password", "bob", name).await,
            vec![("denied".to_owned(), Some("droits insuffisants".to_owned()))],
            "le refus est consigné, sans entrée de réussite"
        );
        // Le titulaire n'a pas été touché, et le mot de passe imposé n'ouvre rien.
        assert_ne!(
            api.login(name, TAKEOVER_PASSWORD).await.status,
            StatusCode::CREATED
        );
    }
    assert_eq!(
        env.hash_of(&env.service.find("ada").await.unwrap().id)
            .await,
        ada_hash
    );
    assert_eq!(
        env.session_ids(&env.service.find("ada").await.unwrap().id)
            .await,
        ada_sessions
    );
    // Rien d'autre n'a changé : le droit des titulaires est intact, aucune écriture de la colonne.
    assert_eq!(env.services_right_of("ada").await, "manage");
    assert_eq!(env.services_right_of("dan").await, "manage");
    assert!(right_writes(&env).await.is_empty());

    // Non-régression : sur un compte sans le droit, et sur lui-même, l'administrateur sans le droit passe.
    for (name, id) in [("cleo", &cleo_id), ("bob", &bob_id)] {
        let reply = api
            .act(
                &bob,
                Method::PUT,
                &format!("/accounts/{id}/password"),
                password_body(),
            )
            .await;
        assert_eq!(reply.status, StatusCode::OK, "{name} : {:?}", reply.body);
        assert_eq!(
            api.login(name, TAKEOVER_PASSWORD).await.status,
            StatusCode::CREATED
        );
    }

    // Un compte qui a le droit passe : sur un autre titulaire, sur un compte sans le droit, sur lui-même.
    for (name, id) in [("dan", &dan_id), ("cleo", &cleo_id), ("ada", &ada_id)] {
        let reply = api
            .act(
                &ada,
                Method::PUT,
                &format!("/accounts/{id}/password"),
                json!({ "password": OTHER_PASSWORD }),
            )
            .await;
        assert_eq!(
            reply.status,
            StatusCode::OK,
            "ada sur {name} : {:?}",
            reply.body
        );
    }
    assert!(right_writes(&env).await.is_empty());
}

/// La règle suit le droit LU à cet instant : retiré par le serveur, le compte redevient un compte comme les
/// autres ; accordé à l'administrateur qui demande, il passe ; la ligne de commande du serveur passe
/// toujours (c'est le serveur, BR-GRANT-005).
#[tokio::test]
async fn the_protection_follows_the_right_read_at_the_time_and_the_server_always_passes() {
    use hearth_agent::domain::accounts::Authority;

    let env = env().await;
    let api = Api::new(&env);
    env.create("ada", Role::Admin).await;
    let bob = env.actor(&api, "bob", Role::Admin).await;
    let ada = env.service.find("ada").await.unwrap().id;
    env.grant_services("ada").await;
    let path = format!("/accounts/{ada}/password");

    let refused = api.act(&bob, Method::PUT, &path, password_body()).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{:?}", refused.body);
    // Le serveur (la ligne de commande) passe, quel que soit le droit de la cible.
    env.service
        .set_password(
            &ada,
            support::secret(OTHER_PASSWORD),
            &Authority::Server,
            support::by(),
        )
        .await
        .unwrap();
    // L'administrateur reçoit le droit sur le serveur : il passe, sur un titulaire.
    env.grant_services("bob").await;
    let accepted = api.act(&bob, Method::PUT, &path, password_body()).await;
    assert_eq!(accepted.status, StatusCode::OK, "{:?}", accepted.body);
    // Le droit de la cible est retiré sur le serveur : même un administrateur sans le droit passe.
    env.revoke_services("bob").await;
    env.revoke_services("ada").await;
    let accepted = api.act(&bob, Method::PUT, &path, password_body()).await;
    assert_eq!(accepted.status, StatusCode::OK, "{:?}", accepted.body);
}

/// Inventaire de BR-GRANT-005, les actes que `CLASSES` (mod.rs) déclare « sans danger » : un administrateur
/// sans le droit les accomplit sur un titulaire SANS que cela lui donne le compte ni le droit.
///
/// - changement de rôle : le droit reste, ni mot de passe ni session ne change ;
/// - fermeture des sessions : le titulaire perd ses sessions, garde son droit, et la route n'inscrit aucun
///   poste ni mot de passe ;
/// - suppression : le droit disparaît AVEC la ligne du compte ; un compte recréé sous le même nom a un autre
///   identifiant, naît sans le droit, et le mot de passe choisi à la création n'ouvre aucun acte de service.
#[tokio::test]
async fn the_acts_declared_harmless_do_not_hand_a_holder_to_an_administrator_without_the_right() {
    let env = env().await;
    let api = Api::new(&env);
    let ada = env.actor(&api, "ada", Role::Admin).await;
    let bob = env.actor(&api, "bob", Role::Admin).await;
    env.create("dan", Role::ReadOnly).await;
    env.grant_services("ada").await;
    env.grant_services("dan").await;
    install_right_write_detector(&env).await;
    let (ada_id, dan_id) = (id_of(&env, "ada").await, id_of(&env, "dan").await);

    // Changement de rôle d'un titulaire : accepté, le droit reste, le mot de passe aussi.
    let reply = api
        .act(
            &bob,
            Method::PATCH,
            &format!("/accounts/{dan_id}"),
            json!({ "role": "admin" }),
        )
        .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{:?}", reply.body);
    assert_eq!(env.services_right_of("dan").await, "manage");
    assert_eq!(api.login("dan", PASSWORD).await.status, StatusCode::CREATED);
    assert_ne!(
        api.login("dan", TAKEOVER_PASSWORD).await.status,
        StatusCode::CREATED
    );

    // Fermeture des sessions d'un titulaire : acceptée, le droit reste, sa session est fermée, et
    // l'administrateur ne tient toujours aucun jeton qui passe le niveau « services ».
    let reply = api
        .act(
            &bob,
            Method::DELETE,
            &format!("/accounts/{ada_id}/sessions"),
            json!({}),
        )
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    assert_eq!(env.services_right_of("ada").await, "manage");
    assert_eq!(
        api.get("/me").token(&ada.token).send().await.status,
        StatusCode::UNAUTHORIZED
    );
    let env_route = api.get("/services/c_x/env").token(&bob.token).send().await;
    assert_eq!(env_route.status, StatusCode::FORBIDDEN);
    assert_eq!(api.login("ada", PASSWORD).await.status, StatusCode::CREATED);

    // Suppression puis recréation sous le même nom : le droit ne renaît JAMAIS.
    let reply = api
        .act(
            &bob,
            Method::DELETE,
            &format!("/accounts/{ada_id}"),
            json!({}),
        )
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM accounts WHERE id = ?")
        .bind(&ada_id)
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert_eq!(rows, 0, "le droit disparaît avec la ligne du compte");
    let reply = api
        .act(
            &bob,
            Method::POST,
            "/accounts",
            json!({ "username": "ada", "password": TAKEOVER_PASSWORD, "role": "admin" }),
        )
        .await;
    assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.body);
    let new_ada = id_of(&env, "ada").await;
    assert_ne!(new_ada, ada_id, "un autre identifiant");
    assert_eq!(env.services_right_of("ada").await, "none");
    let login = api.login("ada", TAKEOVER_PASSWORD).await;
    assert_eq!(login.status, StatusCode::CREATED, "{:?}", login.body);
    let token = login.body["token"].as_str().unwrap().to_owned();
    for endpoint in ENDPOINTS.iter().filter(|e| e.access == Access::Services) {
        let reply = api
            .call(endpoint.method.clone(), &concrete(endpoint.path))
            .token(&token)
            .json(&json!({}))
            .send()
            .await;
        assert_eq!(
            reply.status,
            StatusCode::FORBIDDEN,
            "{} {}",
            endpoint.method,
            endpoint.path
        );
    }
    // Aucun de ces actes n'a écrit la colonne du droit (la création du compte la laisse à son défaut).
    assert!(right_writes(&env).await.is_empty());
}

/// Garde générale (B1) : AUCUN enchaînement d'appels réseau partant d'un administrateur sans le droit
/// n'aboutit à une requête acceptée au niveau `Services`. Le balayage part de la TABLE : chaque route
/// d'administration qui modifie, existante ou ajoutée plus tard, est appelée par `bob` sur chaque
/// compte (titulaires administrateur et lecture seule, compte sans droit, compte inexistant) avec le
/// mot de passe qu'il voudrait imposer ; puis il supprime et recrée les titulaires sous leur nom. Il essaie
/// ensuite ce mot de passe sur tous les comptes : tout jeton obtenu doit être refusé sur CHAQUE route de
/// niveau `Services`, et aucun compte qui a le droit ne doit s'ouvrir avec ce mot de passe.
///
/// Exclues du balayage, avec la raison : la mise à jour de l'agent (binaire signé seulement, jamais une
/// entrée du demandeur : `update_http.rs`) et le mode attaque (réglage du serveur entier, ne vise aucun
/// compte ; l'activer ferait refuser les connexions du balayage lui-même).
#[tokio::test]
async fn no_chain_of_calls_from_an_administrator_without_the_right_reaches_a_services_request() {
    let env = env().await;
    let api = Api::new(&env);
    let _ada = env.actor(&api, "ada", Role::Admin).await;
    let bob = env.actor(&api, "bob", Role::Admin).await;
    env.create("cleo", Role::ReadOnly).await;
    env.create("dan", Role::ReadOnly).await;
    env.grant_services("ada").await;
    env.grant_services("dan").await;
    install_right_write_detector(&env).await;
    let originals = [
        ("ada", id_of(&env, "ada").await),
        ("dan", id_of(&env, "dan").await),
        ("cleo", id_of(&env, "cleo").await),
    ];

    let swept_routes: Vec<_> = ENDPOINTS
        .iter()
        .filter(|e| e.access == Access::Admin && e.modifies())
        .filter(|e| !matches!(e.path, "/agent/update" | "/security/attack-mode"))
        .collect();
    assert!(
        swept_routes.len() >= 4,
        "{} routes balayées",
        swept_routes.len()
    );

    // Phase 1 : chaque route sur chaque compte (hors suppressions), le tout en visant ce que `bob`
    // pourrait vouloir prendre. `bob` lui-même n'est pas visé : ses propres actes ferment sa session.
    let mut targets: Vec<String> = originals.iter().map(|(_, id)| id.clone()).collect();
    targets.push("01JNOSUCHACCOUNT0000000000".to_owned());
    let mut number = 0;
    for endpoint in swept_routes
        .iter()
        .filter(|e| !(e.method == Method::DELETE && e.path == "/accounts/{id}"))
    {
        for target in &targets {
            number += 1;
            env.clock.advance(time::Duration::seconds(61));
            let path = endpoint.path.replace("{id}", target);
            let body = json!({
                "username": format!("mallory{number}"), "password": TAKEOVER_PASSWORD,
                "role": "admin"
            });
            let reply = api.act(&bob, endpoint.method.clone(), &path, body).await;
            assert!(
                reply.status != StatusCode::UNAUTHORIZED,
                "{} {path} : la session de bob doit tenir : {:?}",
                endpoint.method,
                reply.body
            );
        }
    }
    // Les titulaires n'ont pas changé de mot de passe : celui que bob voulait imposer n'ouvre rien.
    for name in ["ada", "dan"] {
        assert_ne!(
            api.login(name, TAKEOVER_PASSWORD).await.status,
            StatusCode::CREATED,
            "{name} ne doit pas s'ouvrir avec le mot de passe de bob"
        );
        assert_eq!(env.services_right_of(name).await, "manage");
    }

    // Phase 2 : suppression de chaque compte puis recréation sous le même nom, mot de passe choisi par bob.
    for (name, id) in &originals {
        let reply = api
            .act(&bob, Method::DELETE, &format!("/accounts/{id}"), json!({}))
            .await;
        assert_eq!(reply.status, StatusCode::OK, "{name} : {:?}", reply.body);
        let reply = api
            .act(
                &bob,
                Method::POST,
                "/accounts",
                json!({ "username": name, "password": TAKEOVER_PASSWORD, "role": "admin" }),
            )
            .await;
        assert_eq!(
            reply.status,
            StatusCode::CREATED,
            "{name} : {:?}",
            reply.body
        );
    }
    // Aucun compte de départ n'existe plus sous son identifiant, et aucun compte n'a le droit.
    for (name, id) in &originals {
        assert_ne!(
            &id_of(&env, name).await,
            id,
            "{name} : un autre identifiant"
        );
    }
    let holders: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM accounts WHERE services_right <> 'none'")
            .fetch_one(env.db.pool())
            .await
            .unwrap();
    assert_eq!(
        holders, 0,
        "le droit ne renaît jamais, ni par suppression ni par création"
    );

    // Phase 3 : le mot de passe de bob sur TOUS les comptes ; tout jeton obtenu est refusé partout au niveau
    // « services ».
    let names: Vec<String> = sqlx::query_scalar("SELECT username FROM accounts ORDER BY id")
        .fetch_all(env.db.pool())
        .await
        .unwrap();
    assert!(names.len() >= 5, "{names:?}");
    let mut opened = 0;
    for name in &names {
        let login = api.login(name, TAKEOVER_PASSWORD).await;
        if login.status != StatusCode::CREATED {
            continue;
        }
        opened += 1;
        let token = login.body["token"].as_str().unwrap().to_owned();
        assert_eq!(env.services_right_of(name).await, "none", "{name}");
        for endpoint in ENDPOINTS.iter().filter(|e| e.access == Access::Services) {
            let reply = api
                .call(endpoint.method.clone(), &concrete(endpoint.path))
                .token(&token)
                .json(&json!({}))
                .send()
                .await;
            assert_eq!(
                (reply.status, reply.code()),
                (StatusCode::FORBIDDEN, "FORBIDDEN_ROLE"),
                "{name} : {} {} doit rester refusé",
                endpoint.method,
                endpoint.path
            );
        }
    }
    assert!(
        opened >= 3,
        "le balayage a bien ouvert des comptes : {opened}"
    );
    // Et `bob`, avec sa propre session, est refusé lui aussi.
    for endpoint in ENDPOINTS.iter().filter(|e| e.access == Access::Services) {
        let reply = api
            .call(endpoint.method.clone(), &concrete(endpoint.path))
            .token(&bob.token)
            .json(&json!({}))
            .send()
            .await;
        assert_eq!(
            reply.status,
            StatusCode::FORBIDDEN,
            "bob : {}",
            endpoint.path
        );
    }
    assert!(right_writes(&env).await.is_empty());
}

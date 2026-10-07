//! Le mode attaque sur l'API HTTPS et le flux (HRT-25, ADR-0025, BR-TRUST-013, 018, 028, 030 à
//! 032) : `PUT /security/attack-mode` et sa garde (administrateur, mot de passe, clé inscrite ET
//! prouvée), `GET /security`, une session présentée seule refusée sur chaque route et sur le flux, sans
//! rien révéler. Vraie base SQLite, vraies signatures Ed25519, vrai agent TLS pour le flux.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::http::StatusCode;
use hearth_agent::application::ports::IdentityStore;
use hearth_agent::domain::accounts::Role;
use hearth_agent::domain::session_token::SessionToken;
use hearth_agent::domain::trust::attack_mode::EndHow;
use hearth_agent::entrypoint::http::{Access, ENDPOINTS};
use hearth_agent::infrastructure::tls::FileIdentityStore;
use hearth_proto::device_proof::Binding;
use hearth_proto::fingerprint::Fingerprint;
use hearth_proto::stream::{SecurityMessage, ServerMessage, SessionNotice, Topic};
use serde_json::{Value, json};
use support::api::{Api, Reply};
use support::device::{DeviceKey, attack_mode_body, device_json, login_token};
use support::https::{self, Agent};
use support::probe::metering;
use support::ws::{self, End, WsClient};
use support::{CLIENT_ADDR, Env, PASSWORD, SERVER_FINGERPRINT, by, client_at, env, secret};
use time::Duration;

const WRONG: &str = "Wrong-Horse-9999";
const ELSEWHERE: &str = "10.0.0.99";

fn fingerprint() -> Fingerprint {
    Fingerprint::from_bytes(SERVER_FINGERPRINT)
}

/// Un administrateur dont la clé est inscrite : sa session est ouverte AVEC la preuve de la clé.
async fn admin_with_key(env: &Env, api: &Api, name: &str) -> (DeviceKey, String) {
    env.create(name, Role::Admin).await;
    let key = DeviceKey::new();
    let token = login_token(api, &key, name, PASSWORD).await;
    (key, token)
}

async fn put(api: &Api, token: &str, body: &Value) -> Reply {
    api.put("/security/attack-mode")
        .token(token)
        .json(body)
        .send()
        .await
}

async fn challenge(api: &Api, username: &str, purpose: &str) -> String {
    let reply = api
        .post("/sessions/challenge")
        .json(&json!({ "username": username, "purpose": purpose }))
        .send()
        .await;
    reply.body["challenge"].as_str().unwrap().to_owned()
}

fn hash_of(token: &str) -> [u8; 32] {
    *SessionToken::parse(token).unwrap().hash().as_bytes()
}

async fn scalar(env: &Env, sql: &'static str) -> i64 {
    sqlx::query_scalar(sql)
        .fetch_one(env.db.pool())
        .await
        .unwrap()
}

async fn active(env: &Env) -> i64 {
    scalar(env, "SELECT active FROM attack_mode WHERE id = 1").await
}

async fn journal_of(env: &Env, action: &str) -> Vec<(String, Option<String>, Option<String>)> {
    env.audit_recorder.flush_all().await;
    sqlx::query_as("SELECT outcome, account, reason FROM audit_events WHERE action = ? ORDER BY id")
        .bind(action)
        .fetch_all(env.db.pool())
        .await
        .unwrap()
}

// ---------------------------------------------------------------------------------------------
// L'activation : administrateur + mot de passe + clé inscrite et prouvée
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn an_administrator_with_a_proved_key_and_the_password_enables_then_disables_the_mode() {
    let env = env().await;
    let api = Api::new(&env);
    let (key, token) = admin_with_key(&env, &api, "marie").await;

    let body = attack_mode_body(&api, &key, "marie", &token, true, PASSWORD).await;
    let reply = put(&api, &token, &body).await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    assert_eq!(reply.body["state"], "active");
    assert!(reply.body["since"].is_string());
    assert_eq!(active(&env).await, 1);

    // L'état est lisible tout de suite, par la route et par le message du flux.
    let state = api.get("/security").token(&token).send().await;
    assert_eq!(state.body["attack_mode"]["state"], "active");

    // Marie, administratrice, reste reconnue une fois le mode actif : session + adresse retenue.
    let body = attack_mode_body(&api, &key, "marie", &token, false, PASSWORD).await;
    let reply = put(&api, &token, &body).await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    assert_eq!(reply.body["state"], "off");
    assert_eq!(reply.body["last_end"], "manual");
    assert_eq!(active(&env).await, 0);

    let enabled = journal_of(&env, "attack_mode.enable").await;
    let disabled = journal_of(&env, "attack_mode.disable").await;
    assert_eq!(enabled.len(), 1);
    assert_eq!(disabled.len(), 1);
    assert_eq!(enabled[0].0, "ok");
    assert_eq!(enabled[0].1.as_deref(), Some("marie"));
    assert_eq!(disabled[0].1.as_deref(), Some("marie"));
}

#[tokio::test]
async fn a_read_only_account_is_refused_with_403_and_the_refusal_is_journaled() {
    let env = env().await;
    let api = Api::new(&env);
    env.create("lucas", Role::ReadOnly).await;
    let key = DeviceKey::new();
    let token = login_token(&api, &key, "lucas", PASSWORD).await;
    let body = attack_mode_body(&api, &key, "lucas", &token, true, PASSWORD).await;
    let reply = put(&api, &token, &body).await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN, "{:?}", reply.body);
    assert_eq!(reply.code(), "FORBIDDEN_ROLE");
    assert_eq!(
        reply.body["error"]["message"],
        "Tu n'as pas la permission d'activer le mode attaque. C'est réservé aux administrateurs."
    );
    assert_eq!(active(&env).await, 0);
    // Un refus de rôle est consigné « refusé » sous `attack_mode.enable` (geste non lu).
    let refused = journal_of(&env, "attack_mode.enable").await;
    assert_eq!(refused.len(), 1);
    assert_eq!(refused[0].0, "denied");
    assert_eq!(refused[0].1.as_deref(), Some("lucas"));
    // Le corps n'est pas lu pour un refus de rôle : le geste n'est pas connu, la désactivation refusée
    // est consignée sous le code par défaut, l'activation.
    let body = attack_mode_body(&api, &key, "lucas", &token, false, PASSWORD).await;
    assert_eq!(put(&api, &token, &body).await.status, StatusCode::FORBIDDEN);
    assert!(journal_of(&env, "attack_mode.disable").await.is_empty());
    let all = journal_of(&env, "attack_mode.enable").await;
    assert!(!all.is_empty() && all.iter().all(|entry| entry.0 == "denied"));
}

#[tokio::test]
async fn an_administrator_without_a_proved_key_is_refused_with_409_a_typed_reason_and_nothing_is_written()
 {
    let env = env().await;
    let api = Api::new(&env);
    // Une session sans clé (client ancien, poste non inscrit) : ni preuve, ni poste.
    env.create("marie", Role::Admin).await;
    let token = api.token_of("marie").await;
    let state_before: (i64, Option<String>, Option<String>) =
        sqlx::query_as("SELECT active, activation_id, ended_how FROM attack_mode")
            .fetch_one(env.db.pool())
            .await
            .unwrap();
    let reply = put(
        &api,
        &token,
        &json!({ "active": true, "password": PASSWORD }),
    )
    .await;
    assert_eq!(reply.status, StatusCode::CONFLICT, "{:?}", reply.body);
    assert_eq!(reply.code(), "POST_NOT_RECOGNIZED");
    assert_eq!(reply.body["error"]["details"]["field"], "device");
    assert_eq!(reply.body["error"]["details"]["reason"], "proof_missing");
    // Désactiver exige la même preuve.
    let reply = put(
        &api,
        &token,
        &json!({ "active": false, "password": PASSWORD }),
    )
    .await;
    assert_eq!(reply.code(), "POST_NOT_RECOGNIZED");
    let state_after: (i64, Option<String>, Option<String>) =
        sqlx::query_as("SELECT active, activation_id, ended_how FROM attack_mode")
            .fetch_one(env.db.pool())
            .await
            .unwrap();
    assert_eq!(state_after, state_before, "aucune écriture");
    // Les refus sont consignés « refusé », sous le code du geste demandé.
    for action in ["attack_mode.enable", "attack_mode.disable"] {
        let refused = journal_of(&env, action).await;
        assert_eq!(refused.len(), 1, "{action}");
        assert_eq!(refused[0].0, "denied", "{action}");
        assert_eq!(
            refused[0].2.as_deref(),
            Some("mode attaque : poste non reconnu"),
            "{action}"
        );
    }
    // Aucune tentative de mot de passe n'a été faite : sans preuve, la session volée ne devine rien.
    assert_eq!(
        scalar(
            &env,
            "SELECT COALESCE(SUM(failures), 0) FROM login_attempts"
        )
        .await,
        0
    );
}

#[tokio::test]
async fn every_kind_of_wrong_proof_is_refused_with_409_and_leaves_the_mode_off() {
    let env = env().await;
    let api = Api::new(&env);
    let (key, token) = admin_with_key(&env, &api, "marie").await;
    let (paul_key, _paul_token) = admin_with_key(&env, &api, "paul").await;
    let hash = hash_of(&token);
    let fp = fingerprint();
    async fn attempt(api: &Api, token: &str, proof: Value, active: bool) -> Reply {
        put(
            api,
            token,
            &json!({ "active": active, "password": PASSWORD, "device": proof }),
        )
        .await
    }

    // (a) une clé qui n'est pas inscrite du tout.
    let stranger = DeviceKey::new();
    let issued = challenge(&api, "marie", "attack_mode").await;
    let proof = stranger.sign(
        &fp,
        Binding::AttackMode {
            token_hash: &hash,
            activate: true,
        },
        "marie",
        &issued,
    );
    let reply = attempt(&api, &token, device_json(&proof), true).await;
    assert_eq!(
        (reply.status, reply.code()),
        (StatusCode::CONFLICT, "POST_NOT_RECOGNIZED")
    );
    assert_eq!(reply.body["error"]["details"]["reason"], "proof_invalid");

    // (b) la clé INSCRITE d'un autre compte, signée correctement.
    let issued = challenge(&api, "marie", "attack_mode").await;
    let proof = paul_key.sign(
        &fp,
        Binding::AttackMode {
            token_hash: &hash,
            activate: true,
        },
        "marie",
        &issued,
    );
    let reply = attempt(&api, &token, device_json(&proof), true).await;
    assert_eq!(reply.code(), "POST_NOT_RECOGNIZED");

    // (c) la preuve d'un autre usage (connexion, flux, retrait).
    for (purpose, binding) in [
        ("login", Binding::Login),
        ("session", Binding::Session { token_hash: &hash }),
    ] {
        let issued = challenge(&api, "marie", purpose).await;
        let proof = key.sign(&fp, binding, "marie", &issued);
        let reply = attempt(&api, &token, device_json(&proof), true).await;
        assert_eq!(reply.code(), "POST_NOT_RECOGNIZED", "usage {purpose}");
    }
    // Un défi demandé pour le mode attaque mais signé avec l'octet d'usage d'une connexion.
    let issued = challenge(&api, "marie", "attack_mode").await;
    let proof = key.sign(&fp, Binding::Login, "marie", &issued);
    assert_eq!(
        attempt(&api, &token, device_json(&proof), true)
            .await
            .code(),
        "POST_NOT_RECOGNIZED"
    );

    // (d) la preuve de l'autre geste : une preuve d'activation ne désactive pas, et inversement.
    let issued = challenge(&api, "marie", "attack_mode").await;
    let activate_proof = key.sign(
        &fp,
        Binding::AttackMode {
            token_hash: &hash,
            activate: true,
        },
        "marie",
        &issued,
    );
    assert_eq!(
        attempt(&api, &token, device_json(&activate_proof), false)
            .await
            .code(),
        "POST_NOT_RECOGNIZED",
        "preuve d'activation pour désactiver"
    );
    let issued = challenge(&api, "marie", "attack_mode").await;
    let deactivate_proof = key.sign(
        &fp,
        Binding::AttackMode {
            token_hash: &hash,
            activate: false,
        },
        "marie",
        &issued,
    );
    assert_eq!(
        attempt(&api, &token, device_json(&deactivate_proof), true)
            .await
            .code(),
        "POST_NOT_RECOGNIZED",
        "preuve de désactivation pour activer"
    );

    // (e) la preuve liée au jeton d'une AUTRE session.
    let other = api.token_of("marie").await;
    let other_hash = hash_of(&other);
    let issued = challenge(&api, "marie", "attack_mode").await;
    let proof = key.sign(
        &fp,
        Binding::AttackMode {
            token_hash: &other_hash,
            activate: true,
        },
        "marie",
        &issued,
    );
    assert_eq!(
        attempt(&api, &token, device_json(&proof), true)
            .await
            .code(),
        "POST_NOT_RECOGNIZED"
    );

    // (f) une preuve périmée (60 secondes sur l'horloge monotone).
    let issued = challenge(&api, "marie", "attack_mode").await;
    let proof = key.sign(
        &fp,
        Binding::AttackMode {
            token_hash: &hash,
            activate: true,
        },
        "marie",
        &issued,
    );
    env.monotonic.advance(Duration::seconds(61));
    assert_eq!(
        attempt(&api, &token, device_json(&proof), true)
            .await
            .code(),
        "POST_NOT_RECOGNIZED"
    );

    assert_eq!(active(&env).await, 0);
    assert!(
        journal_of(&env, "attack_mode.enable")
            .await
            .iter()
            .all(|entry| entry.0 == "denied"),
        "aucune activation réussie"
    );
}

#[tokio::test]
async fn a_replayed_proof_is_refused_and_a_proof_that_did_not_serve_is_not_burned() {
    let env = env().await;
    let api = Api::new(&env);
    let (key, token) = admin_with_key(&env, &api, "marie").await;
    // Un mot de passe faux ne brûle pas la preuve : le même corps, une fois le mot de passe corrigé, sert.
    let mut body = attack_mode_body(&api, &key, "marie", &token, true, WRONG).await;
    let reply = put(&api, &token, &body).await;
    assert_eq!(
        reply.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{:?}",
        reply.body
    );
    assert_eq!(reply.code(), "WRONG_PASSWORD");
    assert_eq!(active(&env).await, 0);
    body["password"] = json!(PASSWORD);
    let served = put(&api, &token, &body).await;
    assert_eq!(served.status, StatusCode::OK, "{:?}", served.body);
    // Rejouée : le défi a servi.
    let _ = put(
        &api,
        &token,
        &attack_mode_body(&api, &key, "marie", &token, false, PASSWORD).await,
    )
    .await;
    assert_eq!(active(&env).await, 0);
    let replay = put(&api, &token, &body).await;
    assert_eq!(replay.code(), "POST_NOT_RECOGNIZED", "{:?}", replay.body);
    assert_eq!(active(&env).await, 0, "la preuve rejouée n'active rien");
}

#[tokio::test]
async fn a_wrong_password_counts_as_a_login_failure_with_the_same_counters_and_slowdown() {
    let env = env().await;
    let api = Api::new(&env);
    let (key, token) = admin_with_key(&env, &api, "marie").await;
    let pair = |env: &Env| {
        let pool = env.db.pool().clone();
        async move {
            sqlx::query_scalar::<_, i64>("SELECT COALESCE(MAX(failures), 0) FROM login_attempts")
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };
    let before = pair(&env).await;
    for attempt in 1..=4 {
        let body = attack_mode_body(&api, &key, "marie", &token, true, WRONG).await;
        let reply = put(&api, &token, &body).await;
        assert_eq!(
            reply.code(),
            "WRONG_PASSWORD",
            "tentative {attempt} : {:?}",
            reply.body
        );
        assert_eq!(
            pair(&env).await,
            before + attempt,
            "un échec de connexion de plus"
        );
    }
    // Le cinquième échec ouvre l'attente du couple, exactement comme à la connexion.
    let body = attack_mode_body(&api, &key, "marie", &token, true, WRONG).await;
    let reply = put(&api, &token, &body).await;
    assert!(
        matches!(reply.status.as_u16(), 422 | 429),
        "{:?}",
        reply.body
    );
    let body = attack_mode_body(&api, &key, "marie", &token, true, PASSWORD).await;
    let reply = put(&api, &token, &body).await;
    assert_eq!(
        reply.status,
        StatusCode::TOO_MANY_REQUESTS,
        "{:?}",
        reply.body
    );
    assert_eq!(reply.code(), "TOO_MANY_ATTEMPTS");
    assert_eq!(active(&env).await, 0);
}

#[tokio::test]
async fn the_route_needs_a_session_and_an_administrator() {
    let env = env().await;
    let api = Api::new(&env);
    let reply = api
        .put("/security/attack-mode")
        .json(&json!({ "active": true }))
        .send()
        .await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    let endpoint = ENDPOINTS
        .iter()
        .find(|endpoint| endpoint.path == "/security/attack-mode")
        .unwrap();
    assert_eq!(endpoint.access, Access::Admin);
    assert!(endpoint.tracked && endpoint.modifies());
}

// ---------------------------------------------------------------------------------------------
// Une session présentée seule : refusée sur chaque route, sans rien révéler
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_session_alone_is_refused_on_every_route_like_an_unknown_token_and_works_again_at_the_end()
 {
    let env = env().await;
    let api = Api::new(&env);
    let (_key, token) = admin_with_key(&env, &api, "marie").await;
    // Le même jeton, présenté depuis une autre adresse que celle de la connexion.
    let mut elsewhere = Api::new(&env);
    elsewhere.addr = format!("{ELSEWHERE}:40000").parse().unwrap();
    env.attack.change(true, by(), EndHow::Manual).await.unwrap();

    let unknown = SessionToken::from_bytes([9; 32]).encode();
    let mut swept = 0;
    for endpoint in ENDPOINTS
        .iter()
        .filter(|endpoint| !matches!(endpoint.access, Access::Public | Access::FirstMessage))
    {
        let path = endpoint.path.replace("{id}", "x");
        let refused = elsewhere
            .call(endpoint.method.clone(), &path)
            .token(&token)
            .json(&json!({}))
            .send()
            .await;
        let reference = elsewhere
            .call(endpoint.method.clone(), &path)
            .token(&unknown)
            .json(&json!({}))
            .send()
            .await;
        assert_eq!(
            refused.status,
            StatusCode::UNAUTHORIZED,
            "{} {path}",
            endpoint.method
        );
        assert_eq!(
            refused.code(),
            "SESSION_EXPIRED",
            "{} {path}",
            endpoint.method
        );
        // La même réponse, octet pour octet, qu'un jeton inconnu : corps et en-têtes.
        assert_eq!(refused.text, reference.text, "{} {path}", endpoint.method);
        let mut a: Vec<_> = refused
            .headers
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();
        let mut b: Vec<_> = reference
            .headers
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();
        a.sort();
        b.sort();
        assert_eq!(a, b, "{} {path}", endpoint.method);
        swept += 1;
    }
    assert!(swept >= 15, "{swept} routes balayées");
    // Les routes publiques ne disent rien de plus (`/hello`, défi, connexion).
    assert_eq!(elsewhere.get("/hello").send().await.status, StatusCode::OK);
    let hello = elsewhere.get("/hello").send().await;
    assert!(!hello.text.contains("attack"));

    // Depuis l'adresse retenue, la même session passe : session + adresse retenue.
    assert_eq!(
        api.get("/me").token(&token).send().await.status,
        StatusCode::OK
    );
    // Elle n'a pas été détruite : à la fin du mode, elle refonctionne de partout, sans reconnexion.
    env.attack
        .change(false, by(), EndHow::Manual)
        .await
        .unwrap();
    assert_eq!(
        elsewhere.get("/me").token(&token).send().await.status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn the_security_route_tells_the_mode_to_an_authenticated_account_and_follows_every_change() {
    let env = env().await;
    let api = Api::new(&env);
    env.create("lucas", Role::ReadOnly).await;
    let lucas = api.token_of("lucas").await;
    let state = |token: &str| {
        let api = &api;
        let token = token.to_owned();
        async move { api.get("/security").token(&token).send().await }
    };
    assert_eq!(
        state(&lucas).await.body["attack_mode"],
        json!({ "state": "off" })
    );
    env.attack.on_start().await.unwrap();
    env.attack.change(true, by(), EndHow::Manual).await.unwrap();
    let reply = state(&lucas).await;
    assert_eq!(reply.body["attack_mode"]["state"], "active");
    assert!(reply.body["attack_mode"]["since"].is_string());
    assert!(reply.body["attack_mode"].get("resumes_in_s").is_none());
    // Redémarrage de la machine : suspendu, avec le temps restant exact.
    env.boot.set_id(Some("boot-2"));
    env.boot.set_uptime(Duration::minutes(10));
    env.attack.on_start().await.unwrap();
    let reply = state(&lucas).await;
    assert_eq!(reply.body["attack_mode"]["state"], "suspended");
    assert_eq!(reply.body["attack_mode"]["resumes_in_s"], 1200);
    // Sortie automatique : « arrêt automatique ».
    env.boot.set_uptime(Duration::minutes(31));
    env.attack.sweep().await.unwrap();
    env.monotonic.advance(Duration::minutes(31));
    assert!(env.attack.sweep().await.unwrap().auto_disabled);
    let reply = state(&lucas).await;
    assert_eq!(reply.body["attack_mode"]["state"], "off");
    assert_eq!(reply.body["attack_mode"]["last_end"], "auto");
}

#[tokio::test]
async fn the_local_command_is_seen_at_once_by_the_routes_of_a_running_agent() {
    let env = env().await;
    let api = Api::new(&env);
    env.create("lucas", Role::ReadOnly).await;
    let lucas = api.token_of("lucas").await;
    env.attack.change(true, by(), EndHow::Manual).await.unwrap();
    // Une session depuis une autre adresse est refusée...
    let mut elsewhere = Api::new(&env);
    elsewhere.addr = format!("{ELSEWHERE}:40000").parse().unwrap();
    assert_eq!(
        elsewhere.get("/me").token(&lucas).send().await.status,
        StatusCode::UNAUTHORIZED
    );
    // ...jusqu'à ce que `attack-mode off` (un autre processus, la même base) écrive. Le service lit la
    // ligne à chaque décision : aucune mémoire à invalider.
    let other_process = hearth_agent::application::attack_mode::AttackModeService::new(
        std::sync::Arc::new(
            hearth_agent::infrastructure::sqlite::SqliteAttackModeRepo::new(env.db.pool().clone()),
        ),
        std::sync::Arc::new(hearth_agent::infrastructure::sqlite::SqliteStore::new(
            env.db.pool().clone(),
        )),
        env.clock.clone(),
        env.monotonic.clone(),
        env.boot.clone(),
        std::sync::Arc::new(support::SequentialIds::starting_at(70_000)),
        env.trail.clone(),
        std::sync::Arc::new(
            hearth_agent::infrastructure::security_feed::BroadcastSecurityFeed::new(),
        ),
    );
    assert!(other_process.disable_from_cli().await.unwrap());
    assert_eq!(
        elsewhere.get("/me").token(&lucas).send().await.status,
        StatusCode::OK
    );
}

// ---------------------------------------------------------------------------------------------
// Le flux
// ---------------------------------------------------------------------------------------------

async fn token_from(env: &Env, name: &str, from: &str) -> String {
    env.sessions
        .login(name, secret(PASSWORD), &client_at(from))
        .await
        .unwrap()
        .token
        .encode()
}

/// Ouvre le flux avec ce premier message et rend ce qui le suit : le client (session acceptée, le
/// snapshot est lu) ou le premier message d'erreur et la fin de la connexion.
async fn open_with(agent: &Agent, first: &Value) -> Result<WsClient, (ServerMessage, End)> {
    let mut client = ws::open(agent).await;
    client.send_text(&first.to_string()).await;
    client.subscribe(&[Topic::Metrics]).await;
    match client.expect().await {
        ServerMessage::Snapshot { .. } => Ok(client),
        other => {
            let end = client.until_end().await;
            Err((other, end))
        }
    }
}

#[tokio::test]
async fn a_session_alone_is_refused_on_the_stream_like_an_unknown_token_and_a_key_proof_opens_it() {
    let env = env().await;
    let agent = https::start_booted(&env, metering(), env.boot.clone()).await;
    let fingerprint = FileIdentityStore::new(env.dir.path())
        .load_or_create()
        .unwrap()
        .fingerprint;
    env.create("marie", Role::Admin).await;
    // marie : une clé inscrite (connexion depuis 10.0.0.7), puis le mode attaque.
    let key = DeviceKey::new();
    let proof = key.login_proof(&env, "marie", CLIENT_ADDR);
    let outcome = env
        .sessions
        .login_with_device(
            "marie",
            secret(PASSWORD),
            &client_at(CLIENT_ADDR),
            Some(&proof),
        )
        .await
        .unwrap();
    let token = outcome.token.encode();
    env.attack.change(true, by(), EndHow::Manual).await.unwrap();

    // Le flux vient de 127.0.0.1, qui n'est pas une adresse retenue : la session seule est refusée,
    // par la réponse EXACTE d'un jeton inconnu.
    let alone = json!({ "type": "auth", "token": token });
    let Err((refused, end)) = open_with(&agent, &alone).await else {
        panic!("la session seule ne doit pas ouvrir le flux");
    };
    let unknown = SessionToken::from_bytes([9; 32]).encode();
    let Err((reference, reference_end)) =
        open_with(&agent, &json!({ "type": "auth", "token": unknown })).await
    else {
        panic!("un jeton inconnu ne doit pas ouvrir le flux");
    };
    assert_eq!(
        serde_json::to_string(&refused).unwrap(),
        serde_json::to_string(&reference).unwrap()
    );
    assert_eq!(end, reference_end);
    match refused {
        ServerMessage::Error(detail) => {
            assert_eq!(detail.code, hearth_proto::error::ErrorCode::SessionExpired)
        }
        other => panic!("{other:?}"),
    }

    // Session + preuve de clé liée à ce jeton : le flux s'ouvre et l'adresse devient retenue.
    let issued = agent
        .request("POST", "/sessions/challenge")
        .json(&json!({ "username": "marie", "purpose": "session" }))
        .send()
        .await
        .body["challenge"]
        .as_str()
        .unwrap()
        .to_owned();
    let hash = hash_of(&token);
    let proof = key.sign(
        &fingerprint,
        Binding::Session { token_hash: &hash },
        "marie",
        &issued,
    );
    let first = json!({ "type": "auth", "token": token, "device": device_json(&proof) });
    let Ok(client) = open_with(&agent, &first).await else {
        panic!("session + clé : le flux s'ouvre");
    };
    drop(client);
    assert_eq!(
        scalar(
            &env,
            "SELECT COUNT(*) FROM known_addresses WHERE address = '127.0.0.1'"
        )
        .await,
        1
    );
    // Désormais la session seule passe depuis cette adresse (session + adresse retenue).
    assert!(open_with(&agent, &alone).await.is_ok());
    agent.shutdown().await;
}

#[tokio::test]
async fn a_stream_opened_before_the_activation_falls_when_it_holds_only_a_session_and_the_state_follows()
 {
    let env = env().await;
    let agent = https::start_booted(&env, metering(), env.boot.clone()).await;
    env.create("marie", Role::Admin).await;
    env.create("lucas", Role::ReadOnly).await;
    // marie a été retenue depuis 127.0.0.1 (son poste réel) ; lucas n'a que sa session ouverte depuis
    // une autre adresse.
    let marie = token_from(&env, "marie", "127.0.0.1").await;
    let lucas = token_from(&env, "lucas", "10.0.0.7").await;

    let first = |token: &str| json!({ "type": "auth", "token": token });
    let mut marie_stream = open_with(&agent, &first(&marie)).await.expect("marie");
    let mut lucas_stream = open_with(&agent, &first(&lucas)).await.expect("lucas");
    let SecurityMessage::Security(view) = marie_stream.next_security().await;
    assert_eq!(
        serde_json::to_value(&view.attack_mode).unwrap(),
        json!({ "state": "off" })
    );

    // L'activation se fait ailleurs que dans cet agent (la sous-commande, un autre processus ou le
    // client) : le flux relit l'état à chaque contrôle de la session.
    env.attack.change(true, by(), EndHow::Manual).await.unwrap();
    // marie (adresse retenue) reste connectée et apprend que le mode est actif.
    let SecurityMessage::Security(view) = marie_stream.next_security().await;
    assert_eq!(
        serde_json::to_value(&view.attack_mode).unwrap()["state"],
        json!("active")
    );
    // lucas n'a que sa session : le flux tombe, avec l'avis d'une session expirée (jamais « mode »).
    let mut notice = None;
    let end = loop {
        match lucas_stream.next().await {
            Ok(ServerMessage::Session { kind }) => notice = Some(kind),
            Ok(_) => {}
            Err(end) => break end,
        }
    };
    assert_eq!(notice, Some(SessionNotice::Expired));
    assert_eq!(end, End::Closed(1008));

    // La sortie du mode (un autre processus) : le flux de marie le sait sans rien demander.
    env.attack.disable_from_cli().await.unwrap();
    let SecurityMessage::Security(view) = marie_stream.next_security().await;
    assert_eq!(
        serde_json::to_value(&view.attack_mode).unwrap()["state"],
        json!("off")
    );
    // Et la session de lucas, non détruite, rouvre un flux sans nouvelle connexion.
    assert!(open_with(&agent, &first(&lucas)).await.is_ok());
    agent.shutdown().await;
}

#[tokio::test]
async fn stopping_the_service_writes_the_alert_entry_still_in_flight() {
    let env = env().await;
    let agent = https::start(&env).await;
    env.create("marie", Role::Admin).await;
    // Onze échecs d'adresses différentes : le onzième ouvre l'alerte, dont l'entrée est écrite par une
    // tâche détachée. On arrête le service tout de suite : `settle` l'attend.
    for n in 0..11 {
        let reply = agent
            .request("POST", "/sessions")
            .json(&json!({ "username": "marie", "password": WRONG }))
            .send()
            .await;
        assert!(matches!(reply.status, 401 | 429));
        env.clock.advance(Duration::minutes(16));
        let _ = n;
    }
    agent.stop_gracefully().await;
    assert_eq!(
        scalar(
            &env,
            "SELECT COUNT(*) FROM audit_events WHERE action = 'security.alert'"
        )
        .await,
        1,
        "l'entrée de début d'alerte n'est pas perdue à l'arrêt"
    );
}

#[tokio::test]
async fn the_suspension_countdown_alone_never_sends_a_new_security_message() {
    let env = env().await;
    let agent = https::start_booted(&env, metering(), env.boot.clone()).await;
    env.create("marie", Role::Admin).await;
    let marie = token_from(&env, "marie", "127.0.0.1").await;
    env.attack.on_start().await.unwrap();
    env.attack.change(true, by(), EndHow::Manual).await.unwrap();
    env.boot.set_id(Some("boot-2"));
    env.boot.set_uptime(Duration::minutes(2));
    env.attack.on_start().await.unwrap();
    let mut stream = open_with(&agent, &json!({ "type": "auth", "token": marie }))
        .await
        .expect("marie");
    let SecurityMessage::Security(first) = stream.next_security().await;
    assert_eq!(first.attack_mode.resumes_in_s, Some(1680));
    // Le temps écoulé avance de dix minutes, des dizaines de contrôles de la session passent (50 ms) :
    // `resumes_in_s` change, aucun message ne part.
    env.boot.set_uptime(Duration::minutes(12));
    let more = tokio::time::timeout(
        std::time::Duration::from_millis(600),
        stream.next_security(),
    )
    .await;
    assert!(more.is_err(), "un message pour le seul compte à rebours");
    // Un vrai changement part, avec le temps restant d'alors.
    env.attack
        .change(false, by(), EndHow::Manual)
        .await
        .unwrap();
    let SecurityMessage::Security(view) = stream.next_security().await;
    assert_eq!(
        serde_json::to_value(&view.attack_mode).unwrap()["state"],
        json!("off")
    );
    agent.shutdown().await;
}

/// Une requête dont le corps signale qu'on l'a lu.
fn watched_request(
    token: Option<&str>,
) -> (axum::http::Request<axum::body::Body>, Arc<AtomicBool>) {
    let read = Arc::new(AtomicBool::new(false));
    let flag = read.clone();
    let stream = futures_util::stream::once(async move {
        flag.store(true, Ordering::SeqCst);
        Ok::<_, std::io::Error>(axum::body::Bytes::from(vec![b' '; 2 << 20]))
    });
    let mut builder = axum::http::Request::builder()
        .method("PUT")
        .uri("/api/v1/security/attack-mode")
        .header("x-hearth-api", "1")
        .header("content-type", "application/json");
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    let mut request = builder.body(axum::body::Body::from_stream(stream)).unwrap();
    request.extensions_mut().insert(axum::extract::ConnectInfo(
        "10.0.0.7:40000".parse::<std::net::SocketAddr>().unwrap(),
    ));
    (request, read)
}

#[tokio::test]
async fn a_request_without_a_token_or_without_the_role_makes_nobody_read_its_body() {
    use tower::ServiceExt;
    let env = env().await;
    let api = Api::new(&env);
    env.create("lucas", Role::ReadOnly).await;
    let lucas = api.token_of("lucas").await;
    let router = hearth_agent::entrypoint::http::router(support::api::state(&env));
    for (token, status) in [
        (None, StatusCode::UNAUTHORIZED),
        (Some("pas-un-jeton"), StatusCode::UNAUTHORIZED),
        (Some(lucas.as_str()), StatusCode::FORBIDDEN),
    ] {
        let (request, read) = watched_request(token);
        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), status, "{token:?}");
        assert!(
            !read.load(Ordering::SeqCst),
            "le corps ne doit pas être lu ({token:?})"
        );
    }
    // Un administrateur authentifié, lui, fait lire le corps (le geste y est).
    let (_key, marie) = admin_with_key(&env, &api, "marie").await;
    let (request, read) = watched_request(Some(&marie));
    let response = router.clone().oneshot(request).await.unwrap();
    assert!(read.load(Ordering::SeqCst));
    assert!(response.status().is_client_error());
    // Aucune requête d'un non authentifié ne laisse d'entrée `attack_mode.*`.
    let by_nobody: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_events WHERE action LIKE 'attack_mode.%' AND account IS NULL",
    )
    .fetch_one(env.db.pool())
    .await
    .unwrap();
    assert_eq!(by_nobody, 0);
}

#[tokio::test]
async fn an_authenticated_administrator_with_an_unreadable_or_missing_body_is_journaled_as_an_enable()
 {
    let env = env().await;
    let api = Api::new(&env);
    let (_key, token) = admin_with_key(&env, &api, "marie").await;
    for raw in ["pas du json", ""] {
        let reply = api
            .put("/security/attack-mode")
            .token(&token)
            .raw_body(raw)
            .send()
            .await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY, "{raw:?}");
    }
    let entries = journal_of(&env, "attack_mode.enable").await;
    assert!(!entries.is_empty() && entries.iter().all(|entry| entry.0 == "failed"));
    assert!(journal_of(&env, "attack_mode.disable").await.is_empty());
}

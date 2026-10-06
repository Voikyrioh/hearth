//! Identité d'appareil sur l'interface HTTP (HRT-22, ADR-0023) : `POST /sessions/challenge`,
//! `POST /sessions` avec la preuve, `GET` et `DELETE /me/devices`. Routeur en processus (sans TLS)
//! sur une vraie base ; les signatures sont de vraies signatures Ed25519.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;

use axum::http::StatusCode;
use hearth_agent::domain::accounts::Role;
use hearth_proto::device_proof::Binding;
use hearth_proto::fingerprint::Fingerprint;
use serde_json::{Value, json};
use support::api::{Api, Reply, state};
use support::device::DeviceKey;
use support::{PASSWORD, SERVER_FINGERPRINT, env};

fn server() -> Fingerprint {
    Fingerprint::from_bytes(SERVER_FINGERPRINT)
}

async fn challenge(api: &Api, username: &str, purpose: &str) -> Reply {
    api.post("/sessions/challenge")
        .json(&json!({ "username": username, "purpose": purpose }))
        .send()
        .await
}

/// Le corps d'une connexion qui porte la preuve de cette clé.
async fn login_body(api: &Api, key: &DeviceKey, username: &str, password: &str) -> Value {
    let reply = challenge(api, username, "login").await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    let challenge = reply.body["challenge"].as_str().unwrap();
    let proof = key.sign(&server(), Binding::Login, username, challenge);
    json!({
        "username": username,
        "password": password,
        "device": {
            "algorithm": proof.algorithm,
            "public_key": proof.public_key,
            "challenge": proof.challenge,
            "signature": proof.signature,
        }
    })
}

async fn enrolled_token(api: &Api, key: &DeviceKey, username: &str) -> String {
    let body = login_body(api, key, username, PASSWORD).await;
    let reply = api.post("/sessions").json(&body).send().await;
    assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.body);
    reply.body["token"].as_str().unwrap().to_owned()
}

fn header_names(reply: &Reply) -> Vec<String> {
    let mut names: Vec<String> = reply
        .headers
        .keys()
        .map(|n| n.as_str().to_owned())
        .collect();
    names.sort();
    names
}

// ---------------------------------------------------------------------------------------------
// Le défi
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn the_challenge_answers_the_same_for_an_existing_and_a_missing_identifier() {
    let env = env().await;
    let api = Api::new(&env);
    env.create("marie", Role::Admin).await;
    // Avec un poste déjà inscrit, sans, et pour un identifiant qui n'existe pas.
    let key = DeviceKey::new();
    enrolled_token(&api, &key, "marie").await;
    env.create("paul", Role::ReadOnly).await;

    let mut seen = Vec::new();
    for username in ["marie", "paul", "fantome", "MARIE", "x", &"y".repeat(9_000)] {
        for purpose in ["login", "session", "attack_mode"] {
            let reply = challenge(&api, username, purpose).await;
            assert_eq!(reply.status, StatusCode::OK, "{username} {purpose}");
            let object = reply.body.as_object().unwrap();
            let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
            keys.sort_unstable();
            assert_eq!(keys, vec!["challenge", "expires_in_s"]);
            assert_eq!(reply.body["expires_in_s"], 60);
            assert_eq!(reply.body["challenge"].as_str().unwrap().len(), 76);
            seen.push((reply.status, header_names(&reply), reply.text.len()));
        }
    }
    assert!(
        seen.windows(2).all(|pair| pair[0] == pair[1]),
        "code, en-têtes et taille du corps identiques : {seen:?}"
    );
    // Deux défis ne se ressemblent pas (le nonce change à chaque demande).
    let first = challenge(&api, "marie", "login").await;
    let second = challenge(&api, "marie", "login").await;
    assert_ne!(first.body["challenge"], second.body["challenge"]);
}

#[tokio::test]
async fn the_challenge_is_public_versioned_and_strict_about_its_body() {
    let env = env().await;
    let api = Api::new(&env);
    // Une purpose inconnue, un corps illisible, un champ manquant : 422.
    for body in [
        json!({ "username": "marie", "purpose": "autre" }),
        json!({ "username": "marie" }),
        json!({ "purpose": "login" }),
        json!({ "username": 5, "purpose": "login" }),
    ] {
        let reply = api.post("/sessions/challenge").json(&body).send().await;
        assert_eq!(
            (reply.status, reply.code()),
            (StatusCode::UNPROCESSABLE_ENTITY, "VALIDATION_ERROR"),
            "{body}"
        );
    }
    let reply = api
        .post("/sessions/challenge")
        .raw_body("pas du json")
        .send()
        .await;
    assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
    // La version d'interface est contrôlée comme sur le reste de l'API.
    let reply = api
        .post("/sessions/challenge")
        .version(Some("7"))
        .json(&json!({ "username": "marie", "purpose": "login" }))
        .send()
        .await;
    assert_eq!(reply.status, StatusCode::UPGRADE_REQUIRED);
    let reply = api
        .post("/sessions/challenge")
        .version(None)
        .json(&json!({ "username": "marie", "purpose": "login" }))
        .send()
        .await;
    assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
    // Les autres méthodes ne sont pas des routes.
    let reply = api.get("/sessions/challenge").send().await;
    assert_eq!(reply.status, StatusCode::METHOD_NOT_ALLOWED);
    // Aucun défi, nulle part, n'écrit au journal ni en base.
    let audit: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_events")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert_eq!(audit, 0);
}

#[tokio::test]
async fn an_agent_without_the_device_identity_answers_404_like_an_unknown_route() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let mut old = state(&env);
    old.sessions = Arc::new(support::plain_sessions(&env));
    let api = Api::from_state(old);
    let reply = challenge(&api, "marie", "login").await;
    assert_eq!(
        (reply.status, reply.code()),
        (StatusCode::NOT_FOUND, "NOT_FOUND")
    );
    // La connexion sans clé fonctionne comme avant, avec ou sans champ `device`.
    let key = DeviceKey::new();
    let body = json!({
        "username": "marie", "password": PASSWORD,
        "device": { "algorithm": "ed25519", "public_key": key.key_id(),
                    "challenge": "x", "signature": "y" }
    });
    let reply = api.post("/sessions").json(&body).send().await;
    assert_eq!(reply.status, StatusCode::CREATED);
    assert!(reply.body.get("device").is_none());
    let token = reply.body["token"].as_str().unwrap();
    for (method, path) in [("GET", "/me/devices"), ("DELETE", "/me/devices/UNPOSTE")] {
        let reply = api
            .call(method.parse().unwrap(), path)
            .token(token)
            .send()
            .await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{method} {path}");
    }
}

// ---------------------------------------------------------------------------------------------
// La connexion avec la preuve
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_login_with_a_proof_enrolls_and_a_login_without_one_answers_as_before() {
    let env = env().await;
    let api = Api::new(&env);
    env.create("marie", Role::Admin).await;
    let key = DeviceKey::new();

    // Sans clé : la réponse n'a pas le champ `device`, exactement la forme d'avant.
    let plain = api.login("marie", PASSWORD).await;
    assert_eq!(plain.status, StatusCode::CREATED);
    let mut fields: Vec<&str> = plain
        .body
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    fields.sort_unstable();
    assert_eq!(fields, vec!["account", "expires_at", "token"]);

    // Avec la clé : inscrite.
    let body = login_body(&api, &key, "marie", PASSWORD).await;
    let first = api.post("/sessions").json(&body).send().await;
    assert_eq!(first.status, StatusCode::CREATED, "{:?}", first.body);
    assert_eq!(first.body["device"], "enrolled");
    // Le même corps rejoué : le défi est consommé, la connexion réussit comme sans clé.
    env.clock.advance(time::Duration::minutes(1));
    let replay = api.post("/sessions").json(&body).send().await;
    assert_eq!(replay.status, StatusCode::CREATED);
    assert!(replay.body.get("device").is_none(), "{:?}", replay.body);
    // Une nouvelle preuve de la même clé : reconnue.
    let again = login_body(&api, &key, "marie", PASSWORD).await;
    let second = api.post("/sessions").json(&again).send().await;
    assert_eq!(second.body["device"], "proven");
}

#[tokio::test]
async fn a_malformed_device_field_never_turns_a_login_into_an_error() {
    let env = env().await;
    let api = Api::new(&env);
    env.create("marie", Role::Admin).await;
    for device in [
        json!(5),
        json!("texte"),
        json!(null),
        json!([]),
        json!({}),
        json!({ "algorithm": "ed25519" }),
        json!({ "algorithm": 1, "public_key": 2, "challenge": 3, "signature": 4 }),
        json!({ "algorithm": "ed25519", "public_key": "", "challenge": "", "signature": "" }),
    ] {
        env.clock.advance(time::Duration::minutes(1));
        let reply = api
            .post("/sessions")
            .json(&json!({ "username": "marie", "password": PASSWORD, "device": device }))
            .send()
            .await;
        assert_eq!(
            reply.status,
            StatusCode::CREATED,
            "{device}: {:?}",
            reply.body
        );
        assert!(reply.body.get("device").is_none(), "{device}");
    }
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM trusted_devices")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert_eq!(rows, 0);
}

#[tokio::test]
async fn a_wrong_password_answers_the_same_with_a_valid_proof_a_false_one_or_none() {
    let env = env().await;
    let api = Api::new(&env);
    env.create("marie", Role::Admin).await;
    let key = DeviceKey::new();
    let mut replies = Vec::new();
    for (username, with_proof) in [
        ("marie", false),
        ("marie", true),
        ("fantome", false),
        ("fantome", true),
    ] {
        env.clock.advance(time::Duration::minutes(10));
        let body = if with_proof {
            login_body(&api, &key, username, "Mauvais-Mot-De-Passe-1").await
        } else {
            json!({ "username": username, "password": "Mauvais-Mot-De-Passe-1" })
        };
        let reply = api.post("/sessions").json(&body).send().await;
        replies.push((reply.status, reply.text.clone(), header_names(&reply)));
    }
    assert!(
        replies.windows(2).all(|pair| pair[0] == pair[1]),
        "401 identiques, au caractère près : {replies:?}"
    );
    assert_eq!(replies[0].0, StatusCode::UNAUTHORIZED);
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM trusted_devices")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert_eq!(rows, 0);
}

// ---------------------------------------------------------------------------------------------
// La liste et le retrait
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn the_list_shows_the_devices_of_the_caller_without_any_key_material() {
    let env = env().await;
    let api = Api::new(&env);
    env.create("marie", Role::Admin).await;
    env.create("paul", Role::ReadOnly).await;
    let (a, b) = (DeviceKey::new(), DeviceKey::new());
    let on_a = enrolled_token(&api, &a, "marie").await;
    env.clock.advance(time::Duration::minutes(1));
    enrolled_token(&api, &b, "marie").await;
    let paul = enrolled_token(&api, &DeviceKey::new(), "paul").await;

    let reply = api.get("/me/devices").token(&on_a).send().await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    assert_eq!(reply.body["max"], 8);
    let devices = reply.body["devices"].as_array().unwrap();
    assert_eq!(devices.len(), 2, "ceux de marie seulement");
    assert_eq!(devices.iter().filter(|d| d["current"] == true).count(), 1);
    assert_eq!(devices[0]["current"], true);
    for device in devices {
        let mut fields: Vec<&str> = device
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        fields.sort_unstable();
        assert_eq!(
            fields,
            vec![
                "created_at",
                "current",
                "id",
                "last_addr",
                "last_proved_at",
                "name"
            ]
        );
        assert_eq!(device["name"], "inconnu", "aucun X-Hearth-Client envoyé");
        assert_eq!(device["last_addr"], "10.0.0.7");
    }
    for key in [&a, &b] {
        let text = &reply.text;
        assert!(!text.contains(&key.key_id()), "l'empreinte d'une clé");
        assert!(!text.contains(&base64_of(key)), "la clé publique");
    }
    // Un compte en lecture seule liste les siens.
    let reply = api.get("/me/devices").token(&paul).send().await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.body["devices"].as_array().unwrap().len(), 1);
    // Sans session : 401.
    let reply = api.get("/me/devices").send().await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
}

fn base64_of(key: &DeviceKey) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(key.public_key())
}

#[tokio::test]
async fn a_device_is_removed_by_its_owner_only_and_never_from_itself() {
    let env = env().await;
    let api = Api::new(&env);
    env.create("marie", Role::Admin).await;
    env.create("paul", Role::Admin).await;
    let (a, b) = (DeviceKey::new(), DeviceKey::new());
    let on_a = enrolled_token(&api, &a, "marie").await;
    env.clock.advance(time::Duration::minutes(1));
    let on_b = enrolled_token(&api, &b, "marie").await;
    let paul = enrolled_token(&api, &DeviceKey::new(), "paul").await;

    let list = api.get("/me/devices").token(&on_a).send().await;
    let current = list.body["devices"][0]["id"].as_str().unwrap().to_owned();
    let other = list.body["devices"][1]["id"].as_str().unwrap().to_owned();
    let paul_device = api.get("/me/devices").token(&paul).send().await.body["devices"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    // Le poste d'un autre compte, ou inexistant : 404 (indiscernables).
    for id in [
        paul_device.as_str(),
        "01JINCONNUINCONNUINCONNU0",
        "%27%20OR%201%3D1%20--",
    ] {
        let reply = api
            .delete(&format!("/me/devices/{id}"))
            .token(&on_a)
            .send()
            .await;
        assert_eq!(
            (reply.status, reply.code()),
            (StatusCode::NOT_FOUND, "NOT_FOUND"),
            "{id}"
        );
    }
    // Le poste d'où part la requête : 422.
    let reply = api
        .delete(&format!("/me/devices/{current}"))
        .token(&on_a)
        .send()
        .await;
    assert_eq!(
        (reply.status, reply.code()),
        (StatusCode::UNPROCESSABLE_ENTITY, "VALIDATION_ERROR")
    );
    // Rien n'a été retiré, ni chez marie ni chez paul.
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM trusted_devices")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert_eq!(rows, 3);

    // Un autre poste de marie : 204, sa session est fermée (accès révoqué), l'autre fonctionne.
    let reply = api
        .delete(&format!("/me/devices/{other}"))
        .token(&on_a)
        .send()
        .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{:?}", reply.body);
    let dead = api.get("/me").token(&on_b).send().await;
    assert_eq!(
        (dead.status, dead.code()),
        (StatusCode::UNAUTHORIZED, "SESSION_REVOKED")
    );
    assert_eq!(
        api.get("/me").token(&on_a).send().await.status,
        StatusCode::OK
    );
    let list = api.get("/me/devices").token(&on_a).send().await;
    assert_eq!(list.body["devices"].as_array().unwrap().len(), 1);
    // Sans session : 401, rien ne change.
    let reply = api.delete(&format!("/me/devices/{current}")).send().await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn removing_a_device_replays_with_its_operation_key_and_leaves_one_journal_entry() {
    let env = env().await;
    let api = Api::new(&env);
    env.create("marie", Role::Admin).await;
    let (a, b) = (DeviceKey::new(), DeviceKey::new());
    let on_a = enrolled_token(&api, &a, "marie").await;
    env.clock.advance(time::Duration::minutes(1));
    enrolled_token(&api, &b, "marie").await;
    let list = api.get("/me/devices").token(&on_a).send().await;
    let other = list.body["devices"][1]["id"].as_str().unwrap().to_owned();
    let key = "01J9ZY0G3Q8M2K6W4T7V5N1B9D";

    let first = api
        .delete(&format!("/me/devices/{other}"))
        .token(&on_a)
        .key(key)
        .send()
        .await;
    assert_eq!(first.status, StatusCode::NO_CONTENT);
    let second = api
        .delete(&format!("/me/devices/{other}"))
        .token(&on_a)
        .key(key)
        .send()
        .await;
    assert_eq!(second.status, StatusCode::NO_CONTENT, "rejoué, pas un 404");
    assert!(second.headers.contains_key("idempotent-replayed"));
    let entries: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM audit_events WHERE action = 'device.remove'")
            .fetch_one(env.db.pool())
            .await
            .unwrap();
    assert_eq!(entries, 1);
}

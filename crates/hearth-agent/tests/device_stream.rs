//! Identité d'appareil de bout en bout sur un vrai agent HTTPS (TLS 1.3) : le défi et la connexion avec
//! preuve sous l'empreinte du VRAI certificat, puis la preuve de clé au premier message du flux, qui
//! fait retenir l'adresse (session + clé, BR-TRUST-007) quand la session seule n'apprend rien.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use hearth_agent::application::ports::IdentityStore;
use hearth_agent::domain::accounts::Role;
use hearth_agent::domain::session_token::SessionToken;
use hearth_agent::infrastructure::tls::FileIdentityStore;
use hearth_proto::device_proof::Binding;
use hearth_proto::stream::{ServerMessage, Topic};
use serde_json::{Value, json};
use support::device::DeviceKey;
use support::https::{self, Agent};
use support::ws::{self, WsClient};
use support::{Env, PASSWORD, env};

async fn challenge(agent: &Agent, username: &str, purpose: &str) -> String {
    let reply = agent
        .request("POST", "/sessions/challenge")
        .json(&json!({ "username": username, "purpose": purpose }))
        .send()
        .await;
    assert_eq!(reply.status, 200, "{:?}", reply.body);
    reply.body["challenge"].as_str().unwrap().to_owned()
}

fn device_json(proof: &hearth_proto::api::sessions::DeviceProof) -> Value {
    json!({
        "algorithm": proof.algorithm,
        "public_key": proof.public_key,
        "challenge": proof.challenge,
        "signature": proof.signature,
    })
}

async fn addresses(env: &Env) -> Vec<(String, Option<String>)> {
    sqlx::query_as("SELECT address, device_id FROM known_addresses ORDER BY address")
        .fetch_all(env.db.pool())
        .await
        .unwrap()
}

/// Ouvre le flux avec ce premier message et rend le client une fois le snapshot lu (la session
/// fonctionne).
async fn open_with(agent: &Agent, first_message: &Value) -> WsClient {
    let mut client = ws::open(agent).await;
    client.send_text(&first_message.to_string()).await;
    client.subscribe(&[Topic::Metrics]).await;
    assert!(
        matches!(client.expect().await, ServerMessage::Snapshot { .. }),
        "la session est acceptée"
    );
    client
}

#[tokio::test]
async fn the_key_is_proven_over_tls_enrolled_by_the_login_and_the_stream_proof_retains_the_address()
{
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let agent = https::start(&env).await;
    let fingerprint = FileIdentityStore::new(env.dir.path())
        .load_or_create()
        .unwrap()
        .fingerprint;
    let key = DeviceKey::new();

    // Connexion avec la preuve, signée pour l'empreinte du certificat de ce serveur.
    let issued = challenge(&agent, "marie", "login").await;
    let proof = key.sign(&fingerprint, Binding::Login, "marie", &issued);
    let reply = agent
        .request("POST", "/sessions")
        .json(&json!({ "username": "marie", "password": PASSWORD, "device": device_json(&proof) }))
        .send()
        .await;
    assert_eq!(reply.status, 201, "{:?}", reply.body);
    assert_eq!(reply.body["device"], "enrolled");
    let token = reply.body["token"].as_str().unwrap().to_owned();
    assert_eq!(addresses(&env).await.len(), 1);
    assert!(
        addresses(&env).await[0].1.is_some(),
        "adresse liée au poste"
    );

    // Le poste change d'adresse (la ligne retenue passe à une autre adresse).
    sqlx::query("UPDATE known_addresses SET address = '10.9.9.9'")
        .execute(env.db.pool())
        .await
        .unwrap();

    // Session seule au premier message du flux : l'adresse de la connexion n'est PAS apprise.
    let token_hash = SessionToken::parse(&token).unwrap().hash();
    let alone = json!({ "type": "auth", "token": token });
    let client = open_with(&agent, &alone).await;
    drop(client);
    let rows = addresses(&env).await;
    assert_eq!(
        rows.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(),
        vec!["10.9.9.9"],
        "la session seule n'apprend aucune adresse"
    );

    // Une preuve fausse au premier message : ignorée, la session fonctionne, rien n'est appris.
    let bad = json!({ "type": "auth", "token": token, "device": {
        "algorithm": "ed25519", "public_key": "x", "challenge": "y", "signature": "z" } });
    drop(open_with(&agent, &bad).await);
    assert_eq!(addresses(&env).await.len(), 1);
    assert_eq!(addresses(&env).await[0].0, "10.9.9.9");

    // Une preuve de CONNEXION (mauvais usage) au premier message : ignorée aussi.
    let login_issued = challenge(&agent, "marie", "login").await;
    let wrong_usage = key.sign(&fingerprint, Binding::Login, "marie", &login_issued);
    let first = json!({ "type": "auth", "token": token, "device": device_json(&wrong_usage) });
    drop(open_with(&agent, &first).await);
    assert_eq!(addresses(&env).await[0].0, "10.9.9.9");

    // Session + clé : la preuve d'usage « session », liée à ce jeton, fait retenir l'adresse.
    let session_issued = challenge(&agent, "marie", "session").await;
    let session_proof = key.sign(
        &fingerprint,
        Binding::Session {
            token_hash: token_hash.as_bytes(),
        },
        "marie",
        &session_issued,
    );
    let first = json!({ "type": "auth", "token": token, "device": device_json(&session_proof) });
    drop(open_with(&agent, &first).await);
    let rows = addresses(&env).await;
    assert_eq!(rows.len(), 1, "le poste n'a qu'une adresse : {rows:?}");
    assert_eq!(
        rows[0].0, "127.0.0.1",
        "l'adresse de la connexion TCP est retenue"
    );
    assert!(rows[0].1.is_some());

    // La même preuve rejouée sur un nouveau flux : le défi est consommé, la session fonctionne.
    sqlx::query("UPDATE known_addresses SET address = '10.9.9.9'")
        .execute(env.db.pool())
        .await
        .unwrap();
    drop(open_with(&agent, &first).await);
    assert_eq!(
        addresses(&env).await[0].0,
        "10.9.9.9",
        "rejeu : rien n'est appris"
    );
    agent.shutdown().await;
}

#[tokio::test]
async fn a_challenge_asked_over_tls_does_not_depend_on_the_identifier() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let agent = https::start(&env).await;
    let mut shapes = Vec::new();
    for username in ["marie", "fantome"] {
        let reply = agent
            .request("POST", "/sessions/challenge")
            .json(&json!({ "username": username, "purpose": "login" }))
            .send()
            .await;
        let mut names: Vec<String> = reply
            .headers
            .iter()
            .map(|(n, _)| n.to_lowercase())
            .collect();
        names.retain(|name| name != "date");
        names.sort();
        shapes.push((reply.status, names, reply.text.len()));
    }
    assert_eq!(shapes[0], shapes[1], "{shapes:?}");
    assert_eq!(shapes[0].0, 200);
    agent.shutdown().await;
}

//! Le côté client de la preuve de clé, pour les tests : une paire Ed25519 (`ring`), un défi demandé
//! à l'agent, et le message signé construit par `hearth_proto::device_proof` (la même disposition
//! que l'agent vérifie). Rien ici n'est du code de production.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use hearth_agent::application::trust::TrustService;
use hearth_proto::api::sessions::{ChallengePurpose, DeviceProof};
use hearth_proto::device_proof::{Binding, CHALLENGE_LEN, signing_bytes};
use hearth_proto::fingerprint::Fingerprint;
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};

use super::{Env, SERVER_FINGERPRINT};

pub struct DeviceKey {
    pair: Ed25519KeyPair,
}

pub fn purpose_of(binding: &Binding<'_>) -> ChallengePurpose {
    match binding {
        Binding::Login => ChallengePurpose::Login,
        Binding::Session { .. } => ChallengePurpose::Session,
        Binding::AttackMode { .. } => ChallengePurpose::AttackMode,
        Binding::DeviceRemoval { .. } => ChallengePurpose::DeviceRemoval,
    }
}

impl DeviceKey {
    pub fn new() -> Self {
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).expect("clé");
        Self {
            pair: Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).expect("pkcs8"),
        }
    }

    pub fn public_key(&self) -> [u8; 32] {
        self.pair
            .public_key()
            .as_ref()
            .try_into()
            .expect("32 octets")
    }

    /// L'empreinte de la clé publique, comme l'agent la calcule.
    pub fn key_id(&self) -> String {
        hearth_proto::device_proof::key_id(&self.public_key())
    }

    /// Signe ce défi pour ce serveur (empreinte donnée), cet identifiant et cet usage.
    pub fn sign(
        &self,
        fingerprint: &Fingerprint,
        binding: Binding<'_>,
        username: &str,
        challenge_b64: &str,
    ) -> DeviceProof {
        let raw = STANDARD.decode(challenge_b64).expect("défi en base64");
        let challenge: [u8; CHALLENGE_LEN] = raw.try_into().expect("56 octets");
        let message = signing_bytes(binding, fingerprint, username, &challenge);
        DeviceProof {
            algorithm: "ed25519".into(),
            public_key: STANDARD.encode(self.public_key()),
            challenge: challenge_b64.to_owned(),
            signature: STANDARD.encode(self.pair.sign(&message).as_ref()),
        }
    }

    /// Demande un défi au service (depuis cette adresse) puis le signe : le chemin d'un client
    /// honnête, pour le serveur des tests.
    pub fn prove(
        &self,
        trust: &TrustService,
        binding: Binding<'_>,
        username: &str,
        addr: &str,
    ) -> DeviceProof {
        let response = trust
            .issue_challenge(username, purpose_of(&binding), addr)
            .expect("défi");
        self.sign(
            &Fingerprint::from_bytes(SERVER_FINGERPRINT),
            binding,
            username,
            &response.challenge,
        )
    }

    pub fn login_proof(&self, env: &Env, username: &str, addr: &str) -> DeviceProof {
        self.prove(&env.trust, Binding::Login, username, addr)
    }
}

/// Corps de `POST /sessions` avec la preuve de cette clé (défi demandé par la route, signé pour le
/// serveur des tests).
pub async fn device_login_body(
    api: &super::api::Api,
    key: &DeviceKey,
    username: &str,
    password: &str,
) -> serde_json::Value {
    let reply = api
        .post("/sessions/challenge")
        .json(&serde_json::json!({ "username": username, "purpose": "login" }))
        .send()
        .await;
    let challenge = reply.body["challenge"].as_str().expect("défi").to_owned();
    let proof = key.sign(
        &Fingerprint::from_bytes(SERVER_FINGERPRINT),
        Binding::Login,
        username,
        &challenge,
    );
    serde_json::json!({ "username": username, "password": password, "device": device_json(&proof) })
}

/// Ouvre une session avec la preuve de cette clé et rend son jeton.
pub async fn login_token(
    api: &super::api::Api,
    key: &DeviceKey,
    username: &str,
    password: &str,
) -> String {
    let body = device_login_body(api, key, username, password).await;
    let reply = api.post("/sessions").json(&body).send().await;
    assert_eq!(
        reply.status,
        axum::http::StatusCode::CREATED,
        "{:?}",
        reply.body
    );
    reply.body["token"].as_str().expect("jeton").to_owned()
}

pub fn device_json(proof: &DeviceProof) -> serde_json::Value {
    serde_json::json!({
        "algorithm": proof.algorithm,
        "public_key": proof.public_key,
        "challenge": proof.challenge,
        "signature": proof.signature,
    })
}

/// Corps de `DELETE /me/devices/{target}` : le mot de passe et la preuve de possession de la clé du
/// poste courant (usage « retrait », liée au jeton de la session `token` et au poste visé).
pub async fn removal_body(
    api: &super::api::Api,
    key: &DeviceKey,
    username: &str,
    token: &str,
    target: &str,
    password: &str,
) -> serde_json::Value {
    let reply = api
        .post("/sessions/challenge")
        .json(&serde_json::json!({ "username": username, "purpose": "device_removal" }))
        .send()
        .await;
    let challenge = reply.body["challenge"].as_str().expect("défi").to_owned();
    let hash = hearth_agent::domain::session_token::SessionToken::parse(token)
        .expect("jeton")
        .hash();
    let proof = key.sign(
        &Fingerprint::from_bytes(SERVER_FINGERPRINT),
        Binding::DeviceRemoval {
            token_hash: hash.as_bytes(),
            target,
        },
        username,
        &challenge,
    );
    serde_json::json!({ "password": password, "device": device_json(&proof) })
}

/// Corps de `PUT /security/attack-mode` : le mot de passe et la preuve de possession d'une clé inscrite
/// (usage `0x03`, liée au jeton de la session `token` et au geste demandé, `active`).
pub async fn attack_mode_body(
    api: &super::api::Api,
    key: &DeviceKey,
    username: &str,
    token: &str,
    active: bool,
    password: &str,
) -> serde_json::Value {
    let reply = api
        .post("/sessions/challenge")
        .json(&serde_json::json!({ "username": username, "purpose": "attack_mode" }))
        .send()
        .await;
    let challenge = reply.body["challenge"].as_str().expect("défi").to_owned();
    let hash = hearth_agent::domain::session_token::SessionToken::parse(token)
        .expect("jeton")
        .hash();
    let proof = key.sign(
        &Fingerprint::from_bytes(SERVER_FINGERPRINT),
        Binding::AttackMode {
            token_hash: hash.as_bytes(),
            activate: active,
        },
        username,
        &challenge,
    );
    serde_json::json!({ "active": active, "password": password, "device": device_json(&proof) })
}

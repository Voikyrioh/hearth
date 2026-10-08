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
        Binding::DeviceRemoval { .. } => ChallengePurpose::DeviceRemoval,
        Binding::AdminAct { .. } => ChallengePurpose::AdminAct,
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

/// Le membre `reauth` d'un acte d'administration (HRT-28) : le mot de passe (vide : absent) et la preuve
/// d'usage `0x05` de cette clé, liée au jeton de la session `token` et à l'acte `act` (défi demandé à la
/// route, signé pour le serveur des tests).
pub async fn reauth_member(
    api: &super::api::Api,
    key: &DeviceKey,
    username: &str,
    token: &str,
    act: &hearth_proto::admin_act::AdminAct<'_>,
    password: &str,
) -> serde_json::Value {
    let reply = api
        .post("/sessions/challenge")
        .json(&serde_json::json!({ "username": username, "purpose": "admin_act" }))
        .send()
        .await;
    let challenge = reply.body["challenge"].as_str().expect("défi").to_owned();
    let hash = hearth_agent::domain::session_token::SessionToken::parse(token)
        .expect("jeton")
        .hash();
    let proof = key.sign(
        &Fingerprint::from_bytes(SERVER_FINGERPRINT),
        Binding::AdminAct {
            token_hash: hash.as_bytes(),
            act,
        },
        username,
        &challenge,
    );
    let mut member = serde_json::json!({ "device": device_json(&proof) });
    if !password.is_empty() {
        member["password"] = serde_json::Value::String(password.to_owned());
    }
    member
}

/// Le corps `body` d'un acte, augmenté de son membre `reauth`.
pub async fn with_reauth(
    api: &super::api::Api,
    key: &DeviceKey,
    username: &str,
    token: &str,
    act: &hearth_proto::admin_act::AdminAct<'_>,
    password: &str,
    mut body: serde_json::Value,
) -> serde_json::Value {
    body["reauth"] = reauth_member(api, key, username, token, act, password).await;
    body
}

/// Un poste de test : un compte, la clé de ce poste (inscrite par la connexion) et le jeton de la session
/// ouverte AVEC la preuve de cette clé. C'est ce que l'agent qui EXIGE demande à tout acte d'administration
/// (HRT-30) : un acte part par [`Api::act`] ou [`Actor::confirm`], jamais avec la session seule.
pub struct Actor {
    pub name: String,
    pub key: DeviceKey,
    pub token: String,
}

impl Env {
    /// Crée le compte et ouvre sa session depuis un poste dont la clé est inscrite.
    pub async fn actor(
        &self,
        api: &super::api::Api,
        name: &str,
        role: hearth_agent::domain::accounts::Role,
    ) -> Actor {
        self.create(name, role).await;
        self.login_actor(api, name).await
    }

    /// Ouvre une session de plus pour un compte existant, depuis un nouveau poste.
    pub async fn login_actor(&self, api: &super::api::Api, name: &str) -> Actor {
        let key = DeviceKey::new();
        let token = login_token(api, &key, name, super::PASSWORD).await;
        Actor {
            name: name.to_owned(),
            key,
            token,
        }
    }
}

/// L'acte que dit une requête (méthode, chemin, corps), comme l'agent le reconstruit : les 10 actes du
/// contrat commun. Le retrait d'un poste a son propre contrat (`removal_body`).
pub fn act_of<'a>(
    method: &str,
    path: &'a str,
    body: &'a serde_json::Value,
) -> hearth_proto::admin_act::AdminAct<'a> {
    use hearth_proto::admin_act::AdminAct;
    let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    let text = |field: &str| body[field].as_str().unwrap_or_default();
    let role = || serde_json::from_value(body["role"].clone()).expect("rôle de l'acte");
    match (method, parts.as_slice()) {
        ("POST", ["accounts"]) => AdminAct::AccountCreate {
            username: text("username"),
            role: role(),
        },
        ("PATCH", ["accounts", id]) => AdminAct::AccountRole {
            target: id,
            role: role(),
        },
        ("PUT", ["accounts", id, "password"]) => AdminAct::AccountPassword { target: id },
        ("DELETE", ["accounts", id]) => AdminAct::AccountDelete { target: id },
        ("DELETE", ["accounts", id, "sessions"]) => AdminAct::SessionsRevoke { target: id },
        ("POST", ["agent", "update"]) => AdminAct::AgentUpdate {
            version: text("version"),
            sha256: text("sha256"),
        },
        ("PUT", ["security", "attack-mode"]) => AdminAct::AttackMode {
            enable: body["active"].as_bool().unwrap_or_default(),
        },
        ("PUT", ["me", "password"]) => AdminAct::AccountPasswordOwn,
        ("PUT", ["me", "reauth"]) => AdminAct::ReauthSetting {
            mode: serde_json::from_value(body["password"].clone()).expect("réglage"),
        },
        _ => panic!("{method} {path} n'est pas un acte du contrat commun"),
    }
}

impl Actor {
    /// Le corps de cet acte, augmenté de son membre `reauth` : le mot de passe de ce compte et une preuve
    /// d'un défi NEUF. À envoyer tel quel pour un rejeu à l'identique (même clé d'opération).
    pub async fn confirm(
        &self,
        api: &super::api::Api,
        method: &axum::http::Method,
        path: &str,
        body: serde_json::Value,
    ) -> serde_json::Value {
        self.confirm_with(api, method, path, body, super::PASSWORD)
            .await
    }

    /// Comme [`Actor::confirm`], avec un autre mot de passe de confirmation (un faux, par exemple).
    pub async fn confirm_with(
        &self,
        api: &super::api::Api,
        method: &axum::http::Method,
        path: &str,
        mut body: serde_json::Value,
        password: &str,
    ) -> serde_json::Value {
        let member = {
            let act = act_of(method.as_str(), path, &body);
            reauth_member(api, &self.key, &self.name, &self.token, &act, password).await
        };
        body["reauth"] = member;
        body
    }
}

impl super::api::Api {
    /// Un acte d'administration CONFIRMÉ, envoyé par ce poste, avec le mot de passe du compte.
    pub async fn act(
        &self,
        who: &Actor,
        method: axum::http::Method,
        path: &str,
        body: serde_json::Value,
    ) -> super::api::Reply {
        let body = who.confirm(self, &method, path, body).await;
        self.call(method, path)
            .token(&who.token)
            .json(&body)
            .send()
            .await
    }
}

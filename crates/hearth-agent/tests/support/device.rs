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

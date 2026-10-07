//! Clé d'appareil Ed25519 (HRT-23, ADR-0023, BR-TRUST-003 et 005) : SEUL endroit du client qui
//! touche `ring`. La clé privée est un document PKCS#8 en base64 rangé au coffre
//! (`SecretKind::DeviceKey`, un `Secret`, effacé de la mémoire à la libération) ; le type
//! `DeviceKey` est opaque : aucun accès à la clé privée, `Debug` masqué, une seule opération qui
//! l'utilise, `sign`. La paire de `ring` n'est reconstruite que le temps d'une signature, jamais
//! gardée.

use std::fmt;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use hearth_proto::api::sessions::DeviceProof;
use hearth_proto::device_proof::{
    ALGORITHM_ED25519, Binding, CHALLENGE_LEN, PUBLIC_KEY_LEN, SIGNATURE_LEN, signing_bytes,
};
use hearth_proto::fingerprint::Fingerprint;
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};
use thiserror::Error;
use zeroize::Zeroize;

use crate::domain::secret::Secret;

/// Pourquoi une clé n'a pas pu être créée ou relue. Le message ne contient jamais la clé.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum DeviceKeyError {
    #[error("hasard du système indisponible")]
    Random,
    #[error("clé d'appareil illisible")]
    Unreadable,
}

/// La clé privée de ce PC pour un serveur. Opaque.
pub struct DeviceKey {
    /// PKCS#8, base64 standard.
    pkcs8: Secret,
}

impl fmt::Debug for DeviceKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DeviceKey(***)")
    }
}

impl DeviceKey {
    /// Tire une nouvelle paire du hasard du système.
    pub fn generate() -> Result<Self, DeviceKeyError> {
        let document = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
            .map_err(|_| DeviceKeyError::Random)?;
        Ok(Self {
            pkcs8: Secret::new(STANDARD.encode(document.as_ref())),
        })
    }

    /// Relit la clé rangée au coffre ; refuse tout ce qui n'est pas un PKCS#8 Ed25519 valide.
    pub fn from_secret(secret: &Secret) -> Result<Self, DeviceKeyError> {
        let key = Self {
            pkcs8: secret.clone(),
        };
        key.pair()?;
        Ok(key)
    }

    /// Ce qu'on range au coffre.
    pub fn to_secret(&self) -> Secret {
        self.pkcs8.clone()
    }

    fn pair(&self) -> Result<Ed25519KeyPair, DeviceKeyError> {
        let mut bytes = STANDARD
            .decode(self.pkcs8.expose())
            .map_err(|_| DeviceKeyError::Unreadable)?;
        let pair = Ed25519KeyPair::from_pkcs8(&bytes).map_err(|_| DeviceKeyError::Unreadable);
        bytes.zeroize();
        pair
    }

    /// La clé publique (32 octets) : la seule chose de la paire qui part vers l'agent.
    pub fn public_key(&self) -> Option<[u8; PUBLIC_KEY_LEN]> {
        let pair = self.pair().ok()?;
        pair.public_key().as_ref().try_into().ok()
    }

    fn sign(&self, message: &[u8]) -> Option<[u8; SIGNATURE_LEN]> {
        let pair = self.pair().ok()?;
        pair.sign(message).as_ref().try_into().ok()
    }

    /// La preuve de possession pour ce défi, cet usage et ce serveur : le message est celui de
    /// `hearth_proto::device_proof::signing_bytes`, jamais recopié. `challenge` est le défi tel que
    /// l'agent l'a rendu (base64 de 56 octets) ; un défi de la mauvaise forme ne donne aucune
    /// preuve (`None`), jamais une erreur : la connexion continue sans clé.
    pub fn prove(
        &self,
        binding: Binding<'_>,
        fingerprint: &Fingerprint,
        username: &str,
        challenge: &str,
    ) -> Option<DeviceProof> {
        let raw = STANDARD.decode(challenge.trim()).ok()?;
        let raw: [u8; CHALLENGE_LEN] = raw.try_into().ok()?;
        let message = signing_bytes(binding, fingerprint, username, &raw);
        let signature = self.sign(&message)?;
        Some(DeviceProof {
            algorithm: ALGORITHM_ED25519.to_owned(),
            public_key: STANDARD.encode(self.public_key()?),
            challenge: challenge.to_owned(),
            signature: STANDARD.encode(signature),
        })
    }
}

#[cfg(test)]
mod tests {
    use ring::signature::{ED25519, UnparsedPublicKey};

    use super::*;

    fn fingerprint() -> Fingerprint {
        Fingerprint::from_bytes([0xaa; 32])
    }

    fn challenge() -> ([u8; CHALLENGE_LEN], String) {
        let mut raw = [0_u8; CHALLENGE_LEN];
        for (index, byte) in raw.iter_mut().enumerate() {
            *byte = u8::try_from(index).unwrap_or(0);
        }
        (raw, STANDARD.encode(raw))
    }

    #[test]
    fn a_proof_verifies_with_the_public_key_against_the_shared_message_layout() {
        let key = DeviceKey::generate().unwrap();
        let (raw, text) = challenge();
        let proof = key
            .prove(Binding::Login, &fingerprint(), "Marie", &text)
            .unwrap();
        assert_eq!(proof.algorithm, "ed25519");
        assert_eq!(proof.challenge, text);
        let public = STANDARD.decode(&proof.public_key).unwrap();
        let signature = STANDARD.decode(&proof.signature).unwrap();
        assert_eq!((public.len(), signature.len()), (32, 64));
        // Le message vérifié est reconstruit par la source unique, identifiant normalisé compris.
        let message = signing_bytes(Binding::Login, &fingerprint(), "marie", &raw);
        UnparsedPublicKey::new(&ED25519, &public)
            .verify(&message, &signature)
            .unwrap();
        // Un autre usage ne vérifie pas.
        let session = signing_bytes(
            Binding::Session {
                token_hash: &[1; 32],
            },
            &fingerprint(),
            "marie",
            &raw,
        );
        assert!(
            UnparsedPublicKey::new(&ED25519, &public)
                .verify(&session, &signature)
                .is_err()
        );
    }

    #[test]
    fn the_key_survives_the_vault_round_trip_and_keeps_its_public_key() {
        let key = DeviceKey::generate().unwrap();
        let again = DeviceKey::from_secret(&key.to_secret()).unwrap();
        assert_eq!(key.public_key(), again.public_key());
        assert!(key.public_key().is_some());
        assert_ne!(
            key.public_key(),
            DeviceKey::generate().unwrap().public_key(),
            "deux clés tirées sont différentes"
        );
    }

    #[test]
    fn a_damaged_secret_or_challenge_gives_no_key_and_no_proof_and_never_panics() {
        assert!(DeviceKey::from_secret(&Secret::from("pas du base64 !")).is_err());
        assert!(DeviceKey::from_secret(&Secret::from("AAAA")).is_err());
        assert!(DeviceKey::from_secret(&Secret::from("")).is_err());
        let key = DeviceKey::generate().unwrap();
        for bad in [
            "",
            "###",
            &STANDARD.encode([1_u8; 55]),
            &STANDARD.encode([1_u8; 57]),
        ] {
            assert!(
                key.prove(Binding::Login, &fingerprint(), "marie", bad)
                    .is_none(),
                "{bad}"
            );
        }
    }

    #[test]
    fn the_debug_of_a_key_and_a_proof_never_shows_the_private_key() {
        let key = DeviceKey::generate().unwrap();
        let secret = key.to_secret();
        let (_, text) = challenge();
        let proof = key
            .prove(Binding::Login, &fingerprint(), "marie", &text)
            .unwrap();
        let shown = format!(
            "{key:?} {:?} {proof:?} {:?}",
            Some(&key),
            DeviceKeyError::Unreadable
        );
        assert!(!shown.contains(secret.expose()), "{shown}");
        assert_eq!(format!("{key:?}"), "DeviceKey(***)");
    }
}

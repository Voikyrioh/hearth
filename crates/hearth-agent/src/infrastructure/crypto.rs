//! Cryptographie de la clé d'appareil par `ring` (HRT-22, ADR-0023) : vérification Ed25519 et code
//! d'authentification des défis (HMAC-SHA256). `ring` est déjà compilé par `rustls`
//! (ADR-0005, ADR-0009) : ni OpenSSL ni aws-lc.

use hearth_proto::device_proof::ALGORITHM_ED25519;
use ring::hmac;
use ring::signature::{ED25519, UnparsedPublicKey};

use crate::application::ports::{ChallengeCrypto, CryptoError, ProofVerifier};

/// Vérifie les signatures Ed25519 des preuves.
pub struct RingProofVerifier;

impl ProofVerifier for RingProofVerifier {
    fn verify(&self, algorithm: &str, public_key: &[u8], message: &[u8], signature: &[u8]) -> bool {
        // `ring` refuse proprement une clé ou une signature de mauvaise longueur : aucune entrée ne
        // fait paniquer.
        algorithm == ALGORITHM_ED25519
            && UnparsedPublicKey::new(&ED25519, public_key)
                .verify(message, signature)
                .is_ok()
    }
}

/// Hasard et code d'authentification des défis. La clé est tirée au démarrage du service et ne
/// quitte jamais ce type : un redémarrage invalide les défis en cours (le client en redemande un).
pub struct HmacChallengeCrypto {
    key: hmac::Key,
}

impl HmacChallengeCrypto {
    /// Tire une clé de 32 octets du hasard du système.
    pub fn new() -> Result<Self, CryptoError> {
        let mut bytes = [0_u8; 32];
        getrandom::fill(&mut bytes).map_err(|error| CryptoError::Random(error.to_string()))?;
        Ok(Self::with_key(&bytes))
    }

    /// Clé imposée (tests).
    pub fn with_key(bytes: &[u8; 32]) -> Self {
        Self {
            key: hmac::Key::new(hmac::HMAC_SHA256, bytes),
        }
    }
}

impl ChallengeCrypto for HmacChallengeCrypto {
    fn nonce(&self) -> Result<[u8; 16], CryptoError> {
        let mut nonce = [0_u8; 16];
        getrandom::fill(&mut nonce).map_err(|error| CryptoError::Random(error.to_string()))?;
        Ok(nonce)
    }

    fn mac(&self, input: &[u8]) -> [u8; 32] {
        let tag = hmac::sign(&self.key, input);
        let mut mac = [0_u8; 32];
        // HMAC-SHA256 rend 32 octets : la copie couvre exactement le tableau.
        mac.copy_from_slice(tag.as_ref());
        mac
    }
}

#[cfg(test)]
mod tests {
    use ring::rand::SystemRandom;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    use super::*;

    fn key_pair() -> Ed25519KeyPair {
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap()
    }

    #[test]
    fn a_valid_signature_verifies_and_any_change_breaks_it() {
        let pair = key_pair();
        let public = pair.public_key().as_ref().to_vec();
        let signature = pair.sign(b"message");
        let verifier = RingProofVerifier;
        assert!(verifier.verify("ed25519", &public, b"message", signature.as_ref()));
        assert!(!verifier.verify("ed25519", &public, b"messagE", signature.as_ref()));
        let mut forged = signature.as_ref().to_vec();
        forged[0] ^= 1;
        assert!(!verifier.verify("ed25519", &public, b"message", &forged));
        let other = key_pair();
        assert!(!verifier.verify(
            "ed25519",
            other.public_key().as_ref(),
            b"message",
            signature.as_ref()
        ));
    }

    #[test]
    fn an_unknown_algorithm_or_a_malformed_input_is_false_never_a_panic() {
        let pair = key_pair();
        let public = pair.public_key().as_ref().to_vec();
        let signature = pair.sign(b"m");
        let verifier = RingProofVerifier;
        assert!(!verifier.verify("p256", &public, b"m", signature.as_ref()));
        assert!(!verifier.verify("", &public, b"m", signature.as_ref()));
        for len in [0, 1, 31, 33, 64, 4096] {
            assert!(!verifier.verify("ed25519", &vec![7; len], b"m", signature.as_ref()));
            assert!(!verifier.verify("ed25519", &public, b"m", &vec![7; len]));
        }
        assert!(!verifier.verify("ed25519", &[], b"", &[]));
    }

    /// Ce que `ring` ne vérifie pas (ring 0.17.14, `ec/curve25519/ed25519/verification.rs`) : l'ordre de
    /// la clé publique. Avec le point neutre et la signature (R = point neutre, S = 0), une même signature
    /// vérifie tout message. C'est pourquoi le domaine refuse ces clés avant (`has_small_order`).
    #[test]
    fn ring_itself_accepts_the_neutral_point_key_so_the_domain_must_refuse_it() {
        let mut neutral = [0_u8; 32];
        neutral[0] = 1;
        let mut signature = [0_u8; 64];
        signature[0] = 1;
        let verifier = RingProofVerifier;
        assert!(verifier.verify("ed25519", &neutral, b"un message", &signature));
        assert!(verifier.verify("ed25519", &neutral, b"un tout autre message", &signature));
        assert!(crate::domain::trust::has_small_order(&neutral));
    }

    /// Ce que `ring` vérifie : S doit être réduit (< L), et R est comparé octet à octet à la valeur
    /// recalculée : une signature rendue malléable par S + L est refusée.
    #[test]
    fn ring_refuses_a_non_canonical_scalar_s() {
        let pair = key_pair();
        let public = pair.public_key().as_ref().to_vec();
        let signature = pair.sign(b"m");
        // L = ordre du sous-groupe, petit-boutiste.
        const L: [u8; 32] = [
            0xed, 0xd3, 0xf5, 0x5c, 0x1a, 0x63, 0x12, 0x58, 0xd6, 0x9c, 0xf7, 0xa2, 0xde, 0xf9,
            0xde, 0x14, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x10,
        ];
        let mut malleated = signature.as_ref().to_vec();
        let mut carry = 0_u16;
        for (i, limb) in L.iter().enumerate() {
            let total = u16::from(malleated[32 + i]) + u16::from(*limb) + carry;
            malleated[32 + i] = (total & 0xff) as u8;
            carry = total >> 8;
        }
        let verifier = RingProofVerifier;
        assert!(verifier.verify("ed25519", &public, b"m", signature.as_ref()));
        assert!(!verifier.verify("ed25519", &public, b"m", &malleated));
    }

    #[test]
    fn the_code_depends_on_the_key_and_the_input_and_is_stable() {
        let a = HmacChallengeCrypto::with_key(&[1; 32]);
        let b = HmacChallengeCrypto::with_key(&[2; 32]);
        assert_eq!(a.mac(b"x"), a.mac(b"x"));
        assert_ne!(a.mac(b"x"), a.mac(b"y"));
        assert_ne!(a.mac(b"x"), b.mac(b"x"));
    }

    #[test]
    fn nonces_do_not_repeat_and_two_services_do_not_share_a_key() {
        let a = HmacChallengeCrypto::new().unwrap();
        let b = HmacChallengeCrypto::new().unwrap();
        assert_ne!(a.nonce().unwrap(), a.nonce().unwrap());
        assert_ne!(a.mac(b"x"), b.mac(b"x"));
    }
}

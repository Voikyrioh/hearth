//! Vérification des signatures minisign (Ed25519, ADR-0008) avec la clé publique **embarquée** :
//! compilée dans le binaire (`update-key.pub`, ou le fichier de `HEARTH_UPDATE_PUBKEY_FILE` donné
//! à la construction : tests de bout en bout seulement). Aucune clé ne se lit sur le disque de la
//! machine : qui peut écrire un fichier sur le serveur ne peut pas faire accepter son binaire.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use minisign_verify::{PublicKey, Signature};

use crate::application::ports::{SignatureError, SignatureVerifier};

/// La clé publique de cette construction.
pub const EMBEDDED_PUBLIC_KEY: &str = include_str!(concat!(env!("OUT_DIR"), "/update-key.pub"));

pub struct MinisignVerifier {
    key: PublicKey,
    key_id: [u8; 8],
}

impl MinisignVerifier {
    /// Avec la clé embarquée.
    pub fn embedded() -> Result<Self, String> {
        Self::new(EMBEDDED_PUBLIC_KEY)
    }

    /// Avec cette clé publique (le contenu d'un fichier `.pub` de minisign).
    pub fn new(public_key: &str) -> Result<Self, String> {
        let key = PublicKey::decode(public_key.trim())
            .map_err(|error| format!("clé publique de mise à jour illisible : {error}"))?;
        let key_id = key_id_of(public_key).ok_or_else(|| {
            "clé publique de mise à jour illisible : identifiant absent".to_owned()
        })?;
        Ok(Self { key, key_id })
    }
}

/// Les 8 octets d'identifiant de clé : octets 2 à 10 de la ligne en base64 (clé publique ou
/// signature : même disposition).
fn key_id_of(text: &str) -> Option<[u8; 8]> {
    let line = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("untrusted comment:"))?;
    let bytes = STANDARD.decode(line).ok()?;
    bytes.get(2..10)?.try_into().ok()
}

/// Le texte d'une signature : le contenu d'un `.minisig`, ou son encodage base64 (le format du
/// greffon de mise à jour de Tauri).
fn signature_text(input: &str) -> Option<String> {
    let input = input.trim();
    if input.starts_with("untrusted comment:") {
        return Some(input.to_owned());
    }
    let decoded = STANDARD
        .decode(input.split_whitespace().collect::<String>())
        .ok()?;
    let text = String::from_utf8(decoded).ok()?;
    text.trim()
        .starts_with("untrusted comment:")
        .then(|| text.trim().to_owned())
}

impl SignatureVerifier for MinisignVerifier {
    fn check_format(&self, signature: &str) -> Result<(), SignatureError> {
        let text = signature_text(signature).ok_or(SignatureError::Malformed)?;
        Signature::decode(&text).map_err(|_| SignatureError::Malformed)?;
        match key_id_of(&text) {
            Some(id) if id == self.key_id => Ok(()),
            _ => Err(SignatureError::Malformed),
        }
    }

    fn verify(&self, data: &[u8], signature: &str) -> Result<(), SignatureError> {
        let text = signature_text(signature).ok_or(SignatureError::Malformed)?;
        let signature = Signature::decode(&text).map_err(|_| SignatureError::Malformed)?;
        self.key
            .verify(data, &signature, false)
            .map_err(|_| SignatureError::Wrong)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    struct Pair {
        public: String,
        secret: minisign::SecretKey,
    }

    fn pair() -> Pair {
        let keys = minisign::KeyPair::generate_unencrypted_keypair().expect("clés");
        Pair {
            public: keys.pk.to_box().expect("boîte").to_string(),
            secret: keys.sk,
        }
    }

    fn sign(pair: &Pair, data: &[u8]) -> String {
        let public = minisign::PublicKey::from_box(
            minisign::PublicKeyBox::from_string(&pair.public).expect("clé"),
        )
        .ok();
        minisign::sign(public.as_ref(), &pair.secret, Cursor::new(data), None, None)
            .expect("signature")
            .to_string()
    }

    #[test]
    fn a_good_signature_is_accepted_as_text_and_as_base64() {
        let pair = pair();
        let verifier = MinisignVerifier::new(&pair.public).unwrap();
        let data = b"binaire de l'agent";
        let text = sign(&pair, data);
        assert!(verifier.check_format(&text).is_ok());
        assert!(verifier.verify(data, &text).is_ok());
        let encoded = STANDARD.encode(&text);
        assert!(verifier.check_format(&encoded).is_ok());
        assert!(verifier.verify(data, &encoded).is_ok());
    }

    #[test]
    fn a_signature_of_other_content_is_wrong_not_malformed() {
        let pair = pair();
        let verifier = MinisignVerifier::new(&pair.public).unwrap();
        let text = sign(&pair, b"autre contenu");
        assert!(
            verifier.check_format(&text).is_ok(),
            "bien formée, de la bonne clé"
        );
        assert_eq!(
            verifier.verify(b"binaire", &text),
            Err(SignatureError::Wrong)
        );
    }

    #[test]
    fn a_signature_by_another_key_is_refused_before_any_download() {
        let ours = pair();
        let theirs = pair();
        let verifier = MinisignVerifier::new(&ours.public).unwrap();
        let text = sign(&theirs, b"binaire");
        assert_eq!(verifier.check_format(&text), Err(SignatureError::Malformed));
        assert!(verifier.verify(b"binaire", &text).is_err());
    }

    #[test]
    fn garbage_is_malformed() {
        let verifier = MinisignVerifier::new(&pair().public).unwrap();
        for text in [
            "",
            "n'importe quoi",
            "untrusted comment: x\nnon-base64!!",
            "!!!!",
        ] {
            assert_eq!(
                verifier.check_format(text),
                Err(SignatureError::Malformed),
                "{text:?}"
            );
            assert!(verifier.verify(b"x", text).is_err(), "{text:?}");
        }
    }

    #[test]
    fn the_embedded_key_is_a_valid_public_key() {
        assert!(MinisignVerifier::embedded().is_ok());
    }
}

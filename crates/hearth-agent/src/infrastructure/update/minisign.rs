//! Vérification des signatures minisign (Ed25519, ADR-0008) avec la clé publique **embarquée** :
//! compilée dans le binaire (`update-key.pub`, ou le fichier de `HEARTH_UPDATE_PUBKEY_FILE` donné
//! à la construction : tests de bout en bout seulement). Aucune clé ne se lit sur le disque de la
//! machine : qui peut écrire un fichier sur le serveur ne peut pas faire accepter son binaire.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use minisign_verify::{PublicKey, Signature};

use crate::application::ports::{SignatureError, SignatureVerifier};
use crate::domain::install::Version;
use crate::domain::update::check_signed_version;

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
    fn check_format(&self, signature: &str, version: &Version) -> Result<(), SignatureError> {
        let text = signature_text(signature).ok_or(SignatureError::Malformed)?;
        let decoded = Signature::decode(&text).map_err(|_| SignatureError::Malformed)?;
        match key_id_of(&text) {
            Some(id) if id == self.key_id => {}
            _ => return Err(SignatureError::Malformed),
        }
        // Lu sans le contenu, donc pas encore garanti : refus précoce seulement. `verify` fait foi.
        check_signed_version(decoded.trusted_comment(), version).map_err(SignatureError::Version)
    }

    fn verify(
        &self,
        data: &[u8],
        signature: &str,
        version: &Version,
    ) -> Result<(), SignatureError> {
        let text = signature_text(signature).ok_or(SignatureError::Malformed)?;
        let signature = Signature::decode(&text).map_err(|_| SignatureError::Malformed)?;
        // minisign-verify contrôle la signature des octets ET la signature globale (octets de la
        // signature + commentaire signé) : le commentaire lu ensuite est donc authentique.
        self.key
            .verify(data, &signature, false)
            .map_err(|_| SignatureError::Wrong)?;
        check_signed_version(signature.trusted_comment(), version).map_err(SignatureError::Version)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::domain::update::SignedVersionRefusal;

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

    const V: Version = Version::new(0, 2, 0);

    /// Signé avec ce commentaire de confiance (ou aucun : minisign met alors un horodatage).
    fn sign_with(pair: &Pair, data: &[u8], comment: Option<&str>) -> String {
        let public = minisign::PublicKey::from_box(
            minisign::PublicKeyBox::from_string(&pair.public).expect("clé"),
        )
        .ok();
        minisign::sign(
            public.as_ref(),
            &pair.secret,
            Cursor::new(data),
            comment,
            None,
        )
        .expect("signature")
        .to_string()
    }

    /// Signé comme le fait `xtask` : `version:0.2.0` dans le commentaire de confiance.
    fn sign(pair: &Pair, data: &[u8]) -> String {
        sign_with(pair, data, Some("timestamp:1\tfile:agent\tversion:0.2.0"))
    }

    #[test]
    fn a_good_signature_is_accepted_as_text_and_as_base64() {
        let pair = pair();
        let verifier = MinisignVerifier::new(&pair.public).unwrap();
        let data = b"binaire de l'agent";
        let text = sign(&pair, data);
        assert!(verifier.check_format(&text, &V).is_ok());
        assert!(verifier.verify(data, &text, &V).is_ok());
        let encoded = STANDARD.encode(&text);
        assert!(verifier.check_format(&encoded, &V).is_ok());
        assert!(verifier.verify(data, &encoded, &V).is_ok());
    }

    #[test]
    fn a_signature_of_other_content_is_wrong_not_malformed() {
        let pair = pair();
        let verifier = MinisignVerifier::new(&pair.public).unwrap();
        let text = sign(&pair, b"autre contenu");
        assert!(
            verifier.check_format(&text, &V).is_ok(),
            "bien formée, de la bonne clé"
        );
        assert_eq!(
            verifier.verify(b"binaire", &text, &V),
            Err(SignatureError::Wrong)
        );
    }

    #[test]
    fn a_signature_by_another_key_is_refused_before_any_download() {
        let ours = pair();
        let theirs = pair();
        let verifier = MinisignVerifier::new(&ours.public).unwrap();
        let text = sign(&theirs, b"binaire");
        assert_eq!(
            verifier.check_format(&text, &V),
            Err(SignatureError::Malformed)
        );
        assert!(verifier.verify(b"binaire", &text, &V).is_err());
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
                verifier.check_format(text, &V),
                Err(SignatureError::Malformed),
                "{text:?}"
            );
            assert!(verifier.verify(b"x", text, &V).is_err(), "{text:?}");
        }
    }

    #[test]
    fn a_signature_without_a_signed_version_is_refused_before_and_after_the_download() {
        let pair = pair();
        let verifier = MinisignVerifier::new(&pair.public).unwrap();
        let data = b"binaire";
        // Sans commentaire choisi (horodatage seul), ou avec un commentaire sans version.
        for text in [
            sign_with(&pair, data, None),
            sign_with(&pair, data, Some("timestamp:1\tfile:agent")),
        ] {
            let refusal = Err(SignatureError::Version(SignedVersionRefusal::Missing));
            assert_eq!(verifier.check_format(&text, &V), refusal);
            assert_eq!(verifier.verify(data, &text, &V), refusal);
        }
    }

    #[test]
    fn a_signed_version_other_than_the_requested_one_is_refused_smaller_and_greater() {
        let pair = pair();
        let verifier = MinisignVerifier::new(&pair.public).unwrap();
        let data = b"binaire";
        for signed in ["0.1.0", "0.1.9", "0.2.1", "1.0.0"] {
            let text = sign_with(&pair, data, Some(&format!("file:agent\tversion:{signed}")));
            assert!(matches!(
                verifier.check_format(&text, &V),
                Err(SignatureError::Version(
                    SignedVersionRefusal::Mismatch { .. }
                ))
            ));
            assert!(matches!(
                verifier.verify(data, &text, &V),
                Err(SignatureError::Version(
                    SignedVersionRefusal::Mismatch { .. }
                ))
            ));
        }
    }

    #[test]
    fn the_signed_comment_is_covered_by_the_global_signature() {
        // Un ancien binaire authentique (signé 0.1.0) dont on remplace le commentaire par
        // `version:0.2.0` : la lecture seule du commentaire l'accepterait, la signature globale
        // (octets de la signature + commentaire) le refuse.
        let pair = pair();
        let verifier = MinisignVerifier::new(&pair.public).unwrap();
        let data = b"ancien agent";
        let honest = sign_with(&pair, data, Some("file:agent\tversion:0.1.0"));
        let forged = honest.replace("version:0.1.0", "version:0.2.0");
        assert_ne!(honest, forged);
        assert!(
            verifier.check_format(&forged, &V).is_ok(),
            "le refus précoce ne voit que le texte"
        );
        assert_eq!(
            verifier.verify(data, &forged, &V),
            Err(SignatureError::Wrong),
            "la signature globale fait foi"
        );
    }

    #[test]
    fn the_embedded_key_is_a_valid_public_key() {
        assert!(MinisignVerifier::embedded().is_ok());
    }
}

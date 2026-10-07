//! Empreinte à clé des requêtes suivies par `ring` (HRT-32, ADR-0033) : HMAC-SHA-256 de
//! `domain::operations::canonical_request`, clé par le secret de l'installation. `ring` est déjà
//! compilé par `rustls` (ADR-0005, ADR-0009).

use ring::hmac;

use crate::application::ports::RequestFingerprinter;
use crate::domain::fingerprint_secret::FingerprintSecret;
use crate::domain::operations::{FINGERPRINT_LEN, RequestFingerprint, canonical_request};

pub struct HmacFingerprinter {
    key: hmac::Key,
}

impl HmacFingerprinter {
    pub fn new(secret: &FingerprintSecret) -> Self {
        Self {
            key: hmac::Key::new(hmac::HMAC_SHA256, secret.expose()),
        }
    }
}

/// Pour les chemins qui n'ont aucune requête suivie à servir (sous-commandes `account` et
/// `attack-mode`, assemblages sans HTTP) : n'a pas de secret, ne lit ni ne crée aucun fichier, et
/// rend toujours une empreinte effacée, égale à rien. Si une requête suivie arrivait par là, aucune
/// clé ne serait reconnue : rien ne s'exécuterait « au cas où ».
pub struct NoFingerprint;

impl RequestFingerprinter for NoFingerprint {
    fn fingerprint(&self, _method: &str, _path: &str, _body: &[u8]) -> RequestFingerprint {
        RequestFingerprint::purged()
    }
}

impl RequestFingerprinter for HmacFingerprinter {
    fn fingerprint(&self, method: &str, path: &str, body: &[u8]) -> RequestFingerprint {
        let tag = hmac::sign(&self.key, &canonical_request(method, path, body));
        let mut mac = [0_u8; FINGERPRINT_LEN];
        // HMAC-SHA256 rend 32 octets : la copie couvre exactement le tableau.
        mac.copy_from_slice(tag.as_ref());
        RequestFingerprint::from_mac(mac)
    }
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    use super::*;

    fn with(byte: u8) -> HmacFingerprinter {
        HmacFingerprinter::new(&FingerprintSecret::from_bytes([byte; 32]))
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn the_same_request_gives_the_same_fingerprint() {
        assert_eq!(
            with(1).fingerprint("PUT", "/me/password", b"{}"),
            with(1).fingerprint("PUT", "/me/password", b"{}")
        );
    }

    #[test]
    fn method_path_or_body_changes_the_fingerprint() {
        let f = with(1);
        let base = f.fingerprint("PUT", "/a", b"x");
        assert_ne!(base, f.fingerprint("DELETE", "/a", b"x"));
        assert_ne!(base, f.fingerprint("PUT", "/b", b"x"));
        assert_ne!(base, f.fingerprint("PUT", "/a", b"y"));
        // Une frontière déplacée entre les parties ne donne pas la même empreinte.
        assert_ne!(f.fingerprint("a", "bc", b""), f.fingerprint("ab", "c", b""));
        assert_ne!(
            f.fingerprint("PUT", "/ab", b""),
            f.fingerprint("PUT", "/a", b"b")
        );
    }

    #[test]
    fn two_secrets_give_two_fingerprints() {
        assert_ne!(
            with(1).fingerprint("PUT", "/a", b"x"),
            with(2).fingerprint("PUT", "/a", b"x")
        );
    }

    /// GARDE (HRT-32) : l'empreinte ne doit jamais redevenir calculable sans le secret. Un attaquant
    /// qui lit la base connaît la méthode, le chemin et devine le corps (un mot de passe candidat) :
    /// s'il retrouve l'empreinte stockée avec un haché sans clé, ou avec une clé vide, le défaut est
    /// revenu. Ce test échoue alors.
    #[test]
    fn the_fingerprint_cannot_be_recomputed_without_the_secret() {
        let body = br#"{"username":"marie","password":"Un-bon-mot-de-passe-1","role":"admin"}"#;
        let stored = with(9).fingerprint("POST", "/accounts", body).to_stored();

        // L'ancien calcul (SHA-256 nu, séparateurs nuls) et le SHA-256 nu de l'encodage actuel.
        let mut legacy = b"POST\0/accounts\0".to_vec();
        legacy.extend_from_slice(body);
        assert_ne!(stored, hex(&Sha256::digest(&legacy)));
        assert_ne!(
            stored,
            hex(&Sha256::digest(canonical_request(
                "POST",
                "/accounts",
                body
            )))
        );
        // Une clé vide, ou toute à zéro, ne redonne pas l'empreinte.
        let empty = hmac::Key::new(hmac::HMAC_SHA256, b"");
        let tag = hmac::sign(&empty, &canonical_request("POST", "/accounts", body));
        assert_ne!(stored, hex(tag.as_ref()));
        assert_ne!(
            stored,
            with(0).fingerprint("POST", "/accounts", body).to_stored()
        );
    }

    #[test]
    fn without_a_secret_nothing_is_ever_recognised() {
        let none = NoFingerprint.fingerprint("PUT", "/a", b"x");
        assert!(none.is_purged());
        assert_ne!(none, with(1).fingerprint("PUT", "/a", b"x"));
    }
}

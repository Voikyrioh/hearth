//! Empreinte du certificat de l'agent (BR-CONN-001, BR-INSTALL-004).
//!
//! Vit dans `hearth-proto` parce que l'agent et `hearth-link` doivent calculer et afficher
//! exactement la même chose.
//!
//! L'empreinte est le SHA-256 du certificat au format DER. La comparaison porte
//! toujours sur les 32 octets ; l'affichage n'en montre que les 16 premiers, en
//! 8 groupes de 4 caractères hexadécimaux majuscules séparés par des espaces.

use std::fmt;

use sha2::{Digest, Sha256};
use thiserror::Error;

const LEN: usize = 32;
/// Nombre d'octets visibles dans l'affichage court.
const SHORT_BYTES: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fingerprint([u8; LEN]);

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FingerprintError {
    #[error("l'empreinte doit contenir 64 caractères hexadécimaux, reçu {0}")]
    BadLength(usize),
    #[error("caractère non hexadécimal dans l'empreinte")]
    NotHex,
}

impl Fingerprint {
    pub const fn from_bytes(bytes: [u8; LEN]) -> Self {
        Self(bytes)
    }

    /// Calcule l'empreinte d'un certificat encodé en DER.
    pub fn of_certificate_der(der: &[u8]) -> Self {
        Self(Sha256::digest(der).into())
    }

    pub const fn as_bytes(&self) -> &[u8; LEN] {
        &self.0
    }

    /// Forme complète : 64 caractères hexadécimaux minuscules, sans séparateur.
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Relit la forme complète produite par [`Fingerprint::to_hex`] (casse indifférente).
    pub fn from_hex(text: &str) -> Result<Self, FingerprintError> {
        let text = text.trim();
        if text.len() != LEN * 2 {
            return Err(FingerprintError::BadLength(text.len()));
        }
        // `from_str_radix` accepte un signe : on filtre avant toute conversion.
        if !text.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(FingerprintError::NotHex);
        }
        let mut bytes = [0u8; LEN];
        for (byte, pair) in bytes.iter_mut().zip(text.as_bytes().chunks(2)) {
            let pair = std::str::from_utf8(pair).map_err(|_| FingerprintError::NotHex)?;
            *byte = u8::from_str_radix(pair, 16).map_err(|_| FingerprintError::NotHex)?;
        }
        Ok(Self(bytes))
    }

    /// Affichage court destiné à la comparaison visuelle : 8 groupes de 4 hexadécimaux.
    pub fn short(&self) -> String {
        let groups: Vec<String> = self.0[..SHORT_BYTES]
            .chunks(2)
            .map(|pair| format!("{:02X}{:02X}", pair[0], pair[1]))
            .collect();
        groups.join(" ")
    }
}

impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.short())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp() -> Fingerprint {
        let mut bytes = [0u8; LEN];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = 0x0a + (i as u8) * 7;
        }
        Fingerprint::from_bytes(bytes)
    }

    #[test]
    fn short_form_is_eight_groups_of_four_uppercase_hex() {
        let fp = Fingerprint::from_bytes([0xab; LEN]);
        assert_eq!(fp.short(), "ABAB ABAB ABAB ABAB ABAB ABAB ABAB ABAB");
        let short = ramp().short();
        let groups: Vec<&str> = short.split(' ').collect();
        assert_eq!(groups.len(), 8);
        for group in groups {
            assert_eq!(group.len(), 4);
            assert!(
                group
                    .chars()
                    .all(|c| c.is_ascii_digit() || ('A'..='F').contains(&c))
            );
        }
    }

    #[test]
    fn short_form_uses_only_the_first_sixteen_bytes() {
        let mut other = *ramp().as_bytes();
        other[SHORT_BYTES..].fill(0xff);
        let other = Fingerprint::from_bytes(other);
        assert_eq!(ramp().short(), other.short());
    }

    #[test]
    fn equality_compares_all_thirty_two_bytes() {
        let mut other = *ramp().as_bytes();
        other[LEN - 1] ^= 1;
        let other = Fingerprint::from_bytes(other);
        assert_eq!(ramp().short(), other.short());
        assert_ne!(ramp(), other);
    }

    #[test]
    fn display_is_the_short_form() {
        assert_eq!(ramp().to_string(), ramp().short());
    }

    #[test]
    fn hashes_the_der_with_sha256() {
        // SHA-256 de l'entrée vide : valeur de référence connue.
        let fp = Fingerprint::of_certificate_der(b"");
        assert_eq!(
            fp.to_hex(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(fp.short(), "E3B0 C442 98FC 1C14 9AFB F4C8 996F B924");
    }

    #[test]
    fn hex_round_trips_and_ignores_case() {
        let fp = ramp();
        assert_eq!(Fingerprint::from_hex(&fp.to_hex()), Ok(fp));
        assert_eq!(Fingerprint::from_hex(&fp.to_hex().to_uppercase()), Ok(fp));
    }

    #[test]
    fn parse_rejects_a_sign_in_front_of_a_byte() {
        let mut text = Fingerprint::from_bytes([0x11; LEN]).to_hex();
        text.replace_range(0..2, "+a");
        assert_eq!(Fingerprint::from_hex(&text), Err(FingerprintError::NotHex));
    }

    #[test]
    fn parse_rejects_bad_input() {
        assert_eq!(
            Fingerprint::from_hex("abcd"),
            Err(FingerprintError::BadLength(4))
        );
        assert_eq!(
            Fingerprint::from_hex(&"zz".repeat(32)),
            Err(FingerprintError::NotHex)
        );
    }
}

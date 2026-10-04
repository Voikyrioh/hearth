//! Jeton de session : 32 octets aléatoires, encodés en hexadécimal pour le fil, jamais conservés.
//! Seule leur empreinte SHA-256 est stockée : une base lue par un tiers ne donne aucune session.
//!
//! Le hasard vient du port `TokenGen` (système d'exploitation) ; ici on ne fait que de la
//! représentation, du hachage et de la comparaison, sans E/S.

use std::fmt;

use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use thiserror::Error;
use zeroize::Zeroize;

pub const TOKEN_LEN: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("jeton illisible")]
pub struct InvalidToken;

/// Le jeton en clair, tel que le client le présente. Pas de `Clone`, `Debug` masqué, effacé de
/// la mémoire à la libération.
pub struct SessionToken([u8; TOKEN_LEN]);

impl SessionToken {
    pub fn from_bytes(bytes: [u8; TOKEN_LEN]) -> Self {
        Self(bytes)
    }

    /// Relit un jeton reçu sur le fil : exactement 64 chiffres hexadécimaux.
    pub fn parse(text: &str) -> Result<Self, InvalidToken> {
        decode_hex(text).map(Self)
    }

    /// Forme du fil : hexadécimal minuscule, 64 caractères.
    pub fn encode(&self) -> String {
        encode_hex(&self.0)
    }

    pub fn hash(&self) -> TokenHash {
        TokenHash(Sha256::digest(self.0).into())
    }
}

impl fmt::Debug for SessionToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SessionToken(***)")
    }
}

impl Drop for SessionToken {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// Empreinte SHA-256 d'un jeton : ce qui est stocké et retrouvé. La comparaison se fait en
/// temps constant.
#[derive(Clone)]
pub struct TokenHash([u8; TOKEN_LEN]);

impl TokenHash {
    /// Forme stockée en base : hexadécimal minuscule.
    pub fn to_hex(&self) -> String {
        encode_hex(&self.0)
    }

    pub fn from_hex(text: &str) -> Result<Self, InvalidToken> {
        decode_hex(text).map(Self)
    }
}

impl PartialEq for TokenHash {
    fn eq(&self, other: &Self) -> bool {
        self.0.ct_eq(&other.0).into()
    }
}

impl Eq for TokenHash {}

impl fmt::Debug for TokenHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TokenHash(***)")
    }
}

fn encode_hex(bytes: &[u8; TOKEN_LEN]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(TOKEN_LEN * 2);
    for byte in bytes {
        text.push(char::from(DIGITS[usize::from(byte >> 4)]));
        text.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    text
}

fn decode_hex(text: &str) -> Result<[u8; TOKEN_LEN], InvalidToken> {
    let digits = text.as_bytes();
    if digits.len() != TOKEN_LEN * 2 {
        return Err(InvalidToken);
    }
    let nibble = |digit: u8| -> Result<u8, InvalidToken> {
        match digit {
            b'0'..=b'9' => Ok(digit - b'0'),
            b'a'..=b'f' => Ok(digit - b'a' + 10),
            b'A'..=b'F' => Ok(digit - b'A' + 10),
            _ => Err(InvalidToken),
        }
    };
    let mut bytes = [0_u8; TOKEN_LEN];
    for (byte, pair) in bytes.iter_mut().zip(digits.chunks_exact(2)) {
        *byte = nibble(pair[0])? << 4 | nibble(pair[1])?;
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(fill: u8) -> SessionToken {
        SessionToken::from_bytes([fill; TOKEN_LEN])
    }

    #[test]
    fn the_wire_form_is_64_lowercase_hex_digits_that_round_trip() {
        let encoded = token(0xab).encode();
        assert_eq!(encoded, "ab".repeat(32));
        let back = SessionToken::parse(&encoded).unwrap();
        assert_eq!(back.encode(), encoded);
    }

    #[test]
    fn uppercase_hex_is_read_back() {
        let parsed = SessionToken::parse(&"AB".repeat(32)).unwrap();
        assert_eq!(parsed.encode(), "ab".repeat(32));
    }

    #[test]
    fn a_token_of_the_wrong_length_or_alphabet_is_refused() {
        for bad in [
            "",
            "ab",
            &"ab".repeat(31),
            &"ab".repeat(33),
            &"zz".repeat(32),
            &format!("{}é", "a".repeat(63)),
            &format!(" {}", "a".repeat(63)),
        ] {
            assert_eq!(
                SessionToken::parse(bad).unwrap_err(),
                InvalidToken,
                "{bad:?}"
            );
        }
    }

    #[test]
    fn the_hash_is_the_sha256_of_the_raw_bytes_and_differs_from_the_token() {
        let hash = token(0x01).hash();
        let expected = Sha256::digest([0x01_u8; TOKEN_LEN]);
        assert_eq!(hash.to_hex(), encode_hex(&expected.into()));
        assert_ne!(hash.to_hex(), token(0x01).encode());
    }

    #[test]
    fn hashes_compare_equal_only_when_every_byte_matches() {
        assert_eq!(token(1).hash(), token(1).hash());
        assert_ne!(token(1).hash(), token(2).hash());
        let mut bytes = [1_u8; TOKEN_LEN];
        bytes[TOKEN_LEN - 1] = 2;
        assert_ne!(token(1).hash(), SessionToken::from_bytes(bytes).hash());
    }

    #[test]
    fn a_stored_hash_round_trips() {
        let hash = token(7).hash();
        assert_eq!(TokenHash::from_hex(&hash.to_hex()).unwrap(), hash);
        assert!(TokenHash::from_hex("pas un haché").is_err());
    }

    #[test]
    fn debug_never_shows_the_token_or_its_hash() {
        let token = token(0xab);
        let text = format!("{token:?} {:?}", token.hash());
        assert!(!text.contains("abab"), "{text}");
    }
}

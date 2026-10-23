//! Hasard du système d'exploitation, pour les jetons de session.

use zeroize::Zeroize;

use crate::application::ports::{RandomError, SecretRandom, TokenGen, TokenGenError};
use crate::domain::session_token::{SessionToken, TOKEN_LEN};

pub struct OsTokenGen;

impl TokenGen for OsTokenGen {
    fn generate(&self) -> Result<SessionToken, TokenGenError> {
        let mut bytes = [0_u8; TOKEN_LEN];
        getrandom::fill(&mut bytes).map_err(|error| TokenGenError(error.to_string()))?;
        let token = SessionToken::from_bytes(bytes);
        bytes.zeroize();
        Ok(token)
    }
}

/// Les octets d'un secret de jeu : le hasard du système, sans repli.
pub struct OsSecretRandom;

impl SecretRandom for OsSecretRandom {
    fn fill(&self, buf: &mut [u8]) -> Result<(), RandomError> {
        getrandom::fill(buf).map_err(|error| RandomError(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_fills_a_buffer_and_never_the_same_twice() {
        let (mut a, mut b) = ([0_u8; 32], [0_u8; 32]);
        OsSecretRandom.fill(&mut a).unwrap();
        OsSecretRandom.fill(&mut b).unwrap();
        assert_ne!(a, b);
        assert_ne!(a, [0_u8; 32]);
    }

    #[test]
    fn tokens_are_64_hex_digits_and_never_repeat() {
        let first = OsTokenGen.generate().unwrap().encode();
        let second = OsTokenGen.generate().unwrap().encode();
        assert_eq!(first.len(), 64);
        assert!(first.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_ne!(first, second);
    }
}

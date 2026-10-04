//! Hasard du système d'exploitation, pour les jetons de session.

use zeroize::Zeroize;

use crate::application::ports::{TokenGen, TokenGenError};
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_64_hex_digits_and_never_repeat() {
        let first = OsTokenGen.generate().unwrap().encode();
        let second = OsTokenGen.generate().unwrap().encode();
        assert_eq!(first.len(), 64);
        assert!(first.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_ne!(first, second);
    }
}

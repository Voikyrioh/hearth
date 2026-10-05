//! Validation du port d'écoute choisi à l'installation.

use hearth_proto::product::DEFAULT_PORT;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PortError {
    #[error("Le numéro de port doit être un nombre compris entre 1 et 65535.")]
    Invalid,
}

/// Lit un numéro de port saisi (1 à 65535). Une saisie vide donne le port par défaut (7341).
pub fn parse_port(raw: &str) -> Result<u16, PortError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(DEFAULT_PORT);
    }
    if !raw.bytes().all(|b| b.is_ascii_digit()) {
        return Err(PortError::Invalid);
    }
    match raw.parse::<u32>() {
        Ok(port @ 1..=65535) => u16::try_from(port).map_err(|_| PortError::Invalid),
        _ => Err(PortError::Invalid),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_entry_is_the_default_port() {
        assert_eq!(parse_port(""), Ok(7341));
        assert_eq!(parse_port("  "), Ok(7341));
    }

    #[test]
    fn the_bounds_are_one_and_65535() {
        assert_eq!(parse_port("1"), Ok(1));
        assert_eq!(parse_port("65535"), Ok(65535));
        assert!(parse_port("0").is_err());
        assert!(parse_port("65536").is_err());
        assert!(parse_port("99999999999").is_err());
    }

    #[test]
    fn a_sign_or_a_letter_is_refused() {
        for raw in ["-1", "+80", "80a", "0x50", "7 341"] {
            assert!(parse_port(raw).is_err(), "{raw}");
        }
    }
}

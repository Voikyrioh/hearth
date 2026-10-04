//! Identifiant stable d'une installation de l'agent (BR-INSTALL-004 : généré une seule fois).

use std::fmt;

use thiserror::Error;

const BYTES: usize = 16;

/// 16 octets aléatoires écrits en 32 caractères hexadécimaux minuscules.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InstallId(String);

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("identifiant d'installation invalide : 32 caractères hexadécimaux minuscules attendus")]
pub struct InvalidInstallId;

impl InstallId {
    pub fn from_bytes(bytes: [u8; BYTES]) -> Self {
        Self(bytes.iter().map(|b| format!("{b:02x}")).collect())
    }

    pub fn parse(text: &str) -> Result<Self, InvalidInstallId> {
        let text = text.trim();
        let valid = text.len() == BYTES * 2
            && text
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if valid {
            Ok(Self(text.to_owned()))
        } else {
            Err(InvalidInstallId)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for InstallId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_become_lowercase_hex() {
        let id = InstallId::from_bytes([0xab; BYTES]);
        assert_eq!(id.as_str(), "abababababababababababababababab");
    }

    #[test]
    fn parse_accepts_what_from_bytes_produces() {
        let id = InstallId::from_bytes([7; BYTES]);
        assert_eq!(InstallId::parse(id.as_str()), Ok(id));
    }

    #[test]
    fn parse_rejects_bad_input() {
        assert_eq!(InstallId::parse("abc"), Err(InvalidInstallId));
        assert_eq!(InstallId::parse(&"G".repeat(32)), Err(InvalidInstallId));
        assert_eq!(InstallId::parse(&"A".repeat(32)), Err(InvalidInstallId));
    }
}

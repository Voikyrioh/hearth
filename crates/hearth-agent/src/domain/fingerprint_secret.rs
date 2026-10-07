//! Secret d'installation qui clé l'empreinte des requêtes suivies (HRT-32, ADR-0033).
//!
//! 32 octets aléatoires, créés une fois par installation, gardés dans un fichier à droits
//! restreints du dossier de données, **jamais en base, au journal, dans une erreur ni dans la
//! sauvegarde de la base**. Le type n'a ni `Clone`, ni `Display`, ni `PartialEq` ; son `Debug` est
//! masqué ; la mémoire est effacée à la libération. La lecture passe par `expose`, qui rend
//! l'usage visible en revue.

use std::fmt;

use thiserror::Error;
use zeroize::Zeroize;

/// Taille du secret, en octets.
pub const SECRET_LEN: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("le secret d'empreinte doit faire exactement {SECRET_LEN} octets")]
pub struct WrongSecretLength;

pub struct FingerprintSecret([u8; SECRET_LEN]);

impl FingerprintSecret {
    pub fn from_bytes(bytes: [u8; SECRET_LEN]) -> Self {
        Self(bytes)
    }

    /// Relit un secret stocké : exactement `SECRET_LEN` octets, sinon refus (jamais complété ni
    /// tronqué).
    pub fn from_slice(bytes: &[u8]) -> Result<Self, WrongSecretLength> {
        <[u8; SECRET_LEN]>::try_from(bytes)
            .map(Self)
            .map_err(|_| WrongSecretLength)
    }

    /// Donne accès au secret en clair : pour clé le code d'authentification, nulle part ailleurs.
    pub fn expose(&self) -> &[u8; SECRET_LEN] {
        &self.0
    }
}

impl fmt::Debug for FingerprintSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FingerprintSecret(***)")
    }
}

impl Drop for FingerprintSecret {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_exactly_32_bytes_make_a_secret() {
        for len in [0, 1, 31, 33, 64] {
            assert_eq!(
                FingerprintSecret::from_slice(&vec![1; len]).err(),
                Some(WrongSecretLength),
                "{len}"
            );
        }
        let secret = FingerprintSecret::from_slice(&[7; SECRET_LEN]).unwrap();
        assert_eq!(secret.expose(), &[7; SECRET_LEN]);
    }

    #[test]
    fn debug_never_shows_the_bytes() {
        let secret = FingerprintSecret::from_bytes([0xAB; SECRET_LEN]);
        let shown = format!("{secret:?} {secret:#?}");
        assert!(!shown.contains("171") && !shown.to_lowercase().contains("ab"));
        assert!(shown.contains("***"));
    }
}

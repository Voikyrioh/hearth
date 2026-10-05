//! Épinglage de l'empreinte du serveur (BR-CONN-001, 002, 003, 011).
//!
//! L'empreinte est le SHA-256 du certificat DER ; la comparaison porte sur les 32 octets. Tant
//! que l'empreinte n'est pas connue **et** égale à celle mémorisée, aucun identifiant ni jeton
//! ne part vers le serveur.

use hearth_proto::fingerprint::Fingerprint;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinDecision {
    /// Aucune empreinte mémorisée : première connexion, l'utilisateur doit confirmer.
    FirstConnection,
    /// Identique à l'empreinte mémorisée.
    Match,
    /// Différente : agent réinstallé, machine changée ou interception. Blocage.
    Mismatch { expected: Fingerprint },
}

/// Compare l'empreinte présentée par le serveur à celle mémorisée.
pub fn decide(stored: Option<&Fingerprint>, presented: &Fingerprint) -> PinDecision {
    match stored {
        None => PinDecision::FirstConnection,
        Some(expected) if expected == presented => PinDecision::Match,
        Some(expected) => PinDecision::Mismatch {
            expected: *expected,
        },
    }
}

impl PinDecision {
    /// Une requête authentifiée (identifiant, jeton) peut-elle partir ? Seulement si l'empreinte
    /// présentée est celle que l'utilisateur a confirmée.
    pub fn allows_credentials(&self) -> bool {
        matches!(self, Self::Match)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fp(byte: u8) -> Fingerprint {
        Fingerprint::from_bytes([byte; 32])
    }

    #[test]
    fn no_stored_fingerprint_is_a_first_connection() {
        let decision = decide(None, &fp(1));
        assert_eq!(decision, PinDecision::FirstConnection);
        assert!(!decision.allows_credentials());
    }

    #[test]
    fn the_same_fingerprint_matches() {
        let decision = decide(Some(&fp(1)), &fp(1));
        assert_eq!(decision, PinDecision::Match);
        assert!(decision.allows_credentials());
    }

    #[test]
    fn a_different_fingerprint_blocks_and_sends_nothing() {
        let decision = decide(Some(&fp(1)), &fp(2));
        assert_eq!(decision, PinDecision::Mismatch { expected: fp(1) });
        assert!(!decision.allows_credentials());
    }

    #[test]
    fn a_difference_in_the_last_byte_alone_is_a_mismatch() {
        let mut bytes = [7u8; 32];
        let stored = Fingerprint::from_bytes(bytes);
        bytes[31] = 8;
        let presented = Fingerprint::from_bytes(bytes);
        // Même affichage court (16 premiers octets), mais pas la même empreinte.
        assert_eq!(stored.short(), presented.short());
        assert!(matches!(
            decide(Some(&stored), &presented),
            PinDecision::Mismatch { .. }
        ));
    }
}

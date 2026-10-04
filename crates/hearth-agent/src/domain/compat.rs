//! Compatibilité des versions d'interface (BR-CONN-014).
//!
//! Le client annonce la version d'interface qu'il parle ; l'agent accepte la plage
//! `[min, max]` définie dans `hearth_proto::version`. Hors plage, la décision dit qui doit se
//! mettre à jour : un client plus ancien que le minimum est « trop ancien », un client plus récent
//! que le maximum parle à un agent « trop ancien ».

use hearth_proto::version::{API_MIN_SUPPORTED, API_VERSION};

/// Qui doit se mettre à jour pour que les deux puissent se parler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Incompatibility {
    /// Le client parle une version plus ancienne que celles de l'agent : mettre à jour le client.
    ClientTooOld,
    /// Le client parle une version plus récente que celles de l'agent : mettre à jour l'agent.
    AgentTooOld,
}

/// Version annoncée par le client contre la plage de cet agent.
pub fn check(client_api: u32) -> Result<(), Incompatibility> {
    check_against(client_api, API_MIN_SUPPORTED, API_VERSION)
}

/// Même décision avec une plage explicite (testable sans dépendre de la révision courante).
pub fn check_against(client_api: u32, min: u32, max: u32) -> Result<(), Incompatibility> {
    if client_api < min {
        Err(Incompatibility::ClientTooOld)
    } else if client_api > max {
        Err(Incompatibility::AgentTooOld)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_whole_range_is_accepted_bounds_included() {
        for version in 3..=5 {
            assert_eq!(check_against(version, 3, 5), Ok(()), "{version}");
        }
    }

    #[test]
    fn a_client_below_the_minimum_must_upgrade() {
        assert_eq!(check_against(2, 3, 5), Err(Incompatibility::ClientTooOld));
        assert_eq!(check_against(0, 3, 5), Err(Incompatibility::ClientTooOld));
    }

    #[test]
    fn a_client_above_the_maximum_means_the_agent_must_upgrade() {
        assert_eq!(check_against(6, 3, 5), Err(Incompatibility::AgentTooOld));
        assert_eq!(
            check_against(u32::MAX, 3, 5),
            Err(Incompatibility::AgentTooOld)
        );
    }

    #[test]
    fn the_current_revision_accepts_its_own_version_and_only_the_proto_range() {
        assert_eq!(check(API_VERSION), Ok(()));
        assert_eq!(check(API_MIN_SUPPORTED), Ok(()));
        assert!(check(API_VERSION + 1).is_err());
        assert_eq!(
            check(API_VERSION + 1).is_ok(),
            hearth_proto::version::is_supported(API_VERSION + 1)
        );
    }
}

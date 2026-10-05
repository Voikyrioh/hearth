//! Compatibilité de versions d'interface, côté client (BR-CONN-014).
//!
//! L'agent annonce dans `/hello` la plage `[min, max]` qu'il accepte ; le client parle une seule
//! version. Hors plage, on dit qui doit se mettre à jour.

use hearth_proto::api::hello::ApiRange;
use hearth_proto::version::API_VERSION;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compatibility {
    Compatible,
    /// Le client parle une version plus ancienne que le minimum de l'agent.
    UpdateClient,
    /// Le client parle une version plus récente que le maximum de l'agent.
    UpdateAgent,
}

/// Décision pour la version d'interface de cette révision du client.
pub fn check(range: ApiRange) -> Compatibility {
    check_version(API_VERSION, range)
}

/// Même décision pour une version explicite.
pub fn check_version(client_api: u32, range: ApiRange) -> Compatibility {
    if client_api < range.min {
        Compatibility::UpdateClient
    } else if client_api > range.max {
        Compatibility::UpdateAgent
    } else {
        Compatibility::Compatible
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RANGE: ApiRange = ApiRange { min: 3, max: 5 };

    #[test]
    fn the_whole_range_is_compatible_bounds_included() {
        for version in 3..=5 {
            assert_eq!(check_version(version, RANGE), Compatibility::Compatible);
        }
    }

    #[test]
    fn a_client_below_the_minimum_must_update() {
        assert_eq!(check_version(2, RANGE), Compatibility::UpdateClient);
        assert_eq!(check_version(0, RANGE), Compatibility::UpdateClient);
    }

    #[test]
    fn a_client_above_the_maximum_means_the_agent_must_update() {
        assert_eq!(check_version(6, RANGE), Compatibility::UpdateAgent);
        assert_eq!(check_version(u32::MAX, RANGE), Compatibility::UpdateAgent);
    }

    #[test]
    fn the_current_version_is_compatible_with_the_current_range() {
        let range = ApiRange {
            min: hearth_proto::version::API_MIN_SUPPORTED,
            max: API_VERSION,
        };
        assert_eq!(check(range), Compatibility::Compatible);
    }
}

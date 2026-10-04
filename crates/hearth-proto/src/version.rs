//! Version de l'interface `/api/v{n}` parlée par cette révision du code.

/// Version d'interface émise par un client construit avec cette révision.
pub const API_VERSION: u32 = 1;

/// Plus ancienne version d'interface encore acceptée par l'agent.
pub const API_MIN_SUPPORTED: u32 = 1;

/// Indique si un client parlant `client_api` peut dialoguer avec cet agent.
pub fn is_supported(client_api: u32) -> bool {
    (API_MIN_SUPPORTED..=API_VERSION).contains(&client_api)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_version_is_supported() {
        assert!(is_supported(API_VERSION));
    }

    #[test]
    fn versions_outside_the_range_are_rejected() {
        assert!(!is_supported(API_MIN_SUPPORTED - 1));
        assert!(!is_supported(API_VERSION + 1));
    }
}

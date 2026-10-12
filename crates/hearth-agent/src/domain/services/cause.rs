//! La liste fermée des causes d'une issue (action, déploiement), partagée entre l'agent, le journal
//! et l'interface. Déclarée complète dès la tranche 1 ; la lecture seule n'en produit aucune.

/// Pourquoi une action ou un déploiement s'est terminé comme il l'a fait.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Cause {
    ForcedStop,
    PortInUse,
    StoppedRightAfterStart,
    EngineUnavailable,
    ServiceNotFound,
    Forbidden,
    Busy,
    AlreadyInState,
    RestartedByHost,
    ImageNotFound,
    TagNotFound,
    ImagePrivateOrMissing,
    RegistryUnreachable,
    RegistryRateLimited,
    PlatformMismatch,
    DiskFull,
    NameTaken,
    FolderRefused,
    Cancelled,
    Timeout,
    Interrupted,
    EngineRefused,
}

impl Cause {
    pub const ALL: [Cause; 22] = [
        Cause::ForcedStop,
        Cause::PortInUse,
        Cause::StoppedRightAfterStart,
        Cause::EngineUnavailable,
        Cause::ServiceNotFound,
        Cause::Forbidden,
        Cause::Busy,
        Cause::AlreadyInState,
        Cause::RestartedByHost,
        Cause::ImageNotFound,
        Cause::TagNotFound,
        Cause::ImagePrivateOrMissing,
        Cause::RegistryUnreachable,
        Cause::RegistryRateLimited,
        Cause::PlatformMismatch,
        Cause::DiskFull,
        Cause::NameTaken,
        Cause::FolderRefused,
        Cause::Cancelled,
        Cause::Timeout,
        Cause::Interrupted,
        Cause::EngineRefused,
    ];

    /// Le code qui voyage sur le fil et au journal.
    pub fn code(self) -> &'static str {
        match self {
            Cause::ForcedStop => "forced_stop",
            Cause::PortInUse => "port_in_use",
            Cause::StoppedRightAfterStart => "stopped_right_after_start",
            Cause::EngineUnavailable => "engine_unavailable",
            Cause::ServiceNotFound => "service_not_found",
            Cause::Forbidden => "forbidden",
            Cause::Busy => "busy",
            Cause::AlreadyInState => "already_in_state",
            Cause::RestartedByHost => "restarted_by_host",
            Cause::ImageNotFound => "image_not_found",
            Cause::TagNotFound => "tag_not_found",
            Cause::ImagePrivateOrMissing => "image_private_or_missing",
            Cause::RegistryUnreachable => "registry_unreachable",
            Cause::RegistryRateLimited => "registry_rate_limited",
            Cause::PlatformMismatch => "platform_mismatch",
            Cause::DiskFull => "disk_full",
            Cause::NameTaken => "name_taken",
            Cause::FolderRefused => "folder_refused",
            Cause::Cancelled => "cancelled",
            Cause::Timeout => "timeout",
            Cause::Interrupted => "interrupted",
            Cause::EngineRefused => "engine_refused",
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn the_codes_are_distinct_snake_case_words() {
        let codes: HashSet<_> = Cause::ALL.iter().map(|cause| cause.code()).collect();
        assert_eq!(codes.len(), Cause::ALL.len());
        for code in codes {
            assert!(
                code.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "{code}"
            );
        }
    }
}

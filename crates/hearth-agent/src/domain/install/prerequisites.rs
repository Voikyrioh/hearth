//! Les prérequis d'une installation, contrôlés **avant toute écriture** (BR-INSTALL-001, 006,
//! 012). Le premier qui manque arrête l'installation.

use thiserror::Error;

use super::platform::{SUPPORTED_ARCHITECTURES, parse_arch};

/// Espace libre exigé sur le disque du dossier de données : le binaire (copié deux fois pendant
/// un remplacement atomique), la base et le journal (jusqu'à 50 000 entrées).
pub const MIN_FREE_BYTES: u64 = 256 * 1024 * 1024;

/// Ce que l'adaptateur a observé.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prerequisites {
    /// L'utilisateur a les droits d'administration de la machine (BR-INSTALL-001).
    pub privileged: bool,
    /// Système d'exploitation (`linux`) et architecture (`x86_64`…).
    pub os: String,
    pub arch: String,
    pub port: u16,
    /// Un autre processus que l'agent écoute déjà sur ce port.
    pub port_taken: bool,
    /// Espace libre sur le disque du dossier de données.
    pub free_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Blocker {
    #[error(
        "Droits d'administration requis. Relance cette commande avec les droits d'administration."
    )]
    NotPrivileged,
    #[error("L'installation est prise en charge sous Linux seulement.")]
    UnsupportedOs { os: String },
    #[error(
        "Cette architecture n'est pas prise en charge. Architectures supportées : {SUPPORTED_ARCHITECTURES}."
    )]
    UnsupportedArch { arch: String },
    #[error("Le port configuré est déjà utilisé. Relance en choisissant un autre port.")]
    PortTaken { port: u16 },
    #[error("Espace disque insuffisant : {free_mib} Mio libres, {needed_mib} Mio nécessaires.")]
    NotEnoughSpace { free_mib: u64, needed_mib: u64 },
}

/// BR-INSTALL-001 : seuls les utilisateurs qui ont les droits d'administration de la machine
/// installent. Contrôlé seul d'abord : sans ces droits, rien d'autre n'est observé.
pub fn check_rights(privileged: bool) -> Result<(), Blocker> {
    if privileged {
        Ok(())
    } else {
        Err(Blocker::NotPrivileged)
    }
}

/// Contrôle les prérequis dans l'ordre : droits, système, architecture, port, espace disque.
pub fn check_prerequisites(found: &Prerequisites) -> Result<(), Blocker> {
    check_rights(found.privileged)?;
    if found.os != "linux" {
        return Err(Blocker::UnsupportedOs {
            os: found.os.clone(),
        });
    }
    if parse_arch(&found.arch).is_none() {
        return Err(Blocker::UnsupportedArch {
            arch: found.arch.clone(),
        });
    }
    if found.port_taken {
        return Err(Blocker::PortTaken { port: found.port });
    }
    if found.free_bytes < MIN_FREE_BYTES {
        return Err(Blocker::NotEnoughSpace {
            free_mib: found.free_bytes / (1024 * 1024),
            needed_mib: MIN_FREE_BYTES / (1024 * 1024),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok() -> Prerequisites {
        Prerequisites {
            privileged: true,
            os: "linux".into(),
            arch: "x86_64".into(),
            port: 7341,
            port_taken: false,
            free_bytes: 10 * MIN_FREE_BYTES,
        }
    }

    #[test]
    fn everything_in_order_passes() {
        assert_eq!(check_prerequisites(&ok()), Ok(()));
        let arm = Prerequisites {
            arch: "aarch64".into(),
            ..ok()
        };
        assert_eq!(check_prerequisites(&arm), Ok(()));
    }

    #[test]
    fn without_administration_rights_nothing_else_is_looked_at() {
        let found = Prerequisites {
            privileged: false,
            port_taken: true,
            arch: "mips".into(),
            ..ok()
        };
        assert_eq!(check_prerequisites(&found), Err(Blocker::NotPrivileged));
    }

    #[test]
    fn the_rights_are_checked_on_their_own() {
        assert_eq!(check_rights(true), Ok(()));
        assert_eq!(check_rights(false), Err(Blocker::NotPrivileged));
    }

    #[test]
    fn an_unsupported_architecture_is_refused_with_the_supported_list() {
        let found = Prerequisites {
            arch: "riscv64".into(),
            ..ok()
        };
        let error = check_prerequisites(&found).unwrap_err();
        assert_eq!(
            error.to_string(),
            "Cette architecture n'est pas prise en charge. Architectures supportées : x86_64, arm64."
        );
    }

    #[test]
    fn another_operating_system_is_refused() {
        let found = Prerequisites {
            os: "windows".into(),
            ..ok()
        };
        assert_eq!(
            check_prerequisites(&found),
            Err(Blocker::UnsupportedOs {
                os: "windows".into()
            })
        );
    }

    #[test]
    fn a_taken_port_is_refused_with_the_specified_message() {
        let found = Prerequisites {
            port_taken: true,
            ..ok()
        };
        let error = check_prerequisites(&found).unwrap_err();
        assert_eq!(error, Blocker::PortTaken { port: 7341 });
        assert_eq!(
            error.to_string(),
            "Le port configuré est déjà utilisé. Relance en choisissant un autre port."
        );
    }

    #[test]
    fn too_little_free_space_is_refused_just_below_the_threshold() {
        let found = Prerequisites {
            free_bytes: MIN_FREE_BYTES - 1,
            ..ok()
        };
        assert!(matches!(
            check_prerequisites(&found),
            Err(Blocker::NotEnoughSpace { .. })
        ));
        let found = Prerequisites {
            free_bytes: MIN_FREE_BYTES,
            ..ok()
        };
        assert_eq!(check_prerequisites(&found), Ok(()));
    }

    #[test]
    fn the_checks_come_in_the_order_rights_system_architecture_port_space() {
        let mut found = Prerequisites {
            privileged: true,
            os: "freebsd".into(),
            arch: "mips".into(),
            port: 80,
            port_taken: true,
            free_bytes: 0,
        };
        assert!(matches!(
            check_prerequisites(&found),
            Err(Blocker::UnsupportedOs { .. })
        ));
        found.os = "linux".into();
        assert!(matches!(
            check_prerequisites(&found),
            Err(Blocker::UnsupportedArch { .. })
        ));
        found.arch = "x86_64".into();
        assert!(matches!(
            check_prerequisites(&found),
            Err(Blocker::PortTaken { .. })
        ));
        found.port_taken = false;
        assert!(matches!(
            check_prerequisites(&found),
            Err(Blocker::NotEnoughSpace { .. })
        ));
    }
}

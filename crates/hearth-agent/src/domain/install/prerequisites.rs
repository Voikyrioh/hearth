//! Les prérequis d'une installation, contrôlés **avant toute écriture** (BR-INSTALL-001, 006,
//! 012). Le premier qui manque arrête l'installation.

use thiserror::Error;

use super::platform::{SUPPORTED_ARCHITECTURES, parse_arch};

/// Espace libre exigé sur le disque du dossier de données : le binaire (copié deux fois pendant
/// un remplacement atomique), la base et le journal (jusqu'à 50 000 entrées).
pub const MIN_FREE_BYTES: u64 = 256 * 1024 * 1024;

/// Le dossier de données tel qu'il est sur le disque.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataDirState {
    pub exists: bool,
    /// C'est un dossier (un fichier à cet endroit est refusé : sans cela, il serait dit « ouvert
    /// à d'autres utilisateurs », ce qui est trompeur).
    pub directory: bool,
    /// Appartient à root (sans objet s'il n'existe pas).
    pub owned_by_root: bool,
    /// Fermé aux autres utilisateurs (0700, sans objet s'il n'existe pas).
    pub private: bool,
}

impl DataDirState {
    pub const ABSENT: Self = Self {
        exists: false,
        directory: true,
        owned_by_root: true,
        private: true,
    };
}

/// Pourquoi un chemin de l'installation est refusé : il finit dans une unité systemd et dans des
/// suppressions faites en root, il doit être sans surprise.
pub fn unsafe_path_reason(path: &str) -> Option<&'static str> {
    if !path.starts_with('/') {
        return Some("le chemin doit être absolu");
    }
    if path.split('/').any(|part| part == "..") {
        return Some("le chemin ne doit pas contenir « .. »");
    }
    if path.len() > 200
        || !path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/_.-+@:".contains(c))
    {
        return Some("seuls les lettres, chiffres et / _ . - + @ : sont acceptés");
    }
    None
}

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
    pub data_dir: String,
    pub config: String,
    pub data_dir_state: DataDirState,
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
        "Cette architecture n'est pas prise en charge. Architecture supportée : {SUPPORTED_ARCHITECTURES} (arm64 viendra plus tard)."
    )]
    UnsupportedArch { arch: String },
    #[error("Le {what} ({path}) n'est pas utilisable : {reason}. Rien n'a été modifié.")]
    BadPath {
        what: &'static str,
        path: String,
        reason: &'static str,
    },
    #[error(
        "Le chemin du dossier de données existe mais n'est pas un dossier. Retire ce fichier ou choisis un autre dossier. Rien n'a été modifié."
    )]
    DataDirNotDirectory,
    #[error(
        "Le dossier de données existe mais n'appartient pas à root. Corrige-le (chown root) ou choisis un autre dossier. Rien n'a été modifié."
    )]
    DataDirNotRoot,
    #[error(
        "Le dossier de données est ouvert à d'autres utilisateurs. Corrige-le (chmod 700) ou choisis un autre dossier. Rien n'a été modifié."
    )]
    DataDirOpen,
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
    for (what, path) in [
        ("dossier de données", &found.data_dir),
        ("fichier de configuration", &found.config),
    ] {
        if let Some(reason) = unsafe_path_reason(path) {
            return Err(Blocker::BadPath {
                what,
                path: path.clone(),
                reason,
            });
        }
    }
    if found.data_dir_state.exists {
        // FIX:01M460GA87EM6ZF9M5R9CWVXW3 : un fichier à cet endroit n'est pas « ouvert aux autres ».
        if !found.data_dir_state.directory {
            return Err(Blocker::DataDirNotDirectory);
        }
        if !found.data_dir_state.owned_by_root {
            return Err(Blocker::DataDirNotRoot);
        }
        if !found.data_dir_state.private {
            return Err(Blocker::DataDirOpen);
        }
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
            data_dir: "/var/lib/hearth".into(),
            config: "/etc/hearth/agent.toml".into(),
            data_dir_state: DataDirState::ABSENT,
        }
    }

    #[test]
    fn a_data_dir_or_config_path_must_be_absolute_plain_and_without_dots() {
        for bad in [
            "relative/dir",
            "/var/lib/hearth dir",
            "/var/lib/../etc",
            "/srv/h;rm",
            "",
            "/a
b",
        ] {
            let found = Prerequisites {
                data_dir: bad.into(),
                ..ok()
            };
            assert!(
                matches!(check_prerequisites(&found), Err(Blocker::BadPath { .. })),
                "{bad:?}"
            );
        }
        let found = Prerequisites {
            config: "agent.toml".into(),
            ..ok()
        };
        assert!(matches!(
            check_prerequisites(&found),
            Err(Blocker::BadPath {
                what: "fichier de configuration",
                ..
            })
        ));
    }

    #[test]
    fn an_existing_data_dir_must_belong_to_root_and_be_private() {
        let found = Prerequisites {
            data_dir_state: DataDirState {
                exists: true,
                directory: true,
                owned_by_root: false,
                private: true,
            },
            ..ok()
        };
        assert_eq!(check_prerequisites(&found), Err(Blocker::DataDirNotRoot));
        let found = Prerequisites {
            data_dir_state: DataDirState {
                exists: true,
                directory: true,
                owned_by_root: true,
                private: false,
            },
            ..ok()
        };
        assert_eq!(check_prerequisites(&found), Err(Blocker::DataDirOpen));
        let found = Prerequisites {
            data_dir_state: DataDirState {
                exists: true,
                directory: true,
                owned_by_root: true,
                private: true,
            },
            ..ok()
        };
        assert_eq!(check_prerequisites(&found), Ok(()));
        // Un fichier à la place du dossier : dit tel quel, pas « ouvert à d'autres utilisateurs ».
        let found = Prerequisites {
            data_dir_state: DataDirState {
                exists: true,
                directory: false,
                owned_by_root: true,
                private: false,
            },
            ..ok()
        };
        assert_eq!(
            check_prerequisites(&found),
            Err(Blocker::DataDirNotDirectory)
        );
    }

    #[test]
    fn everything_in_order_passes() {
        assert_eq!(check_prerequisites(&ok()), Ok(()));
        let arm = Prerequisites {
            arch: "aarch64".into(),
            ..ok()
        };
        assert!(matches!(
            check_prerequisites(&arm),
            Err(Blocker::UnsupportedArch { .. })
        ));
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
            "Cette architecture n'est pas prise en charge. Architecture supportée : x86_64 (arm64 viendra plus tard)."
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
            data_dir: "/var/lib/hearth".into(),
            config: "/etc/hearth/agent.toml".into(),
            data_dir_state: DataDirState::ABSENT,
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

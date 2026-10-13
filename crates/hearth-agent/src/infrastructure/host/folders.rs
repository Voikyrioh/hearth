//! `HostFolders` sur le vrai système de fichiers (HRT-75, BR-DEPLOY-012).
//!
//! Lecture seule : `readlink` d'un chemin, et le fichier des comptes relu à chaque demande. Le
//! comportement sur un vrai système Linux (liens, `/etc/passwd`) est prouvé par la CI Linux
//! (`tests/host_folders_linux.rs`) ; le poste de développement Windows ne l'exécute pas.

use std::io::ErrorKind;
use std::path::Path;

use crate::application::ports::HostFolders;
use crate::domain::services::{Probe, home_folders};

/// Le fichier des comptes du serveur.
pub const PASSWD_PATH: &str = "/etc/passwd";

#[derive(Debug, Clone)]
pub struct SystemFolders {
    passwd: String,
}

impl SystemFolders {
    pub fn new() -> Self {
        Self {
            passwd: PASSWD_PATH.to_owned(),
        }
    }

    /// Un autre fichier de comptes (tests).
    pub fn with_passwd(passwd: impl Into<String>) -> Self {
        Self {
            passwd: passwd.into(),
        }
    }
}

impl Default for SystemFolders {
    fn default() -> Self {
        Self::new()
    }
}

impl HostFolders for SystemFolders {
    fn probe(&self, path: &str) -> Probe {
        match std::fs::read_link(Path::new(path)) {
            Ok(target) => match target.to_str() {
                Some(text) => Probe::Link(text.to_owned()),
                // une cible qui n'est pas du texte valide ne peut pas être jugée
                None => Probe::Unreadable,
            },
            // pas un lien (EINVAL), n'existe pas, ou un composant parent n'est pas un dossier
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::NotFound | ErrorKind::InvalidInput | ErrorKind::NotADirectory
                ) =>
            {
                Probe::Plain
            }
            Err(_) => Probe::Unreadable,
        }
    }

    fn account_homes(&self) -> Option<Vec<String>> {
        std::fs::read_to_string(&self.passwd)
            .ok()
            .map(|text| home_folders(&text))
    }
}

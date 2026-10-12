//! « Un moteur est-il installé sur ce serveur ? » : ce qui sépare « Moteur arrêté » d'« Aucun moteur
//! de conteneurs détecté » quand le socket n'existe pas (`BR-ENGINE-002`, `BR-ENGINE-005`,
//! `BR-ENGINE-007`).
//!
//! On cherche le binaire `dockerd` ou une unité `docker.service` / `docker.socket` aux emplacements
//! des distributions courantes (Debian, Fedora, Arch, NixOS). Lecture de métadonnées seulement :
//! rien n'est exécuté, rien n'est ouvert.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct InstallMarkers {
    paths: Vec<PathBuf>,
}

impl InstallMarkers {
    /// Les emplacements d'un serveur Linux.
    pub fn linux() -> Self {
        let paths = [
            "/usr/bin/dockerd",
            "/usr/sbin/dockerd",
            "/usr/local/bin/dockerd",
            "/run/current-system/sw/bin/dockerd",
            "/usr/lib/systemd/system/docker.service",
            "/usr/lib/systemd/system/docker.socket",
            "/lib/systemd/system/docker.service",
            "/lib/systemd/system/docker.socket",
            "/etc/systemd/system/docker.service",
            "/etc/systemd/system/docker.socket",
            "/run/systemd/system/docker.service",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect();
        Self { paths }
    }

    /// Ces emplacements-là (tests).
    pub fn at(paths: impl IntoIterator<Item = impl AsRef<Path>>) -> Self {
        Self {
            paths: paths
                .into_iter()
                .map(|path| path.as_ref().to_path_buf())
                .collect(),
        }
    }

    /// Au moins un repère existe. `symlink_metadata` : sur NixOS les unités sont des liens.
    pub fn any_present(&self) -> bool {
        self.paths
            .iter()
            .any(|path| std::fs::symlink_metadata(path).is_ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_listed_means_no_engine() {
        assert!(!InstallMarkers::at(Vec::<PathBuf>::new()).any_present());
        assert!(!InstallMarkers::at(["/nonexistent/dockerd-xyz"]).any_present());
    }

    #[test]
    fn an_existing_marker_means_an_engine_is_installed() {
        let here = std::env::current_exe().expect("exe");
        assert!(InstallMarkers::at(["/nonexistent/dockerd-xyz".into(), here]).any_present());
    }
}

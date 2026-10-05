use std::any::Any;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;

use hearth_proto::fingerprint::Fingerprint;
use thiserror::Error;

use crate::domain::install::{BinaryState, DataDirState, DataState};

/// Les endroits fixes de l'installation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallPaths {
    /// `/usr/local/bin/hearth-agent`
    pub binary: PathBuf,
    /// `/etc/hearth/agent.toml`
    pub config: PathBuf,
    /// `/var/lib/hearth` : base, certificat, clé.
    pub data_dir: PathBuf,
    /// Verrou d'une installation en cours (`/run/hearth-agent-install.lock`).
    pub lock: PathBuf,
}

impl InstallPaths {
    pub fn system(config: Option<PathBuf>, data_dir: PathBuf) -> Self {
        Self {
            binary: PathBuf::from("/usr/local/bin/hearth-agent"),
            config: config.unwrap_or_else(|| PathBuf::from("/etc/hearth/agent.toml")),
            data_dir,
            lock: PathBuf::from("/run/hearth-agent-install.lock"),
        }
    }

    /// Le fichier de sauvegarde du binaire remplacé.
    pub fn backup(&self) -> PathBuf {
        Self::backup_of(&self.binary)
    }

    /// Le fichier de sauvegarde de ce binaire (l'installation comme la mise à jour s'en servent).
    pub fn backup_of(binary: &Path) -> PathBuf {
        binary.with_file_name(".hearth-agent.previous")
    }
}

/// Ce que contient le fichier de configuration de l'installation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigSpec {
    pub port: u16,
    pub managed: bool,
    /// Seulement s'il diffère du dossier par défaut.
    pub data_dir: Option<PathBuf>,
}

/// Ce que l'adaptateur a relevé sur le disque.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostFacts {
    pub binary: BinaryState,
    pub data: DataState,
    pub config_exists: bool,
    /// Le port écrit dans la configuration existante.
    pub configured_port: Option<u16>,
}

/// Le binaire copié : de quoi le rétablir.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryInstalled {
    /// L'ancien binaire, gardé de côté s'il y en avait un.
    pub backup: Option<PathBuf>,
}

/// Ce que répond l'agent qui vient de démarrer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answered {
    /// L'empreinte du certificat que l'agent a **réellement servi**.
    pub served: Fingerprint,
}

/// Une installation en cours : tant que cet objet existe, une autre installation est refusée.
pub struct InstallLock(#[allow(dead_code)] pub Box<dyn Any + Send>);

#[derive(Debug, Error)]
pub enum HostError {
    #[error("{action} : {path} : {source}")]
    Io {
        action: &'static str,
        path: String,
        source: std::io::Error,
    },
    /// Une autre installation tient déjà le verrou.
    #[error("une autre installation est déjà en cours")]
    AlreadyRunning,
    #[error("{0}")]
    Other(String),
}

/// La machine vue par l'installation : ce qu'elle observe, les fichiers qu'elle écrit. Chemins
/// toujours fournis par l'appelant (jamais construits ici). Aucun interpréteur de commandes.
pub trait InstallHost: Send + Sync {
    /// Système d'exploitation (`linux`) et architecture (`x86_64`, `aarch64`) de la machine.
    fn os(&self) -> String;
    fn arch(&self) -> String;

    /// L'utilisateur a les droits d'administration (BR-INSTALL-001).
    fn is_privileged(&self) -> bool;

    /// Un processus écoute déjà sur ce port.
    fn port_taken(&self, addr: IpAddr, port: u16) -> bool;

    /// Octets libres sur le disque qui porte (ou porterait) ce chemin.
    fn free_bytes(&self, path: &Path) -> Result<u64, HostError>;

    /// Nom de la machine, pour l'adresse à saisir dans le client.
    fn hostname(&self) -> String;

    /// Relève ce qui est installé. `source` est le binaire qu'on s'apprête à installer (pour dire
    /// si l'installé est identique). Aucune écriture.
    fn inspect(&self, paths: &InstallPaths, source: &Path) -> Result<HostFacts, HostError>;

    /// Prend le verrou d'installation, ou `AlreadyRunning`.
    fn lock(&self, path: &Path) -> Result<InstallLock, HostError>;

    /// Copie `source` vers `dest` en écriture atomique (fichier voisin puis renommage), droits
    /// 0755 ; l'ancien binaire est gardé en `backup`.
    fn install_binary(
        &self,
        source: &Path,
        dest: &Path,
        backup: &Path,
    ) -> Result<BinaryInstalled, HostError>;

    /// Rétablit l'ancien binaire (ou supprime `dest` s'il n'y en avait pas).
    fn restore_binary(&self, dest: &Path, installed: &BinaryInstalled) -> Result<(), HostError>;

    /// Oublie la sauvegarde de l'ancien binaire, une fois l'installation réussie.
    fn discard_backup(&self, installed: &BinaryInstalled);

    /// Crée le dossier de données (0700) s'il manque ; rend `true` s'il l'a créé.
    fn ensure_data_dir(&self, dir: &Path) -> Result<bool, HostError>;

    /// Écrit la configuration **si elle n'existe pas** (jamais d'écrasement) ; rend `true` si elle
    /// l'a écrite.
    fn write_config_new(&self, path: &Path, spec: &ConfigSpec) -> Result<bool, HostError>;

    /// Supprime un fichier (sans erreur s'il n'existe pas).
    fn remove_file(&self, path: &Path) -> Result<(), HostError>;

    /// Les noms de ce que contient un dossier (vide s'il n'existe pas).
    fn list_dir(&self, path: &Path) -> Result<Vec<String>, HostError>;

    /// Le dossier de données tel qu'il est : existe, propriétaire, droits. Aucune écriture.
    fn data_dir_state(&self, path: &Path) -> DataDirState;

    /// Supprime un dossier seulement s'il est vide (sans erreur sinon). **Jamais de suppression
    /// récursive** : un chemin qui vient de la configuration n'est pas digne d'un `rm -r` en root.
    fn remove_dir_if_empty(&self, path: &Path) -> Result<(), HostError>;

    /// Attend que l'agent réponde sur `GET /api/v1/hello` en HTTPS, et rend l'empreinte du
    /// certificat servi. `cancelled` est consulté pendant l'attente.
    fn wait_for_hello(
        &self,
        addr: SocketAddr,
        timeout: Duration,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Answered, HostError>;
}

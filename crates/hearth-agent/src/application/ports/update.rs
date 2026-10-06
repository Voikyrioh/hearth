//! Ports de la mise à jour de l'agent (HRT-17) : le téléchargement, la signature, la machine
//! (fichiers de `update/`, lancement du superviseur), le contrôle de l'agent et la diffusion de la
//! progression. Aucun interpréteur de commandes derrière : des chemins et des listes d'arguments.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use hearth_proto::api::update::UpdateProgress;
use hearth_proto::fingerprint::Fingerprint;
use thiserror::Error;
use tokio::sync::broadcast;

use crate::domain::install::Version;
use crate::domain::update::{Job, SupervisorState, UpdateRecord};

#[derive(Debug, Error)]
pub enum FetchError {
    /// L'adresse n'est pas joignable (résolution, connexion, TLS) : le serveur n'a pas accès à
    /// Internet, ou pas à cette adresse (BR-UPDATE-019).
    #[error("adresse injoignable : {0}")]
    Unreachable(String),
    /// Statut d'erreur, coupure en cours de route, redirection hors HTTPS.
    #[error("téléchargement impossible : {0}")]
    Failed(String),
    #[error("le fichier dépasse la taille permise")]
    TooLarge,
}

/// Télécharge un fichier **en mémoire** (HTTPS seulement). Rien n'est écrit sur le disque : la
/// signature et la somme sont vérifiées avant (BR-UPDATE-013).
#[async_trait]
pub trait Downloader: Send + Sync {
    /// `progress(reçu, total)` est appelé à chaque morceau ; `total` vient de `Content-Length`.
    async fn fetch(
        &self,
        url: &str,
        max_bytes: u64,
        progress: &(dyn Fn(u64, Option<u64>) + Send + Sync),
    ) -> Result<Vec<u8>, FetchError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SignatureError {
    /// La signature n'est pas du minisign lisible, ou n'est pas de la clé de l'agent.
    #[error("signature illisible ou d'une autre clé")]
    Malformed,
    /// La signature est lisible mais ne correspond pas au contenu.
    #[error("signature fausse")]
    Wrong,
}

/// Vérifie une signature minisign avec la clé publique **embarquée** dans l'agent.
pub trait SignatureVerifier: Send + Sync {
    /// Lisible, et faite par la clé embarquée (identifiant de clé) : sans le contenu, donc avant
    /// tout téléchargement.
    fn check_format(&self, signature: &str) -> Result<(), SignatureError>;

    /// La signature est celle de ces octets.
    fn verify(&self, data: &[u8], signature: &str) -> Result<(), SignatureError>;
}

#[derive(Debug, Error)]
pub enum UpdateHostError {
    #[error("{action} : {path} : {source}")]
    Io {
        action: &'static str,
        path: String,
        source: std::io::Error,
    },
    #[error("{0}")]
    Other(String),
    /// Un superviseur travaille déjà.
    #[error("un superviseur de mise à jour travaille déjà")]
    AlreadyRunning,
}

/// Le superviseur en cours : tant que cet objet existe, le verrou est tenu.
pub struct SupervisorLock(#[allow(dead_code)] pub Box<dyn std::any::Any + Send>);

/// L'espace libre sous un chemin : le port que le superviseur interroge avant de copier la base
/// (jamais de vrai disque dans ses tests unitaires).
pub trait FreeSpace: Send + Sync {
    /// Octets disponibles sous `path` (ou son plus proche ancêtre existant) ; l'échec de la
    /// mesure est une erreur, jamais « beaucoup de place ».
    fn free_bytes(&self, path: &Path) -> Result<u64, UpdateHostError>;
}

/// La machine vue par la mise à jour : les fichiers de `update/` dans le dossier de données, et
/// le lancement du superviseur détaché.
pub trait UpdateHost: Send + Sync {
    /// Un superviseur travaille (verrou tenu).
    fn supervisor_running(&self) -> bool;

    /// Prend le verrou du superviseur, ou `AlreadyRunning`.
    fn take_supervisor_lock(&self) -> Result<SupervisorLock, UpdateHostError>;

    fn read_last(&self) -> Result<Option<UpdateRecord>, UpdateHostError>;
    fn write_last(&self, record: &UpdateRecord) -> Result<(), UpdateHostError>;

    fn read_job(&self) -> Result<Option<Job>, UpdateHostError>;

    /// Ce chemin existe-t-il (la sauvegarde de l'ancien binaire, par exemple) ?
    fn path_exists(&self, path: &Path) -> bool;

    /// Copie la base (service arrêté) avant l'échange des binaires : fichier voisin puis
    /// renommage, `hearth.db` et son journal. Un retour arrière ne laisse jamais un ancien binaire
    /// devant une base déjà migrée (BR-UPDATE-029).
    fn backup_database(&self) -> Result<(), UpdateHostError>;

    /// Taille de la base et de son journal : pour contrôler l'espace avant la copie.
    fn database_size(&self) -> u64;

    /// Le dossier de données (pour mesurer l'espace libre).
    fn data_dir(&self) -> PathBuf;

    /// Une copie de la base attend d'être remise.
    fn database_copy_present(&self) -> bool;

    /// Remet la base telle qu'elle était avant l'échange (service arrêté) : copie dans un fichier
    /// voisin puis renommage, **puis la copie est retirée** : elle ne sert qu'une fois.
    fn restore_database(&self) -> Result<(), UpdateHostError>;

    /// Retire un fichier (la sauvegarde de l'ancien binaire d'un retour arrière déjà fait).
    fn remove_path(&self, path: &Path) -> Result<(), UpdateHostError>;

    /// Retire `state.json` et `job.json` (illisibles), sans toucher aux copies (binaire, base).
    fn discard_work_files(&self);

    fn read_state(&self) -> Result<Option<SupervisorState>, UpdateHostError>;
    fn write_state(&self, state: &SupervisorState) -> Result<(), UpdateHostError>;

    /// Dépose le binaire vérifié dans `update/` (écriture atomique, droits 0700).
    fn stage(&self, bytes: &[u8]) -> Result<PathBuf, UpdateHostError>;

    /// La version que ce binaire annonce (`--version`, délai court) : il doit s'exécuter sur cette
    /// machine et être celui qu'on croit.
    fn staged_version(&self, path: &Path) -> Result<Version, UpdateHostError>;

    /// Copie le binaire en cours d'exécution dans `update/` : l'ancien binaire joue le superviseur
    /// pendant que le nouveau le remplace.
    fn prepare_supervisor(&self) -> Result<PathBuf, UpdateHostError>;

    /// Écrit le travail du superviseur et rend son chemin.
    fn write_job(&self, job: &Job) -> Result<PathBuf, UpdateHostError>;

    /// Lance `supervisor update-supervise --job <job>` **détaché** du service (hors de son groupe
    /// de contrôle : l'arrêt du service ne le tue pas).
    fn launch(&self, supervisor: &Path, job: &Path) -> Result<(), UpdateHostError>;

    /// Retire ce que la mise à jour a déposé (binaire, copie, travail, étape, copie de la base),
    /// jamais le résultat.
    fn clear_staging(&self);
}

/// Ce que répond un agent à `GET /hello`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Greeting {
    pub version: Version,
    pub fingerprint: Fingerprint,
}

/// Interroge l'agent qui tourne (le contrôle du superviseur).
pub trait HelloProbe: Send + Sync {
    fn hello(&self, addr: SocketAddr, timeout: Duration) -> Result<Greeting, String>;
}

/// Diffusion interne de la progression, à destination du flux temps réel (sujet `update`).
pub trait UpdateFeed: Send + Sync {
    fn publish(&self, progress: UpdateProgress);
    fn subscribe(&self) -> broadcast::Receiver<UpdateProgress>;
}

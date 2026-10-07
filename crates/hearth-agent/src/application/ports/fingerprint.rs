//! Empreinte à clé des requêtes suivies (HRT-32, ADR-0033) : deux ports. Le premier calcule
//! l'empreinte avec le secret de l'installation, que seul l'adaptateur connaît ; le second charge
//! ce secret, ou le crée au premier démarrage.

use thiserror::Error;

use crate::domain::fingerprint_secret::FingerprintSecret;
use crate::domain::operations::RequestFingerprint;

/// Calcule l'empreinte d'une requête suivie. Sans le secret, elle ne se calcule pas : le domaine ne
/// sait que comparer des empreintes déjà calculées.
pub trait RequestFingerprinter: Send + Sync {
    fn fingerprint(&self, method: &str, path: &str, body: &[u8]) -> RequestFingerprint;
}

/// Les messages nomment un chemin, une taille ou des droits : jamais un octet du secret.
#[derive(Debug, Error)]
pub enum FingerprintSecretError {
    /// Le fichier existe mais n'est pas utilisable : on ne le remplace jamais en silence (les
    /// opérations en cours perdraient leur garde de rejeu sans que l'opérateur le sache).
    #[error(
        "le fichier {path} du secret d'empreinte fait {len} octets au lieu de {expected} : corrige-le ou retire-le (les rejeux en cours ne seront plus reconnus), puis relance l'agent"
    )]
    WrongSize {
        path: String,
        len: u64,
        expected: usize,
    },
    /// Droits au-delà de 0600 : le secret a pu être lu par d'autres.
    #[error(
        "le fichier {path} du secret d'empreinte est ouvert aux autres utilisateurs (droits {mode:o}) ; corrige-le avec `chmod 600 {path}` ou retire-le pour qu'un nouveau soit créé"
    )]
    TooOpen { path: String, mode: u32 },
    #[error("génération du secret d'empreinte impossible : {0}")]
    Generation(String),
    #[error("accès au secret d'empreinte {path} impossible : {source}")]
    Storage {
        path: String,
        source: std::io::Error,
    },
}

/// Conservation du secret d'empreinte.
pub trait FingerprintSecretStore {
    /// Le secret existant ; à défaut (fichier absent), un secret neuf créé de façon atomique. Deux
    /// démarrages simultanés rendent le même secret.
    fn load_or_create(&self) -> Result<FingerprintSecret, FingerprintSecretError>;
}

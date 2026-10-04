use std::fmt;

use thiserror::Error;

use crate::domain::fingerprint::Fingerprint;
use crate::domain::install_id::InstallId;

/// Identité cryptographique de l'installation : certificat, clé privée et identifiant.
#[derive(Clone)]
pub struct Identity {
    /// Certificat auto-signé, encodé en DER.
    pub certificate_der: Vec<u8>,
    /// Clé privée PKCS#8, encodée en DER. Ne doit jamais être journalisée.
    pub private_key_der: Vec<u8>,
    pub install_id: InstallId,
    pub fingerprint: Fingerprint,
}

// Écrit à la main pour ne jamais exposer la clé privée.
impl fmt::Debug for Identity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Identity")
            .field("install_id", &self.install_id)
            .field("fingerprint", &self.fingerprint)
            .field("private_key_der", &"<masquée>")
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Error)]
pub enum IdentityError {
    /// Une partie de l'identité existe sans l'autre : on ne régénère jamais en silence,
    /// car l'empreinte ne doit pas changer (BR-INSTALL-004).
    #[error("identité incomplète dans {0} : fichier manquant {1}")]
    Incomplete(String, String),
    #[error("identité illisible : {0}")]
    Corrupt(String),
    #[error("génération du certificat impossible : {0}")]
    Generation(String),
    #[error("accès au stockage de l'identité impossible : {0}")]
    Storage(#[from] std::io::Error),
}

/// Stockage durable de l'identité de l'agent.
pub trait IdentityStore {
    /// Charge l'identité existante ou la crée à la première exécution.
    /// Un second appel renvoie toujours la même identité.
    fn load_or_create(&self) -> Result<Identity, IdentityError>;
}

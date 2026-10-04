use hearth_proto::fingerprint::Fingerprint;
use thiserror::Error;

use crate::domain::identity_policy::IdentityPart;
use crate::domain::install_id::InstallId;

/// Partie publique de l'identité : ce que le reste de l'agent a le droit de connaître.
/// La clé privée n'en fait pas partie : seul l'adaptateur TLS la lit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicIdentity {
    pub install_id: InstallId,
    pub fingerprint: Fingerprint,
}

#[derive(Debug, Error)]
pub enum IdentityError {
    /// Une partie de l'identité manque alors que le certificat existe : on ne régénère jamais
    /// en silence, car l'empreinte ne doit pas changer (BR-INSTALL-004).
    #[error("identité incomplète dans {dir} : éléments manquants {missing:?}")]
    Incomplete {
        dir: String,
        missing: Vec<IdentityPart>,
    },
    #[error("identité illisible : {0}")]
    Corrupt(String),
    #[error("génération du certificat impossible : {0}")]
    Generation(String),
    /// Un autre processus est en train de créer l'identité et n'a pas fini à temps.
    #[error("identité occupée par un autre processus")]
    LockTimeout,
    #[error("accès à {path} impossible : {source}")]
    Storage {
        path: String,
        source: std::io::Error,
    },
}

/// Conservation de l'identité de l'agent.
pub trait IdentityStore {
    /// Charge l'identité existante ou la crée à la première exécution (selon
    /// `domain::identity_policy`). Plusieurs appels, même simultanés depuis plusieurs
    /// processus, renvoient la même identité.
    fn load_or_create(&self) -> Result<PublicIdentity, IdentityError>;
}

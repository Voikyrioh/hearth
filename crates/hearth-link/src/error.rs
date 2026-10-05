//! Erreurs publiques de la bibliothèque. Aucune ne contient de secret.

use hearth_proto::error::ErrorCode;
use thiserror::Error;

use crate::domain::compat::Compatibility;
use crate::ports::transport::{ApiError, TransportError};

#[derive(Debug, Clone, PartialEq, Error)]
pub enum LinkError {
    #[error("serveur inconnu")]
    UnknownServer,
    #[error("ce serveur est déjà enregistré")]
    AlreadyExists,
    #[error("paramètre invalide : {0}")]
    InvalidInput(&'static str),
    /// Le lien n'est pas « Connecté » : rien n'a été envoyé (BR-RESIL-008).
    #[error("le lien avec le serveur n'est pas établi")]
    NotConnected,
    #[error("trop d'opérations en suspens")]
    TooManyPending,
    #[error("l'empreinte du serveur a changé")]
    FingerprintChanged,
    #[error("versions incompatibles")]
    Incompatible(Compatibility),
    #[error("identifiant ou mot de passe refusé")]
    InvalidCredentials,
    #[error("trop de tentatives, réessaie dans {retry_after_s} s")]
    TooManyAttempts { retry_after_s: u64 },
    #[error("le serveur a refusé : {0:?}")]
    Rejected(Option<ErrorCode>),
    #[error("serveur injoignable : {0}")]
    Unreachable(String),
    #[error("réponse inattendue du serveur : {0}")]
    Protocol(String),
    #[error("délai dépassé")]
    Timeout,
    #[error("stockage : {0}")]
    Store(String),
    #[error("coffre : {0}")]
    Vault(String),
    /// La tâche du lien a été relancée pendant l'appel : réessaie.
    #[error("le lien a été relancé")]
    TaskRestarted,
    #[error("la bibliothèque est arrêtée")]
    Stopped,
}

impl From<TransportError> for LinkError {
    fn from(error: TransportError) -> Self {
        match error {
            TransportError::Connect(reason) | TransportError::Io(reason) => {
                Self::Unreachable(reason)
            }
            TransportError::Timeout => Self::Timeout,
            TransportError::Closed(_) => Self::Unreachable("flux fermé".into()),
            TransportError::FingerprintMismatch { .. } => Self::FingerprintChanged,
            TransportError::Protocol(reason) => Self::Protocol(reason),
            TransportError::Api(error) => Self::from_api(&error),
        }
    }
}

impl LinkError {
    fn from_api(error: &ApiError) -> Self {
        match error.code {
            Some(ErrorCode::InvalidCredentials) => Self::InvalidCredentials,
            Some(ErrorCode::TooManyAttempts) => Self::TooManyAttempts {
                retry_after_s: error.retry_after_s.unwrap_or(60),
            },
            code => Self::Rejected(code),
        }
    }
}

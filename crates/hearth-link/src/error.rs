//! Erreurs publiques de la bibliothèque. Aucune ne contient de secret.

use hearth_proto::error::ErrorCode;
use thiserror::Error;

use crate::domain::book::BookError;
use crate::domain::compat::Compatibility;
use crate::ports::transport::{ApiError, TransportError};

/// Le champ d'une saisie refusée (jamais un texte : l'interface choisit son message).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputField {
    Name,
    Address,
    Port,
    Credentials,
    Fingerprint,
    /// Un filtre de lecture refusé (journal d'activité).
    Filter,
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum LinkError {
    #[error("serveur inconnu")]
    UnknownServer,
    #[error("ce serveur est déjà enregistré")]
    AlreadyExists,
    /// Un autre serveur du carnet porte déjà ce nom (BR-CONN-008).
    #[error("un serveur porte déjà ce nom")]
    NameTaken,
    /// L'adresse change : l'empreinte doit être relue et confirmée de nouveau (BR-CONN-009).
    #[error("nouvelle vérification de l'empreinte requise")]
    VerificationRequired,
    #[error("paramètre invalide : {0:?}")]
    InvalidInput(InputField),
    /// Le lien n'est pas « Connecté » : rien n'a été envoyé (BR-RESIL-008).
    #[error("le lien avec le serveur n'est pas établi")]
    NotConnected,
    /// Ce PC n'a pas de clé d'appareil pour ce serveur (agent ancien, coffre vide ou en panne) :
    /// un acte qui exige la preuve de la clé (retrait d'un poste de confiance) n'est pas parti.
    #[error("ce PC n'a pas de clé d'appareil pour ce serveur")]
    NoDeviceKey,
    /// Ce PC a une clé, mais l'agent ne donne pas de défi à signer (coupure du seul défi, réponse
    /// illisible, délai) : rien n'est parti, ni mot de passe ni preuve. Ce n'est PAS « serveur
    /// injoignable » (le reste répond) ; il n'y a PAS de repli automatique sans clé après N échecs
    /// (un poste reconnu se présenterait comme un inconnu) : l'utilisateur réessaie, et un repli
    /// demandé par lui est à étudier (réglage, Q15).
    #[error("l'agent ne donne pas de défi à signer pour la clé de ce PC")]
    DeviceChallengeUnavailable,
    #[error("trop d'opérations en suspens")]
    TooManyPending,
    /// Le suivi de l'action n'a pas pu être écrit sur disque : l'action n'a PAS été lancée.
    #[error("suivi impossible : l'action n'a pas été lancée")]
    TrackingUnavailable,
    /// Le disque n'a pas écrit le suivi à temps : l'action n'a PAS été lancée.
    #[error("disque trop lent : l'action n'a pas été lancée")]
    TrackingSlow,
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

impl From<BookError> for LinkError {
    fn from(error: BookError) -> Self {
        match error {
            BookError::NameRequired | BookError::NameTooLong => {
                Self::InvalidInput(InputField::Name)
            }
            BookError::NameTaken => Self::NameTaken,
            BookError::BadAddress => Self::InvalidInput(InputField::Address),
            BookError::BadPort => Self::InvalidInput(InputField::Port),
            BookError::BadUsername => Self::InvalidInput(InputField::Credentials),
        }
    }
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

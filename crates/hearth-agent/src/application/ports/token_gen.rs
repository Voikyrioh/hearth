use thiserror::Error;

use crate::domain::session_token::SessionToken;

#[derive(Debug, Error)]
#[error("le générateur aléatoire du système est indisponible : {0}")]
pub struct TokenGenError(pub String);

/// Générateur de jetons de session : 32 octets du hasard cryptographique du système.
/// Un échec est une erreur, jamais un repli sur un hasard faible.
pub trait TokenGen: Send + Sync {
    fn generate(&self) -> Result<SessionToken, TokenGenError>;
}

//! Coffre des secrets : mot de passe et jeton de chaque serveur. L'adaptateur du coffre Windows
//! vient avec l'application ; la bibliothèque ne voit que ce port.

use thiserror::Error;

use crate::domain::secret::Secret;
use crate::domain::server::ServerId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecretKind {
    Password,
    Token,
}

/// Le message ne contient jamais de secret.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("coffre indisponible : {0}")]
pub struct VaultError(pub String);

pub trait Vault: Send + Sync {
    fn get(&self, server: &ServerId, kind: SecretKind) -> Result<Option<Secret>, VaultError>;
    fn put(&self, server: &ServerId, kind: SecretKind, secret: &Secret) -> Result<(), VaultError>;
    /// Efface ; sans erreur si rien n'était stocké.
    fn delete(&self, server: &ServerId, kind: SecretKind) -> Result<(), VaultError>;
}

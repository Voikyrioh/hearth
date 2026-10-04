use async_trait::async_trait;
use thiserror::Error;

use crate::domain::accounts::PlainPassword;
use crate::domain::secret::Secret;

#[derive(Debug, Error)]
pub enum HashError {
    #[error("hachage du mot de passe impossible : {0}")]
    Hash(String),
    #[error("empreinte de mot de passe illisible")]
    MalformedHash,
}

/// Hachage des mots de passe. Coûteux : l'adaptateur l'exécute hors du runtime asynchrone.
#[async_trait]
pub trait PasswordHasher: Send + Sync {
    /// Haché d'un mot de passe conforme aux règles.
    async fn hash(&self, password: &PlainPassword) -> Result<Secret, HashError>;

    /// Vérifie un mot de passe saisi contre un haché. Pas de règle de complexité ici : le mot
    /// de passe actuel a pu être défini sous d'autres règles.
    async fn verify(&self, password: &Secret, hash: &Secret) -> Result<bool, HashError>;
}

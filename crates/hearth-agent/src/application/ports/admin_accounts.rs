use async_trait::async_trait;
use thiserror::Error;

use crate::domain::accounts::Username;
use crate::domain::secret::Secret;

/// Le mot de passe du premier compte : en clair (haché par l'agent) ou déjà haché.
pub enum AdminCredential {
    Password(Secret),
    /// Haché Argon2id au format PHC (`HEARTH_ADMIN_PASSWORD_HASH`).
    Hash(Secret),
}

impl std::fmt::Debug for AdminCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AdminCredential(***)")
    }
}

#[derive(Debug, Error)]
pub enum AdminAccountsError {
    #[error("comptes de l'agent : {0}")]
    Failed(String),
}

/// Les comptes administrateurs vus par l'installation : la base de l'agent, ouverte à la demande
/// (elle est créée à la première écriture).
#[async_trait]
pub trait AdminAccounts: Send + Sync {
    /// Le haché fourni est-il un haché Argon2id lisible ? Contrôlé avant toute écriture.
    fn check_hash(&self, hash: &Secret) -> Result<(), AdminAccountsError>;

    /// Nombre d'administrateurs ; 0 si la base n'existe pas encore (elle n'est pas créée).
    async fn admin_count(&self) -> Result<u64, AdminAccountsError>;

    /// Crée le premier administrateur (la base et ses migrations sont créées si besoin).
    async fn create_admin(
        &self,
        name: &Username,
        credential: AdminCredential,
    ) -> Result<(), AdminAccountsError>;

    /// Retire un compte que cette installation vient de créer (retour en arrière : la règle du
    /// dernier administrateur protège un compte qui existait avant, pas celui-ci).
    async fn remove_created(&self, name: &Username) -> Result<(), AdminAccountsError>;
}

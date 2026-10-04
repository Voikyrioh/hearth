//! Compte : identité, rôle, empreinte du mot de passe (BR-ACCT-001).

use std::fmt;

use time::OffsetDateTime;

use super::role::Role;
use super::username::Username;
use crate::domain::secret::Secret;

/// Identifiant technique d'un compte (ULID), distinct de l'identifiant saisi par l'utilisateur.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AccountId(String);

impl AccountId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Un compte, tel que conservé. Le hachage est un `Secret` : jamais affiché (BR-ACCT-006).
#[derive(Debug)]
pub struct Account {
    pub id: AccountId,
    pub username: Username,
    pub role: Role,
    pub password_hash: Secret,
    pub created_at: OffsetDateTime,
    pub password_changed_at: OffsetDateTime,
    /// Renseignée à chaque connexion réussie (HRT-04) ; vide tant que le compte ne s'est
    /// jamais connecté.
    pub last_login_at: Option<OffsetDateTime>,
}

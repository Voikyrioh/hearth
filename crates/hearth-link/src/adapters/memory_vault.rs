//! Coffre en mémoire : pour les tests et les essais. Le coffre Windows vient avec l'application.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use crate::domain::secret::Secret;
use crate::domain::server::ServerId;
use crate::ports::vault::{SecretKind, Vault, VaultError};

#[derive(Default)]
pub struct MemoryVault {
    secrets: Mutex<HashMap<(ServerId, SecretKind), Secret>>,
}

impl MemoryVault {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Vault for MemoryVault {
    fn get(&self, server: &ServerId, kind: SecretKind) -> Result<Option<Secret>, VaultError> {
        let secrets = self.secrets.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(secrets.get(&(server.clone(), kind)).cloned())
    }

    fn put(&self, server: &ServerId, kind: SecretKind, secret: &Secret) -> Result<(), VaultError> {
        let mut secrets = self.secrets.lock().unwrap_or_else(PoisonError::into_inner);
        secrets.insert((server.clone(), kind), secret.clone());
        Ok(())
    }

    fn delete(&self, server: &ServerId, kind: SecretKind) -> Result<(), VaultError> {
        let mut secrets = self.secrets.lock().unwrap_or_else(PoisonError::into_inner);
        secrets.remove(&(server.clone(), kind));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_are_kept_per_server_and_kind_and_deleted_cleanly() {
        let vault = MemoryVault::new();
        let a = ServerId::parse("a").unwrap();
        let b = ServerId::parse("b").unwrap();
        assert_eq!(vault.get(&a, SecretKind::Token).unwrap(), None);
        vault
            .put(&a, SecretKind::Token, &Secret::from("t1"))
            .unwrap();
        vault
            .put(&a, SecretKind::Password, &Secret::from("p1"))
            .unwrap();
        vault
            .put(&b, SecretKind::Token, &Secret::from("t2"))
            .unwrap();
        assert_eq!(
            vault.get(&a, SecretKind::Token).unwrap(),
            Some(Secret::from("t1"))
        );
        assert_eq!(
            vault.get(&b, SecretKind::Token).unwrap(),
            Some(Secret::from("t2"))
        );
        vault.delete(&a, SecretKind::Token).unwrap();
        vault.delete(&a, SecretKind::Token).unwrap();
        assert_eq!(vault.get(&a, SecretKind::Token).unwrap(), None);
        assert_eq!(
            vault.get(&a, SecretKind::Password).unwrap(),
            Some(Secret::from("p1"))
        );
    }
}

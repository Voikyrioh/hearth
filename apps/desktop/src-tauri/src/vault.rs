//! Coffre des secrets : adaptateur du port `Vault` de `hearth-link` sur le Gestionnaire
//! d'identification de Windows (BR-CONN-004).
//!
//! Un serveur a deux secrets : le mot de passe mémorisé (« Se souvenir de moi ») sous la clé
//! `Hearth/{id du serveur}`, le jeton de session sous `Hearth/{id}/token`. Rien n'est jamais écrit
//! en clair dans un fichier du client. Le stockage lui-même est derrière [`CredentialBackend`] :
//! Windows en production, une mémoire dans les tests (le passage par le vrai Gestionnaire
//! d'identification est vérifié par un test marqué `ignore`, voir `tests/vault.rs`).

use hearth_link::domain::secret::Secret;
use hearth_link::domain::server::ServerId;
use hearth_link::ports::vault::{SecretKind, Vault, VaultError};

/// Clé d'un secret dans le coffre : `Hearth/{id}` pour le mot de passe, `Hearth/{id}/token`
/// pour le jeton de session.
pub fn credential_target(server: &ServerId, kind: SecretKind) -> String {
    match kind {
        SecretKind::Password => format!("Hearth/{server}"),
        SecretKind::Token => format!("Hearth/{server}/token"),
    }
}

/// Stockage brut d'un secret sous une clé. Les messages d'erreur ne contiennent jamais de secret.
pub trait CredentialBackend: Send + Sync {
    fn read(&self, target: &str) -> Result<Option<Vec<u8>>, String>;
    fn write(&self, target: &str, secret: &[u8]) -> Result<(), String>;
    /// Sans erreur si rien n'était stocké.
    fn remove(&self, target: &str) -> Result<(), String>;
}

/// Le port `Vault` de la liaison, posé sur un stockage de secrets.
pub struct CredentialVault<B> {
    backend: B,
}

impl<B: CredentialBackend> CredentialVault<B> {
    pub fn new(backend: B) -> Self {
        Self { backend }
    }
}

impl<B: CredentialBackend> Vault for CredentialVault<B> {
    fn get(&self, server: &ServerId, kind: SecretKind) -> Result<Option<Secret>, VaultError> {
        let bytes = self
            .backend
            .read(&credential_target(server, kind))
            .map_err(VaultError)?;
        match bytes {
            None => Ok(None),
            Some(bytes) => String::from_utf8(bytes)
                .map(|text| Some(Secret::new(text)))
                .map_err(|_| VaultError("secret illisible".into())),
        }
    }

    fn put(&self, server: &ServerId, kind: SecretKind, secret: &Secret) -> Result<(), VaultError> {
        self.backend
            .write(&credential_target(server, kind), secret.expose().as_bytes())
            .map_err(VaultError)
    }

    fn delete(&self, server: &ServerId, kind: SecretKind) -> Result<(), VaultError> {
        self.backend
            .remove(&credential_target(server, kind))
            .map_err(VaultError)
    }
}

#[cfg(windows)]
pub use windows::WindowsCredentials;

#[cfg(windows)]
mod windows {
    use std::collections::HashMap;
    use std::sync::Arc;

    use keyring_core::api::CredentialStoreApi;
    use keyring_core::{Entry, Error};
    use windows_native_keyring_store::Store;

    use super::CredentialBackend;

    /// Gestionnaire d'identification de Windows (identifiants génériques).
    pub struct WindowsCredentials {
        store: Arc<Store>,
    }

    impl WindowsCredentials {
        pub fn new() -> Result<Self, String> {
            Store::new()
                .map(|store| Self { store })
                .map_err(|error| format!("gestionnaire d'identification : {error}"))
        }

        fn entry(&self, target: &str) -> Result<Entry, String> {
            // `target` = le nom exact de l'identifiant dans le Gestionnaire d'identification.
            let modifiers = HashMap::from([("target", target)]);
            self.store
                .build("Hearth", "Hearth", Some(&modifiers))
                .map_err(|error| format!("gestionnaire d'identification : {error}"))
        }
    }

    impl CredentialBackend for WindowsCredentials {
        fn read(&self, target: &str) -> Result<Option<Vec<u8>>, String> {
            match self.entry(target)?.get_secret() {
                Ok(bytes) => Ok(Some(bytes)),
                Err(Error::NoEntry) => Ok(None),
                Err(error) => Err(format!("gestionnaire d'identification : {error}")),
            }
        }

        fn write(&self, target: &str, secret: &[u8]) -> Result<(), String> {
            self.entry(target)?
                .set_secret(secret)
                .map_err(|error| format!("gestionnaire d'identification : {error}"))
        }

        fn remove(&self, target: &str) -> Result<(), String> {
            match self.entry(target)?.delete_credential() {
                Ok(()) | Err(Error::NoEntry) => Ok(()),
                Err(error) => Err(format!("gestionnaire d'identification : {error}")),
            }
        }
    }
}

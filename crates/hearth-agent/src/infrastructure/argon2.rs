//! Hachage Argon2id des mots de passe : m = 19 Mio, t = 2, p = 1 (recommandation OWASP).
//! Le calcul est coûteux : il s'exécute sur un thread dédié (`spawn_blocking`), jamais sur
//! le runtime asynchrone.

use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher as _, PasswordVerifier as _};
use argon2::{Algorithm, Argon2, Params, Version};
use async_trait::async_trait;

use crate::application::ports::{HashError, PasswordHasher};
use crate::domain::accounts::PlainPassword;
use crate::domain::secret::Secret;

pub const MEMORY_KIB: u32 = 19 * 1024;
pub const ITERATIONS: u32 = 2;
pub const PARALLELISM: u32 = 1;

#[derive(Debug, Clone)]
pub struct Argon2Hasher {
    params: Params,
}

impl Argon2Hasher {
    /// Paramètres de production.
    pub fn new() -> Result<Self, HashError> {
        Self::with_cost(MEMORY_KIB, ITERATIONS, PARALLELISM)
    }

    /// Coût personnalisé (tests uniquement).
    pub fn with_cost(
        memory_kib: u32,
        iterations: u32,
        parallelism: u32,
    ) -> Result<Self, HashError> {
        let params = Params::new(memory_kib, iterations, parallelism, None)
            .map_err(|error| HashError::Hash(error.to_string()))?;
        Ok(Self { params })
    }

    fn engine(&self) -> Argon2<'_> {
        Argon2::new(Algorithm::Argon2id, Version::V0x13, self.params.clone())
    }
}

#[async_trait]
impl PasswordHasher for Argon2Hasher {
    async fn hash(&self, password: &PlainPassword) -> Result<Secret, HashError> {
        let hasher = self.clone();
        let password = Secret::from(password.expose());
        tokio::task::spawn_blocking(move || {
            hasher
                .engine()
                .hash_password(password.expose().as_bytes())
                .map(|hash| Secret::new(hash.to_string()))
                .map_err(|error| HashError::Hash(error.to_string()))
        })
        .await
        .map_err(|error| HashError::Hash(format!("tâche de hachage interrompue : {error}")))?
    }

    async fn verify(&self, password: &Secret, hash: &Secret) -> Result<bool, HashError> {
        let hasher = self.clone();
        let password = Secret::from(password.expose());
        let hash = Secret::from(hash.expose());
        tokio::task::spawn_blocking(move || {
            let parsed = PasswordHash::new(hash.expose()).map_err(|_| HashError::MalformedHash)?;
            Ok(hasher
                .engine()
                .verify_password(password.expose().as_bytes(), &parsed)
                .is_ok())
        })
        .await
        .map_err(|error| HashError::Hash(format!("tâche de vérification interrompue : {error}")))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::accounts::Username;

    fn plain(value: &str) -> PlainPassword {
        PlainPassword::new(Secret::from(value), &Username::parse("marie").unwrap()).unwrap()
    }

    #[tokio::test]
    async fn hash_is_argon2id_with_the_owasp_parameters() {
        let hasher = Argon2Hasher::new().unwrap();
        let hash = hasher.hash(&plain("Abcdefghij12")).await.unwrap();
        assert!(
            hash.expose().starts_with("$argon2id$v=19$m=19456,t=2,p=1$"),
            "{}",
            hash.expose()
        );
    }

    #[tokio::test]
    async fn verify_accepts_the_right_password_and_refuses_another() {
        let hasher = Argon2Hasher::new().unwrap();
        let hash = hasher.hash(&plain("Abcdefghij12")).await.unwrap();
        assert!(
            hasher
                .verify(&Secret::from("Abcdefghij12"), &hash)
                .await
                .unwrap()
        );
        assert!(
            !hasher
                .verify(&Secret::from("Abcdefghij13"), &hash)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn two_hashes_of_the_same_password_differ_by_their_salt() {
        let hasher = Argon2Hasher::with_cost(8, 1, 1).unwrap();
        let first = hasher.hash(&plain("Abcdefghij12")).await.unwrap();
        let second = hasher.hash(&plain("Abcdefghij12")).await.unwrap();
        assert_ne!(first.expose(), second.expose());
    }

    #[tokio::test]
    async fn a_malformed_hash_is_an_error_not_a_refusal() {
        let hasher = Argon2Hasher::with_cost(8, 1, 1).unwrap();
        let result = hasher
            .verify(&Secret::from("Abcdefghij12"), &Secret::from("pas un hash"))
            .await;
        assert!(matches!(result, Err(HashError::MalformedHash)));
    }

    #[tokio::test]
    async fn the_hash_does_not_contain_the_password() {
        let hasher = Argon2Hasher::with_cost(8, 1, 1).unwrap();
        let hash = hasher.hash(&plain("Abcdefghij12")).await.unwrap();
        assert!(!hash.expose().contains("Abcdefghij12"));
    }
}

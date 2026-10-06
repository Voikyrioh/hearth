//! Hachage Argon2id des mots de passe : m = 19 Mio, t = 2, p = 1 (recommandation OWASP).
//! Le calcul est coûteux : il s'exécute sur un thread dédié (`spawn_blocking`), jamais sur
//! le runtime asynchrone.
//!
//! Chaque calcul occupe 19 Mio : la route de connexion étant publique, le nombre de calculs
//! simultanés est plafonné (au plus 4, et pas plus que de cœurs). Les demandes en trop attendent
//! dans une file bornée ; au-delà, refus immédiat (`HashError::Busy`, 503) plutôt que d'épuiser
//! la mémoire de la machine.

use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher as _, PasswordVerifier as _};
use argon2::{Algorithm, Argon2, Params, Version};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::application::ports::{HashError, PasswordHasher};
use crate::domain::accounts::PlainPassword;
use crate::domain::secret::Secret;

pub const MEMORY_KIB: u32 = 19 * 1024;
pub const ITERATIONS: u32 = 2;
pub const PARALLELISM: u32 = 1;
/// Plafond du nombre de calculs simultanés.
pub const MAX_CONCURRENT: usize = 4;
/// Demandes qui peuvent attendre un permis ; la suivante est refusée.
pub const MAX_WAITING: usize = 32;

/// Limite de concurrence partagée par tous les clones du hacheur.
#[derive(Debug)]
struct Gate {
    permits: Arc<Semaphore>,
    waiting: AtomicUsize,
    max_waiting: usize,
}

/// Une demande en attente d'un permis : décompte rendu à la libération, y compris quand le
/// futur est abandonné pendant l'attente (client qui coupe).
struct Waiting<'a>(&'a AtomicUsize);

impl Drop for Waiting<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Gate {
    fn new(permits: usize, max_waiting: usize) -> Self {
        Self {
            permits: Arc::new(Semaphore::new(permits.max(1))),
            waiting: AtomicUsize::new(0),
            max_waiting,
        }
    }

    async fn acquire(&self) -> Result<OwnedSemaphorePermit, HashError> {
        if let Ok(permit) = self.permits.clone().try_acquire_owned() {
            return Ok(permit);
        }
        if self.waiting.fetch_add(1, Ordering::SeqCst) >= self.max_waiting {
            self.waiting.fetch_sub(1, Ordering::SeqCst);
            return Err(HashError::Busy);
        }
        let _waiting = Waiting(&self.waiting);
        let permit = self.permits.clone().acquire_owned().await;
        permit.map_err(|_| HashError::Hash("limiteur de calculs fermé".to_owned()))
    }
}

#[derive(Debug, Clone)]
pub struct Argon2Hasher {
    params: Params,
    /// Haché d'un mot de passe jeté, calculé une fois : voir `PasswordHasher::decoy_hash`.
    decoy: Arc<Secret>,
    gate: Arc<Gate>,
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
        let cores = std::thread::available_parallelism().map_or(1, usize::from);
        Self::with_limits(
            memory_kib,
            iterations,
            parallelism,
            cores.min(MAX_CONCURRENT),
            MAX_WAITING,
        )
    }

    /// Coût et limites personnalisés : `permits` calculs simultanés, `max_waiting` en attente.
    pub fn with_limits(
        memory_kib: u32,
        iterations: u32,
        parallelism: u32,
        permits: usize,
        max_waiting: usize,
    ) -> Result<Self, HashError> {
        let params = Params::new(memory_kib, iterations, parallelism, None)
            .map_err(|error| HashError::Hash(error.to_string()))?;
        let engine = Argon2::new(Algorithm::Argon2id, Version::V0x13, params.clone());
        let decoy = engine
            .hash_password(b"hearth-decoy-password-never-matched")
            .map(|hash| Secret::new(hash.to_string()))
            .map_err(|error| HashError::Hash(error.to_string()))?;
        Ok(Self {
            params,
            decoy: Arc::new(decoy),
            gate: Arc::new(Gate::new(permits, max_waiting)),
        })
    }

    fn engine(&self) -> Argon2<'_> {
        Argon2::new(Algorithm::Argon2id, Version::V0x13, self.params.clone())
    }
}

#[async_trait]
impl PasswordHasher for Argon2Hasher {
    async fn hash(&self, password: &PlainPassword) -> Result<Secret, HashError> {
        let permit = self.gate.acquire().await?;
        let hasher = self.clone();
        let password = Secret::from(password.expose());
        // Le permis vit dans la closure : il n'est rendu que quand le calcul est fini, même si le
        // futur appelant est abandonné (le thread bloquant, lui, continue).
        let result = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            hasher
                .engine()
                .hash_password(password.expose().as_bytes())
                .map(|hash| Secret::new(hash.to_string()))
                .map_err(|error| HashError::Hash(error.to_string()))
        })
        .await;
        result
            .map_err(|error| HashError::Hash(format!("tâche de hachage interrompue : {error}")))?
    }

    async fn verify(&self, password: &Secret, hash: &Secret) -> Result<bool, HashError> {
        let permit = self.gate.acquire().await?;
        let hasher = self.clone();
        let password = Secret::from(password.expose());
        let hash = Secret::from(hash.expose());
        let result = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let parsed = PasswordHash::new(hash.expose()).map_err(|_| HashError::MalformedHash)?;
            Ok(hasher
                .engine()
                .verify_password(password.expose().as_bytes(), &parsed)
                .is_ok())
        })
        .await;
        result.map_err(|error| {
            HashError::Hash(format!("tâche de vérification interrompue : {error}"))
        })?
    }

    fn validate_hash(&self, hash: &Secret) -> Result<(), HashError> {
        // Forme et bornes d'abord (mémoire, itérations, parallélisme, sel, sortie) : un haché
        // fourni ne doit être ni trop faible ni capable d'épuiser la mémoire à chaque connexion.
        crate::domain::install::check_password_hash_format(hash.expose())
            .map_err(|_| HashError::MalformedHash)?;
        let parsed = PasswordHash::new(hash.expose()).map_err(|_| HashError::MalformedHash)?;
        if parsed.algorithm.as_str() == "argon2id" {
            Ok(())
        } else {
            Err(HashError::MalformedHash)
        }
    }

    fn decoy_hash(&self) -> &Secret {
        &self.decoy
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::accounts::Username;

    fn plain(value: &str) -> PlainPassword {
        PlainPassword::new(Secret::from(value), &Username::parse("marie").unwrap()).unwrap()
    }

    #[test]
    fn the_login_ceiling_stays_under_the_hasher_capacity() {
        // Une connexion admise ne reçoit jamais `503 BUSY` du hacheur (ADR-0022).
        const {
            assert!(
                crate::domain::login_policy::MAX_LOGINS_IN_FLIGHT <= MAX_CONCURRENT + MAX_WAITING
            )
        };
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
    async fn the_decoy_has_the_same_parameters_as_real_hashes_and_matches_nothing() {
        let hasher = Argon2Hasher::new().unwrap();
        let real = hasher.hash(&plain("Abcdefghij12")).await.unwrap();
        let prefix = |hash: &Secret| {
            hash.expose()
                .split('$')
                .take(4)
                .collect::<Vec<_>>()
                .join("$")
        };
        assert_eq!(prefix(hasher.decoy_hash()), prefix(&real));
        assert!(
            !hasher
                .verify(&Secret::from("Abcdefghij12"), hasher.decoy_hash())
                .await
                .unwrap()
        );
    }

    #[test]
    fn the_default_limit_is_between_one_and_four_computations() {
        let hasher = Argon2Hasher::with_cost(8, 1, 1).unwrap();
        let permits = hasher.gate.permits.available_permits();
        assert!((1..=MAX_CONCURRENT).contains(&permits), "{permits}");
    }

    /// Tant que tous les permis sont pris, plus aucun calcul ne démarre : la demande attend dans
    /// la file ; la demande de trop est refusée tout de suite ; tout repart à la libération.
    #[tokio::test]
    async fn computations_wait_for_a_permit_and_the_queue_is_bounded() {
        let hasher = Argon2Hasher::with_limits(8, 1, 1, 1, 1).unwrap();
        let held = hasher.gate.permits.clone().acquire_owned().await.unwrap();
        let hash = hasher.decoy_hash().expose().to_owned();

        let waiting = {
            let hasher = hasher.clone();
            let hash = Secret::from(hash.as_str());
            tokio::spawn(async move { hasher.verify(&Secret::from("x"), &hash).await })
        };
        while hasher.gate.waiting.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
        assert!(!waiting.is_finished(), "pas de permis : le calcul attend");

        let refused = hasher
            .verify(&Secret::from("x"), &Secret::from(hash.as_str()))
            .await;
        assert!(matches!(refused, Err(HashError::Busy)), "{refused:?}");
        assert!(matches!(
            hasher.hash(&plain("Abcdefghij12")).await,
            Err(HashError::Busy)
        ));

        drop(held);
        assert!(waiting.await.unwrap().is_ok());
        assert_eq!(hasher.gate.waiting.load(Ordering::SeqCst), 0);
        assert!(hasher.hash(&plain("Abcdefghij12")).await.is_ok());
    }

    /// Le futur abandonné ne libère pas le permis : le calcul continue sur son thread, le permis
    /// ne revient qu'à sa fin, le plafond n'est jamais dépassé.
    #[tokio::test]
    async fn a_cancelled_verification_keeps_its_permit_until_the_computation_ends() {
        let hasher = Argon2Hasher::with_limits(64 * 1024, 3, 1, 1, 4).unwrap();
        let hash = Secret::from(hasher.decoy_hash().expose());
        let task = {
            let hasher = hasher.clone();
            tokio::spawn(async move { hasher.verify(&Secret::from("x"), &hash).await })
        };
        while hasher.gate.permits.available_permits() != 0 {
            tokio::task::yield_now().await;
        }
        task.abort();
        let _ = task.await;
        assert_eq!(
            hasher.gate.permits.available_permits(),
            0,
            "le calcul tourne encore : permis pris"
        );
        let other = Secret::from(hasher.decoy_hash().expose());
        let started = std::time::Instant::now();
        // Une nouvelle demande attend la fin du calcul abandonné au lieu de s'ajouter.
        assert!(hasher.verify(&Secret::from("x"), &other).await.is_ok());
        assert!(started.elapsed() > std::time::Duration::from_millis(1));
        assert_eq!(hasher.gate.permits.available_permits(), 1);
    }

    #[tokio::test]
    async fn cancelled_waits_give_their_place_back_in_the_queue() {
        let hasher = Argon2Hasher::with_limits(8, 1, 1, 1, 3).unwrap();
        let held = hasher.gate.permits.clone().acquire_owned().await.unwrap();
        let hash = hasher.decoy_hash().expose().to_owned();
        let mut waits = Vec::new();
        for _ in 0..3 {
            let hasher = hasher.clone();
            let hash = Secret::from(hash.as_str());
            waits.push(tokio::spawn(async move {
                hasher.verify(&Secret::from("x"), &hash).await
            }));
        }
        while hasher.gate.waiting.load(Ordering::SeqCst) < 3 {
            tokio::task::yield_now().await;
        }
        for wait in waits {
            wait.abort();
            let _ = wait.await;
        }
        assert_eq!(hasher.gate.waiting.load(Ordering::SeqCst), 0);
        drop(held);
        assert!(hasher.hash(&plain("Abcdefghij12")).await.is_ok());
    }

    #[tokio::test]
    async fn a_burst_is_served_and_every_permit_is_given_back() {
        let hasher = Argon2Hasher::with_limits(8, 1, 1, 2, 64).unwrap();
        let hash = Arc::new(Secret::from(hasher.decoy_hash().expose()));
        let mut tasks = Vec::new();
        for _ in 0..16 {
            let hasher = hasher.clone();
            let hash = hash.clone();
            tasks.push(tokio::spawn(async move {
                hasher.verify(&Secret::from("x"), &hash).await.is_ok()
            }));
        }
        for task in tasks {
            assert!(task.await.unwrap());
        }
        assert_eq!(hasher.gate.permits.available_permits(), 2);
    }

    #[tokio::test]
    async fn the_hash_does_not_contain_the_password() {
        let hasher = Argon2Hasher::with_cost(8, 1, 1).unwrap();
        let hash = hasher.hash(&plain("Abcdefghij12")).await.unwrap();
        assert!(!hash.expose().contains("Abcdefghij12"));
    }
}

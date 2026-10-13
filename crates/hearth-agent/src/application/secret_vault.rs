//! Le coffre des secrets des services : qui décide si la clé sert, ou pas (HRT-83, ADR-0044, BR-EDIT-034).
//!
//! La règle est dans `domain::sealed::decide_key` ; ici on lit les faits (le fichier, la base) et on
//! applique la décision. Ce que l'agent garantit :
//!
//! - **Aucune clé n'est créée tant qu'aucun secret n'a besoin d'être scellé.** Un agent mis à jour depuis
//!   `main` démarre sans clé et sans description. La clé naît à la première nécessité, jamais ailleurs.
//! - **Une clé n'est créée que si rien dans la base n'a jamais dépendu d'une clé.** Fichier manquant, base
//!   qui garde des secrets : les secrets sont dits illisibles, la clé n'est PAS régénérée par-dessus.
//! - **Rien n'est supprimé, rien n'est réécrit**, quel que soit l'état du fichier de clé.
//! - L'agent démarre dans tous les cas : seuls les secrets des services sont touchés. Les services qui
//!   tournent continuent.

use std::sync::Arc;

use thiserror::Error;
use tokio::sync::Mutex;

use super::ports::{
    KeyCheckRepo, KeyRead, KeyStoreError, SealerFactory, SecretSealer, ServicesKeyStore, StoreError,
};
use crate::domain::sealed::{
    CheckAgainstDatabase, KeyAction, KeyFileState, KeyProblem, decide_key,
};

/// Ce que le coffre dit de la clé, sans rien créer (pour le démarrage et les diagnostics).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyStatus {
    /// La clé est là et correspond à la base.
    Ready,
    /// Ni clé ni secret : elle sera créée à la première nécessité.
    NotNeededYet,
    /// Les secrets sont illisibles.
    Unavailable(KeyProblem),
}

#[derive(Debug, Error)]
pub enum VaultError {
    #[error(transparent)]
    Key(#[from] KeyProblem),
    #[error("stockage de la base indisponible : {0}")]
    Database(#[from] StoreError),
    #[error(transparent)]
    KeyFile(#[from] KeyStoreError),
}

enum State {
    Unresolved,
    Ready(Arc<dyn SecretSealer>),
}

pub struct SecretVault {
    store: Arc<dyn ServicesKeyStore>,
    repo: Arc<dyn KeyCheckRepo>,
    factory: Arc<dyn SealerFactory>,
    state: Mutex<State>,
}

impl SecretVault {
    pub fn new(
        store: Arc<dyn ServicesKeyStore>,
        repo: Arc<dyn KeyCheckRepo>,
        factory: Arc<dyn SealerFactory>,
    ) -> Self {
        Self {
            store,
            repo,
            factory,
            state: Mutex::new(State::Unresolved),
        }
    }

    /// L'état de la clé, sans rien créer ni rien écrire.
    pub async fn status(&self) -> Result<KeyStatus, VaultError> {
        let (action, _) = self.evaluate().await?;
        Ok(match action {
            KeyAction::Use | KeyAction::UseAndRecordCheck => KeyStatus::Ready,
            KeyAction::Create => KeyStatus::NotNeededYet,
            KeyAction::Refuse(problem) => KeyStatus::Unavailable(problem),
        })
    }

    /// Le scelleur, prêt à servir. Crée la clé si, et seulement si, rien n'en dépend encore. Une clé
    /// inutilisable rend `VaultError::Key` : l'appelant refuse la fonction qui en avait besoin, sans toucher
    /// à rien d'autre. Réévalué à chaque appel tant que la clé est inutilisable (l'opérateur remet le bon
    /// fichier, la fonction revient sans redémarrer l'agent).
    pub async fn sealer(&self) -> Result<Arc<dyn SecretSealer>, VaultError> {
        let mut state = self.state.lock().await;
        if let State::Ready(sealer) = &*state {
            return Ok(sealer.clone());
        }
        let (action, read) = self.evaluate().await?;
        let sealer = match action {
            KeyAction::Refuse(problem) => return Err(problem.into()),
            KeyAction::Use => read.into_sealer()?,
            KeyAction::UseAndRecordCheck => {
                let sealer = read.into_sealer()?;
                self.record(&*sealer).await?;
                sealer
            }
            KeyAction::Create => {
                // Le magasin ne remplace jamais un fichier existant : si un autre démarrage l'a créé
                // entre-temps, c'est la sienne qui est rendue, et la vérification de la base tranche.
                let sealer = self.build(self.store.create()?)?;
                self.record(&*sealer).await?;
                sealer
            }
        };
        *state = State::Ready(sealer.clone());
        Ok(sealer)
    }

    fn build(
        &self,
        key: crate::domain::sealed::ServicesKey,
    ) -> Result<Arc<dyn SecretSealer>, VaultError> {
        self.factory
            .build(key)
            .map_err(|_| VaultError::Key(KeyProblem::Unreadable))
    }

    /// Garde la vérification de cette clé ; si la base en a déjà une qui diffère, la clé n'est pas la bonne.
    async fn record(&self, sealer: &dyn SecretSealer) -> Result<(), VaultError> {
        let check = sealer.key_check();
        let kept = self.repo.store_key_check(&check).await?;
        if kept == check {
            Ok(())
        } else {
            Err(KeyProblem::DoesNotMatchDatabase.into())
        }
    }

    /// Lit les faits et applique la règle. Ne crée rien, n'écrit rien.
    async fn evaluate(&self) -> Result<(KeyAction, Evidence), VaultError> {
        let facts = self.repo.database_facts().await?;
        let (file, evidence) = match self.store.read() {
            KeyRead::Absent => (KeyFileState::Absent, Evidence::None),
            KeyRead::Unusable(why) => (KeyFileState::Unusable(why), Evidence::None),
            KeyRead::Present(key) => {
                let sealer = self.build(key)?;
                let check = sealer.key_check();
                let against = match self.repo.stored_key_check().await? {
                    None => CheckAgainstDatabase::NothingStored,
                    Some(stored) if stored == check => CheckAgainstDatabase::Matches,
                    Some(_) => CheckAgainstDatabase::Differs,
                };
                (KeyFileState::Present(against), Evidence::Sealer(sealer))
            }
        };
        Ok((decide_key(&file, &facts), evidence))
    }
}

/// Le scelleur construit sur la clé lue, gardé le temps de la décision.
enum Evidence {
    None,
    Sealer(Arc<dyn SecretSealer>),
}

impl Evidence {
    fn into_sealer(self) -> Result<Arc<dyn SecretSealer>, VaultError> {
        match self {
            Self::Sealer(sealer) => Ok(sealer),
            // `decide_key` ne dit `Use` que pour une clé présente ; ce chemin est une incohérence.
            Self::None => Err(KeyProblem::Unreadable.into()),
        }
    }
}

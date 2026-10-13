//! La description durable des services créés par Hearth : écrire une révision, la relire (HRT-83,
//! ADR-0044, BR-EDIT-034, BR-EDIT-035).
//!
//! Ce cas d'usage fait le lien entre le domaine (la description, ses bornes, les données associées d'un
//! scellé), le coffre (la clé) et le dépôt (la base). Trois garanties :
//!
//! - **Un secret n'est scellé que valeur par valeur**, avec un nonce neuf, lié à SON service, à SA
//!   révision et à SON champ. Le dépôt ne reçoit que des scellés.
//! - **Une description sans secret n'a pas besoin de clé** : elle s'écrit et se lit même si la clé est
//!   inutilisable ou n'existe pas encore.
//! - **Une clé inutilisable n'empêche pas de lire la description**, seulement ses secrets
//!   ([`SecretsAccess::Unreadable`]). Rien n'est jamais supprimé ni régénéré.
//! - **Une clé inutilisable ne fait jamais disparaître des secrets** : écrire une révision SANS secret
//!   alors que la courante en garde est refusé tant que la clé ne sert pas (`SecretsUnreadable`), et
//!   aucune révision n'est purgée tant que la clé n'ouvre pas ce qu'elle porte. Retirer volontairement
//!   les secrets, clé en main, reste permis.
//! - **L'ensemble des champs est authentifié** : chaque révision qui porte un secret porte aussi un
//!   MANIFESTE scellé (nombre et noms ordonnés des autres champs). Un scellé retiré, ajouté, recopié ou
//!   permuté en base donne [`SecretsAccess::Tampered`], jamais une description tronquée.
//!
//! Aucune valeur de secret n'entre dans une erreur de ce module.

use std::sync::Arc;

use thiserror::Error;

use super::ports::{
    Clock, NewRevision, SealError, SealedField, ServiceSpecRepo, SpecStoreError, StoreError,
    StoredRevision,
};
use super::secret_vault::{SecretVault, VaultError};
use crate::domain::sealed::{KeyProblem, SealContext, SealedBlob, SecretField};
use crate::domain::services::key::{KeyKind, ServiceKey};
use crate::domain::services::spec::{
    SecretsAccess, SpecBody, SpecError, SpecRevision, SpecSecrets, manifest_matches, manifest_plain,
};

#[derive(Debug, Error)]
pub enum SpecServiceError {
    #[error("description refusée : {0}")]
    Invalid(#[from] SpecError),
    /// Seul un service créé par Hearth a une description.
    #[error("seul un service créé par Hearth a une description")]
    NotAHearthService,
    #[error("la description a changé depuis sa lecture")]
    Conflict { current: Option<u32> },
    #[error("un service porte déjà ce nom")]
    NameTaken,
    /// Il faut sceller un secret et la clé est inutilisable : la fonction est refusée, rien n'est touché.
    #[error("les secrets des services sont illisibles : {0}")]
    SecretsUnreadable(KeyProblem),
    /// Les secrets d'une révision ne forment pas ce que son manifeste scellé annonce, ou un scellé ne
    /// s'ouvre pas avec la bonne clé : la révision est altérée, rien n'est rendu comme valide.
    #[error("la description a été altérée")]
    Tampered,
    #[error("le scellement a échoué : {0}")]
    Seal(#[from] SealError),
    #[error("stockage indisponible : {0}")]
    Storage(String),
    #[error("trop de révisions")]
    TooManyRevisions,
}

impl From<VaultError> for SpecServiceError {
    fn from(error: VaultError) -> Self {
        match error {
            VaultError::Key(problem) => Self::SecretsUnreadable(problem),
            VaultError::Database(error) => Self::Storage(error.to_string()),
            VaultError::KeyFile(error) => Self::Storage(error.to_string()),
        }
    }
}

impl From<StoreError> for SpecServiceError {
    fn from(error: StoreError) -> Self {
        Self::Storage(error.to_string())
    }
}

impl From<SpecStoreError> for SpecServiceError {
    fn from(error: SpecStoreError) -> Self {
        match error {
            SpecStoreError::Conflict { current } => Self::Conflict { current },
            SpecStoreError::NameTaken => Self::NameTaken,
            SpecStoreError::Store(error) => error.into(),
        }
    }
}

pub struct ServiceSpecs {
    repo: Arc<dyn ServiceSpecRepo>,
    vault: Arc<SecretVault>,
    clock: Arc<dyn Clock>,
}

impl ServiceSpecs {
    pub fn new(
        repo: Arc<dyn ServiceSpecRepo>,
        vault: Arc<SecretVault>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self { repo, vault, clock }
    }

    /// Écrit la révision suivante de la description de `key` (la première si `expected_rev` est `None`).
    /// Rend son numéro. `expected_rev` est la révision courante que l'appelant a lue : si une autre
    /// modification est passée entre-temps, `Conflict` (BR-EDIT-024).
    pub async fn save(
        &self,
        key: &ServiceKey,
        expected_rev: Option<u32>,
        body: SpecBody,
        secrets: &SpecSecrets,
        by: &str,
    ) -> Result<u32, SpecServiceError> {
        if key.kind() != KeyKind::Hearth {
            return Err(SpecServiceError::NotAHearthService);
        }
        body.validate()?;
        let fields = secrets.fields()?;
        let rev = expected_rev
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(SpecServiceError::TooManyRevisions)?;
        let mut sealed = Vec::with_capacity(fields.len() + 1);
        if fields.is_empty() {
            // Pas de secret à sceller : la clé n'est demandée que si cette révision ferait DISPARAÎTRE les
            // secrets de la courante. Clé inutilisable : refus (rien n'est perdu en silence). La courante
            // est immuable et `append_revision` revérifie `expected_rev` dans sa transaction.
            if let Some(expected) = expected_rev
                && let Some(current) = self.repo.current(key).await?
                && current.rev == expected
                && !current.sealed.is_empty()
            {
                self.vault.sealer().await?;
            }
        } else {
            let sealer = self.vault.sealer().await?;
            // Le manifeste : la liste des champs de CETTE révision, scellée comme eux.
            let names: Vec<SecretField> = fields.iter().map(|(field, _)| field.clone()).collect();
            let manifest = manifest_plain(&names)?;
            let manifest_blob = sealer.seal(
                &SealContext {
                    service: key.as_str(),
                    rev,
                    field: &SecretField::Manifest,
                },
                manifest.expose(),
            )?;
            sealed.push(SealedField {
                field: SecretField::Manifest,
                blob: manifest_blob,
            });
            for (field, plain) in &fields {
                let blob = sealer.seal(
                    &SealContext {
                        service: key.as_str(),
                        rev,
                        field,
                    },
                    plain.expose(),
                )?;
                sealed.push(SealedField {
                    field: field.clone(),
                    blob,
                });
            }
        }
        let written = self
            .repo
            .append_revision(NewRevision {
                key: key.clone(),
                expected_rev,
                body,
                sealed,
                by: by.to_owned(),
                at: self.clock.now(),
            })
            .await?;
        debug_assert_eq!(written, rev);
        Ok(written)
    }

    /// La révision courante (ou la révision `rev`) et l'accès à ses secrets. `None` si elle n'existe pas.
    pub async fn read(
        &self,
        key: &ServiceKey,
        rev: Option<u32>,
    ) -> Result<Option<(SpecRevision, SecretsAccess)>, SpecServiceError> {
        let stored = match rev {
            Some(rev) => self.repo.revision(key, rev).await?,
            None => self.repo.current(key).await?,
        };
        let Some(stored) = stored else {
            return Ok(None);
        };
        let access = self.open_secrets(&stored).await;
        let StoredRevision {
            key,
            rev,
            body,
            updated_by,
            created_at,
            current,
            ..
        } = stored;
        Ok(Some((
            SpecRevision {
                key,
                rev,
                body,
                updated_by,
                created_at,
                current,
            },
            access,
        )))
    }

    /// Les numéros de révision gardés.
    pub async fn revisions(&self, key: &ServiceKey) -> Result<Vec<u32>, SpecServiceError> {
        Ok(self.repo.revisions(key).await?)
    }

    /// Efface les révisions antérieures à `rev` (l'ancien conteneur est supprimé), secrets compris.
    ///
    /// Rien n'est purgé tant que la clé n'ouvre pas ce qui va l'être : une révision qui porte des
    /// scellés doit s'ouvrir en entier (`SecretsUnreadable` si la clé ne sert pas, `Tampered` si la
    /// révision est altérée). La révision courante n'est jamais purgée.
    pub async fn discard_before(
        &self,
        key: &ServiceKey,
        rev: u32,
    ) -> Result<u64, SpecServiceError> {
        for old in self.repo.revisions(key).await? {
            if old >= rev {
                continue;
            }
            let Some(stored) = self.repo.revision(key, old).await? else {
                continue;
            };
            if stored.current || stored.sealed.is_empty() {
                continue;
            }
            match self.open_secrets(&stored).await {
                SecretsAccess::Opened(_) => {}
                SecretsAccess::Unreadable(problem) => {
                    return Err(SpecServiceError::SecretsUnreadable(problem));
                }
                SecretsAccess::Tampered => return Err(SpecServiceError::Tampered),
            }
        }
        Ok(self.repo.discard_before(key, rev).await?)
    }

    /// Efface toute la description d'un service supprimé : ses scellés partent avec (BR-DEL-004).
    pub async fn forget(&self, key: &ServiceKey) -> Result<u64, SpecServiceError> {
        Ok(self.repo.forget(key).await?)
    }

    async fn open_secrets(&self, stored: &StoredRevision) -> SecretsAccess {
        if stored.sealed.is_empty() {
            return SecretsAccess::Opened(SpecSecrets::default());
        }
        let sealer = match self.vault.sealer().await {
            Ok(sealer) => sealer,
            Err(VaultError::Key(problem)) => return SecretsAccess::Unreadable(problem),
            // La base ou le fichier est indisponible : on ne sait pas, on ne prétend pas que c'est altéré.
            Err(_) => return SecretsAccess::Unreadable(KeyProblem::Unreadable),
        };
        // Le manifeste d'abord : sans lui, des scellés ne sont pas une description (un seul, par clé
        // primaire de la table).
        let Some(manifest) = stored
            .sealed
            .iter()
            .find(|sealed| sealed.field == SecretField::Manifest)
        else {
            return SecretsAccess::Tampered;
        };
        let listed: Vec<SecretField> = stored
            .sealed
            .iter()
            .map(|sealed| sealed.field.clone())
            .collect();
        let manifest_context = SealContext {
            service: stored.key.as_str(),
            rev: stored.rev,
            field: &SecretField::Manifest,
        };
        match sealer.open(&manifest_context, &manifest.blob) {
            Ok(plain) if manifest_matches(plain.expose(), &listed) => {}
            _ => return SecretsAccess::Tampered,
        }
        let mut opened = Vec::with_capacity(stored.sealed.len());
        for SealedField { field, blob } in &stored.sealed {
            if *field == SecretField::Manifest {
                continue;
            }
            let context = SealContext {
                service: stored.key.as_str(),
                rev: stored.rev,
                field,
            };
            let blob: &SealedBlob = blob;
            match sealer.open(&context, blob) {
                Ok(plain) => opened.push((field.clone(), plain)),
                Err(_) => return SecretsAccess::Tampered,
            }
        }
        match SpecSecrets::from_fields(opened) {
            Ok(secrets) => SecretsAccess::Opened(secrets),
            Err(_) => SecretsAccess::Tampered,
        }
    }
}

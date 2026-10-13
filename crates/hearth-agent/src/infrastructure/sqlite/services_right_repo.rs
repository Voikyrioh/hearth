//! Écriture du droit « gérer les services » : l'UNIQUE requête de l'agent qui écrit la colonne
//! `accounts.services_right` (BR-GRANT-005). Le garde `tests/services_right_never_written.rs` n'autorise
//! que ce fichier. La requête est vérifiée à la compilation comme les autres (`query!`, entrée dans
//! `.sqlx/`) ; la valeur écrite est le texte stocké de `ServicesRight`, jamais une saisie.

use async_trait::async_trait;

use super::convert::storage;
use super::store::SqliteUnitOfWork;
use crate::application::ports::{ServicesRightTx, StoreError};
use crate::domain::accounts::{AccountId, ServicesRight};

const RESOURCE: &str = "droit sur les services";

#[async_trait]
impl ServicesRightTx for SqliteUnitOfWork {
    /// Une seule ligne doit être écrite : un compte disparu entre la lecture et l'écriture (supprimé
    /// par l'agent qui tourne) est une erreur, et la transaction n'est pas validée, donc aucune entrée
    /// de journal « accordé » n'est laissée pour un droit qui n'existe pas.
    async fn set_services_right(
        &mut self,
        id: &AccountId,
        right: ServicesRight,
    ) -> Result<(), StoreError> {
        let written = sqlx::query!(
            "UPDATE accounts SET services_right = ? WHERE id = ?",
            right.as_str(),
            id.as_str()
        )
        .execute(&mut *self.tx)
        .await
        .map_err(storage(RESOURCE))?
        .rows_affected();
        if written == 1 {
            Ok(())
        } else {
            Err(StoreError::Unavailable {
                resource: RESOURCE,
                message: format!("{written} compte écrit au lieu d'un : le compte a disparu"),
            })
        }
    }
}

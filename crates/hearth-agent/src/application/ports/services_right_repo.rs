use async_trait::async_trait;

use super::StoreError;
use crate::domain::accounts::{AccountId, ServicesRight};

/// L'ÉCRITURE du droit « gérer les services » (BR-GRANT-005, ADR-0041), dans l'unité de travail.
///
/// Un port à part, qui n'est pas dans `AccountTx` : le droit ne se donne que sur le serveur
/// (commande `account services`, question de l'installation). Un seul cas d'usage l'appelle
/// (`ServicesRightService`), et un seul fichier SQL l'écrit ; `tests/services_right_never_written.rs`
/// vérifie les deux.
#[async_trait]
pub trait ServicesRightTx: Send {
    async fn set_services_right(
        &mut self,
        id: &AccountId,
        right: ServicesRight,
    ) -> Result<(), StoreError>;
}

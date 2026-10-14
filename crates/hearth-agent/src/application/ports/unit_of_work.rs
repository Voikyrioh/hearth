use async_trait::async_trait;

use super::{
    AccountTx, AdoptionTx, AttackModeTx, AuditTx, DeviceTx, KnownAddressTx, LoginAttemptTx,
    OperationTx, ServiceOpTx, ServicesRightTx, SessionTx, StoreError,
};

/// Point d'entrée des écritures. Le magasin est neutre : il ne porte aucun sujet (comptes,
/// sessions, tentatives, opérations, journal), l'unité de travail les donne.
#[async_trait]
pub trait Store: Send + Sync {
    /// Ouvre une unité de travail. Les écritures sont sérialisées : ce qu'elle observe ne peut
    /// plus changer avant sa validation. Abandonnée sans `commit`, elle annule tout.
    async fn begin(&self) -> Result<Box<dyn UnitOfWork>, StoreError>;
}

/// Une transaction qui couvre tous les sujets : ils changent ensemble ou pas du tout.
///
/// Forme : l'unité de travail ne porte aucune opération elle-même. Elle donne un accesseur par
/// sujet (`accounts()`, `sessions()`…) qui rend le port d'écriture de ce sujet (`AccountTx`,
/// `SessionTx`…), tous liés à la même transaction. Un nouveau sujet ajoute un accesseur et un
/// port, sans toucher aux autres ; un cas d'usage n'importe que les ports des sujets qu'il
/// touche. Le stockage exécute ; les règles sont décidées par le domaine.
#[async_trait]
pub trait UnitOfWork: Send {
    fn accounts(&mut self) -> &mut dyn AccountTx;

    /// L'écriture du droit « gérer les services » : réservée à la ligne de commande du serveur
    /// (BR-GRANT-005, `ServicesRightService`).
    fn services_right(&mut self) -> &mut dyn ServicesRightTx;

    /// L'écriture de l'adoption d'un service (HRT-80) : un seul cas d'usage, `ServiceAdoption`.
    fn adoptions(&mut self) -> &mut dyn AdoptionTx;

    /// Le journal des opérations de service, pour annuler et décider avec leur entrée du journal d'activité
    /// (HRT-144) : un seul cas d'usage, `ServiceOpAccess`.
    fn service_ops(&mut self) -> &mut dyn ServiceOpTx;

    fn sessions(&mut self) -> &mut dyn SessionTx;

    fn login_attempts(&mut self) -> &mut dyn LoginAttemptTx;

    fn known_addresses(&mut self) -> &mut dyn KnownAddressTx;

    fn devices(&mut self) -> &mut dyn DeviceTx;

    fn attack_mode(&mut self) -> &mut dyn AttackModeTx;

    fn operations(&mut self) -> &mut dyn OperationTx;

    fn audit(&mut self) -> &mut dyn AuditTx;

    async fn commit(self: Box<Self>) -> Result<(), StoreError>;
}

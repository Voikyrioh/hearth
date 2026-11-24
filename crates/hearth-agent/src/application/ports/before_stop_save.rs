//! Port `BeforeStopSave` : l'enregistrement du monde par la console avant un arrêt demandé par Hearth (HRT-292,
//! ADR-0067 point 10, BR-CONSOLE-012).
//!
//! Porté par le cas d'usage de la console, branché sur `ServiceOps` après coup (la console a besoin des opérations).
//! Jamais bloquant : l'implémentation rend toujours la main dans le délai de la fiche.

use async_trait::async_trait;

use crate::domain::services::ServiceKey;

#[async_trait]
pub trait BeforeStopSave: Send + Sync {
    /// La fiche de ce service déclare-t-elle `[remote_console.use.before_stop]` ?
    async fn declares(&self, key: &ServiceKey) -> bool;

    /// Envoie la commande de la fiche et attend la réponse attendue. `true` : réponse vue (« confirmé »). `false` :
    /// jeu pas prêt, console injoignable, muette ou réponse inattendue (« non confirmé »). Jamais d'erreur.
    async fn save(&self, key: &ServiceKey) -> bool;
}

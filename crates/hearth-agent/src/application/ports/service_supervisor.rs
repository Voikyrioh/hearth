use std::collections::HashMap;

use async_trait::async_trait;

use crate::domain::services::{UnitLookup, UnitName};

/// Un conteneur dont on cherche le propriétaire : son identifiant complet et son nom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerRef {
    pub id: String,
    pub name: String,
}

/// Le superviseur de services du serveur (systemd), en DÉTECTION SEULE (HRT-63, ADR-0037).
///
/// Deux méthodes, aucune n'agit : ni démarrer, ni arrêter, ni redémarrer une unité (HRT-64 ajoutera les
/// siennes avec leur faux et leurs tests). Aucune ne rend d'erreur : un système sans superviseur, une
/// commande absente ou muette sont des cas NORMAUX et silencieux (« je ne sais pas »).
///
/// Les noms d'unité rendus sont déjà validés (`UnitName`) : rien de lu sur le système n'entre dans
/// l'agent sous une autre forme.
#[async_trait]
pub trait ServiceSupervisor: Send + Sync {
    /// Pour chaque conteneur de `containers` lancé sous une unité de service, cette unité (clé :
    /// identifiant du conteneur). Un conteneur sans unité, ou dont on ne sait rien, est absent du
    /// résultat. Un conteneur dont le propriétaire est déjà connu n'a pas à être redemandé : il ne
    /// change pas pendant la vie du conteneur.
    async fn owners(&self, containers: &[ContainerRef]) -> HashMap<String, UnitName>;

    /// L'état de l'unité.
    async fn unit(&self, unit: &UnitName) -> UnitLookup;
}

use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use thiserror::Error;

use crate::domain::services::{UnitLookup, UnitName};

/// Un conteneur dont on cherche le propriétaire : son identifiant complet et son nom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerRef {
    pub id: String,
    pub name: String,
}

/// Pourquoi un ordre donné au superviseur n'a pas abouti (HRT-64). Jamais le texte de la commande : une
/// sortie d'erreur de `systemctl` peut nommer un chemin ou une variable, elle n'est pas lue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SupervisorError {
    /// Pas de superviseur sur ce système, ou la commande est absente et ne se lance pas.
    #[error("le superviseur du serveur n'est pas disponible")]
    Unavailable,
    /// Le superviseur a répondu par un échec (code de sortie non nul : unité en échec au démarrage, unité
    /// inconnue, droits).
    #[error("le superviseur du serveur a refusé l'ordre")]
    Refused,
    /// Le superviseur n'a pas rendu la main dans le délai donné : le résultat est inconnu.
    #[error("le superviseur du serveur n'a pas répondu à temps")]
    Timeout,
}

/// Le superviseur de services du serveur (systemd) : détection (HRT-63, ADR-0037), puis les trois ordres de
/// cycle de vie (HRT-64). Jamais `disable`, `mask`, ni l'écriture d'une unité : sur NixOS ce serait d'ailleurs
/// impossible (ADR-0037).
///
/// Les deux méthodes de détection ne rendent pas d'erreur : un système sans superviseur, une commande absente
/// ou muette sont des cas NORMAUX et silencieux (« je ne sais pas »). Les trois ordres, eux, disent pourquoi
/// ils n'ont pas abouti ([`SupervisorError`]).
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

    /// Démarre l'unité et attend que le système ait fini de la démarrer, au plus `wait` (BR-LIFE-004).
    /// Déjà démarrée : réussit sans rien changer. Le nom d'unité est un argument de la commande, jamais
    /// une partie d'une ligne passée à un interpréteur.
    async fn start(&self, unit: &UnitName, wait: Duration) -> Result<(), SupervisorError>;

    /// Arrête l'unité avec SON délai d'arrêt (c'est le système qui l'applique, BR-LIFE-009) et attend
    /// qu'elle soit arrêtée, au plus `wait`. Déjà arrêtée : réussit sans rien changer.
    async fn stop(&self, unit: &UnitName, wait: Duration) -> Result<(), SupervisorError>;

    /// Redémarre l'unité d'un seul geste du système (BR-LIFE-012 : une étape trop brève peut ne pas se voir)
    /// et attend, au plus `wait`.
    async fn restart(&self, unit: &UnitName, wait: Duration) -> Result<(), SupervisorError>;

    /// Combien de temps le dernier arrêt de l'unité a duré, mesuré par le système (BR-LIFE-009) : de la sortie
    /// d'« active » à l'arrivée à « inactive », démarrage d'un redémarrage exclu. `None` : le superviseur ne le
    /// sait pas (pas de réponse, unité jamais arrêtée) ; l'arrêt n'est alors jamais dit forcé.
    async fn last_stop_duration(&self, _unit: &UnitName) -> Option<Duration> {
        None
    }
}

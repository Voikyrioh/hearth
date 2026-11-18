//! Port `ModsSnapshot` : la lecture des mods d'un service que la sauvegarde demande (HRT-293, ADR-0062 point 10).
//!
//! Implémenté par `ModsService` pour que `application/backups.rs` ne dépende pas de `application/mods.rs`. Une lecture
//! qui échoue n'est jamais une erreur de sauvegarde : elle rend `Unreadable`.

use async_trait::async_trait;

use crate::domain::mods::backup_list::{CurrentMod, Snapshot};
use crate::domain::services::ServiceKey;

/// Ce que la lecture a vu, avec de quoi la recouper avec le manifeste de la sauvegarde.
#[derive(Debug, Clone)]
pub struct ModsRead {
    /// Le dossier des mods et celui des mods retirés (un composant chacun), sous le dossier géré qui les porte.
    pub dir: String,
    pub retired_dir: String,
    /// Le point de montage du dossier géré qui porte les mods (`/data`) : retrouve sa racine dans le manifeste.
    pub data_target: String,
    pub snapshot: Snapshot,
}

#[derive(Debug, Clone)]
pub enum ModsReading {
    /// Le moteur du service ne charge pas de mods (ou pas de fiche) : aucune liste.
    NotApplicable,
    /// La lecture a échoué ou dépassé son temps.
    Unreadable,
    Read(ModsRead),
}

/// Les mods actuels, pour la comparaison.
#[derive(Debug, Clone)]
pub struct CurrentMods {
    pub mods: Vec<CurrentMod>,
    /// Jars non lus faute de temps : la comparaison est incomplète.
    pub unread: u32,
}

#[async_trait]
pub trait ModsSnapshot: Send + Sync {
    /// Le service a-t-il une liste de mods (fiche à `[mods.jars]` et moteur qui charge des mods) ?
    async fn applicable(&self, key: &ServiceKey) -> bool;

    /// Lit les mods avant la copie, 60 secondes au plus, sans rien modifier.
    async fn read(&self, key: &ServiceKey) -> ModsReading;

    /// Les mods actuels (comme `GET /mods`, avec son cache). `None` : lecture bloquée ou impossible.
    async fn current(&self, key: &ServiceKey) -> Option<CurrentMods>;
}

//! Port `HostPaths` : ce que l'agent sait d'un chemin du serveur (HRT-85, BR-DEL-008). Lecture seule, jamais un
//! contenu : la forme canonique d'un chemin (liens et `..` résolus) et l'identité du dossier avec celle de tous
//! ses parents (périphérique et inode), pour reconnaître deux noms du même dossier (lien, montage « bind ») et un
//! parent ou un enfant monté sous un autre nom.

use crate::domain::services::delete::FileId;

/// Ce qu'une lecture a établi d'un chemin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    /// Le chemin n'existe pas.
    Absent,
    /// Il n'a pas pu être lu dans le temps imparti ou pour une autre raison que son absence (partage réseau
    /// mort, droit refusé) : on ne sait pas. Jamais un feu vert.
    Unreadable,
    Found {
        /// La forme canonique (liens et `..` résolus).
        canonical: String,
        /// L'identité du chemin d'abord, puis celle de chacun de ses parents jusqu'à la racine incluse.
        lineage: Vec<FileId>,
    },
}

pub trait HostPaths: Send + Sync {
    /// Lit le chemin, borné dans le temps : l'appel ne pend jamais sur un système de fichiers qui ne répond plus.
    fn resolve(&self, path: &str) -> Resolved;
}

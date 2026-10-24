//! Port `HostPaths` : ce que l'agent sait d'un chemin du serveur (HRT-85, BR-DEL-008). Lecture seule, jamais un
//! contenu : la forme canonique d'un chemin (liens et `..` résolus) et l'identité du dossier (périphérique et
//! inode), pour reconnaître deux noms du même dossier (lien, montage « bind »).

use crate::domain::services::delete::FileId;

pub trait HostPaths: Send + Sync {
    /// Le chemin canonique, ou `None` s'il n'existe pas ou ne se lit pas.
    fn canonical(&self, path: &str) -> Option<String>;

    /// L'identité du dossier que ce chemin désigne (liens suivis), ou `None`.
    fn identity(&self, path: &str) -> Option<FileId>;
}

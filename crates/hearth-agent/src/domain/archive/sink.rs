//! Port d'écriture de l'extraction. Le domaine ne touche aucun fichier : il décrit ce qu'il écrit,
//! l'adaptateur (`infrastructure/archive_fs.rs`) le fait sans jamais suivre un lien.

use std::io::Write;

use super::path::ArchivePath;

/// Pourquoi une écriture a échoué. Aucun chemin, aucun texte du système.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SinkError {
    /// L'objet existe déjà : l'extraction ne remplace jamais rien.
    AlreadyExists,
    /// Un lien ou un objet qui n'est pas ce qu'on attendait barre la route : on ne le suit pas.
    Blocked,
    /// Toute autre panne (disque plein, droits...).
    Io(std::io::ErrorKind),
}

/// Ce que l'extraction a le droit de faire dans le dossier de destination, et rien d'autre.
///
/// Contrat de l'adaptateur :
/// - tout chemin reçu est relatif, déjà jugé ; l'écriture reste sous le dossier de destination ;
/// - aucun lien n'est suivi, ni dans la destination ni dans un parent déjà présent ;
/// - les dossiers manquants sont créés par l'adaptateur (propriétaire du serveur, `0750`) ;
/// - un fichier est créé neuf (`0640`, aucun bit d'exécution, propriétaire et dates non repris de
///   l'archive) et ne remplace jamais un objet existant.
pub trait ExtractSink {
    fn create_dir(&mut self, path: &ArchivePath) -> Result<(), SinkError>;
    fn create_file(&mut self, path: &ArchivePath) -> Result<Box<dyn Write + '_>, SinkError>;
}

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
/// - les dossiers manquants sont créés par l'adaptateur, avec le propriétaire et les droits que
///   l'APPELANT a demandés (`WriteSpec` de l'adaptateur), jamais ceux de l'archive ni le masque ambiant ;
/// - un fichier est créé neuf (même propriétaire, même mode fixe demandé par l'appelant, aucun bit
///   d'exécution ni spécial), ne remplace jamais un objet existant, et `flush` le rend durable
///   (`fsync`) : l'extraction appelle `flush` une fois par fichier, après ses derniers octets.
pub trait ExtractSink {
    fn create_dir(&mut self, path: &ArchivePath) -> Result<(), SinkError>;
    fn create_file(&mut self, path: &ArchivePath) -> Result<Box<dyn Write + '_>, SinkError>;
}

//! Port `TransferFiles` : la zone d'attente des transferts (HRT-95, ADR-0048, BR-FILES-019, BR-MOD-015).
//!
//! **Hors de tout dossier de service et de tout montage.** La racine est `0700` à l'utilisateur de l'agent ; chaque
//! fichier est `0600`. Comme la racine des sauvegardes, le port ne LISTE jamais la racine : chaque geste vise un nom
//! dérivé d'un identifiant de transfert, et ceux qui détruisent revérifient le numéro d'inode donné par le registre.
//!
//! Pour un identifiant `<id>` : `<id>.part` (les octets reçus, dans l'ordre), `<id>.dig` (32 octets par morceau
//! reçu, la suite des empreintes) puis, une fois complet et vérifié, `<id>.bin` (le même fichier, posé à son nom
//! définitif d'un seul geste : `RENAME_NOREPLACE`). Un fichier partiel n'a donc JAMAIS le nom d'un fichier complet.
//!
//! Aucun état n'est gardé entre deux gestes (pas de descripteur ouvert pendant que le client est lent) : chaque
//! morceau rouvre la partie, revérifie son identité, y écrit à son rang et la rend durable avant d'être accusé.
//! Aucune erreur ne porte un chemin. Les types d'erreur, d'inode et d'espace sont ceux des sauvegardes.

use std::io::{Read, Seek};

use super::backup_files::{BackupFsError, DiskSpace, Expected, FileIdent, FileStamp, Removed};
use crate::domain::transfers::TransferId;
use hearth_proto::api::transfers::DIGEST_LEN;

/// Un fichier complet qui se lit et se parcourt (la lecture d'archive juge un zip par son index de fin).
pub trait ReadSeek: Read + Seek + Send {}

impl<T: Read + Seek + Send> ReadSeek for T {}

/// Ce que le disque dit d'un envoi en cours.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    /// Les octets que la partie contient réellement.
    pub part_len: u64,
    /// Les empreintes de morceaux entières de la liste, dans l'ordre.
    pub digests: Vec<[u8; DIGEST_LEN]>,
}

pub trait TransferFiles: Send + Sync {
    /// Crée la racine (`0700`) si elle manque et la juge : un vrai dossier de l'agent, ni lien ni autre propriétaire.
    fn ensure_root(&self) -> Result<(), BackupFsError>;

    /// L'espace du disque de la racine.
    fn space(&self) -> Result<DiskSpace, BackupFsError>;

    /// Crée `<id>.part` et `<id>.dig` NEUFS (création exclusive, jamais par un lien), vides. Rend l'identité de la
    /// partie.
    fn create(&self, id: &TransferId) -> Result<FileIdent, BackupFsError>;

    /// Relit l'état de l'envoi : la partie est REVÉRIFIÉE (c'est bien le fichier `expect`, ordinaire, jamais un lien),
    /// et au plus `max_digests` empreintes entières sont lues de la liste.
    fn load(
        &self,
        id: &TransferId,
        expect: FileIdent,
        max_digests: u64,
    ) -> Result<Loaded, BackupFsError>;

    /// Écrit le morceau de rang `index` à l'octet `offset` de la partie (qui est tronquée à `offset` d'abord : un
    /// morceau à moitié écrit par un arrêt est écrasé, jamais conservé), le rend durable (`fdatasync`), puis range
    /// son empreinte au rang `index` de la liste. Le morceau est accusé APRÈS cet appel.
    fn append(
        &self,
        id: &TransferId,
        expect: FileIdent,
        index: u64,
        offset: u64,
        data: &[u8],
        digest: &[u8; DIGEST_LEN],
    ) -> Result<(), BackupFsError>;

    /// Rend la partie définitive : vérifie qu'elle fait exactement `size` octets, la rend durable, puis la renomme en
    /// `<id>.bin` sans jamais remplacer un fichier (atomiquement). Rend l'identité complète du fichier publié.
    fn publish(
        &self,
        id: &TransferId,
        expect: FileIdent,
        size: u64,
    ) -> Result<FileStamp, BackupFsError>;

    /// Ouvre le fichier complet en lecture : jamais par un lien, et SEULEMENT si c'est encore le fichier publié
    /// (inode, taille et date de modification).
    fn open_complete(
        &self,
        id: &TransferId,
        expect: FileStamp,
    ) -> Result<Box<dyn ReadSeek>, BackupFsError>;

    /// Retire la partie SI c'est le fichier de l'inode donné (`None` : seulement un fichier ordinaire de ce nom).
    fn remove_part(
        &self,
        id: &TransferId,
        expect: Option<FileIdent>,
    ) -> Result<Removed, BackupFsError>;

    /// Retire la liste des empreintes (un fichier ordinaire de ce nom, jamais un lien).
    fn remove_digests(&self, id: &TransferId) -> Result<Removed, BackupFsError>;

    /// Retire le fichier complet SI c'est le fichier publié (`Expected::stamp`).
    fn remove_complete(&self, id: &TransferId, expect: Expected) -> Result<Removed, BackupFsError>;
}

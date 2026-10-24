//! Port `SafeFs` : le seul chemin vers le dossier de données d'un service (HRT-145, ADR-0046).
//!
//! Le contenu d'un dossier de données est écrit par le programme du service : hostile par hypothèse
//! (liens, tubes, périphériques, remplacements en cours d'opération), alors que l'agent est root.
//! Rien ici n'accepte un chemin absolu : tout est « une racine de service » + un [`RelPath`] déjà jugé
//! par le domaine. Aucun lien n'est jamais suivi, rien ne sort de la racine (`BR-FILES-003`).

use std::io;
use std::time::Duration;

use crate::domain::files::RelPath;
use crate::domain::services::ServiceKey;

/// Pourquoi un geste sur un fichier est refusé ou a échoué. Aucune variante ne porte de chemin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FsError {
    /// Un composant est un lien, ou un fichier là où un dossier était attendu (`BR-FILES-003`).
    #[error("path blocked: link or not a directory")]
    Blocked,
    /// Un point de montage (autre périphérique que la racine) : jamais franchi.
    #[error("path crosses a mount point")]
    CrossesDevice,
    #[error("not found")]
    NotFound,
    /// La cible est un lien, un tube, un socket ou un périphérique (`BR-FILES-004`).
    #[error("not a regular file")]
    NotRegular,
    /// Fichier à plusieurs noms : lu, supprimé, jamais modifié (`BR-FILES-004`).
    #[error("file has several names")]
    Hardlinked,
    #[error("file too large")]
    TooLarge,
    /// La racine (ou son parent) n'appartient pas à l'agent, ou un autre compte y écrit.
    #[error("unsafe root")]
    UnsafeRoot,
    /// Geste refusé sur la racine elle-même (écrire, retirer) ou arborescence trop profonde.
    #[error("refused on the root or too deep")]
    Refused,
    /// Plate-forme sans écriture sûre (l'agent ne tourne que sous Linux).
    #[error("safe file access is not available on this platform")]
    Unsupported,
    #[error("i/o error: {0:?}")]
    Io(io::ErrorKind),
}

/// Propriétaire numérique donné par l'appelant (jamais repris du contenu du service).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ownership {
    uid: u32,
    gid: u32,
}

impl Ownership {
    /// `None` si un identifiant vaut `u32::MAX` (« ne pas changer » pour `chown`, jamais un compte).
    pub fn new(uid: u32, gid: u32) -> Option<Self> {
        (uid != u32::MAX && gid != u32::MAX).then_some(Self { uid, gid })
    }

    pub fn uid(&self) -> u32 {
        self.uid
    }

    pub fn gid(&self) -> u32 {
        self.gid
    }
}

/// Propriétaire et droits de ce qui est créé. Les droits ne portent jamais setuid, setgid ni sticky.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Perms {
    owner: Ownership,
    mode: u32,
}

impl Perms {
    /// `None` si `mode` a un bit hors de `0o777`.
    pub fn new(owner: Ownership, mode: u32) -> Option<Self> {
        (mode & !0o777 == 0).then_some(Self { owner, mode })
    }

    pub fn owner(&self) -> Ownership {
        self.owner
    }

    pub fn mode(&self) -> u32 {
        self.mode
    }
}

/// Taille d'un arbre. `partial` : le budget de temps ou une borne a coupé le parcours, la valeur est
/// « au moins » (`BR-DEL-009`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Measure {
    /// Taille sur le disque (`st_blocks` × 512), un fichier à plusieurs noms compté une fois.
    pub bytes_on_disk: u64,
    pub items: u64,
    pub partial: bool,
}

/// Ce qu'un effacement borné a fait (`BR-DEL-030`). Le geste ne s'arrête pas à la première
/// erreur : il retire tout ce qui part et compte ce qui reste, que l'appelant dit sans le rejouer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Erased {
    /// Éléments retirés (fichiers, liens, dossiers), le dossier visé compris.
    pub removed: u64,
    /// Éléments qui n'ont pas pu partir (droits refusés, système de fichiers en lecture seule, point de
    /// montage, profondeur au-delà de la borne). Chaque élément resté est compté une fois.
    pub left: u64,
    /// La première erreur rencontrée, pour la trace seulement : jamais un chemin.
    pub first_error: Option<FsError>,
}

/// Accès aux racines de service.
pub trait SafeFs: Send + Sync {
    /// Ouvre `<données>/services/<clé>/` ; le dossier doit exister.
    fn open_service_root(&self, key: &ServiceKey) -> Result<Box<dyn ServiceRoot>, FsError>;

    /// Comme `open_service_root`, en créant `services/` et `<clé>/` (agent, `0700`) s'ils manquent.
    fn ensure_service_root(&self, key: &ServiceKey) -> Result<Box<dyn ServiceRoot>, FsError>;

    /// Retire tout le contenu de la racine du service PUIS la racine elle-même (`BR-DEL-007`) ; c'est
    /// le seul geste qui retire une racine. Un lien est retiré comme lien, aucun montage franchi.
    fn remove_service_root(&self, key: &ServiceKey) -> Result<(), FsError>;

    /// Retire la racine du service SI ELLE EST VIDE (`unlinkat` du dossier, qui refuse un dossier non vide) :
    /// `true` si elle est partie, `false` si elle contient encore quelque chose (un dossier conservé parce
    /// qu'un autre conteneur l'utilise, une donnée qui n'a pas pu être effacée) ou n'existe plus. Ne
    /// touche jamais un contenu.
    fn remove_empty_service_root(&self, key: &ServiceKey) -> Result<bool, FsError>;
}

/// Une racine de service ouverte. Chaque geste ouvre chaque composant depuis le descripteur de son
/// parent, sans suivre de lien.
pub trait ServiceRoot: Send + Sync {
    /// Crée le dossier et ses parents manquants (propriétaire et droits donnés aux dossiers créés) ;
    /// un dossier déjà là est gardé tel quel. `true` si le dernier niveau a été créé.
    fn mkdir(&self, rel: &RelPath, perms: Perms) -> Result<bool, FsError>;

    /// Écrit le fichier de façon atomique : fichier d'attente du même dossier, propriétaire et droits
    /// posés avant la première donnée, `fsync`, renommage. Le dossier parent doit exister. Remplace un
    /// fichier ordinaire à un seul nom ; refuse un lien, un fichier spécial, un fichier à plusieurs noms.
    fn write_atomic(&self, rel: &RelPath, bytes: &[u8], perms: Perms) -> Result<(), FsError>;

    /// Lit un fichier ordinaire de `max` octets au plus (`TooLarge` sinon).
    fn read_bounded(&self, rel: &RelPath, max: u64) -> Result<Vec<u8>, FsError>;

    /// Mesure l'arbre (la racine si `rel` est vide) sans rien lire ni franchir de montage, dans `budget`.
    fn measure(&self, rel: &RelPath, budget: Duration) -> Result<Measure, FsError>;

    /// Retire un fichier, un lien ou un arbre. Jamais la racine (`Refused`).
    fn remove_tree(&self, rel: &RelPath) -> Result<(), FsError>;

    /// Efface un arbre sous la racine pour une suppression de service (`BR-DEL-007`,
    /// `BR-DEL-030`) : mêmes garanties que [`Self::remove_tree`] (un lien est retiré comme lien, aucun montage
    /// franchi, 64 niveaux, jamais la racine), et en plus :
    /// - **mémoire bornée** quel que soit le nombre d'entrées d'un dossier (les noms se lisent par lots, HRT-179) ;
    /// - **faible priorité** : le geste tourne sur un fil à part dont la priorité est abaissée, sans gêner les
    ///   autres services ;
    /// - **suivi** : `progress` reçoit le nombre d'éléments retirés jusqu'ici, après chaque lot ;
    /// - **jamais d'arrêt à la première erreur** : ce qui part part, ce qui reste est compté (`Erased::left`).
    ///
    /// `NotFound` si le dossier visé n'existe pas (rien à effacer).
    fn erase_tree(
        &self,
        rel: &RelPath,
        progress: &mut (dyn FnMut(u64) + Send),
    ) -> Result<Erased, FsError>;
}

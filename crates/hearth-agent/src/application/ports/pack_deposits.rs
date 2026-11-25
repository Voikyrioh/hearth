//! Port `PackDeposits` : les packs de mods déposés par l'utilisateur dans la zone d'attente de l'agent
//! (HRT-124, BR-MOD-007). Le fichier y arrive par un transfert reprenable (HRT-95) ; ce port ne le
//! reçoit pas, il le REND à qui le juge et le range.
//!
//! Un dépôt est désigné par un identifiant que l'agent a donné (jamais un chemin, jamais le nom envoyé par le
//! client). Le fichier est HOSTILE : il est jugé entier par `domain::archive` avant d'être rangé.

use std::io::{Read, Seek};

use thiserror::Error;

use super::safe_fs::Perms;
use crate::domain::files::RelPath;
use crate::domain::services::ServiceKey;

/// Un identifiant de dépôt : lettres, chiffres et tirets, 64 caractères au plus (celui d'un transfert de la zone
/// d'attente est un ULID en majuscules). L'adaptateur le juge à son tour, plus strictement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepositId(String);

impl DepositId {
    pub fn parse(text: &str) -> Option<Self> {
        let ok = !text.is_empty()
            && text.len() <= 64
            && text.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
        ok.then(|| Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DepositError {
    /// Aucun dépôt complet de cet identifiant (jamais reçu, incomplet, déjà rangé ou périmé).
    #[error("dépôt introuvable")]
    NotFound,
    /// Le fichier n'est plus celui qui a été jugé (empreinte différente) : rien n'est rangé.
    #[error("dépôt modifié depuis son jugement")]
    Changed,
    /// Entrée-sortie ou droits : jamais un chemin dans le message.
    #[error("dépôt illisible")]
    Io,
}

/// Le fichier déposé, ouvert en lecture seule.
pub trait DepositFile: Read + Seek + Send {}
impl<T: Read + Seek + Send> DepositFile for T {}

pub trait PackDeposits: Send + Sync {
    /// Ouvre le dépôt COMPLET de ce compte en lecture seule. Ne suit aucun lien. Le dépôt d'un autre compte est
    /// `NotFound` : un compte ne consomme jamais le dépôt d'un autre. **Un dépôt complet n'est plus JAMAIS écrit**
    /// (aucune reprise de transfert sur le même fichier) : le jugement hache le fichier puis le relit pour l'analyser,
    /// et `place` revérifie l'empreinte ; l'adaptateur garantit que ces lectures voient les mêmes octets.
    fn open(&self, id: &DepositId, account: &str) -> Result<Box<dyn DepositFile>, DepositError>;

    /// Range le dépôt dans le dossier géré du service sous `rel` (un nom FIXE choisi par Hearth) en UN geste
    /// atomique, sans remplacer ce qui existe, et CONSOMME le dépôt. L'adaptateur revérifie que le fichier a
    /// l'empreinte SHA-256 jugée (`Changed` sinon, rien n'est rangé). Le fichier rangé a le propriétaire et les droits
    /// que l'appelant donne (`perms` : un pack, lisible par tous et inscriptible par l'agent seul ; un mod, le
    /// propriétaire de la fiche en `0644`, HRT-258).
    fn place(
        &self,
        id: &DepositId,
        account: &str,
        service: &ServiceKey,
        rel: &RelPath,
        sha256: &[u8; 32],
        perms: Perms,
    ) -> Result<(), DepositError>;

    /// Retire le dépôt (pack refusé, BR-MOD-016 : « retiré de la zone d'attente aussitôt »). Idempotent.
    fn discard(&self, id: &DepositId, account: &str);
}

//! Les règles d'un transfert reprenable (HRT-95, ADR-0048, BR-FILES-013, BR-FILES-019, BR-MOD-015). Aucune E/S :
//! l'application écrit, le domaine juge.
//!
//! - [`TransferId`] : un ULID qui entre dans un nom de fichier de la zone d'attente, jugé, jamais pris tel quel ;
//! - [`Receiver`] : la machine d'état d'un envoi. Un morceau est accepté s'il est LE PROCHAIN, de la bonne taille et
//!   de la bonne empreinte ; le fichier est complet quand l'empreinte de la suite des empreintes correspond ;
//! - [`judge_space`] : l'espace à laisser libre après une écriture (BR-FILES-013) ;
//! - [`admit`] : les bornes de transferts en cours et de dépôts ouverts ;
//! - [`judge_name`] : le nom du fichier annoncé (BR-FILES-005).

use std::fmt;

use hearth_proto::api::transfers::{
    CHUNK_BYTES, DIGEST_LEN, MAX_ACTIVE_PER_ACCOUNT, MAX_ACTIVE_TRANSFERS, MAX_OPEN_PER_ACCOUNT,
    NAME_MAX_BYTES, RESERVE_MIN_BYTES, RESERVE_PERCENT, TreeHasher, chunk_count, chunk_len,
};

/// L'identifiant d'un transfert : un ULID en majuscules (26 caractères). Il entre dans les noms de fichiers de la
/// zone d'attente (`<id>.part`, `<id>.dig`, `<id>.bin`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TransferId(String);

impl TransferId {
    /// `None` si le texte n'est pas exactement un ULID en majuscules.
    pub fn parse(text: &str) -> Option<Self> {
        let valid = text.len() == 26
            && text
                .bytes()
                .all(|b| b.is_ascii_digit() || (b.is_ascii_uppercase() && !b"ILOU".contains(&b)));
        valid.then(|| Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// La partie en cours de réception : `<id>.part`.
    pub fn part_name(&self) -> String {
        format!("{}.part", self.0)
    }

    /// La liste des empreintes de morceaux reçus : `<id>.dig`, 32 octets par morceau.
    pub fn digests_name(&self) -> String {
        format!("{}.dig", self.0)
    }

    /// Le fichier complet et vérifié : `<id>.bin`.
    pub fn complete_name(&self) -> String {
        format!("{}.bin", self.0)
    }
}

impl fmt::Display for TransferId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Le nom annoncé d'un fichier est-il valable (BR-FILES-005) : 1 à 255 octets, ni barre oblique, ni barre inverse,
/// ni octet nul ni caractère de contrôle, ni « . » ni « .. » ?
pub fn judge_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= NAME_MAX_BYTES
        && name != "."
        && name != ".."
        && !name
            .chars()
            .any(|c| c == '/' || c == '\\' || c.is_control())
}

/// Ce qu'il faut laisser libre sur un disque de `total` octets : le plus grand de 1 Gio et de 5 % (BR-FILES-013).
pub fn reserve(total: u64) -> u64 {
    RESERVE_MIN_BYTES.max(total / 100 * RESERVE_PERCENT)
}

/// Ce qui manque pour écrire `needed` octets de plus (BR-FILES-013).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shortfall {
    /// « Il manque {x} sur le disque pour écrire ce fichier. »
    pub missing: u64,
    /// « Il resterait {y} de libre. »
    pub free_after: u64,
}

/// Il restera, après l'écriture de `needed` octets sur un disque qui en a `free` de libres sur `total`, au moins
/// [`reserve`] octets libres ? Sinon, de quoi dire ce qui manque.
pub fn judge_space(free: u64, total: u64, needed: u64) -> Result<(), Shortfall> {
    let reserve = reserve(total);
    // Une taille qui dépasse tout disque possible ne tient jamais, même sur un disque annoncé sans fond.
    let Some(required) = needed.checked_add(reserve) else {
        return Err(Shortfall {
            missing: u64::MAX,
            free_after: free.saturating_sub(needed),
        });
    };
    if free >= required {
        Ok(())
    } else {
        Err(Shortfall {
            missing: required - free,
            free_after: free.saturating_sub(needed),
        })
    }
}

/// Pourquoi un transfert de plus est refusé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    /// Quatre transferts en cours pour l'agent.
    Total,
    /// Deux transferts en cours pour ce compte.
    Account,
    /// Huit dépôts ouverts non terminés pour ce compte.
    Open,
}

/// Un transfert de plus : `active_*` comptent les sockets de transfert ouverts, `open_account` les dépôts de ce
/// compte non consommés (reprenables ou complets).
pub fn admit(active_total: usize, active_account: usize, open_account: usize) -> Result<(), Limit> {
    if active_total >= MAX_ACTIVE_TRANSFERS {
        return Err(Limit::Total);
    }
    if active_account >= MAX_ACTIVE_PER_ACCOUNT {
        return Err(Limit::Account);
    }
    if open_account >= MAX_OPEN_PER_ACCOUNT {
        return Err(Limit::Open);
    }
    Ok(())
}

/// Pourquoi un morceau reçu n'est pas accepté.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkRefusal {
    /// Ce n'est pas le prochain morceau, ou sa taille n'est pas celle que ce rang demande : le client reprend à
    /// `expected`.
    Sequence { expected: u64 },
    /// L'empreinte annoncée n'est pas celle des octets reçus : rien n'est écrit, le client renvoie ce morceau.
    Corrupt { expected: u64 },
}

/// Pourquoi la fin d'un envoi est refusée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinishRefusal {
    /// Des morceaux manquent.
    Incomplete,
    /// L'empreinte de la suite des empreintes n'est pas celle que le client annonce.
    Mismatch,
}

/// La machine d'état d'un envoi : les empreintes des morceaux déjà reçus ET écrits, dans l'ordre. Se reprend de la
/// liste gardée à côté de la partie reçue, sans relire le fichier.
#[derive(Debug, Clone)]
pub struct Receiver {
    size: u64,
    digests: Vec<[u8; DIGEST_LEN]>,
}

impl Receiver {
    /// Un envoi de `size` octets (au moins 1) qui reprend avec les empreintes déjà reçues.
    pub fn resume(size: u64, digests: Vec<[u8; DIGEST_LEN]>) -> Option<Self> {
        let count = chunk_count(size);
        (size > 0 && u64::try_from(digests.len()).ok()? <= count).then_some(Self { size, digests })
    }

    pub fn new(size: u64) -> Option<Self> {
        Self::resume(size, Vec::new())
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    /// Le rang du prochain morceau attendu.
    pub fn next_index(&self) -> u64 {
        self.digests.len() as u64
    }

    /// Les octets reçus et écrits : le prochain morceau commence ici.
    pub fn received_bytes(&self) -> u64 {
        self.next_index()
            .saturating_mul(u64::from(CHUNK_BYTES))
            .min(self.size)
    }

    /// Les octets qu'il reste à écrire.
    pub fn remaining_bytes(&self) -> u64 {
        self.size - self.received_bytes()
    }

    pub fn is_complete(&self) -> bool {
        self.next_index() == chunk_count(self.size)
    }

    /// La taille exacte du prochain morceau (`0` quand tout est reçu).
    pub fn next_len(&self) -> u64 {
        chunk_len(self.size, self.next_index())
    }

    /// Les empreintes reçues, dans l'ordre.
    pub fn digests(&self) -> &[[u8; DIGEST_LEN]] {
        &self.digests
    }

    /// Juge un morceau reçu AVANT toute écriture : le rang, la taille, puis l'empreinte (`actual` est celle que
    /// l'agent vient de calculer sur les octets reçus, `claimed` celle de l'en-tête).
    pub fn judge(
        &self,
        index: u64,
        len: usize,
        claimed: &[u8; DIGEST_LEN],
        actual: &[u8; DIGEST_LEN],
    ) -> Result<(), ChunkRefusal> {
        let expected = self.next_index();
        if index != expected || self.is_complete() || u64::try_from(len) != Ok(self.next_len()) {
            return Err(ChunkRefusal::Sequence { expected });
        }
        if claimed != actual {
            return Err(ChunkRefusal::Corrupt { expected });
        }
        Ok(())
    }

    /// Le morceau est écrit : on retient son empreinte.
    pub fn accept(&mut self, digest: [u8; DIGEST_LEN]) {
        self.digests.push(digest);
    }

    /// L'empreinte de la suite des empreintes, si tout est reçu.
    pub fn tree_digest(&self) -> Option<[u8; DIGEST_LEN]> {
        self.is_complete().then(|| {
            let mut hasher = TreeHasher::new();
            for digest in &self.digests {
                hasher.push(digest);
            }
            hasher.finish()
        })
    }

    /// Juge la fin annoncée par le client : tout reçu, et la même empreinte d'ensemble.
    pub fn finish(&self, claimed: &[u8; DIGEST_LEN]) -> Result<[u8; DIGEST_LEN], FinishRefusal> {
        let ours = self.tree_digest().ok_or(FinishRefusal::Incomplete)?;
        if &ours == claimed {
            Ok(ours)
        } else {
            Err(FinishRefusal::Mismatch)
        }
    }
}

#[cfg(test)]
mod tests;

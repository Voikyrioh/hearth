//! Les transferts reprenables : recevoir un fichier de toute taille dans la zone d'attente de l'agent, le reprendre
//! après une coupure, le rendre à qui l'attend (HRT-95, ADR-0048, BR-FILES-013, BR-FILES-019, BR-MOD-015).
//!
//! **Un dépôt** : `open_deposit` juge la demande SANS rien attendre (bornes, espace libre pour toute la taille),
//! crée la partie `<id>.part` et sa liste d'empreintes, inscrit la ligne `receiving`. Le client s'attache ensuite
//! par un socket (`attach`) et envoie des morceaux de 4 Mio : chacun est vérifié (rang, taille, empreinte), écrit et
//! rendu durable AVANT d'être accusé ; l'agent ne garde qu'un morceau en mémoire par transfert, et rien du tout
//! pendant qu'un client est lent ou muet. À la fin, l'empreinte de la suite des empreintes est comparée, la partie
//! est posée sous son nom définitif d'un seul geste (jamais visible sous ce nom tant qu'elle est incomplète).
//!
//! **La reprise est toujours demandée** (jamais seule) : un nouvel `attach` relit le DISQUE (la liste d'empreintes et
//! la longueur de la partie), recoupe ce qui n'a pas été rendu durable et annonce l'offset durable. Un nouvel `attach`
//! prend la place d'un ancien socket resté ouvert (il attend la fin du morceau en cours d'écriture).
//!
//! **Vingt-quatre heures** : sans reprise ni consommateur, la partie reçue (ou le fichier complet) est retirée, ainsi
//! que sa ligne ; c'est le seul fichier que l'agent retire de la zone d'attente, toujours inscrit au registre, toujours
//! par son inode (ADR-0045).
//!
//! **Le consommateur** (lecture d'un pack, d'une archive de mods ou d'un monde) passe par [`DepositZone`] : `take`
//! rend le fichier complet à lecture seule (jamais un chemin), `release` le retire quand le consommateur a fini.
//!
//! **Jamais** : un nom de fichier du client rangé ou tracé, un octet de contenu dans une trace ou une erreur, un
//! chemin du serveur dans une erreur.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use bytes::Bytes;
use hearth_proto::api::transfers::{
    CHUNK_BYTES, DIGEST_LEN, RESUME_WINDOW_S, WINDOW_CHUNKS, chunk_count, digest_of,
};
use thiserror::Error;
use time::OffsetDateTime;
use tokio::sync::Mutex as AsyncMutex;

use super::ports::{
    BackupFsError, BackupRemoved, Clock, Expected, FileIdent, FileStamp, IdGen, NewTransfer,
    ReadSeek, StoreError, TransferFiles, TransferRepo, TransferRow, TransferState,
};
use crate::domain::transfers::{
    ChunkRefusal, FinishRefusal, Limit, Receiver, Shortfall, TransferId, admit, judge_name,
    judge_space,
};

/// Les délais des transferts ; les tests les raccourcissent.
#[derive(Debug, Clone, Copy)]
pub struct TransferTimings {
    /// La reprise reste possible aussi longtemps (24 heures).
    pub resume_window: Duration,
    /// Période du balayage des dépôts expirés.
    pub purge_period: Duration,
}

impl TransferTimings {
    pub fn production() -> Self {
        Self {
            resume_window: Duration::from_secs(RESUME_WINDOW_S),
            purge_period: Duration::from_secs(10 * 60),
        }
    }
}

/// Pourquoi un geste de transfert est refusé.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TransferError {
    /// La zone d'attente n'est pas utilisable (plate-forme sans inode, racine qui n'est pas un vrai dossier).
    #[error("la zone d'attente n'est pas disponible")]
    Unavailable,
    /// Le nom ou la taille annoncés ne sont pas valables (`field` : `name` ou `size`).
    #[error("demande invalide")]
    Invalid { field: &'static str },
    /// Le disque n'a pas la place (BR-FILES-013).
    #[error("espace insuffisant")]
    NoSpace(Shortfall),
    /// Une borne de transferts est atteinte.
    #[error("trop de transferts")]
    Limit(Limit),
    /// Aucun transfert de ce compte sous cet identifiant, ou son fichier a disparu.
    #[error("transfert inconnu")]
    NotFound,
    /// Plus rien à reprendre : 24 heures sans activité.
    #[error("transfert expiré")]
    Expired,
    /// Le registre ou le disque a répondu par une erreur (jamais détaillée au client).
    #[error("stockage indisponible")]
    Storage,
}

impl From<StoreError> for TransferError {
    fn from(error: StoreError) -> Self {
        tracing::warn!(%error, "registre des transferts indisponible");
        Self::Storage
    }
}

fn disk(error: BackupFsError) -> TransferError {
    match error {
        BackupFsError::Unsupported | BackupFsError::Unsafe => TransferError::Unavailable,
        BackupFsError::NotFound => TransferError::NotFound,
        other => {
            tracing::warn!(?other, "zone d'attente : geste impossible");
            TransferError::Storage
        }
    }
}

/// Un dépôt ouvert.
#[derive(Debug, Clone)]
pub struct Opened {
    pub id: TransferId,
    pub expires_at: OffsetDateTime,
}

/// L'état d'un dépôt, tel que son propriétaire le lit.
#[derive(Debug, Clone)]
pub struct DepositStatus {
    pub id: TransferId,
    pub complete: bool,
    pub size: u64,
    pub received_bytes: u64,
    pub expires_at: OffsetDateTime,
    pub tree_digest: Option<[u8; DIGEST_LEN]>,
}

/// Ce qu'un `attach` trouve.
pub enum Attached {
    /// Des morceaux manquent : le socket peut envoyer.
    Receiving(Box<UploadSession>),
    /// Tout est déjà reçu et vérifié.
    Complete {
        size: u64,
        tree_digest: [u8; DIGEST_LEN],
    },
}

/// Pourquoi un morceau n'est pas accepté.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChunkError {
    /// Hors d'ordre, de mauvaise taille : reprendre à `expected`.
    Sequence { expected: u64 },
    /// L'empreinte annoncée n'est pas celle des octets reçus : rien n'est écrit.
    Corrupt { expected: u64 },
    /// Le disque n'a plus la place de la suite.
    NoSpace(Shortfall),
    /// Un autre socket a repris ce transfert.
    Superseded,
    /// Le dépôt a disparu (abandonné, expiré).
    Gone,
    /// L'écriture a échoué.
    Write,
}

/// Pourquoi la fin d'un envoi est refusée.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinishError {
    Incomplete,
    Mismatch,
    Superseded,
    Gone,
    Write,
}

/// La fin d'un envoi réussi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Done {
    pub size: u64,
    pub tree_digest: [u8; DIGEST_LEN],
}

/// Ce que le consommateur lit : le fichier complet.
pub struct CompleteDeposit {
    pub id: TransferId,
    pub size: u64,
    pub tree_digest: [u8; DIGEST_LEN],
    /// Lecture seule et déplaçable, jamais un chemin : une archive se juge par son index de fin.
    pub reader: Box<dyn ReadSeek>,
}

/// Ce que le consommateur d'un dépôt (pack de mods, archive de mods, archive d'un monde) demande à la zone d'attente.
#[async_trait::async_trait]
pub trait DepositZone: Send + Sync {
    /// Le dépôt COMPLET de ce compte, ouvert en lecture. `NotFound` : inconnu, d'un autre compte, ou pas encore
    /// complet (un autre compte ne consomme jamais le dépôt de celui-ci). `Expired` : plus rien.
    async fn take(&self, id: &TransferId, account: &str) -> Result<CompleteDeposit, TransferError>;

    /// Le consommateur a fini (rangé ou refusé) : retire le fichier et sa ligne. Le dépôt doit être à ce compte (un autre
    /// compte : `NotFound`, rien n'est retiré) ; un dépôt déjà parti n'est pas une erreur.
    async fn release(&self, id: &TransferId, account: &str) -> Result<(), TransferError>;
}

/// Qui tient un socket, et la place qu'il prend. Rendue à la chute.
struct Active {
    total: usize,
    per_account: HashMap<String, usize>,
}

struct ActiveGuard {
    transfers: Arc<Transfers>,
    account: String,
}

impl Drop for ActiveGuard {
    fn drop(&mut self) {
        let mut active = self
            .transfers
            .active
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        active.total = active.total.saturating_sub(1);
        if let Some(count) = active.per_account.get_mut(&self.account) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                active.per_account.remove(&self.account);
            }
        }
    }
}

/// Le verrou d'UN transfert : un seul morceau s'écrit à la fois, et un socket plus récent écarte l'ancien.
#[derive(Default)]
struct TransferLock {
    generation: AtomicU64,
    mutex: AsyncMutex<()>,
}

pub struct Transfers {
    repo: Arc<dyn TransferRepo>,
    files: Arc<dyn TransferFiles>,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGen>,
    timings: TransferTimings,
    active: Mutex<Active>,
    locks: Mutex<HashMap<String, Arc<TransferLock>>>,
}

fn expiry(updated: OffsetDateTime, window: Duration) -> OffsetDateTime {
    updated + window
}

impl Transfers {
    pub fn new(
        repo: Arc<dyn TransferRepo>,
        files: Arc<dyn TransferFiles>,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGen>,
        timings: TransferTimings,
    ) -> Arc<Self> {
        Arc::new(Self {
            repo,
            files,
            clock,
            ids,
            timings,
            active: Mutex::new(Active {
                total: 0,
                per_account: HashMap::new(),
            }),
            locks: Mutex::new(HashMap::new()),
        })
    }

    fn lock_of(&self, id: &TransferId) -> Arc<TransferLock> {
        self.locks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(id.as_str().to_owned())
            .or_default()
            .clone()
    }

    fn forget_lock(&self, id: &TransferId) {
        self.locks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(id.as_str());
    }

    /// Les places de socket (quatre pour l'agent, deux par compte) ; `open_account` : dépôts ouverts du compte.
    fn take_place(
        self: &Arc<Self>,
        account: &str,
        open_account: usize,
    ) -> Result<ActiveGuard, Limit> {
        let mut active = self.active.lock().unwrap_or_else(PoisonError::into_inner);
        let own = active.per_account.get(account).copied().unwrap_or(0);
        admit(active.total, own, open_account)?;
        active.total += 1;
        *active.per_account.entry(account.to_owned()).or_insert(0) += 1;
        Ok(ActiveGuard {
            transfers: self.clone(),
            account: account.to_owned(),
        })
    }

    /// Ouvre un dépôt de `size` octets (au moins 1). `name` est jugé (BR-FILES-005) puis oublié : l'agent ne garde
    /// aucun nom de fichier du client.
    pub async fn open_deposit(
        self: &Arc<Self>,
        account: &str,
        name: &str,
        size: u64,
    ) -> Result<Opened, TransferError> {
        if !judge_name(name) {
            return Err(TransferError::Invalid { field: "name" });
        }
        if size == 0 {
            return Err(TransferError::Invalid { field: "size" });
        }
        let open = self.repo.count_open(account).await?;
        // Les dépôts non terminés d'un compte sont bornés (la zone d'attente n'est pas un entrepôt) ; les places de
        // socket se prennent à l'attache, un dépôt s'ouvre pendant qu'un autre s'envoie.
        admit(0, 0, open).map_err(TransferError::Limit)?;
        // L'espace pour TOUTE la taille, avant de créer quoi que ce soit (BR-FILES-013). Le disque se touche sur un fil
        // bloquant, jamais sur un worker de l'exécuteur.
        let id = TransferId::parse(&self.ids.new_id()).ok_or(TransferError::Storage)?;
        let files = self.files.clone();
        let created = id.clone();
        let (space, ident) = tokio::task::spawn_blocking(move || {
            files.ensure_root()?;
            let space = files.space()?;
            // Pas la place : rien n'est créé.
            if judge_space(space.free, space.total, size).is_err() {
                return Ok((space, None));
            }
            files.create(&created).map(|ident| (space, Some(ident)))
        })
        .await
        .map_err(|_| TransferError::Storage)?
        .map_err(disk)?;
        let Some(ident) = ident else {
            return Err(judge_space(space.free, space.total, size)
                .err()
                .map_or(TransferError::Storage, TransferError::NoSpace));
        };
        let now = self.clock.now();
        if let Err(error) = self
            .repo
            .insert(&NewTransfer {
                id: id.clone(),
                account: account.to_owned(),
                size,
                dev: ident.0,
                ino: ident.1,
                now,
            })
            .await
        {
            // Rien n'est resté : les fichiers neufs sont retirés (ils sont à nous, d'inode connu).
            let files = self.files.clone();
            let created = id.clone();
            let _ = tokio::task::spawn_blocking(move || {
                let _ = files.remove_part(&created, Some(ident));
                let _ = files.remove_digests(&created);
            })
            .await;
            return Err(error.into());
        }
        Ok(Opened {
            id,
            expires_at: expiry(now, self.timings.resume_window),
        })
    }

    async fn row_of(&self, id: &TransferId, account: &str) -> Result<TransferRow, TransferError> {
        let row = self.repo.get(id).await?.ok_or(TransferError::NotFound)?;
        // Un autre compte ne voit pas un dépôt : il n'existe pas pour lui.
        if row.account != account {
            return Err(TransferError::NotFound);
        }
        Ok(row)
    }

    fn is_expired(&self, row: &TransferRow) -> bool {
        self.clock.now() > expiry(row.updated_at, self.timings.resume_window)
    }

    /// L'état d'un dépôt de ce compte.
    pub async fn status(
        &self,
        id: &TransferId,
        account: &str,
    ) -> Result<DepositStatus, TransferError> {
        let row = self.row_of(id, account).await?;
        if self.is_expired(&row) {
            self.discard(&row).await;
            return Err(TransferError::Expired);
        }
        let complete = row.state == TransferState::Complete;
        let received = if complete {
            row.size
        } else {
            row.received_chunks
                .saturating_mul(u64::from(CHUNK_BYTES))
                .min(row.size)
        };
        Ok(DepositStatus {
            id: row.id.clone(),
            complete,
            size: row.size,
            received_bytes: received,
            expires_at: expiry(row.updated_at, self.timings.resume_window),
            tree_digest: row.tree_digest,
        })
    }

    /// Abandonne un dépôt de ce compte : retire la partie reçue et la ligne. `false` : il n'y avait plus rien.
    pub async fn abandon(&self, id: &TransferId, account: &str) -> Result<bool, TransferError> {
        let row = match self.row_of(id, account).await {
            Ok(row) => row,
            Err(TransferError::NotFound) => return Ok(false),
            Err(other) => return Err(other),
        };
        // Un socket qui envoie encore est écarté, puis attendu : son morceau en cours finit d'abord.
        let lock = self.lock_of(id);
        lock.generation.fetch_add(1, Ordering::SeqCst);
        let _writing = lock.mutex.lock().await;
        self.discard(&row).await;
        Ok(true)
    }

    /// La partie manque : si `<id>.bin` est le fichier du registre, de la taille annoncée, avec toutes ses empreintes,
    /// l'envoi était fini ; la ligne passe `complete`. Sinon il n'y a plus rien : la ligne et ses restes partent.
    async fn recover_published(
        self: &Arc<Self>,
        row: &TransferRow,
    ) -> Result<Attached, TransferError> {
        let files = self.files.clone();
        let (id, ident, size) = (row.id.clone(), (row.dev, row.ino), row.size);
        let found = tokio::task::spawn_blocking(move || {
            files.load_complete(&id, ident, size, chunk_count(size))
        })
        .await
        .map_err(|_| TransferError::Storage)?;
        let (stamp, digests) = match found {
            Ok(found) => found,
            // Le fichier n'est plus là, ou n'est plus celui du registre : il n'y a plus rien à terminer.
            Err(BackupFsError::NotFound | BackupFsError::Unsafe) => {
                self.discard(row).await;
                return Err(TransferError::NotFound);
            }
            // Une erreur d'entrée-sortie passagère ne jette jamais un fichier complet : l'essai suivant relira.
            Err(error) => return Err(disk(error)),
        };
        let Some(tree_digest) = Receiver::resume(size, digests).and_then(|r| r.tree_digest())
        else {
            self.discard(row).await;
            return Err(TransferError::NotFound);
        };
        if !self
            .repo
            .complete(&row.id, &tree_digest, stamp.mtime_ns, self.clock.now())
            .await?
        {
            return Err(TransferError::NotFound);
        }
        tracing::info!("zone d'attente : un envoi publié avant une coupure est terminé");
        Ok(Attached::Complete { size, tree_digest })
    }

    /// Retire les fichiers d'une ligne (par inode) puis la ligne. Un fichier qui n'est pas celui du registre n'est
    /// pas touché ; la ligne part quand même (elle ne désigne plus rien qui soit à Hearth).
    async fn discard(&self, row: &TransferRow) {
        let ident: FileIdent = (row.dev, row.ino);
        let files = self.files.clone();
        let id = row.id.clone();
        let complete = (row.state == TransferState::Complete).then_some(row.complete_mtime_ns);
        let size = row.size;
        let outcome = tokio::task::spawn_blocking(move || {
            let part = files.remove_part(&id, Some(ident));
            let digests = files.remove_digests(&id);
            // Le fichier publié a l'inode de la partie ; une fois complet, sa taille et sa date le prouvent aussi.
            let published = files.remove_complete(
                &id,
                match complete {
                    Some(mtime_ns) => Expected::stamp(FileStamp {
                        dev: ident.0,
                        ino: ident.1,
                        size,
                        mtime_ns,
                    }),
                    None => Expected::inode(ident),
                },
            );
            (part, digests, published)
        })
        .await;
        match outcome {
            Ok((part, digests, published)) => {
                for (what, result) in [("part", part), ("dig", digests), ("bin", published)] {
                    match result {
                        Ok(BackupRemoved::Done | BackupRemoved::Gone) => {}
                        Ok(BackupRemoved::NotOurs) => {
                            tracing::warn!(
                                what,
                                "zone d'attente : fichier qui n'est pas le nôtre, laissé"
                            );
                        }
                        Err(error) => {
                            tracing::warn!(what, ?error, "zone d'attente : retrait impossible");
                        }
                    }
                }
            }
            Err(error) => tracing::warn!(%error, "zone d'attente : retrait interrompu"),
        }
        if let Err(error) = self.repo.delete(&row.id).await {
            tracing::warn!(%error, "registre des transferts : ligne non retirée");
        }
        self.forget_lock(&row.id);
    }

    /// Retire les dépôts sans activité depuis plus de 24 heures (partie reçue ou fichier complet non consommé).
    /// Rend leur nombre. Appelé au démarrage et périodiquement.
    pub async fn purge_expired(&self) -> usize {
        let before = self.clock.now() - self.timings.resume_window;
        let mut removed = 0;
        loop {
            let rows = match self.repo.expired(before, 100).await {
                Ok(rows) => rows,
                Err(error) => {
                    tracing::warn!(%error, "purge des transferts impossible");
                    return removed;
                }
            };
            if rows.is_empty() {
                return removed;
            }
            let mut gone = 0;
            for row in &rows {
                // Un socket qui tient ce transfert l'a rafraîchi : s'il est encore ici, c'est qu'il est mort.
                let lock = self.lock_of(&row.id);
                lock.generation.fetch_add(1, Ordering::SeqCst);
                let _writing = lock.mutex.lock().await;
                self.discard(row).await;
                // Une ligne qui ne part pas (base en lecture seule) ne fait pas tourner la purge en boucle.
                if matches!(self.repo.get(&row.id).await, Ok(None)) {
                    gone += 1;
                }
            }
            removed += gone;
            if rows.len() < 100 || gone == 0 {
                return removed;
            }
        }
    }

    /// Attache un socket à un dépôt de ce compte : relit le DISQUE, recoupe ce qui n'est pas durable, annonce
    /// l'offset.
    pub async fn attach(
        self: &Arc<Self>,
        id: &TransferId,
        account: &str,
    ) -> Result<Attached, TransferError> {
        let row = self.row_of(id, account).await?;
        if self.is_expired(&row) {
            self.discard(&row).await;
            return Err(TransferError::Expired);
        }
        if row.state == TransferState::Complete {
            let tree_digest = row.tree_digest.ok_or(TransferError::Storage)?;
            return Ok(Attached::Complete {
                size: row.size,
                tree_digest,
            });
        }
        let guard = self.take_place(account, 0).map_err(TransferError::Limit)?;
        let attached = self.attach_locked(id, account, guard).await;
        if attached.is_err() {
            // Le verrou pris pour cet essai ne reste pas dans la table si rien n'a été attaché : vérifié ET retiré
            // sous la MÊME prise de la table (sinon un attach concurrent pourrait en voir un autre recréé).
            let mut locks = self.locks.lock().unwrap_or_else(PoisonError::into_inner);
            if locks
                .get(id.as_str())
                .is_some_and(|lock| Arc::strong_count(lock) == 1)
            {
                locks.remove(id.as_str());
            }
        }
        attached
    }

    async fn attach_locked(
        self: &Arc<Self>,
        id: &TransferId,
        account: &str,
        guard: ActiveGuard,
    ) -> Result<Attached, TransferError> {
        // Le plus récent gagne : l'ancien socket est écarté, et son morceau en cours finit d'abord.
        let lock = self.lock_of(id);
        let generation = lock.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let _writing = lock.mutex.lock().await;
        // Relu sous le verrou : le dépôt a pu être abandonné ou terminé pendant l'attente.
        let row = self.row_of(id, account).await?;
        if row.state == TransferState::Complete {
            let tree_digest = row.tree_digest.ok_or(TransferError::Storage)?;
            return Ok(Attached::Complete {
                size: row.size,
                tree_digest,
            });
        }
        let files = self.files.clone();
        let (loaded_id, ident, size) = (id.clone(), (row.dev, row.ino), row.size);
        let loaded =
            tokio::task::spawn_blocking(move || files.load(&loaded_id, ident, chunk_count(size)))
                .await
                .map_err(|_| TransferError::Storage)?;
        let loaded = match loaded {
            Ok(loaded) => loaded,
            // La partie n'est plus là : l'agent a pu s'arrêter ENTRE la publication de `<id>.bin` et l'écriture de la
            // ligne `complete`. Un fichier complet, vérifié, du registre et de la bonne taille n'est jamais jeté.
            Err(BackupFsError::NotFound) => return self.recover_published(&row).await,
            // Ce n'est plus le fichier du registre : il n'y a plus rien à reprendre.
            Err(BackupFsError::Unsafe) => {
                self.discard(&row).await;
                return Err(TransferError::NotFound);
            }
            Err(error) => return Err(disk(error)),
        };
        let mut digests = loaded.digests;
        // Un morceau n'est compté que si ses octets sont VRAIMENT sur le disque.
        while !digests.is_empty()
            && loaded.part_len
                < (digests.len() as u64)
                    .saturating_mul(u64::from(CHUNK_BYTES))
                    .min(size)
        {
            digests.pop();
        }
        let receiver = Receiver::resume(size, digests).ok_or(TransferError::Storage)?;
        let now = self.clock.now();
        if !self
            .repo
            .note_progress(id, receiver.next_index(), now)
            .await?
        {
            return Err(TransferError::NotFound);
        }
        Ok(Attached::Receiving(Box::new(UploadSession {
            transfers: self.clone(),
            id: id.clone(),
            ident,
            lock: lock.clone(),
            generation,
            receiver,
            _place: guard,
        })))
    }
}

/// Un socket d'envoi attaché à un dépôt.
pub struct UploadSession {
    transfers: Arc<Transfers>,
    id: TransferId,
    ident: FileIdent,
    lock: Arc<TransferLock>,
    generation: u64,
    receiver: Receiver,
    _place: ActiveGuard,
}

impl UploadSession {
    /// Les octets durables : reprendre ici.
    pub fn offset(&self) -> u64 {
        self.receiver.received_bytes()
    }

    pub fn window(&self) -> u32 {
        WINDOW_CHUNKS
    }

    pub fn size(&self) -> u64 {
        self.receiver.size()
    }

    pub fn next_index(&self) -> u64 {
        self.receiver.next_index()
    }

    pub fn is_complete(&self) -> bool {
        self.receiver.is_complete()
    }

    /// Vérifie puis écrit le morceau de rang `index`, et le rend durable AVANT de rendre la main (l'accusé suit).
    /// Jamais plus d'un morceau en mémoire : `data` est le seul tampon.
    pub async fn put_chunk(
        &mut self,
        index: u64,
        claimed: [u8; DIGEST_LEN],
        data: Bytes,
    ) -> Result<u64, ChunkError> {
        let lock = self.lock.clone();
        let _writing = lock.mutex.lock().await;
        if lock.generation.load(Ordering::SeqCst) != self.generation {
            return Err(ChunkError::Superseded);
        }
        let len = data.len();
        let hashed = data.clone();
        let actual = tokio::task::spawn_blocking(move || digest_of(&hashed))
            .await
            .map_err(|_| ChunkError::Write)?;
        self.receiver
            .judge(index, len, &claimed, &actual)
            .map_err(|refusal| match refusal {
                ChunkRefusal::Sequence { expected } => ChunkError::Sequence { expected },
                ChunkRefusal::Corrupt { expected } => ChunkError::Corrupt { expected },
            })?;
        // BR-FILES-013 : la place de CE morceau, refaite à chaque morceau (un envoi est long).
        let space = self
            .transfers
            .files
            .space()
            .map_err(|_| ChunkError::Write)?;
        judge_space(space.free, space.total, len as u64).map_err(ChunkError::NoSpace)?;
        let files = self.transfers.files.clone();
        let (id, ident, offset) = (self.id.clone(), self.ident, self.receiver.received_bytes());
        tokio::task::spawn_blocking(move || {
            files.append(&id, ident, index, offset, &data, &actual)
        })
        .await
        .map_err(|_| ChunkError::Write)?
        .map_err(|error| {
            tracing::warn!(?error, "zone d'attente : morceau non écrit");
            ChunkError::Write
        })?;
        self.receiver.accept(actual);
        let now = self.transfers.clock.now();
        match self
            .transfers
            .repo
            .note_progress(&self.id, self.receiver.next_index(), now)
            .await
        {
            Ok(true) => Ok(self.receiver.received_bytes()),
            // Abandonné pendant l'écriture : plus rien n'est à garder.
            Ok(false) => Err(ChunkError::Gone),
            Err(error) => {
                tracing::warn!(%error, "registre des transferts indisponible");
                Err(ChunkError::Write)
            }
        }
    }

    /// Termine l'envoi : tout reçu, même empreinte d'ensemble, puis la partie est posée sous son nom définitif.
    pub async fn finish(&mut self, claimed: [u8; DIGEST_LEN]) -> Result<Done, FinishError> {
        let lock = self.lock.clone();
        let _writing = lock.mutex.lock().await;
        if lock.generation.load(Ordering::SeqCst) != self.generation {
            return Err(FinishError::Superseded);
        }
        let tree_digest = self
            .receiver
            .finish(&claimed)
            .map_err(|refusal| match refusal {
                FinishRefusal::Incomplete => FinishError::Incomplete,
                FinishRefusal::Mismatch => FinishError::Mismatch,
            })?;
        let files = self.transfers.files.clone();
        let (id, ident, size) = (self.id.clone(), self.ident, self.receiver.size());
        let stamp = tokio::task::spawn_blocking(move || files.publish(&id, ident, size))
            .await
            .map_err(|_| FinishError::Write)?
            .map_err(|error| {
                tracing::warn!(?error, "zone d'attente : publication impossible");
                FinishError::Write
            })?;
        let now = self.transfers.clock.now();
        match self
            .transfers
            .repo
            .complete(&self.id, &tree_digest, stamp.mtime_ns, now)
            .await
        {
            Ok(true) => Ok(Done { size, tree_digest }),
            Ok(false) => {
                // Abandonné pendant la publication : le fichier posé est retiré, il n'a plus de ligne.
                let _ = self
                    .transfers
                    .files
                    .remove_complete(&self.id, Expected::stamp(stamp));
                Err(FinishError::Gone)
            }
            Err(error) => {
                tracing::warn!(%error, "registre des transferts indisponible");
                Err(FinishError::Write)
            }
        }
    }
}

#[async_trait::async_trait]
impl DepositZone for Transfers {
    async fn take(&self, id: &TransferId, account: &str) -> Result<CompleteDeposit, TransferError> {
        let row = self.row_of(id, account).await?;
        if self.is_expired(&row) {
            self.discard(&row).await;
            return Err(TransferError::Expired);
        }
        if row.state != TransferState::Complete {
            return Err(TransferError::NotFound);
        }
        let tree_digest = row.tree_digest.ok_or(TransferError::Storage)?;
        let stamp = FileStamp {
            dev: row.dev,
            ino: row.ino,
            size: row.size,
            mtime_ns: row.complete_mtime_ns,
        };
        let reader = self
            .files
            .open_complete(id, stamp)
            .map_err(|error| match error {
                // Le fichier n'est plus celui que Hearth a posé : jamais lu.
                BackupFsError::Unsafe | BackupFsError::NotFound => TransferError::NotFound,
                other => disk(other),
            })?;
        Ok(CompleteDeposit {
            id: row.id,
            size: row.size,
            tree_digest,
            reader,
        })
    }

    async fn release(&self, id: &TransferId, account: &str) -> Result<(), TransferError> {
        if let Some(row) = self.repo.get(id).await? {
            if row.account != account {
                return Err(TransferError::NotFound);
            }
            let lock = self.lock_of(id);
            lock.generation.fetch_add(1, Ordering::SeqCst);
            let _writing = lock.mutex.lock().await;
            self.discard(&row).await;
        }
        Ok(())
    }
}

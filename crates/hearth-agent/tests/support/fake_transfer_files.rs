#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]
//! Faux `TransferFiles` en mémoire (HRT-95) : mêmes règles visibles que le vrai (`infrastructure/transfer_files.rs`),
//! sans système de fichiers. Chaque fichier a un numéro d'inode ; un fichier « remplacé à la main » en reçoit un neuf.
//!
//! Deux modes : par défaut, les octets sont GARDÉS (pour relire ce qui est arrivé) ; en mode « compte seulement »
//! (`counting_only`), ils sont comptés puis jetés : un fichier de plusieurs Gio traverse l'agent sans que le test ne
//! le garde en mémoire. Dans les deux cas le fake note la plus grosse écriture et la taille de la partie.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use hearth_agent::application::ports::{
    BackupFsError, BackupRemoved, DiskSpace, Expected, FileIdent, FileStamp, Loaded, ReadSeek,
    TransferFiles,
};
use hearth_agent::domain::transfers::TransferId;

const DEV: u64 = 64_769;

#[derive(Debug, Clone)]
struct Node {
    ino: u64,
    mtime_ns: i64,
    /// Les octets, ou seulement leur nombre en mode « compte seulement ».
    bytes: Vec<u8>,
    len: u64,
}

#[derive(Debug, Default)]
struct State {
    /// Nom de fichier -> contenu.
    files: BTreeMap<String, Node>,
    space: Option<DiskSpace>,
    counting_only: bool,
    biggest_write: usize,
    /// Chaque geste d'écriture, dans l'ordre.
    writes: Vec<String>,
    /// Après ce nombre d'appels à `space`, l'espace libre tombe à ce qui est donné (le disque se remplit).
    fill_after: Option<(u32, DiskSpace)>,
    space_calls: u32,
    root_refused: bool,
    /// `append` échoue (disque en défaut).
    write_fails: bool,
}

#[derive(Debug, Clone)]
pub struct FakeTransferFiles {
    state: Arc<Mutex<State>>,
    next_ino: Arc<AtomicU64>,
    clock: Arc<AtomicU64>,
}

impl Default for FakeTransferFiles {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeTransferFiles {
    pub fn new() -> Self {
        let fake = Self {
            state: Arc::new(Mutex::new(State::default())),
            next_ino: Arc::new(AtomicU64::new(5000)),
            clock: Arc::new(AtomicU64::new(1_700_000_000_000_000_000)),
        };
        fake.set_space(DiskSpace {
            free: 500 * 1024 * 1024 * 1024,
            total: 1000 * 1024 * 1024 * 1024,
        });
        fake
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn tick(&self) -> i64 {
        i64::try_from(self.clock.fetch_add(1_000_000, Ordering::SeqCst)).expect("date")
    }

    pub fn set_space(&self, space: DiskSpace) {
        self.lock().space = Some(space);
    }

    /// Le disque se remplit : après `calls` mesures de l'espace, il ne reste que `then`.
    pub fn fill_after(&self, calls: u32, then: DiskSpace) {
        let mut state = self.lock();
        state.space_calls = 0;
        state.fill_after = Some((calls, then));
    }

    pub fn counting_only(&self) {
        self.lock().counting_only = true;
    }

    pub fn refuse_root(&self) {
        self.lock().root_refused = true;
    }

    pub fn fail_writes(&self, fail: bool) {
        self.lock().write_fails = fail;
    }

    /// La plus grosse écriture d'un seul morceau vue jusqu'ici (jamais plus qu'un morceau).
    pub fn biggest_write(&self) -> usize {
        self.lock().biggest_write
    }

    pub fn writes(&self) -> Vec<String> {
        self.lock().writes.clone()
    }

    /// Les noms de fichiers présents (la zone d'attente d'un test ne contient que ce que l'agent y a posé).
    pub fn names(&self) -> Vec<String> {
        self.lock().files.keys().cloned().collect()
    }

    pub fn exists(&self, name: &str) -> bool {
        self.lock().files.contains_key(name)
    }

    /// Les octets d'un fichier gardé.
    pub fn bytes_of(&self, name: &str) -> Option<Vec<u8>> {
        self.lock().files.get(name).map(|node| node.bytes.clone())
    }

    /// La taille d'un fichier, gardé ou compté.
    pub fn len_of(&self, name: &str) -> Option<u64> {
        self.lock().files.get(name).map(|node| node.len)
    }

    /// « Remplacé à la main » : le fichier a un contenu étranger et un autre inode.
    pub fn replace_by_hand(&self, name: &str, bytes: &[u8]) {
        let ino = self.next_ino.fetch_add(1, Ordering::SeqCst);
        let mtime_ns = self.tick();
        self.lock().files.insert(
            name.to_owned(),
            Node {
                ino,
                mtime_ns,
                bytes: bytes.to_vec(),
                len: bytes.len() as u64,
            },
        );
    }

    /// Une partie à moitié écrite par un arrêt : des octets de plus que la liste d'empreintes n'en dit.
    pub fn append_garbage(&self, name: &str, bytes: &[u8]) {
        let mut state = self.lock();
        let node = state.files.get_mut(name).expect("fichier");
        node.bytes.extend_from_slice(bytes);
        node.len += bytes.len() as u64;
    }

    /// Une partie qui a perdu ses derniers octets (disque en défaut).
    pub fn truncate(&self, name: &str, len: u64) {
        let mut state = self.lock();
        let node = state.files.get_mut(name).expect("fichier");
        node.bytes.truncate(usize::try_from(len).unwrap());
        node.len = len;
    }

    pub fn ident_of(&self, name: &str) -> Option<FileIdent> {
        self.lock().files.get(name).map(|node| (DEV, node.ino))
    }
}

fn ident(node: &Node) -> FileIdent {
    (DEV, node.ino)
}

fn stamp(node: &Node) -> FileStamp {
    FileStamp {
        dev: DEV,
        ino: node.ino,
        size: node.len,
        mtime_ns: node.mtime_ns,
    }
}

impl TransferFiles for FakeTransferFiles {
    fn ensure_root(&self) -> Result<(), BackupFsError> {
        if self.lock().root_refused {
            return Err(BackupFsError::Unsafe);
        }
        Ok(())
    }

    fn space(&self) -> Result<DiskSpace, BackupFsError> {
        let mut state = self.lock();
        state.space_calls += 1;
        if let Some((after, then)) = state.fill_after
            && state.space_calls > after
        {
            return Ok(then);
        }
        state.space.ok_or(BackupFsError::Unsupported)
    }

    fn create(&self, id: &TransferId) -> Result<FileIdent, BackupFsError> {
        self.ensure_root()?;
        let mtime_ns = self.tick();
        let ino = self.next_ino.fetch_add(1, Ordering::SeqCst);
        let mut state = self.lock();
        let (part, digests) = (id.part_name(), id.digests_name());
        if state.files.contains_key(&part) || state.files.contains_key(&digests) {
            return Err(BackupFsError::Exists);
        }
        let node = |ino| Node {
            ino,
            mtime_ns,
            bytes: Vec::new(),
            len: 0,
        };
        state.files.insert(part.clone(), node(ino));
        let digest_ino = ino + 1_000_000;
        state.files.insert(digests, node(digest_ino));
        state.writes.push(format!("create {id}"));
        Ok((DEV, ino))
    }

    fn load(
        &self,
        id: &TransferId,
        expect: FileIdent,
        max_digests: u64,
    ) -> Result<Loaded, BackupFsError> {
        let state = self.lock();
        let part = state
            .files
            .get(&id.part_name())
            .ok_or(BackupFsError::NotFound)?;
        if ident(part) != expect {
            return Err(BackupFsError::Unsafe);
        }
        let digests = state
            .files
            .get(&id.digests_name())
            .ok_or(BackupFsError::NotFound)?;
        let whole = digests.bytes.len() / 32;
        let take = usize::try_from(max_digests)
            .unwrap_or(usize::MAX)
            .min(whole);
        let digests = (0..take)
            .map(|i| {
                let mut d = [0u8; 32];
                d.copy_from_slice(&digests.bytes[i * 32..(i + 1) * 32]);
                d
            })
            .collect();
        Ok(Loaded {
            part_len: part.len,
            digests,
        })
    }

    fn load_complete(
        &self,
        id: &TransferId,
        expect: FileIdent,
        size: u64,
        max_digests: u64,
    ) -> Result<(FileStamp, Vec<[u8; 32]>), BackupFsError> {
        let state = self.lock();
        let file = state
            .files
            .get(&id.complete_name())
            .ok_or(BackupFsError::NotFound)?;
        if ident(file) != expect || file.len != size {
            return Err(BackupFsError::Unsafe);
        }
        let list = state
            .files
            .get(&id.digests_name())
            .ok_or(BackupFsError::NotFound)?;
        let take = usize::try_from(max_digests)
            .unwrap_or(usize::MAX)
            .min(list.bytes.len() / 32);
        let digests = (0..take)
            .map(|i| {
                let mut d = [0u8; 32];
                d.copy_from_slice(&list.bytes[i * 32..(i + 1) * 32]);
                d
            })
            .collect();
        Ok((stamp(file), digests))
    }

    fn append(
        &self,
        id: &TransferId,
        expect: FileIdent,
        index: u64,
        offset: u64,
        data: &[u8],
        digest: &[u8; 32],
    ) -> Result<(), BackupFsError> {
        let mtime_ns = self.tick();
        let mut state = self.lock();
        if state.write_fails {
            return Err(BackupFsError::Io(std::io::ErrorKind::Other));
        }
        state.biggest_write = state.biggest_write.max(data.len());
        let counting_only = state.counting_only;
        let part = state
            .files
            .get_mut(&id.part_name())
            .ok_or(BackupFsError::NotFound)?;
        if ident(part) != expect {
            return Err(BackupFsError::Unsafe);
        }
        if part.len < offset {
            return Err(BackupFsError::Unsafe);
        }
        part.len = offset + data.len() as u64;
        part.mtime_ns = mtime_ns;
        if counting_only {
            part.bytes.clear();
        } else {
            part.bytes.truncate(usize::try_from(offset).unwrap());
            part.bytes.extend_from_slice(data);
        }
        let at = usize::try_from(index).unwrap() * 32;
        let list = state
            .files
            .get_mut(&id.digests_name())
            .ok_or(BackupFsError::NotFound)?;
        list.bytes.truncate(at);
        list.bytes.extend_from_slice(digest);
        state.writes.push(format!("append {id} {index}"));
        Ok(())
    }

    fn publish(
        &self,
        id: &TransferId,
        expect: FileIdent,
        size: u64,
    ) -> Result<FileStamp, BackupFsError> {
        let mut state = self.lock();
        let part = state
            .files
            .get(&id.part_name())
            .ok_or(BackupFsError::NotFound)?;
        if ident(part) != expect || part.len != size {
            return Err(BackupFsError::Unsafe);
        }
        let name = id.complete_name();
        if state.files.contains_key(&name) {
            return Err(BackupFsError::Exists);
        }
        let node = state.files.remove(&id.part_name()).expect("partie");
        let published = stamp(&node);
        state.files.insert(name, node);
        state.writes.push(format!("publish {id}"));
        Ok(published)
    }

    fn open_complete(
        &self,
        id: &TransferId,
        expect: FileStamp,
    ) -> Result<Box<dyn ReadSeek>, BackupFsError> {
        let state = self.lock();
        let node = state
            .files
            .get(&id.complete_name())
            .ok_or(BackupFsError::NotFound)?;
        if stamp(node) != expect {
            return Err(BackupFsError::Unsafe);
        }
        Ok(Box::new(Cursor::new(node.bytes.clone())))
    }

    fn remove_part(
        &self,
        id: &TransferId,
        expect: Option<FileIdent>,
    ) -> Result<BackupRemoved, BackupFsError> {
        self.remove(&id.part_name(), |node| {
            expect.is_none_or(|expected| ident(node) == expected)
        })
    }

    fn remove_digests(&self, id: &TransferId) -> Result<BackupRemoved, BackupFsError> {
        self.remove(&id.digests_name(), |_| true)
    }

    fn remove_complete(
        &self,
        id: &TransferId,
        expect: Expected,
    ) -> Result<BackupRemoved, BackupFsError> {
        self.remove(&id.complete_name(), |node| {
            let found = stamp(node);
            (found.dev, found.ino) == (expect.dev, expect.ino)
                && expect.size.is_none_or(|size| size == found.size)
                && expect.mtime_ns.is_none_or(|mtime| mtime == found.mtime_ns)
        })
    }
}

impl FakeTransferFiles {
    fn remove(
        &self,
        name: &str,
        ours: impl Fn(&Node) -> bool,
    ) -> Result<BackupRemoved, BackupFsError> {
        let mut state = self.lock();
        let Some(node) = state.files.get(name) else {
            return Ok(BackupRemoved::Gone);
        };
        if !ours(node) {
            return Ok(BackupRemoved::NotOurs);
        }
        state.files.remove(name);
        state.writes.push(format!("remove {name}"));
        Ok(BackupRemoved::Done)
    }
}

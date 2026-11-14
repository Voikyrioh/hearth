#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]
//! Faux `BackupFiles` en mémoire (HRT-97) : mêmes règles visibles que le vrai (suite `backup_files_contract.rs`),
//! sans système de fichiers. Chaque fichier a un numéro d'inode ; un fichier « déposé à la main » ou « remplacé à la
//! main » en reçoit un neuf, comme sur un disque. Sert aux tests des cas d'usage sur le poste de développement.

use std::collections::BTreeMap;
use std::io::{self, Cursor, Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use hearth_agent::application::ports::{
    BackupFiles, BackupFsError, BackupRemoved, BackupSink, DiskSpace, FileIdent,
};
use hearth_agent::domain::backups::ownership::Seen;

const DEV: u64 = 64_768;

#[derive(Debug, Clone)]
struct Node {
    ino: u64,
    bytes: Vec<u8>,
}

#[derive(Debug, Default)]
struct State {
    /// `(dossier du service, nom) -> fichier`.
    files: BTreeMap<(String, String), Node>,
    space: Option<DiskSpace>,
    /// Après ce nombre d'appels à `space`, l'espace libre tombe à ce qui est donné (le disque se remplit).
    fill_after: Option<(u32, DiskSpace)>,
    space_calls: u32,
    /// Chaque geste d'écriture, dans l'ordre (`create`, `publish`, `write_new`, `remove`).
    writes: Vec<String>,
    /// Un octet de l'archive est changé avant sa publication (la vérification doit le voir).
    corrupt_on_publish: bool,
    root_refused: bool,
}

#[derive(Debug, Clone)]
pub struct FakeBackupFiles {
    state: Arc<Mutex<State>>,
    next_ino: Arc<AtomicU64>,
}

impl Default for FakeBackupFiles {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeBackupFiles {
    pub fn new() -> Self {
        let fake = Self {
            state: Arc::new(Mutex::new(State::default())),
            next_ino: Arc::new(AtomicU64::new(1000)),
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

    fn ino(&self) -> u64 {
        self.next_ino.fetch_add(1, Ordering::SeqCst)
    }

    pub fn set_space(&self, space: DiskSpace) {
        self.lock().space = Some(space);
    }

    /// La mesure de l'espace échoue.
    pub fn space_unreadable(&self) {
        self.lock().space = None;
    }

    /// À partir du `calls`-ième appel à `space`, l'espace libre est `space`.
    pub fn fill_after(&self, calls: u32, space: DiskSpace) {
        self.lock().fill_after = Some((calls, space));
    }

    pub fn corrupt_on_publish(&self) {
        self.lock().corrupt_on_publish = true;
    }

    /// La racine n'est pas sûre (lien, autre propriétaire) : `ensure_root` refuse.
    pub fn refuse_root(&self) {
        self.lock().root_refused = true;
    }

    /// Un fichier déposé à la main : un inode neuf, jamais dans le registre.
    pub fn plant(&self, dir: &str, name: &str, bytes: &[u8]) -> FileIdent {
        let ino = self.ino();
        self.lock().files.insert(
            (dir.to_owned(), name.to_owned()),
            Node {
                ino,
                bytes: bytes.to_vec(),
            },
        );
        (DEV, ino)
    }

    /// Le fichier est remplacé à la main : même nom, autre inode, autre contenu.
    pub fn replace_by_hand(&self, dir: &str, name: &str, bytes: &[u8]) -> FileIdent {
        self.plant(dir, name, bytes)
    }

    /// Un octet du fichier est changé (même inode : le fichier de Hearth, altéré).
    pub fn flip_a_byte(&self, dir: &str, name: &str) {
        let mut state = self.lock();
        let node = state
            .files
            .get_mut(&(dir.to_owned(), name.to_owned()))
            .expect("fichier");
        let middle = node.bytes.len() / 2;
        node.bytes[middle] ^= 0xff;
    }

    pub fn remove_by_hand(&self, dir: &str, name: &str) {
        self.lock().files.remove(&(dir.to_owned(), name.to_owned()));
    }

    pub fn content(&self, dir: &str, name: &str) -> Option<Vec<u8>> {
        self.lock()
            .files
            .get(&(dir.to_owned(), name.to_owned()))
            .map(|node| node.bytes.clone())
    }

    pub fn ident(&self, dir: &str, name: &str) -> Option<FileIdent> {
        self.lock()
            .files
            .get(&(dir.to_owned(), name.to_owned()))
            .map(|node| (DEV, node.ino))
    }

    /// Tous les fichiers de la racine : dossier, nom, inode, contenu.
    pub fn all(&self) -> Vec<(String, String, u64, Vec<u8>)> {
        self.lock()
            .files
            .iter()
            .map(|((dir, name), node)| (dir.clone(), name.clone(), node.ino, node.bytes.clone()))
            .collect()
    }

    /// Les noms d'un dossier de service.
    pub fn names(&self, dir: &str) -> Vec<String> {
        self.lock()
            .files
            .keys()
            .filter(|(d, _)| d == dir)
            .map(|(_, name)| name.clone())
            .collect()
    }

    /// Les gestes d'écriture, dans l'ordre.
    pub fn writes(&self) -> Vec<String> {
        self.lock().writes.clone()
    }
}

struct MemSink {
    state: Arc<Mutex<State>>,
    key: (String, String),
}

impl Write for MemSink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(node) = state.files.get_mut(&self.key) {
            node.bytes.extend_from_slice(buf);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl BackupSink for MemSink {
    fn finish(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn component(name: &str) -> Result<(), BackupFsError> {
    if name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\\') {
        return Err(BackupFsError::Unsafe);
    }
    Ok(())
}

impl BackupFiles for FakeBackupFiles {
    fn ensure_root(&self) -> Result<(), BackupFsError> {
        if self.lock().root_refused {
            return Err(BackupFsError::Unsafe);
        }
        Ok(())
    }

    fn space(&self) -> Result<DiskSpace, BackupFsError> {
        let mut state = self.lock();
        state.space_calls += 1;
        if let Some((after, low)) = state.fill_after
            && state.space_calls > after
        {
            return Ok(low);
        }
        state.space.ok_or(BackupFsError::Io(io::ErrorKind::Other))
    }

    fn create(
        &self,
        service_dir: &str,
        name: &str,
    ) -> Result<(Box<dyn BackupSink>, FileIdent), BackupFsError> {
        component(service_dir)?;
        component(name)?;
        let ino = self.ino();
        let key = (service_dir.to_owned(), name.to_owned());
        let mut state = self.lock();
        if state.files.contains_key(&key) {
            return Err(BackupFsError::Exists);
        }
        state.files.insert(
            key.clone(),
            Node {
                ino,
                bytes: Vec::new(),
            },
        );
        state.writes.push(format!("create {name}"));
        drop(state);
        Ok((
            Box::new(MemSink {
                state: Arc::clone(&self.state),
                key,
            }),
            (DEV, ino),
        ))
    }

    fn publish(
        &self,
        service_dir: &str,
        part: &str,
        name: &str,
        expect: FileIdent,
    ) -> Result<(), BackupFsError> {
        component(service_dir)?;
        component(part)?;
        component(name)?;
        let mut state = self.lock();
        let from = (service_dir.to_owned(), part.to_owned());
        let to = (service_dir.to_owned(), name.to_owned());
        match state.files.get(&from) {
            None => return Err(BackupFsError::NotFound),
            Some(node) if (DEV, node.ino) != expect => return Err(BackupFsError::Unsafe),
            Some(_) => {}
        }
        if state.files.contains_key(&to) {
            return Err(BackupFsError::Exists);
        }
        let mut node = state.files.remove(&from).expect("présent");
        if state.corrupt_on_publish && !node.bytes.is_empty() {
            let middle = node.bytes.len() / 2;
            node.bytes[middle] ^= 0xff;
        }
        state.files.insert(to, node);
        state.writes.push(format!("publish {name}"));
        Ok(())
    }

    fn write_new(
        &self,
        service_dir: &str,
        name: &str,
        bytes: &[u8],
    ) -> Result<FileIdent, BackupFsError> {
        component(service_dir)?;
        component(name)?;
        let ino = self.ino();
        let key = (service_dir.to_owned(), name.to_owned());
        let mut state = self.lock();
        if state.files.contains_key(&key) {
            return Err(BackupFsError::Exists);
        }
        state.files.insert(
            key,
            Node {
                ino,
                bytes: bytes.to_vec(),
            },
        );
        state.writes.push(format!("write_new {name}"));
        Ok((DEV, ino))
    }

    fn seen(&self, service_dir: &str, name: &str) -> Result<Seen, BackupFsError> {
        component(service_dir)?;
        component(name)?;
        Ok(
            match self
                .lock()
                .files
                .get(&(service_dir.to_owned(), name.to_owned()))
            {
                Some(node) => Seen::File {
                    dev: DEV,
                    ino: node.ino,
                },
                None => Seen::Absent,
            },
        )
    }

    fn open(
        &self,
        service_dir: &str,
        name: &str,
    ) -> Result<(Box<dyn Read + Send>, FileIdent), BackupFsError> {
        component(service_dir)?;
        component(name)?;
        let state = self.lock();
        let node = state
            .files
            .get(&(service_dir.to_owned(), name.to_owned()))
            .ok_or(BackupFsError::NotFound)?;
        Ok((Box::new(Cursor::new(node.bytes.clone())), (DEV, node.ino)))
    }

    fn remove(
        &self,
        service_dir: &str,
        name: &str,
        expect: FileIdent,
    ) -> Result<BackupRemoved, BackupFsError> {
        component(service_dir)?;
        component(name)?;
        let key = (service_dir.to_owned(), name.to_owned());
        let mut state = self.lock();
        match state.files.get(&key) {
            None => Ok(BackupRemoved::Gone),
            Some(node) if (DEV, node.ino) != expect => Ok(BackupRemoved::NotOurs),
            Some(_) => {
                state.files.remove(&key);
                state.writes.push(format!("remove {name}"));
                Ok(BackupRemoved::Done)
            }
        }
    }

    fn remove_service_dir_if_empty(&self, _service_dir: &str) -> Result<(), BackupFsError> {
        Ok(())
    }
}

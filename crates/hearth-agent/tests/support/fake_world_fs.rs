//! Un faux `WorldFs` en mémoire (HRT-105) : des dossiers ouverts avec des fichiers, un dossier d'import qui
//! reçoit la copie, et de quoi suspendre la copie ou fausser ce qui est écrit. Ni disque ni Linux requis.

use std::collections::BTreeMap;
use std::io::{self, Cursor, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use hearth_agent::application::ports::{
    ImportDir, Ownership, SinkFile, WorldDirEntry, WorldDirListing, WorldEntryKind, WorldFileStat,
    WorldFs, WorldFsError, WorldSink, WorldSource,
};
use hearth_agent::domain::files::RelPath;
use hearth_agent::domain::services::ServiceKey;

type Files = BTreeMap<String, Vec<u8>>;

/// Ce qui a été mis en place : le dossier géré visé et ce qu'il contient.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    pub key: String,
    pub folder: String,
    pub files: Files,
    pub owner: (u32, u32),
}

#[derive(Default)]
pub struct FakeWorldFs {
    /// Dossier ouvert (texte de la configuration) -> chemin relatif -> contenu.
    roots: Mutex<BTreeMap<String, Files>>,
    /// Nom du dossier d'import -> contenu écrit.
    imports: Mutex<BTreeMap<String, Files>>,
    placed: Mutex<Vec<Placed>>,
    removed: Mutex<Vec<String>>,
    owners: Mutex<BTreeMap<String, (u32, u32)>>,
    /// Une copie qui ne finit pas tant que `hold_copy(false)` n'est pas dit : le test peut annuler ou concurrencer.
    hold: AtomicBool,
    started: AtomicBool,
    /// Altère le premier octet de tout ce qui est écrit : la relecture doit le voir.
    corrupt: AtomicBool,
    /// Fait changer la date de ce chemin relatif à chaque relevé : un fichier modifié pendant la copie.
    unstable: Mutex<Option<String>>,
    stats: AtomicU64,
    /// Chaque chemin relatif ouvert en lecture.
    opened: Mutex<Vec<String>>,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl FakeWorldFs {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn put(&self, root: &str, rel: &str, bytes: &[u8]) {
        lock(&self.roots)
            .entry(root.to_owned())
            .or_default()
            .insert(rel.to_owned(), bytes.to_vec());
    }

    pub fn root_snapshot(&self, root: &str) -> Files {
        lock(&self.roots).get(root).cloned().unwrap_or_default()
    }

    pub fn hold_copy(&self, hold: bool) {
        self.hold.store(hold, Ordering::SeqCst);
    }

    pub fn copy_started(&self) -> bool {
        self.started.load(Ordering::SeqCst)
    }

    pub fn corrupt_writes(&self, corrupt: bool) {
        self.corrupt.store(corrupt, Ordering::SeqCst);
    }

    pub fn make_unstable(&self, rel: &str) {
        *lock(&self.unstable) = Some(rel.to_owned());
    }

    pub fn placed(&self) -> Vec<Placed> {
        lock(&self.placed).clone()
    }

    pub fn removed(&self) -> Vec<String> {
        lock(&self.removed).clone()
    }

    pub fn opened(&self) -> Vec<String> {
        lock(&self.opened).clone()
    }

    /// Des dossiers d'import qui existent encore (ni mis en place ni retirés).
    pub fn leftover_imports(&self) -> Vec<String> {
        lock(&self.imports).keys().cloned().collect()
    }
}

struct Source {
    fs: Arc<FakeWorldFs>,
    root: String,
}

impl WorldSource for Source {
    fn list(&self, dir: &RelPath, limit: usize) -> Result<WorldDirListing, WorldFsError> {
        let roots = lock(&self.fs.roots);
        let files = roots.get(&self.root).ok_or(WorldFsError::NotFound)?;
        let prefix = if dir.is_root() {
            String::new()
        } else {
            format!("{dir}/")
        };
        let mut seen: BTreeMap<String, WorldEntryKind> = BTreeMap::new();
        for path in files.keys() {
            let Some(rest) = path.strip_prefix(&prefix) else {
                continue;
            };
            match rest.split_once('/') {
                Some((first, _)) => {
                    seen.insert(first.to_owned(), WorldEntryKind::Dir);
                }
                None => {
                    seen.insert(rest.to_owned(), WorldEntryKind::File);
                }
            }
        }
        let mut entries: Vec<WorldDirEntry> = seen
            .into_iter()
            .map(|(name, kind)| {
                let size = files
                    .get(&format!("{prefix}{name}"))
                    .map_or(0, |bytes| bytes.len() as u64);
                WorldDirEntry {
                    name,
                    kind,
                    size,
                    mtime_ns: 1_700_000_000_000_000_000,
                }
            })
            .collect();
        let truncated = entries.len() > limit;
        entries.truncate(limit);
        Ok(WorldDirListing { entries, truncated })
    }

    fn stat(&self, file: &RelPath) -> Result<WorldFileStat, WorldFsError> {
        let roots = lock(&self.fs.roots);
        let bytes = roots
            .get(&self.root)
            .and_then(|files| files.get(&file.to_string()))
            .ok_or(WorldFsError::NotFound)?;
        let mut mtime_ns = 1_700_000_000_000_000_000;
        if lock(&self.fs.unstable).as_deref() == Some(file.to_string().as_str()) {
            mtime_ns += i128::from(self.fs.stats.fetch_add(1, Ordering::SeqCst)) + 1;
        }
        Ok(WorldFileStat {
            size: bytes.len() as u64,
            mtime_ns,
        })
    }

    fn open(&self, file: &RelPath) -> Result<Box<dyn Read + Send>, WorldFsError> {
        let bytes = lock(&self.fs.roots)
            .get(&self.root)
            .and_then(|files| files.get(&file.to_string()))
            .cloned()
            .ok_or(WorldFsError::NotFound)?;
        lock(&self.fs.opened).push(file.to_string());
        self.fs.started.store(true, Ordering::SeqCst);
        Ok(Box::new(Held {
            fs: Arc::clone(&self.fs),
            inner: Cursor::new(bytes),
        }))
    }
}

/// Un lecteur qui attend que le test lâche la copie.
struct Held {
    fs: Arc<FakeWorldFs>,
    inner: Cursor<Vec<u8>>,
}

impl Read for Held {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        while self.fs.hold.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(10));
        }
        self.inner.read(buf)
    }
}

struct Sink {
    fs: Arc<FakeWorldFs>,
    name: String,
}

struct Writer {
    fs: Arc<FakeWorldFs>,
    import: String,
    path: String,
    buf: Vec<u8>,
}

impl Write for Writer {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.buf.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl SinkFile for Writer {
    fn finish(mut self: Box<Self>) -> io::Result<()> {
        if self.fs.corrupt.load(Ordering::SeqCst)
            && let Some(first) = self.buf.first_mut()
        {
            *first ^= 0xFF;
        }
        lock(&self.fs.imports)
            .entry(self.import.clone())
            .or_default()
            .insert(self.path.clone(), std::mem::take(&mut self.buf));
        Ok(())
    }
}

impl WorldSink for Sink {
    fn mkdir(&self, _dir: &RelPath) -> Result<(), WorldFsError> {
        Ok(())
    }

    fn create(&self, file: &RelPath) -> Result<Box<dyn SinkFile>, WorldFsError> {
        Ok(Box::new(Writer {
            fs: Arc::clone(&self.fs),
            import: self.name.clone(),
            path: file.to_string(),
            buf: Vec::new(),
        }))
    }

    fn open(&self, file: &RelPath) -> Result<Box<dyn Read + Send>, WorldFsError> {
        let bytes = lock(&self.fs.imports)
            .get(&self.name)
            .and_then(|files| files.get(&file.to_string()))
            .cloned()
            .ok_or(WorldFsError::NotFound)?;
        Ok(Box::new(Cursor::new(bytes)))
    }
}

/// `WorldFs` sur une poignée partagée : le test garde la sienne, l'agent reçoit la même.
pub struct Handle(pub Arc<FakeWorldFs>);

impl WorldFs for Handle {
    fn open_source(&self, root: &Path) -> Result<Box<dyn WorldSource>, WorldFsError> {
        let text = root.to_string_lossy().into_owned();
        if !lock(&self.0.roots).contains_key(&text) {
            return Err(WorldFsError::NotFound);
        }
        Ok(Box::new(Source {
            fs: Arc::clone(&self.0),
            root: text,
        }))
    }

    fn create_import(
        &self,
        op: &str,
        owner: Ownership,
    ) -> Result<(Box<dyn WorldSink>, ImportDir), WorldFsError> {
        let name = format!("import-{op}");
        lock(&self.0.imports).insert(name.clone(), Files::new());
        lock(&self.0.owners).insert(name.clone(), (owner.uid(), owner.gid()));
        Ok((
            Box::new(Sink {
                fs: Arc::clone(&self.0),
                name: name.clone(),
            }),
            ImportDir {
                rel: name,
                dev: 1,
                ino: 42,
            },
        ))
    }

    fn place(
        &self,
        import: &ImportDir,
        key: &ServiceKey,
        folder: &str,
    ) -> Result<(), WorldFsError> {
        let files = lock(&self.0.imports)
            .remove(&import.rel)
            .ok_or(WorldFsError::NotFound)?;
        let owner = lock(&self.0.owners)
            .get(&import.rel)
            .copied()
            .unwrap_or((0, 0));
        lock(&self.0.placed).push(Placed {
            key: key.as_str().to_owned(),
            folder: folder.to_owned(),
            files,
            owner,
        });
        Ok(())
    }

    fn remove_import(&self, import: &ImportDir) -> Result<(), WorldFsError> {
        lock(&self.0.imports).remove(&import.rel);
        lock(&self.0.removed).push(import.rel.clone());
        Ok(())
    }
}

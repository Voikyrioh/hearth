#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]
//! Faux serveur hôte pour les tests du déploiement (HRT-68) : les sockets en écoute, et l'espace libre du disque
//! où le moteur range ses images. Aucun test ne lit `/proc` ni ne lance `df`.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

use hearth_agent::application::ports::{FreeSpace, HostPorts, UpdateHostError};
use hearth_agent::domain::services::ports::Listener;
use hearth_proto::api::deploy::DeployProtocol;

#[derive(Default)]
pub struct FakeHostPorts {
    listeners: Mutex<Vec<(u16, DeployProtocol, Option<String>)>>,
    unreadable: AtomicBool,
}

impl FakeHostPorts {
    pub fn new() -> Self {
        Self::default()
    }

    /// Un programme du serveur écoute ce port.
    pub fn listen(&self, port: u16, protocol: DeployProtocol, program: &str) {
        self.listeners
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((port, protocol, Some(program.to_owned())));
    }

    /// La liste des sockets ne se lit pas.
    pub fn set_unreadable(&self, unreadable: bool) {
        self.unreadable.store(unreadable, Ordering::SeqCst);
    }
}

impl HostPorts for FakeHostPorts {
    fn listening(&self) -> Option<Vec<Listener>> {
        if self.unreadable.load(Ordering::SeqCst) {
            return None;
        }
        Some(
            self.listeners
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .iter()
                .map(|(port, protocol, _)| Listener {
                    port: *port,
                    protocol: *protocol,
                    program: None,
                })
                .collect(),
        )
    }

    fn program_of(&self, port: u16, protocol: DeployProtocol) -> Option<String> {
        self.listeners
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .find(|(p, proto, _)| *p == port && *proto == protocol)
            .and_then(|(_, _, program)| program.clone())
    }
}

/// L'espace libre : beaucoup par défaut.
pub struct FakeFreeSpace {
    bytes: AtomicU64,
    failing: AtomicBool,
    /// Un autre disque sous ce préfixe de chemin (la racine de données, par exemple).
    under: Mutex<Vec<(String, u64)>>,
}

impl FakeFreeSpace {
    pub fn new() -> Self {
        Self {
            bytes: AtomicU64::new(500 * 1024 * 1024 * 1024),
            failing: AtomicBool::new(false),
            under: Mutex::new(Vec::new()),
        }
    }

    /// Les chemins sous `prefix` ont `bytes` octets libres (un autre disque que celui des images).
    pub fn set_under(&self, prefix: &str, bytes: u64) {
        self.under
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((prefix.to_owned(), bytes));
    }

    pub fn set(&self, bytes: u64) {
        self.bytes.store(bytes, Ordering::SeqCst);
    }

    pub fn fail(&self, failing: bool) {
        self.failing.store(failing, Ordering::SeqCst);
    }
}

impl Default for FakeFreeSpace {
    fn default() -> Self {
        Self::new()
    }
}

impl FreeSpace for FakeFreeSpace {
    fn free_bytes(&self, path: &Path) -> Result<u64, UpdateHostError> {
        if self.failing.load(Ordering::SeqCst) {
            return Err(UpdateHostError::Other("illisible".into()));
        }
        let wanted = path.to_string_lossy();
        if let Some((_, bytes)) = self
            .under
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .find(|(prefix, _)| wanted.starts_with(prefix.as_str()))
        {
            return Ok(*bytes);
        }
        Ok(self.bytes.load(Ordering::SeqCst))
    }
}

/// Les chemins de l'hôte sans le plafond de lectures de production : `canonicalize` et `stat` du chemin et de ses
/// parents, dans le fil de l'appelant (FIX:01M855R2F0XJ3KDBH325R1NR2R : le vrai lecteur partage huit places entre tous
/// les tests d'un même processus). Les tests qui visent le vrai lecteur règlent `host_paths` eux-mêmes.
#[derive(Debug, Clone, Copy, Default)]
pub struct FakeHostPaths;

impl hearth_agent::application::ports::HostPaths for FakeHostPaths {
    fn resolve(&self, path: &str) -> hearth_agent::application::ports::Resolved {
        use hearth_agent::application::ports::Resolved;
        let canonical = match std::fs::canonicalize(Path::new(path)) {
            Ok(canonical) => canonical,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Resolved::Absent,
            Err(_) => return Resolved::Unreadable,
        };
        let mut lineage = Vec::new();
        for ancestor in canonical.ancestors() {
            match std::fs::metadata(ancestor) {
                Ok(meta) => lineage.extend(file_id(&meta)),
                Err(_) => return Resolved::Unreadable,
            }
        }
        let Some(text) = canonical.to_str() else {
            return Resolved::Unreadable;
        };
        let canonical = text.strip_prefix(r#"\\?\"#).unwrap_or(text).to_owned();
        Resolved::Found { canonical, lineage }
    }
}

#[cfg(unix)]
fn file_id(meta: &std::fs::Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    Some((meta.dev(), meta.ino()))
}

#[cfg(not(unix))]
fn file_id(_meta: &std::fs::Metadata) -> Option<(u64, u64)> {
    None
}

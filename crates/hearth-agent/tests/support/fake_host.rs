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
}

impl FakeFreeSpace {
    pub fn new() -> Self {
        Self {
            bytes: AtomicU64::new(500 * 1024 * 1024 * 1024),
            failing: AtomicBool::new(false),
        }
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
    fn free_bytes(&self, _path: &Path) -> Result<u64, UpdateHostError> {
        if self.failing.load(Ordering::SeqCst) {
            return Err(UpdateHostError::Other("illisible".into()));
        }
        Ok(self.bytes.load(Ordering::SeqCst))
    }
}

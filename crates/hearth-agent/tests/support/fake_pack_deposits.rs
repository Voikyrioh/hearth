//! Une zone d'attente de packs en mémoire (HRT-124) et un fabricant d'archives zip « stockées » : aucun fichier
//! d'archive n'est versionné, chaque test construit la sienne octet par octet.
//!
//! `place` écrit le pack dans le dossier géré du service par la faute `FakeSafeFs` (comme le fait l'adaptateur réel
//! par `SafeFs`) après avoir revérifié l'empreinte jugée.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::sync::{Arc, Mutex, PoisonError};

use flate2::Crc;
use hearth_agent::application::ports::{
    DepositError, DepositFile, DepositId, Ownership, PackDeposits, Perms, SafeFs,
};
use hearth_agent::domain::files::RelPath;
use hearth_agent::domain::services::ServiceKey;
use sha2::{Digest, Sha256};

use super::fake_safe_fs::FakeSafeFs;

#[derive(Default)]
struct Inner {
    /// (compte, identifiant) vers les octets du dépôt complet.
    files: BTreeMap<(String, String), Vec<u8>>,
    discarded: Vec<String>,
}

pub struct FakePackDeposits {
    inner: Mutex<Inner>,
    safe_fs: Arc<FakeSafeFs>,
}

impl FakePackDeposits {
    pub fn new(safe_fs: Arc<FakeSafeFs>) -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
            safe_fs,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn put(&self, account: &str, id: &str, bytes: Vec<u8>) {
        self.lock().files.insert((account.into(), id.into()), bytes);
    }

    pub fn has(&self, account: &str, id: &str) -> bool {
        self.lock().files.contains_key(&(account.into(), id.into()))
    }

    pub fn discarded(&self) -> Vec<String> {
        self.lock().discarded.clone()
    }

    /// Remplace les octets d'un dépôt après son jugement (une course : le fichier a changé).
    pub fn swap(&self, account: &str, id: &str, bytes: Vec<u8>) {
        self.put(account, id, bytes);
    }
}

impl PackDeposits for FakePackDeposits {
    fn open(&self, id: &DepositId, account: &str) -> Result<Box<dyn DepositFile>, DepositError> {
        let inner = self.lock();
        let bytes = inner
            .files
            .get(&(account.to_owned(), id.as_str().to_owned()))
            .ok_or(DepositError::NotFound)?;
        Ok(Box::new(Cursor::new(bytes.clone())))
    }

    fn place(
        &self,
        id: &DepositId,
        account: &str,
        service: &ServiceKey,
        rel: &RelPath,
        sha256: &[u8; 32],
    ) -> Result<(), DepositError> {
        let key = (account.to_owned(), id.as_str().to_owned());
        let bytes = self
            .lock()
            .files
            .get(&key)
            .cloned()
            .ok_or(DepositError::NotFound)?;
        let digest: [u8; 32] = Sha256::digest(&bytes).into();
        if &digest != sha256 {
            return Err(DepositError::Changed);
        }
        let root = self
            .safe_fs
            .ensure_service_root(service)
            .map_err(|_| DepositError::Io)?;
        let perms = Perms::new(Ownership::new(0, 0).expect("propriétaire"), 0o644).expect("droits");
        root.write_atomic(rel, &bytes, perms)
            .map_err(|_| DepositError::Io)?;
        self.lock().files.remove(&key);
        Ok(())
    }

    fn discard(&self, id: &DepositId, account: &str) {
        let mut inner = self.lock();
        inner
            .files
            .remove(&(account.to_owned(), id.as_str().to_owned()));
        inner.discarded.push(id.as_str().to_owned());
    }
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = Crc::new();
    crc.update(data);
    crc.sum()
}

fn u16le(out: &mut Vec<u8>, v: u16) {
    out.extend(v.to_le_bytes());
}

fn u32le(out: &mut Vec<u8>, v: u32) {
    out.extend(v.to_le_bytes());
}

/// Une archive zip dont les entrées sont stockées sans compression, avec des noms et des octets donnés.
pub fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut offsets = Vec::new();
    for (name, data) in entries {
        offsets.push(out.len() as u32);
        u32le(&mut out, 0x0403_4b50);
        u16le(&mut out, 20);
        u16le(&mut out, 0);
        u16le(&mut out, 0);
        u16le(&mut out, 0);
        u16le(&mut out, 0x21);
        u32le(&mut out, crc32(data));
        u32le(&mut out, data.len() as u32);
        u32le(&mut out, data.len() as u32);
        u16le(&mut out, name.len() as u16);
        u16le(&mut out, 0);
        out.extend(name.as_bytes());
        out.extend(*data);
    }
    let index_start = out.len() as u32;
    for ((name, data), offset) in entries.iter().zip(&offsets) {
        u32le(&mut out, 0x0201_4b50);
        u16le(&mut out, 0x031E);
        u16le(&mut out, 20);
        u16le(&mut out, 0);
        u16le(&mut out, 0);
        u16le(&mut out, 0);
        u16le(&mut out, 0x21);
        u32le(&mut out, crc32(data));
        u32le(&mut out, data.len() as u32);
        u32le(&mut out, data.len() as u32);
        u16le(&mut out, name.len() as u16);
        u16le(&mut out, 0);
        u16le(&mut out, 0);
        u16le(&mut out, 0);
        u16le(&mut out, 0);
        u32le(&mut out, 0o100_644 << 16);
        u32le(&mut out, *offset);
        out.extend(name.as_bytes());
    }
    let index_size = out.len() as u32 - index_start;
    u32le(&mut out, 0x0605_4b50);
    u16le(&mut out, 0);
    u16le(&mut out, 0);
    u16le(&mut out, entries.len() as u16);
    u16le(&mut out, entries.len() as u16);
    u32le(&mut out, index_size);
    u32le(&mut out, index_start);
    u16le(&mut out, 0);
    out
}

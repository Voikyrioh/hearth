//! Adaptateur du port `PackDeposits` sur la zone d'attente des transferts (HRT-124 sur HRT-95, ADR-0048, ADR-0054).
//!
//! Le pack arrive par un transfert reprenable ; ce que l'agent en lit est le fichier COMPLET d'un transfert COMPLET du
//! compte qui installe (`DepositZone::take` refuse le dépôt d'un autre compte, un dépôt incomplet ou périmé). Le
//! rangement copie en flux dans le dossier géré du service par `ServiceRoot::write_stream` (nom d'attente puis
//! renommage sans remplacement, empreinte revérifiée avant le renommage) puis libère le dépôt. Les trois gestes sont
//! BLOQUANTS : l'appelant les exécute hors du fil asynchrone, ce qui permet de rejoindre les appels asynchrones de la
//! zone par `Handle::block_on`.

use std::io::{self, Read, Seek, SeekFrom};
use std::sync::Arc;

use tokio::runtime::Handle;

use crate::application::pack_intake::MAX_PACK_BYTES;
use crate::application::ports::{
    DepositError, DepositFile, DepositId, FsError, Ownership, PackDeposits, Perms, ReadSeek, SafeFs,
};
use crate::application::transfers::{DepositZone, TransferError};
use crate::domain::files::RelPath;
use crate::domain::services::ServiceKey;
use crate::domain::transfers::TransferId;

/// Droits du pack rangé : lisible par tous (le conteneur le monte en lecture seule sous son propre compte), écrit par
/// l'agent seul.
const PACK_MODE: u32 = 0o644;

pub struct ZonePackDeposits {
    zone: Arc<dyn DepositZone>,
    fs: Arc<dyn SafeFs>,
    runtime: Handle,
}

impl ZonePackDeposits {
    pub fn new(zone: Arc<dyn DepositZone>, fs: Arc<dyn SafeFs>, runtime: Handle) -> Self {
        Self { zone, fs, runtime }
    }

    fn take(
        &self,
        id: &DepositId,
        account: &str,
    ) -> Result<(TransferId, Box<dyn ReadSeek>), DepositError> {
        let transfer = TransferId::parse(id.as_str()).ok_or(DepositError::NotFound)?;
        match self.runtime.block_on(self.zone.take(&transfer, account)) {
            Ok(deposit) => Ok((transfer, deposit.reader)),
            Err(TransferError::NotFound | TransferError::Expired) => Err(DepositError::NotFound),
            Err(_) => Err(DepositError::Io),
        }
    }
}

/// Le lecteur du dépôt, vu comme le fichier d'un pack.
struct Reader(Box<dyn ReadSeek>);

impl Read for Reader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.0.read(buffer)
    }
}

impl Seek for Reader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.0.seek(position)
    }
}

impl PackDeposits for ZonePackDeposits {
    fn open(&self, id: &DepositId, account: &str) -> Result<Box<dyn DepositFile>, DepositError> {
        let (_, reader) = self.take(id, account)?;
        Ok(Box::new(Reader(reader)))
    }

    fn place(
        &self,
        id: &DepositId,
        account: &str,
        service: &ServiceKey,
        rel: &RelPath,
        sha256: &[u8; 32],
    ) -> Result<(), DepositError> {
        let (transfer, mut reader) = self.take(id, account)?;
        let root = self
            .fs
            .ensure_service_root(service)
            .map_err(|_| DepositError::Io)?;
        let perms = Ownership::new(0, 0)
            .and_then(|owner| Perms::new(owner, PACK_MODE))
            .ok_or(DepositError::Io)?;
        match root.write_stream(rel, &mut reader, perms, MAX_PACK_BYTES, sha256) {
            Ok(_) => {}
            // Ce qui a été lu n'a pas l'empreinte jugée : le fichier a changé, rien n'est rangé.
            Err(FsError::Io(io::ErrorKind::InvalidData)) => return Err(DepositError::Changed),
            Err(_) => return Err(DepositError::Io),
        }
        // Le pack est rangé : le dépôt est consommé. Un échec du retrait n'annule pas le rangement (la purge de
        // 24 heures de la zone d'attente le reprendra).
        let _ = self.runtime.block_on(self.zone.release(&transfer, account));
        Ok(())
    }

    fn discard(&self, id: &DepositId, account: &str) {
        if let Some(transfer) = TransferId::parse(id.as_str()) {
            let _ = self.runtime.block_on(self.zone.release(&transfer, account));
        }
    }
}

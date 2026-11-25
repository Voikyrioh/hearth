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
    AccountRepo, DepositError, DepositFile, DepositId, FsError, PackDeposits, Perms, ReadSeek,
    SafeFs,
};
use crate::application::transfers::{DepositZone, TransferError};
use crate::domain::accounts::Username;
use crate::domain::files::RelPath;
use crate::domain::services::ServiceKey;
use crate::domain::transfers::TransferId;

/// Droits du pack rangé : lisible par tous (le conteneur le monte en lecture seule sous son propre compte), écrit par
/// l'agent seul. Posés par l'appelant (`PackDeposits::place`) ; ici pour qui range un pack.
pub const PACK_MODE: u32 = 0o644;

pub struct ZonePackDeposits {
    zone: Arc<dyn DepositZone>,
    accounts: Arc<dyn AccountRepo>,
    fs: Arc<dyn SafeFs>,
    runtime: Handle,
}

impl ZonePackDeposits {
    pub fn new(
        zone: Arc<dyn DepositZone>,
        accounts: Arc<dyn AccountRepo>,
        fs: Arc<dyn SafeFs>,
        runtime: Handle,
    ) -> Self {
        Self {
            zone,
            accounts,
            fs,
            runtime,
        }
    }

    // FIX:01M7G6V7E6DY5PHJ6TQ848YJ7A — la zone range les dépôts sous l'IDENTIFIANT du compte, l'appelant donne son NOM (docs/bugs/FIX-01M7G6V7E6DY5PHJ6TQ848YJ7A.md)
    /// L'identifiant technique du compte dont l'appelant donne le nom : la clé sous laquelle la zone d'attente range
    /// ses dépôts (`POST /deposits` les ouvre au nom de `session.account.id`). Un nom inconnu : aucun dépôt.
    fn owner(&self, account: &str) -> Result<String, DepositError> {
        let username = Username::parse(account).map_err(|_| DepositError::NotFound)?;
        match self
            .runtime
            .block_on(self.accounts.find_by_username(&username))
        {
            Ok(Some(found)) => Ok(found.id.as_str().to_owned()),
            Ok(None) => Err(DepositError::NotFound),
            Err(_) => Err(DepositError::Io),
        }
    }

    fn take(
        &self,
        id: &DepositId,
        account: &str,
    ) -> Result<(TransferId, Box<dyn ReadSeek>), DepositError> {
        let transfer = TransferId::parse(id.as_str()).ok_or(DepositError::NotFound)?;
        let owner = self.owner(account)?;
        match self.runtime.block_on(self.zone.take(&transfer, &owner)) {
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
        perms: Perms,
    ) -> Result<(), DepositError> {
        let (transfer, mut reader) = self.take(id, account)?;
        let root = self
            .fs
            .ensure_service_root(service)
            .map_err(|_| DepositError::Io)?;
        match root.write_stream(rel, &mut reader, perms, MAX_PACK_BYTES, sha256) {
            Ok(_) => {}
            // Ce qui a été lu n'a pas l'empreinte jugée : le fichier a changé, rien n'est rangé.
            Err(FsError::Io(io::ErrorKind::InvalidData)) => return Err(DepositError::Changed),
            Err(_) => return Err(DepositError::Io),
        }
        // Le pack est rangé : le dépôt est consommé. Un échec du retrait n'annule pas le rangement (la purge de
        // 24 heures de la zone d'attente le reprendra).
        if let Ok(owner) = self.owner(account) {
            let _ = self.runtime.block_on(self.zone.release(&transfer, &owner));
        }
        Ok(())
    }

    fn discard(&self, id: &DepositId, account: &str) {
        if let Some(transfer) = TransferId::parse(id.as_str())
            && let Ok(owner) = self.owner(account)
        {
            let _ = self.runtime.block_on(self.zone.release(&transfer, &owner));
        }
    }
}

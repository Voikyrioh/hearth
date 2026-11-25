//! HRT-124 sur HRT-95 : l'adaptateur `ZonePackDeposits` lit un pack dans la zone d'attente des transferts, le range
//! par le dossier géré du service (empreinte revérifiée) et libère le dépôt. Zone et disque sont des faux.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::io::{Cursor, Read};
use std::sync::{Arc, Mutex};

use hearth_agent::application::ports::{
    AccountRepo, DepositError, DepositId, PackDeposits, StoreError,
};
use hearth_agent::application::transfers::{CompleteDeposit, DepositZone, TransferError};
use hearth_agent::domain::accounts::{Account, AccountId, Role, ServicesRight, Username};
use hearth_agent::domain::files::RelPath;
use hearth_agent::domain::secret::Secret;
use hearth_agent::domain::services::ServiceKey;
use hearth_agent::domain::transfers::TransferId;
use hearth_agent::infrastructure::pack_deposits::ZonePackDeposits;
use sha2::{Digest, Sha256};
use support::fake_safe_fs::FakeSafeFs;

const ID: &str = "01HZY3F8M2K9Q4V6T0R7N5X2PA";

/// Les comptes `ana` et `bob`, d'identifiants techniques DIFFÉRENTS de leur nom : la zone range sous l'identifiant.
struct Accounts;

fn account(name: &str) -> Account {
    let now = time::OffsetDateTime::UNIX_EPOCH;
    Account {
        id: AccountId::new(format!("ID-{name}")),
        username: Username::parse(name).unwrap(),
        role: Role::Admin,
        services_right: ServicesRight::INITIAL,
        password_hash: Secret::new("x".into()),
        created_at: now,
        password_changed_at: now,
        last_login_at: None,
    }
}

#[async_trait::async_trait]
impl AccountRepo for Accounts {
    async fn find_by_id(&self, _: &AccountId) -> Result<Option<Account>, StoreError> {
        Ok(None)
    }
    async fn find_by_username(&self, username: &Username) -> Result<Option<Account>, StoreError> {
        Ok(matches!(username.as_str(), "ana" | "bob").then(|| account(username.as_str())))
    }
    async fn list(&self) -> Result<Vec<Account>, StoreError> {
        Ok(Vec::new())
    }
    async fn reauth_mode(
        &self,
        _: &AccountId,
    ) -> Result<hearth_proto::api::reauth::ReauthMode, StoreError> {
        Ok(hearth_proto::api::reauth::ReauthMode::Window)
    }
}

struct Zone {
    bytes: Vec<u8>,
    released: Mutex<Vec<(String, String)>>,
}

#[async_trait::async_trait]
impl DepositZone for Zone {
    async fn take(&self, id: &TransferId, account: &str) -> Result<CompleteDeposit, TransferError> {
        if account != "ID-ana" {
            return Err(TransferError::NotFound);
        }
        Ok(CompleteDeposit {
            id: id.clone(),
            size: self.bytes.len() as u64,
            tree_digest: [0; 32],
            reader: Box::new(Cursor::new(self.bytes.clone())),
        })
    }
    async fn release(&self, id: &TransferId, account: &str) -> Result<(), TransferError> {
        self.released
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push((id.as_str().to_owned(), account.to_owned()));
        Ok(())
    }
}

fn adapter(bytes: &[u8]) -> (Arc<Zone>, Arc<FakeSafeFs>, Arc<ZonePackDeposits>) {
    let zone = Arc::new(Zone {
        bytes: bytes.to_vec(),
        released: Mutex::new(Vec::new()),
    });
    let fs = Arc::new(FakeSafeFs::new());
    let deposits = Arc::new(ZonePackDeposits::new(
        zone.clone(),
        Arc::new(Accounts),
        fs.clone(),
        tokio::runtime::Handle::current(),
    ));
    (zone, fs, deposits)
}

fn sha(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_complete_deposit_is_read_placed_and_released() {
    let pack = b"PK-fake-pack-bytes".to_vec();
    let (zone, fs, deposits) = adapter(&pack);
    let key = ServiceKey::parse("h_01J0ABCDEFGHJKMNPQRSTVWXYZ").unwrap();
    let id = DepositId::parse(ID).unwrap();
    let rel = RelPath::parse("pack.zip").unwrap();
    let d = deposits.clone();
    let (i, r, k) = (id.clone(), rel.clone(), key.clone());
    let result = tokio::task::spawn_blocking(move || {
        let mut read = Vec::new();
        d.open(&i, "ana").unwrap().read_to_end(&mut read).unwrap();
        assert_eq!(read, b"PK-fake-pack-bytes");
        d.place(
            &i,
            "ana",
            &k,
            &r,
            &sha(&read),
            hearth_agent::application::ports::Perms::new(
                hearth_agent::application::ports::Ownership::new(0, 0).unwrap(),
                0o644,
            )
            .unwrap(),
        )
    })
    .await
    .unwrap();
    assert_eq!(result, Ok(()));
    assert_eq!(fs.content(&key, "pack.zip").as_deref(), Some(&pack[..]));
    assert_eq!(
        zone.released
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone(),
        // Libéré sous l'IDENTIFIANT du compte (FIX:01M7G6V7E6DY5PHJ6TQ848YJ7A), jamais sous son nom.
        vec![(ID.to_owned(), "ID-ana".to_owned())]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn another_account_or_a_bad_id_finds_nothing_and_nothing_is_placed() {
    let (zone, fs, deposits) = adapter(b"x");
    let key = ServiceKey::parse("h_01J0ABCDEFGHJKMNPQRSTVWXYZ").unwrap();
    let rel = RelPath::parse("pack.zip").unwrap();
    let d = deposits.clone();
    let k = key.clone();
    let out = tokio::task::spawn_blocking(move || {
        let id = DepositId::parse(ID).unwrap();
        let other = d.place(&id, "bob", &k, &rel, &sha(b"x"), perms());
        let not_a_ulid = DepositId::parse("not-a-transfer").unwrap();
        (other, d.open(&not_a_ulid, "ana").err())
    })
    .await
    .unwrap();
    assert_eq!(out.0, Err(DepositError::NotFound));
    assert_eq!(out.1, Some(DepositError::NotFound));
    assert!(fs.content(&key, "pack.zip").is_none());
    assert!(
        zone.released
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_empty()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pack_that_changed_since_its_judgement_is_not_placed_and_stays_in_the_zone() {
    let (zone, fs, deposits) = adapter(b"changed bytes");
    let key = ServiceKey::parse("h_01J0ABCDEFGHJKMNPQRSTVWXYZ").unwrap();
    let rel = RelPath::parse("pack.zip").unwrap();
    let d = deposits.clone();
    let k = key.clone();
    let result = tokio::task::spawn_blocking(move || {
        let id = DepositId::parse(ID).unwrap();
        d.place(&id, "ana", &k, &rel, &sha(b"judged bytes"), perms())
    })
    .await
    .unwrap();
    assert_eq!(result, Err(DepositError::Changed));
    assert!(fs.content(&key, "pack.zip").is_none());
    assert!(
        zone.released
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_empty()
    );
}

fn perms() -> hearth_agent::application::ports::Perms {
    hearth_agent::application::ports::Perms::new(
        hearth_agent::application::ports::Ownership::new(0, 0).unwrap(),
        0o644,
    )
    .unwrap()
}

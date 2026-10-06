//! Ports : interfaces que l'application attend de l'infrastructure.
//!
//! Un sujet (comptes, sessions, tentatives de connexion, opérations, journal) a un fichier qui porte son
//! port de lecture (`*Repo`, sur le pool) et son port d'écriture (`*Tx`, dans l'unité de travail).

mod account_repo;
mod admin_accounts;
mod audit_repo;
mod audit_sink;
mod clock;
mod gpu_probe;
mod id_gen;
mod identity_store;
mod install_host;
mod login_attempt_repo;
mod machine_info;
mod monotonic_clock;
mod operation_repo;
mod password_hasher;
mod service_manager;
mod session_repo;
mod store_error;
mod system_probe;
mod token_gen;
mod unit_of_work;
mod update;

pub use account_repo::{AccountRepo, AccountTx};
pub use admin_accounts::{AdminAccounts, AdminAccountsError, AdminCredential};
pub use audit_repo::{AuditRepo, AuditTx};
pub use audit_sink::{AuditFeed, AuditSink};
pub use clock::Clock;
pub use gpu_probe::GpuProbe;
pub use id_gen::IdGen;
pub use identity_store::{IdentityError, IdentityStore, PublicIdentity};
pub use install_host::{
    Answered, BinaryInstalled, ConfigSpec, HostError, HostFacts, InstallHost, InstallLock,
    InstallPaths,
};
pub use login_attempt_repo::{LoginAttemptRepo, LoginAttemptTx};
pub use machine_info::MachineInfo;
pub use monotonic_clock::MonotonicClock;
pub use operation_repo::{OperationRepo, OperationTx};
pub use password_hasher::{HashError, PasswordHasher};
pub use service_manager::{ServiceError, ServiceKind, ServiceManager, ServiceSpec};
pub use session_repo::{SessionRepo, SessionTx};
pub use store_error::StoreError;
pub use system_probe::{ProbeError, SystemProbe};
pub use token_gen::{TokenGen, TokenGenError};
pub use unit_of_work::{Store, UnitOfWork};
pub use update::{
    Downloader, FetchError, FreeSpace, Greeting, HelloProbe, SignatureError, SignatureVerifier,
    SupervisorLock, UpdateFeed, UpdateHost, UpdateHostError,
};

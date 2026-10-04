//! Ports : interfaces que l'application attend de l'infrastructure.

mod identity_store;
mod machine_info;

pub use identity_store::{Identity, IdentityError, IdentityStore};
pub use machine_info::MachineInfo;

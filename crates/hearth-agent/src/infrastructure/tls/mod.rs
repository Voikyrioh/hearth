//! TLS : identité de l'agent (certificat auto-signé persistant) et configuration rustls.

mod identity;
mod server_config;

pub use identity::FileIdentityStore;
pub use server_config::{TlsError, server_config};

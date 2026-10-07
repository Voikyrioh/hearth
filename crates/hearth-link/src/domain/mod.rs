//! Règles de la liaison : pures, sans E/S, sans horloge propre (le temps est un paramètre).

pub mod act;
pub mod agent_identity;
pub mod audit_query;
pub mod backoff;
pub mod book;
pub mod compat;
pub mod event;
pub mod pending_ops;
pub mod pinning;
pub mod secret;
pub mod server;
pub mod state;
pub mod time;
pub mod triggers;

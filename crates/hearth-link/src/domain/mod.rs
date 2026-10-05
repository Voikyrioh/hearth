//! Règles de la liaison : pures, sans E/S, sans horloge propre (le temps est un paramètre).

pub mod backoff;
pub mod compat;
pub mod event;
pub mod pending_ops;
pub mod pinning;
pub mod secret;
pub mod server;
pub mod state;
pub mod time;
pub mod triggers;

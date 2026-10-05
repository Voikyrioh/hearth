//! Adaptateurs vers le monde extérieur : TLS et fichiers, SQLite, configuration, système.
//! Aucune règle métier.

pub mod argon2;
pub mod audit_feed;
pub mod clock;
pub mod config;
pub mod data_dir;
pub mod ids;
pub mod logging;
pub mod random;
pub mod sqlite;
pub mod system;
pub mod tls;

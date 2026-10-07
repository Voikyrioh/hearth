//! Adaptateurs vers le monde extérieur : TLS et fichiers, SQLite, configuration, système.
//! Aucune règle métier.

pub mod argon2;
pub mod audit_feed;
pub mod clock;
pub mod config;
pub mod crypto;
pub mod data_dir;
pub mod file_lock;
pub mod fingerprint;
pub mod fingerprint_secret;
pub mod ids;
pub mod install;
pub mod logging;
pub mod random;
pub mod security_feed;
pub mod service;
pub mod sqlite;
pub mod system;
pub mod tls;
pub mod update;

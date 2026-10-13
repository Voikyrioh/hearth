//! Adaptateurs vers le monde extérieur : TLS et fichiers, SQLite, configuration, système.
//! Aucune règle métier.

pub mod archive_fs;
pub mod argon2;
pub mod audit_feed;
pub mod clock;
pub mod config;
pub mod created_objects;
pub mod crypto;
pub mod data_dir;
pub mod engine;
pub mod file_lock;
pub mod fingerprint;
pub mod fingerprint_secret;
pub mod host;
pub mod ids;
pub mod install;
pub mod logging;
pub mod ops_observer;
pub mod random;
pub mod sealer;
pub mod security_feed;
pub mod service;
pub mod services_key;
pub mod sqlite;
pub mod supervisor;
pub mod system;
pub mod tls;
pub mod update;

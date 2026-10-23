//! Corps de requêtes et de réponses de l'interface `/api/v1`, un module par groupe de routes.
//!
//! Les dates sont des textes RFC 3339 en UTC (`2026-10-04T10:30:15.250Z`). Les types qui portent
//! un secret (mot de passe, jeton) ont un `Debug` qui le masque.

pub mod accounts;
pub mod audit;
pub mod audit_csv;
pub mod catalog;
pub mod deploy;
pub mod devices;
pub mod hello;
pub mod machine;
pub mod metrics;
pub mod operations;
pub mod reauth;
pub mod security;
pub mod service_logs;
pub mod service_ops;
pub mod service_spec;
pub mod services;
pub mod sessions;
pub mod update;

//! Règles métier de l'agent. Aucune E/S, aucune dépendance vers axum, SQLx, rustls, tokio ou le système.
//! (L'empreinte, partagée avec `hearth-link`, vit dans `hearth_proto::fingerprint`.)

pub mod accounts;
pub mod identity_policy;
pub mod install_id;
pub mod secret;
pub mod sessions;

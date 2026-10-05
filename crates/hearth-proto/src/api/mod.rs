//! Corps de requêtes et de réponses de l'interface `/api/v1`, un module par groupe de routes.
//!
//! Les dates sont des textes RFC 3339 en UTC (`2026-10-04T10:30:15.250Z`). Les types qui portent
//! un secret (mot de passe, jeton) ont un `Debug` qui le masque.

pub mod accounts;
pub mod audit;
pub mod hello;
pub mod machine;
pub mod metrics;
pub mod operations;
pub mod sessions;

//! Agent Hearth : service installé sur le serveur piloté.
//!
//! Architecture hexagonale : `domain` (règles pures), `application` (cas d'usage et ports),
//! `infrastructure` (adaptateurs), `entrypoint` (HTTP, ligne de commande). `app` assemble le tout.

pub mod app;
pub mod application;
pub mod build_info;
pub mod domain;
pub mod entrypoint;
pub mod infrastructure;

/// Dossiers temporaires des tests : racine `target/hearth-test-tmp`, suppression vérifiée (T43).
#[cfg(test)]
#[path = "../tests/support/tmp.rs"]
pub(crate) mod test_tmp;

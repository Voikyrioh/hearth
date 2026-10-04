//! Agent Hearth : service installé sur le serveur piloté.
//!
//! Architecture hexagonale : `domain` (règles pures), `application` (cas d'usage et ports),
//! `infrastructure` (adaptateurs), `entrypoint` (HTTP, ligne de commande). `app` assemble le tout.

pub mod app;
pub mod application;
pub mod domain;
pub mod entrypoint;
pub mod infrastructure;

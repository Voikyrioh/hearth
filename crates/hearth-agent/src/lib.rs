//! Agent Hearth : service installé sur le serveur piloté.
//!
//! Architecture hexagonale : `domain` (règles pures), `application` (cas d usage et ports),
//! `infrastructure` (adaptateurs), `entrypoint` (HTTP, ligne de commande).

pub mod application;
pub mod domain;
pub mod infrastructure;

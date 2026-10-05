//! Liaison cliente vers un agent Hearth : épinglage, session, état du lien, reconnexion.
//!
//! Architecture hexagonale : `domain` (règles pures, sans E/S, horloge injectée), puis les ports
//! et les adaptateurs. Rien ici ne panique : toute fonction publique rend un `Result` typé.

pub mod domain;

pub use hearth_proto::version::API_VERSION;

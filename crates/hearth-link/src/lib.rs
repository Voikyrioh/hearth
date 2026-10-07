//! Liaison cliente vers un agent Hearth : épinglage, session, état du lien, reconnexion.
//!
//! Architecture hexagonale : `domain` (règles pures, sans E/S, horloge injectée), `ports` (ce dont
//! la bibliothèque a besoin), `adapters` (réseau, fichiers, système). Rien ici ne panique : toute
//! fonction publique rend un `Result` typé.

pub mod adapters;
pub mod domain;
pub mod error;
pub mod manager;
pub mod ports;

pub use hearth_proto::version::API_VERSION;

pub use error::{InputField, LinkError};
pub use manager::{
    AccountsRead, ActionOutcome, ActionRequest, AuditExportFile, EventStream, LinkConfig,
    LinkManager, LoginInfo, NewServer, Ports, ProbeResult, ReauthState, ServerUpdate,
};

//! Adaptateurs de l'installation : la machine (`SystemHost` : droits, port, espace, fichiers) et
//! la sonde `GET /hello` qui relève le certificat réellement servi.

mod host;
pub mod probe;
pub mod scrub;

pub use host::SystemHost;
pub use probe::AgentHelloProbe;

//! Adaptateurs : ce qui remplit les ports avec le monde réel (réseau, fichiers, système).

pub mod file_store;
pub mod http_transport;
pub mod memory_vault;
pub mod net_watch;
pub mod system;
pub mod tls;

pub use file_store::{FileServerStore, FileSnapshotStore};
pub use http_transport::{HttpTransport, HttpTransportConfig};
pub use memory_vault::MemoryVault;
pub use net_watch::SystemNetWatcher;
pub use system::{OsRng, SystemClock, TokioClock};

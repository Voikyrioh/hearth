//! Ports : ce dont la bibliothèque a besoin du monde extérieur. Les adaptateurs les remplissent
//! (`adapters/`) ; les tests en injectent de simulés.

pub mod clock;
pub mod event_sink;
pub mod net_watcher;
pub mod operation_store;
pub mod rng;
pub mod server_store;
pub mod snapshot_store;
pub mod transport;
pub mod vault;

pub use clock::Clock;
pub use event_sink::EventSink;
pub use net_watcher::NetWatcher;
pub use operation_store::OperationStore;
pub use rng::Rng;
pub use server_store::ServerStore;
pub use snapshot_store::SnapshotStore;
pub use transport::{StreamConn, Transport};
pub use vault::Vault;

//! Règles de la mise à jour de l'agent à distance (BR-UPDATE-011 à 019) : fonctions pures, sans
//! E/S. L'application télécharge, vérifie et lance ; ces règles décident (qui est refusé, quelle
//! cible est valable, où en est le pourcentage, quand le nouvel agent est tenu pour bon ou pour
//! muet) et fixent ce qui est écrit sur le disque (travail du superviseur, dernier résultat).
//!
//! Les messages des refus (`Display`) sont ceux de la spécification : tutoiement, pas de tiret
//! cadratin.

mod orphan;
mod progress;
mod record;
mod resume;
mod space;
mod supervise;
mod target;

pub use orphan::{Leftovers, Orphan, classify_orphan};
pub use progress::PercentTracker;
pub use record::{Job, Requester, SupervisorState, UpdateRecord};
pub use resume::{
    BinaryBackup, DatabaseCopy, Entry, MAX_RESUMES, Marker, Phase, RollbackAct, RollbackFacts,
    Stored, binary_backup, binary_restored, database_copy, enter, rollback_step, swap_done,
};
pub use space::{SpaceRefusal, check_space, required_space};
pub use supervise::{Answer, CHECK_POLL, CHECK_WINDOW, STOP_GRACE, Verdict, check_verdict};
pub use target::{
    MAX_BINARY_BYTES, UpdateInput, UpdateRefusal, UpdateTarget, host_is_local, is_local_address,
    plan_update,
};

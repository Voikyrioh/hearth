//! Journal d'activité (BR-AUDIT-*) : ce qui est consigné, sous quelle forme, ce qui peut y entrer
//! (jamais un secret), combien de temps on le garde, comment on le filtre et on l'exporte.
//! Fonctions pures : le stockage observe et exécute, il ne décide de rien.

mod action;
mod csv;
mod event;
mod filter;
mod policy;
mod repeat;

pub use action::AuditAction;
pub use csv::{BOM, SEPARATOR, field as csv_field, render as render_csv};
pub use event::{
    Actor, AuditEvent, AuditRecord, ClientName, MAX_CLIENT_NAME, Origin, OriginKind, Outcome,
    OutcomeKind, Reason, Target,
};
pub use filter::{AuditFilter, FilterError, MAX_PAGE_SIZE, RawFilter, SearchQuery};
pub use policy::{
    MAX_ENTRIES, RETENTION, RequestKind, can_read_journal, excess_entries, is_journaled,
    retention_cutoff,
};
pub use repeat::{MAX_TRACKED, REPEAT_WINDOW, RepeatFilter};

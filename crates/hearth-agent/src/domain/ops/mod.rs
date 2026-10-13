//! Les opérations de service : un seul modèle pour tous les gestes longs et destructifs (HRT-83,
//! ADR-0045). Règles pures, sans moteur, sans base, sans horloge système.
//!
//! - [`kind`] : les genres, statuts, causes et choix, tous en listes fermées, et les transitions permises.
//! - [`plan`] : le plan figé à l'acceptation, les étapes, le journal d'intention.
//! - [`recover`] : ce que l'agent fait d'une opération non terminée quand il redémarre. Il ne reprend,
//!   ne rejoue et ne défait **jamais seul** un geste sur un service ou ses données.
//! - [`decision`] : annuler (avant le point de non-retour), décider, expirer.
//! - [`locks`] : le verrou par service (exclusif, partagé en écriture de fichiers, aucun) et les deux
//!   verrous du serveur (`names`, `heavy_io`).

pub mod decision;
pub mod kind;
pub mod locks;
pub mod plan;
pub mod recover;

pub use decision::{
    CancelVerdict, ChoiceRefused, apply_choice, cancel_verdict, decision_expired, decision_expiry,
};
pub use kind::{Cause, Choice, OpKind, OpStatus};
pub use locks::{Holder, LockMode, LockTable, ReadKind, Refusal};
pub use plan::{
    CreatedObject, MAX_STEPS, ObjectKind, ObjectRoot, OpPlan, PlanError, PlannedStep, StepName,
    StepRecord, StepState, UndoInfo, latest_state,
};
pub use recover::{ConfigSide, Observed, Recovery, recover, recover_unreadable_journal};

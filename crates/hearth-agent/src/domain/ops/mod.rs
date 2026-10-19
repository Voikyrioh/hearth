//! Les opérations de service : un seul modèle pour tous les gestes longs et destructifs (HRT-83,
//! ADR-0045). Règles pures, sans moteur, sans base, sans horloge système.
//!
//! - [`kind`] : les genres, statuts, causes et choix, tous en listes fermées, et les transitions permises.
//! - [`plan`] : le plan figé à l'acceptation, les étapes, le journal d'intention.
//! - [`recover`] : ce que l'agent fait d'une opération non terminée quand il redémarre. Il ne reprend,
//!   ne rejoue et ne défait **jamais seul** un geste sur un service ou ses données.
//! - [`decision`] : annuler (avant le point de non-retour), décider, expirer.
//! - [`access`] : qui voit une opération (le service qu'elle touche décide).
//! - [`index`] : `busy` et `last_action` du fil, projections du journal.
//! - [`lifecycle`] : démarrer, arrêter, redémarrer (HRT-64) : l'état permet-il le geste, combien attendre, l'arrêt
//!   a-t-il été forcé.
//! - [`flush`] : la cadence des messages `operation` d'un abonné (500 ms par opération).
//! - [`observe`] : le décompte des conteneurs du moteur devient l'état que `recover` classe.
//! - [`locks`] : le verrou par service (exclusif, partagé en écriture de fichiers, aucun) et les deux
//!   verrous du serveur (`names`, `heavy_io`).

pub mod access;
pub mod decision;
pub mod flush;
pub mod index;
pub mod kind;
pub mod lifecycle;
pub mod locks;
pub mod observe;
pub mod plan;
pub mod recover;

pub use access::{ServiceOrigin, may_see, origin_of};
pub use decision::{
    CancelVerdict, ChoiceRefused, apply_choice, cancel_verdict, decision_expired, decision_expiry,
};
pub use flush::{MAX_TRACKED_OPS, OpFlush};
pub use index::{MAX_INDEXED_SERVICES, OpSummary, OpsIndex, ServiceOpsEntry};
pub use kind::{Cause, Choice, OpKind, OpStatus};
pub use lifecycle::{
    Gesture, KILLED_EXIT_CODE, RESTART_WATCH, STARTUP_WATCH, Sighting, StateRefusal, StopEvidence,
    WAIT_FLOOR, WAIT_MARGIN, Watch, cause_after, check_state, engine_ready, stop_was_forced,
    wait_budget, wait_budget_with,
};
pub use locks::{Holder, LockMode, LockTable, ReadKind, Refusal};
pub use observe::{Presence, observed_from_presence};
pub use plan::{
    CreatedObject, MAX_STEPS, ObjectKind, ObjectRoot, OpPlan, PlanError, PlannedStep, StepName,
    StepRecord, StepState, UndoInfo, latest_state,
};
pub use recover::{
    ConfigSide, Observed, Recovery, cleanup_on_cancel, recover, recover_unreadable_journal,
};

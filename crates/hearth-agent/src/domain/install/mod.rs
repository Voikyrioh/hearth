//! Règles de l'installation de l'agent (BR-INSTALL-001 à BR-INSTALL-012) : fonctions pures, sans
//! E/S. L'adaptateur observe la machine (`Observed`), ces règles décident (`plan_install`,
//! `check_prerequisites`, `uninstall_plan`, `undo_plan`), l'adaptateur exécute.
//!
//! Les messages des règles refusées (`Display` des erreurs) sont ceux de la spécification
//! fonctionnelle : tutoiement, pas de tiret cadratin.

mod credentials;
mod observed;
mod plan;
mod platform;
mod port;
mod prerequisites;
mod rollback;
mod uninstall;
mod version;

pub use credentials::{
    AdminNameError, AdminPasswordError, HashFormatError, NAME_HELP, PASSWORD_HELP,
    check_admin_password, check_password_hash_format, parse_admin_name,
};
pub use observed::{BinaryState, DataState, Observed, UnitState};
pub use plan::{InstallKind, InstallPlan, Kept, PlanError, ServiceAction, plan_install};
pub use platform::{Arch, SUPPORTED_ARCHITECTURES, parse_arch};
pub use port::{PortError, parse_port};
pub use prerequisites::{
    Blocker, MIN_FREE_BYTES, Prerequisites, check_prerequisites, check_rights,
};
pub use rollback::{Asset, Done, Undo, undo_plan};
pub use uninstall::{
    ChoiceError, DataChoice, UninstallPlan, is_installed, parse_choice, uninstall_plan,
};
pub use version::{Version, VersionError};

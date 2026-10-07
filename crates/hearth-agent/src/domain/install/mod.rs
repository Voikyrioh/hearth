//! Règles de l'installation de l'agent (BR-INSTALL-001 à BR-INSTALL-012) : fonctions pures, sans
//! E/S. L'adaptateur observe la machine (`Observed`), ces règles décident (`plan_install`,
//! `check_prerequisites`, `uninstall_plan`, `undo_plan`), l'adaptateur exécute.
//!
//! Les messages des règles refusées (`Display` des erreurs) sont ceux de la spécification
//! fonctionnelle : tutoiement, pas de tiret cadratin.

mod credentials;
mod files;
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
pub use files::{
    BINARY_TEMP_PREFIX, CERT_FILE, DATA_FILES, DATABASE_FILE, DATABASE_FILES,
    FINGERPRINT_SECRET_FILE, IDENTITY_CONTENT_FILES, IDENTITY_FILES, IDENTITY_LOCK_FILE,
    INSTALL_ID_FILE, KEY_FILE, UNIT_TEMP_EXTENSION, UPDATE_DB_BACKUP_FILE, UPDATE_DIR,
    UPDATE_FILES, UPDATE_JOB_FILE, UPDATE_LAST_FILE, UPDATE_LOCK_FILE, UPDATE_PHASE_FILE,
    UPDATE_STAGED_FILE, UPDATE_STATE_FILE, UPDATE_SUPERVISOR_FILE, UPDATE_WAL_BACKUP_FILE,
    binary_temporary_name, is_binary_temporary, is_database_temporary,
    is_fingerprint_secret_temporary, is_identity_temporary, is_update_temporary,
};
pub use observed::{BinaryState, DataState, Observed, UnitState};
pub use plan::{InstallKind, InstallPlan, Kept, PlanError, ServiceAction, plan_install};
pub use platform::{Arch, SUPPORTED_ARCHITECTURES, parse_arch};
pub use port::{PortError, parse_port};
pub use prerequisites::{
    Blocker, DataDirState, MIN_FREE_BYTES, Prerequisites, check_prerequisites, check_rights,
    unsafe_path_reason,
};
pub use rollback::{Asset, Done, Undo, undo_plan};
pub use uninstall::{
    ChoiceError, DataChoice, UninstallPlan, is_installed, parse_choice, uninstall_plan,
};
pub use version::{Version, VersionError};

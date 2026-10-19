//! Services (conteneurs) et moteur de conteneurs : règles pures (HRT-61, ADR-0036 à ADR-0038).
//!
//! Aucune E/S ici : ni `bollard`, ni SQLx, ni axum, ni tokio. Le moteur est vu par le port
//! `application::ports::ContainerEngine` ; ce dossier ne connaît que ses propres types.
//!
//! Aucune VALEUR de variable d'environnement n'entre dans ces types. Les noms de variables et la
//! commande de lancement (HRT-63, détail d'un service) n'y entrent que dans [`Hidden`], dont `Debug`
//! ne montre rien.

pub mod command;
pub mod deploy;
pub mod drift;
pub mod engine;
pub mod folders;
pub mod hidden;
pub mod image;
pub mod key;
pub mod limits;
pub mod log_brake;
pub mod logs;
pub mod plan;
pub mod ports;
pub mod pull;
pub mod spec;
pub mod state;
pub mod unit;
pub mod visibility;

pub use engine::{
    Announced, ApiVersion, Debounce, EngineProbe, EngineState, MIN_API_VERSION, ProbeOutcome,
    classify_probe, is_supported_engine,
};
pub use folders::{
    ACCOUNT_SECRET_FOLDERS, FolderPath, FolderPolicy, FolderRefusal, MAX_LINK_HOPS, Probe,
    SYSTEM_FOLDERS, home_folders, resolve,
};
pub use hidden::Hidden;
pub use image::{ImageName, image_name};
pub use key::{KeyKind, ServiceKey};
pub use limits::{
    MAX_COMMAND_ARG_CHARS, MAX_COMMAND_ARGS, MAX_ENGINE_TEXT_CHARS, MAX_ENV_NAME_CHARS,
    MAX_ENV_NAMES, MAX_IMAGE_REF_CHARS, MAX_IMAGE_TAG_CHARS, MAX_LABEL_CHARS, MAX_MOUNTS,
    MAX_NAME_CHARS, MAX_PATH_CHARS, MAX_PORTS, bounded, bounded_opt,
};
pub use log_brake::{AccountBrake, BrakeSettings, OpenRefusal, REFUSALS_BEFORE_ENTRY};
pub use state::{
    Derived, MountInfo, MountType, Observation, PortBinding, PortProtocol, PortScope,
    RESTART_LOOP_STARTS, RESTART_LOOP_WINDOW, RestartPolicy, RunState, STARTING_MAX,
    ServiceStateKind, StartWindow, Transition, TransitionKind, compare_names, derive_state,
    derive_supervised, derive_unit_state, honour_stop_request, overlay_transition, stop_grace_s,
};
pub use unit::{
    ClientClaim, STOP_TIMES_PROPERTIES, UnitActivity, UnitLookup, UnitName, UnitStatus, attribute,
    client_refs, is_engine_client, parse_main_pid, parse_show, parse_stop_duration, unit_of_cgroup,
};
pub use visibility::{
    Adoption, CONFIRMATION_TTL, Holder, UNIT_GONE_READS, Visibility, confirmation_expired,
    gone_long_enough, lapsed, may_read_details, unit_answered, unit_is_gone, unread_unit_adoptions,
    visibility_of,
};

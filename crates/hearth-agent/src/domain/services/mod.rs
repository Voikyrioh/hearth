//! Services (conteneurs) et moteur de conteneurs : règles pures (HRT-61, ADR-0036 à ADR-0038).
//!
//! Aucune E/S ici : ni `bollard`, ni SQLx, ni axum, ni tokio. Le moteur est vu par le port
//! `application::ports::ContainerEngine` ; ce dossier ne connaît que ses propres types.
//!
//! Aucune VALEUR de variable d'environnement n'entre dans ces types. Les noms de variables et la
//! commande de lancement (HRT-63, détail d'un service) n'y entrent que dans [`Hidden`], dont `Debug`
//! ne montre rien.

pub mod engine;
pub mod hidden;
pub mod image;
pub mod key;
pub mod limits;
pub mod state;
pub mod unit;
pub mod visibility;

pub use engine::{
    Announced, ApiVersion, Debounce, EngineProbe, EngineState, MIN_API_VERSION, ProbeOutcome,
    classify_probe, is_supported_engine,
};
pub use hidden::Hidden;
pub use image::{ImageName, image_name};
pub use key::{KeyKind, ServiceKey};
pub use limits::{
    MAX_COMMAND_ARG_CHARS, MAX_COMMAND_ARGS, MAX_ENGINE_TEXT_CHARS, MAX_ENV_NAME_CHARS,
    MAX_ENV_NAMES, MAX_IMAGE_REF_CHARS, MAX_IMAGE_TAG_CHARS, MAX_LABEL_CHARS, MAX_MOUNTS,
    MAX_NAME_CHARS, MAX_PATH_CHARS, MAX_PORTS, bounded, bounded_opt,
};
pub use state::{
    Derived, MountInfo, MountType, Observation, PortBinding, PortProtocol, PortScope,
    RESTART_LOOP_STARTS, RESTART_LOOP_WINDOW, RestartPolicy, RunState, STARTING_MAX,
    ServiceStateKind, StartWindow, compare_names, derive_state, derive_supervised,
    derive_unit_state, stop_grace_s,
};
pub use unit::{
    ClientClaim, UnitActivity, UnitLookup, UnitName, UnitStatus, attribute, client_refs,
    is_engine_client, parse_main_pid, parse_show, unit_of_cgroup,
};
pub use visibility::{Adoption, Holder, Visibility, lapsed, may_read_details, visibility_of};

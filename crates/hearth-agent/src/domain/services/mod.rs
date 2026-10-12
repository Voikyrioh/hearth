//! Services (conteneurs) et moteur de conteneurs : règles pures (HRT-61, ADR-0036 à ADR-0038).
//!
//! Aucune E/S ici : ni `bollard`, ni SQLx, ni axum, ni tokio. Le moteur est vu par le port
//! `application::ports::ContainerEngine` ; ce dossier ne connaît que ses propres types.
//!
//! Aucune valeur de variable d'environnement, aucun nom de variable et aucune commande de lancement
//! n'entrent dans ces types : une observation n'en porte pas (`Observation`).

pub mod cause;
pub mod engine;
pub mod image;
pub mod key;
pub mod state;

pub use cause::Cause;
pub use engine::{
    Announced, ApiVersion, Debounce, EngineProbe, EngineState, MIN_API_VERSION, ProbeOutcome,
    classify_probe, is_supported_engine,
};
pub use image::{ImageName, image_name};
pub use key::{KeyKind, ServiceKey};
pub use state::{
    Derived, MountInfo, MountType, Observation, PortBinding, PortProtocol, PortScope,
    RESTART_LOOP_STARTS, RESTART_LOOP_WINDOW, RestartPolicy, RunState, ServiceStateKind,
    StartWindow, compare_names, derive_state,
};

//! Informations sur la machine hôte : nom et adresses MAC (`/hello`), sonde de mesures
//! (`sysinfo`), cartes graphiques (`gpu/`).

pub mod gpu;
mod machine_info;
mod sysinfo_probe;

pub use machine_info::SystemMachineInfo;
pub use sysinfo_probe::SysinfoProbe;

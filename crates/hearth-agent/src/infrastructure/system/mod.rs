//! Informations sur la machine hôte : nom et adresses MAC (`/hello`), sonde de mesures
//! (`sysinfo`), cartes graphiques (`gpu/`).

mod boot;
pub mod gpu;
mod machine_info;
mod sysinfo_probe;

pub use boot::ProcBootInfo;
pub use machine_info::SystemMachineInfo;
pub use sysinfo_probe::SysinfoProbe;

//! Identité de la machine : `GET /api/v1/machine`, et premier contenu du `snapshot` du flux.
//!
//! Ce que l'agent sait de la machine et qui change rarement. Le matériel absent n'est jamais
//! rendu par une valeur inventée : la capacité correspondante est fausse et la liste vide.

use serde::{Deserialize, Serialize};

/// Ce que cette machine sait mesurer (BR-DASH-005 et BR-DASH-006).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    /// Au moins une carte graphique mesurable.
    pub gpu: bool,
    /// Au moins une sonde de température exposée par le système.
    pub temps: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OsInfo {
    /// Nom du système (`Ubuntu`, `NixOS`, `Windows`…).
    pub name: String,
    /// Version lisible (`24.04`, `11 (26200)`…), absente si le système ne la donne pas.
    pub version: Option<String>,
    pub kernel: Option<String>,
    /// Architecture (`x86_64`…).
    pub arch: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CpuInfo {
    /// Modèle commercial (`AMD Ryzen 9 7950X 16-Core Processor`).
    pub model: String,
    pub physical_cores: Option<u32>,
    pub logical_cores: u32,
    pub frequency_mhz: Option<u64>,
}

/// Un disque monté. Un disque monté ou retiré pendant l'affichage apparaît ou disparaît des
/// listes suivantes (BR-DASH-012).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiskInfo {
    /// Nom du volume ou du périphérique (`/dev/nvme0n1p2`, `C:`).
    pub name: String,
    /// Point de montage (`/`, `/mnt/data`, `C:\`).
    pub mount: String,
    pub fs: Option<String>,
    pub total_bytes: u64,
    pub removable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuInfo {
    pub name: String,
    /// Mémoire vidéo totale, absente si la carte ne l'expose pas.
    pub memory_total_bytes: Option<u64>,
}

/// Réponse de `GET /machine`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MachineResponse {
    pub name: String,
    pub os: OsInfo,
    pub cpu: CpuInfo,
    pub memory_total_bytes: u64,
    pub disks: Vec<DiskInfo>,
    /// Vide quand la machine n'a pas de carte graphique mesurable.
    pub gpus: Vec<GpuInfo>,
    pub capabilities: Capabilities,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> MachineResponse {
        MachineResponse {
            name: "forge".into(),
            os: OsInfo {
                name: "NixOS".into(),
                version: Some("25.05".into()),
                kernel: None,
                arch: "x86_64".into(),
            },
            cpu: CpuInfo {
                model: "AMD Ryzen 9".into(),
                physical_cores: Some(16),
                logical_cores: 32,
                frequency_mhz: None,
            },
            memory_total_bytes: 64 << 30,
            disks: vec![DiskInfo {
                name: "/dev/nvme0n1p2".into(),
                mount: "/".into(),
                fs: Some("ext4".into()),
                total_bytes: 1 << 40,
                removable: false,
            }],
            gpus: vec![],
            capabilities: Capabilities {
                gpu: false,
                temps: true,
            },
        }
    }

    #[test]
    fn round_trips_through_json() {
        let text = serde_json::to_string(&sample()).expect("serialization");
        let back: MachineResponse = serde_json::from_str(&text).expect("deserialization");
        assert_eq!(back, sample());
    }

    #[test]
    fn a_machine_without_gpu_serializes_an_empty_list_and_a_false_capability() {
        let json = serde_json::to_value(sample()).expect("serialization");
        assert_eq!(json["gpus"], serde_json::json!([]));
        assert_eq!(
            json["capabilities"],
            serde_json::json!({ "gpu": false, "temps": true })
        );
    }
}

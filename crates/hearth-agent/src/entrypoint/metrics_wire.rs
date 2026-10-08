//! Conversions entre les structures du domaine (mesures, identité) et les types du fil de
//! `hearth-proto`. Partagé par `http/` (`/machine`, `/metrics/history`) et `ws/` (`snapshot`,
//! `metrics`) : le contrat JSON des mesures n'est connu qu'ici.

use hearth_proto::api::machine::{
    Capabilities, CpuInfo, DiskInfo, GpuInfo, MachineResponse, OsInfo,
};
use hearth_proto::api::metrics::{
    DiskSample, GpuSample, HistoryWindow as WireWindow, MemorySample, NetSample, Sample, TempSample,
};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::domain::machine::MachineIdentity;
use crate::domain::metrics::{HistoryWindow, Sample as DomainSample};

/// Date RFC 3339 en UTC. Le formatage ne peut échouer que pour une année hors de 0 à 9999, que
/// l'horloge du système ne produit pas ; le repli est la date de l'époque.
pub fn date(date: OffsetDateTime) -> String {
    date.to_offset(time::UtcOffset::UTC)
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned())
}

pub fn window_to_wire(window: HistoryWindow) -> WireWindow {
    match window {
        HistoryWindow::OneMinute => WireWindow::OneMinute,
        HistoryWindow::FiveMinutes => WireWindow::FiveMinutes,
        HistoryWindow::OneHour => WireWindow::OneHour,
    }
}

pub fn sample(sample: &DomainSample) -> Sample {
    Sample {
        at: date(sample.at),
        uptime_s: sample.uptime_s,
        cpu: sample.cpu,
        cores: sample.cores.clone(),
        mem: MemorySample {
            used_bytes: sample.mem.used_bytes,
            total_bytes: sample.mem.total_bytes,
        },
        disks: sample
            .disks
            .iter()
            .map(|disk| DiskSample {
                name: disk.name.clone(),
                mount: disk.mount.clone(),
                used_bytes: disk.used_bytes,
                total_bytes: disk.total_bytes,
            })
            .collect(),
        net: sample.net.map(|net| NetSample {
            up_bytes_per_s: net.up_bytes_per_s,
            down_bytes_per_s: net.down_bytes_per_s,
        }),
        gpus: sample
            .gpus
            .iter()
            .map(|gpu| GpuSample {
                name: gpu.name.clone(),
                load_percent: gpu.load_percent,
                memory_used_bytes: gpu.memory_used_bytes,
                memory_total_bytes: gpu.memory_total_bytes,
                temp_c: gpu.temp_c,
            })
            .collect(),
        temps: sample
            .temps
            .iter()
            .map(|temp| TempSample {
                label: temp.label.clone(),
                celsius: temp.celsius,
            })
            .collect(),
    }
}

pub fn peak(peak: &crate::domain::metrics::StepPeak) -> hearth_proto::api::metrics::StepPeak {
    hearth_proto::api::metrics::StepPeak {
        cpu: peak.cpu,
        mem_used_bytes: peak.mem_used_bytes,
        net: peak.net.map(|net| NetSample {
            up_bytes_per_s: net.up_bytes_per_s,
            down_bytes_per_s: net.down_bytes_per_s,
        }),
        gpus: peak
            .gpus
            .iter()
            .map(|gpu| hearth_proto::api::metrics::GpuPeak {
                load_percent: gpu.load_percent,
                memory_used_bytes: gpu.memory_used_bytes,
            })
            .collect(),
    }
}

pub fn machine(identity: &MachineIdentity) -> MachineResponse {
    let capabilities = identity.capabilities();
    MachineResponse {
        name: identity.name.clone(),
        os: OsInfo {
            name: identity.os.name.clone(),
            version: identity.os.version.clone(),
            kernel: identity.os.kernel.clone(),
            arch: identity.os.arch.clone(),
        },
        cpu: CpuInfo {
            model: identity.cpu.model.clone(),
            physical_cores: identity.cpu.physical_cores,
            logical_cores: identity.cpu.logical_cores,
            frequency_mhz: identity.cpu.frequency_mhz,
        },
        memory_total_bytes: identity.memory_total_bytes,
        disks: identity
            .disks
            .iter()
            .map(|disk| DiskInfo {
                name: disk.name.clone(),
                mount: disk.mount.clone(),
                fs: disk.fs.clone(),
                total_bytes: disk.total_bytes,
                removable: disk.removable,
            })
            .collect(),
        gpus: identity
            .gpus
            .iter()
            .map(|gpu| GpuInfo {
                name: gpu.name.clone(),
                memory_total_bytes: gpu.memory_total_bytes,
            })
            .collect(),
        capabilities: Capabilities {
            gpu: capabilities.gpu,
            temps: capabilities.temps,
        },
    }
}

#[cfg(test)]
mod tests {
    use time::Duration;

    use super::*;
    use crate::domain::machine::{CpuIdentity, GpuIdentity, OsIdentity};
    use crate::domain::metrics::{GpuReading, MemoryUsage, NetRate, TempReading};

    #[test]
    fn dates_are_rfc_3339_in_utc_with_milliseconds() {
        let at = OffsetDateTime::UNIX_EPOCH + Duration::milliseconds(1_790_000_000_250);
        assert_eq!(date(at), "2026-09-21T14:13:20.25Z");
    }

    #[test]
    fn a_sample_converts_every_field_and_keeps_absent_ones_absent() {
        let domain = DomainSample {
            at: OffsetDateTime::UNIX_EPOCH,
            mono: Duration::ZERO,
            uptime_s: 5,
            cpu: 10.5,
            cores: vec![1.0, 2.0],
            mem: MemoryUsage {
                used_bytes: 1,
                total_bytes: 2,
            },
            disks: vec![],
            net: Some(NetRate {
                up_bytes_per_s: 3,
                down_bytes_per_s: 4,
            }),
            gpus: vec![GpuReading {
                name: "g".into(),
                load_percent: Some(1.0),
                memory_used_bytes: None,
                memory_total_bytes: Some(9),
                temp_c: None,
            }],
            temps: vec![TempReading {
                label: "cpu".into(),
                celsius: 40.0,
            }],
        };
        let wire = sample(&domain);
        assert_eq!(wire.at, "1970-01-01T00:00:00Z");
        assert_eq!(wire.cores, vec![1.0, 2.0]);
        assert_eq!(wire.net.map(|n| n.down_bytes_per_s), Some(4));
        assert_eq!(wire.gpus[0].temp_c, None);
        assert_eq!(wire.gpus[0].memory_total_bytes, Some(9));
        assert_eq!(wire.temps[0].celsius, 40.0);
    }

    #[test]
    fn the_machine_carries_its_capabilities() {
        let identity = MachineIdentity {
            name: "forge".into(),
            os: OsIdentity {
                name: "NixOS".into(),
                version: None,
                kernel: None,
                arch: "x86_64".into(),
            },
            cpu: CpuIdentity {
                model: "cpu".into(),
                physical_cores: Some(4),
                logical_cores: 8,
                frequency_mhz: None,
            },
            memory_total_bytes: 7,
            disks: vec![],
            gpus: vec![GpuIdentity {
                name: "g".into(),
                memory_total_bytes: None,
            }],
            has_temperature_sensors: false,
        };
        let wire = machine(&identity);
        assert!(wire.capabilities.gpu);
        assert!(!wire.capabilities.temps);
        assert_eq!(wire.cpu.logical_cores, 8);
        assert_eq!(wire.gpus.len(), 1);
    }

    #[test]
    fn windows_map_one_to_one() {
        assert_eq!(window_to_wire(HistoryWindow::OneHour), WireWindow::OneHour);
        assert_eq!(
            window_to_wire(HistoryWindow::OneMinute),
            WireWindow::OneMinute
        );
        assert_eq!(
            window_to_wire(HistoryWindow::FiveMinutes),
            WireWindow::FiveMinutes
        );
    }
}

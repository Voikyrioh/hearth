//! Sondes simulées pour les tests : un « processeur » dont la charge croît à chaque échantillon,
//! un disque, une carte graphique sans température. Aucune lecture du système réel.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use hearth_agent::app::Metering;
use hearth_agent::application::ports::{AuditFeed, GpuProbe, ProbeError, SystemProbe};
use hearth_agent::domain::machine::{
    CpuIdentity, DiskIdentity, GpuIdentity, MachineIdentity, OsIdentity,
};
use hearth_agent::domain::metrics::{DiskUsage, GpuReading, MemoryUsage, SystemSample};
use hearth_agent::entrypoint::ws::StreamSettings;
use hearth_agent::infrastructure::audit_feed::NoAuditFeed;
use hearth_agent::infrastructure::clock::{SystemClock, SystemMonotonic};

#[derive(Default)]
pub struct FakeSystem {
    calls: AtomicU32,
    /// Nombre de cœurs rendus (0 : 4) : beaucoup de cœurs gonflent chaque échantillon.
    pub cores: usize,
}

impl FakeSystem {
    /// Sonde dont chaque échantillon porte `cores` cœurs : des messages volumineux.
    pub fn wide(cores: usize) -> Self {
        Self {
            calls: AtomicU32::new(0),
            cores,
        }
    }
}

impl SystemProbe for FakeSystem {
    fn identity(&self) -> MachineIdentity {
        MachineIdentity {
            name: "forge-test".into(),
            os: OsIdentity {
                name: "TestOS".into(),
                version: Some("1.0".into()),
                kernel: None,
                arch: "x86_64".into(),
            },
            cpu: CpuIdentity {
                model: "Test CPU".into(),
                physical_cores: Some(2),
                logical_cores: 4,
                frequency_mhz: None,
            },
            memory_total_bytes: 16 << 30,
            disks: vec![DiskIdentity {
                name: "/dev/test".into(),
                mount: "/".into(),
                fs: Some("ext4".into()),
                total_bytes: 100 << 30,
                removable: false,
            }],
            gpus: vec![],
            has_temperature_sensors: false,
        }
    }

    fn sample(&self) -> Result<SystemSample, ProbeError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(SystemSample {
            uptime_s: 1_000 + u64::from(call),
            cpu: (call % 100) as f32,
            cores: vec![(call % 100) as f32; if self.cores == 0 { 4 } else { self.cores }],
            mem: MemoryUsage {
                used_bytes: 4 << 30,
                total_bytes: 16 << 30,
            },
            disks: vec![DiskUsage {
                name: "/dev/test".into(),
                mount: "/".into(),
                used_bytes: 40 << 30,
                total_bytes: 100 << 30,
            }],
            net: None,
            temps: vec![],
        })
    }
}

/// Cartes qui apparaissent en cours de route : `set` change la liste détectée.
#[derive(Default)]
pub struct ToggleGpu(std::sync::Mutex<Vec<GpuIdentity>>);

impl ToggleGpu {
    pub fn set(&self, names: &[&str]) {
        *self.0.lock().unwrap() = names
            .iter()
            .map(|name| GpuIdentity {
                name: (*name).into(),
                memory_total_bytes: Some(8 << 30),
            })
            .collect();
    }
}

impl GpuProbe for ToggleGpu {
    fn detect(&self) -> Vec<GpuIdentity> {
        self.0.lock().unwrap().clone()
    }

    fn sample(&self) -> Vec<GpuReading> {
        self.detect()
            .into_iter()
            .map(|gpu| GpuReading {
                name: gpu.name,
                load_percent: Some(1.0),
                memory_used_bytes: None,
                memory_total_bytes: gpu.memory_total_bytes,
                temp_c: None,
            })
            .collect()
    }
}

pub struct FakeGpu;

impl GpuProbe for FakeGpu {
    fn detect(&self) -> Vec<GpuIdentity> {
        vec![GpuIdentity {
            name: "Test GPU".into(),
            memory_total_bytes: Some(8 << 30),
        }]
    }

    fn sample(&self) -> Vec<GpuReading> {
        vec![GpuReading {
            name: "Test GPU".into(),
            load_percent: Some(12.0),
            memory_used_bytes: Some(1 << 30),
            memory_total_bytes: Some(8 << 30),
            temp_c: None,
        }]
    }
}

/// Délais raccourcis du flux : la surveillance de la session se fait toutes les 50 ms.
pub fn fast_stream() -> StreamSettings {
    StreamSettings {
        auth_timeout: Duration::from_millis(400),
        idle_timeout: Duration::from_secs(10),
        session_check_period: Duration::from_millis(50),
        send_timeout: Duration::from_secs(5),
        max_pending_total: 16,
        max_pending_per_address: 8,
        max_total: 32,
        max_per_account: 4,
        min_subscribe_interval: Duration::from_millis(50),
    }
}

/// Sondes simulées, un échantillon toutes les 20 ms, pas de journal.
pub fn metering() -> Metering {
    metering_with(Arc::new(NoAuditFeed), fast_stream())
}

pub fn metering_with(audit: Arc<dyn AuditFeed>, stream: StreamSettings) -> Metering {
    Metering {
        system: Arc::new(FakeSystem::default()),
        gpu: Arc::new(FakeGpu),
        // Les échantillons sont datés en temps réel : le flux les distingue par leur date.
        clock: Arc::new(SystemClock),
        monotonic: Arc::new(SystemMonotonic::new()),
        period: Duration::from_millis(20),
        audit,
        stream,
    }
}

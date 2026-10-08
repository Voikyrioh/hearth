//! Sonde système : processeur, mémoire, disques, réseau, températures, par la crate `sysinfo`
//! (ADR-0009). Fonctionne sous Linux et sous Windows (mode dev).
//!
//! Budget : un échantillon coûte moins de 5 ms. On ne rafraîchit que ce qui est publié (charge du
//! processeur, mémoire, stockage des disques, compteurs réseau, sondes), jamais les processus.
//! Le débit réseau est calculé entre deux échantillons ; la première charge de processeur n'est
//! publiée qu'après l'intervalle minimal de `sysinfo` (sinon ce serait un zéro inventé).

use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use sysinfo::{Components, DiskRefreshKind, Disks, MINIMUM_CPU_UPDATE_INTERVAL, Networks, System};

use super::gpu::trunc1;
use crate::application::ports::{ProbeError, SystemProbe};
use crate::domain::machine::{
    CpuIdentity, DiskIdentity, InterfaceKind, MachineIdentity, OsIdentity, Volume,
    throughput_interfaces, visible_volumes,
};
use crate::domain::metrics::{DiskUsage, MemoryUsage, NetRate, SystemSample, TempReading};

/// Hors de cet intervalle, une température est un capteur défaillant, pas une mesure.
const PLAUSIBLE_CELSIUS: std::ops::RangeInclusive<f32> = -50.0..=150.0;

/// Les sondes de température bougent lentement et leur lecture est la plus coûteuse (WMI sous
/// Windows, dizaines de fichiers `hwmon` sous Linux) : relues toutes les 3 s, les valeurs
/// précédentes sont republiées entre deux lectures.
const TEMPERATURE_REFRESH: Duration = Duration::from_secs(3);

/// Sous cet intervalle, un débit calculé n'a pas de sens.
const MIN_NET_INTERVAL: Duration = Duration::from_millis(200);

struct State {
    system: System,
    disks: Disks,
    networks: Networks,
    components: Components,
    cpu_refreshed_at: Instant,
    net_refreshed_at: Instant,
    temps: Vec<TempReading>,
    temps_refreshed_at: Instant,
}

pub struct SysinfoProbe {
    state: Mutex<State>,
}

impl Default for SysinfoProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl SysinfoProbe {
    pub fn new() -> Self {
        let mut system = System::new();
        system.refresh_cpu_all();
        system.refresh_memory();
        let now = Instant::now();
        let components = Components::new_with_refreshed_list();
        Self {
            state: Mutex::new(State {
                system,
                disks: Disks::new_with_refreshed_list_specifics(storage_only()),
                networks: Networks::new_with_refreshed_list(),
                temps: temperatures(&components),
                components,
                cpu_refreshed_at: now,
                net_refreshed_at: now,
                temps_refreshed_at: now,
            }),
        }
    }
}

fn storage_only() -> DiskRefreshKind {
    DiskRefreshKind::nothing().with_storage()
}

fn to_volumes(disks: &Disks) -> Vec<Volume> {
    let volumes = disks
        .list()
        .iter()
        .map(|disk| {
            let mount = disk.mount_point().to_string_lossy().into_owned();
            let name = disk.name().to_string_lossy().into_owned();
            Volume {
                name: if name.is_empty() { mount.clone() } else { name },
                mount,
                fs: Some(disk.file_system().to_string_lossy().into_owned())
                    .filter(|fs| !fs.is_empty()),
                total_bytes: disk.total_space(),
                available_bytes: disk.available_space(),
                removable: disk.is_removable(),
            }
        })
        .collect();
    visible_volumes(volumes)
}

/// Les sondes dont la valeur est lisible et plausible.
fn temperatures(components: &Components) -> Vec<TempReading> {
    components
        .list()
        .iter()
        .filter_map(|component| {
            let celsius = component.temperature()?;
            PLAUSIBLE_CELSIUS.contains(&celsius).then(|| TempReading {
                label: component.label().to_owned(),
                celsius: trunc1(celsius),
            })
        })
        .collect()
}

/// Observe une interface : bouclage (par son nom), matériel (sous Linux, un périphérique est
/// derrière `/sys/class/net/<interface>/device` ; ailleurs on ne sait pas, la règle se rabat sur
/// toutes les interfaces sauf le bouclage).
fn observe_interface(name: &str) -> InterfaceKind {
    let lower = name.to_ascii_lowercase();
    InterfaceKind {
        loopback: lower == "lo" || lower == "lo0" || lower.contains("loopback"),
        physical: cfg!(target_os = "linux")
            && std::path::Path::new("/sys/class/net")
                .join(name)
                .join("device")
                .exists(),
    }
}

fn percent(value: f32) -> f32 {
    trunc1(value.clamp(0.0, 100.0))
}

impl SystemProbe for SysinfoProbe {
    fn identity(&self) -> MachineIdentity {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.disks.refresh_specifics(true, storage_only());
        state.components.refresh(false);
        state.temps = temperatures(&state.components);
        state.temps_refreshed_at = Instant::now();
        let first_cpu = state.system.cpus().first();
        MachineIdentity {
            name: gethostname::gethostname().to_string_lossy().into_owned(),
            os: OsIdentity {
                name: System::name().unwrap_or_else(|| std::env::consts::OS.to_owned()),
                version: System::long_os_version().or_else(System::os_version),
                kernel: System::kernel_version(),
                arch: System::cpu_arch(),
            },
            cpu: CpuIdentity {
                model: first_cpu
                    .map(|cpu| cpu.brand().trim().to_owned())
                    .unwrap_or_default(),
                physical_cores: System::physical_core_count().and_then(|n| u32::try_from(n).ok()),
                logical_cores: u32::try_from(state.system.cpus().len()).unwrap_or(u32::MAX),
                frequency_mhz: first_cpu.map(|cpu| cpu.frequency()).filter(|mhz| *mhz > 0),
            },
            memory_total_bytes: state.system.total_memory(),
            disks: to_volumes(&state.disks)
                .into_iter()
                .map(|volume| DiskIdentity {
                    name: volume.name,
                    mount: volume.mount,
                    fs: volume.fs,
                    total_bytes: volume.total_bytes,
                    removable: volume.removable,
                })
                .collect(),
            gpus: Vec::new(),
            has_temperature_sensors: !state.temps.is_empty(),
        }
    }

    fn sample(&self) -> Result<SystemSample, ProbeError> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);

        // Processeur : `sysinfo` exige un intervalle minimal entre deux rafraîchissements.
        let since = state.cpu_refreshed_at.elapsed();
        if since < MINIMUM_CPU_UPDATE_INTERVAL {
            std::thread::sleep(MINIMUM_CPU_UPDATE_INTERVAL - since);
        }
        state.system.refresh_cpu_usage();
        state.cpu_refreshed_at = Instant::now();
        state.system.refresh_memory();
        let cores: Vec<f32> = state
            .system
            .cpus()
            .iter()
            .map(|cpu| percent(cpu.cpu_usage()))
            .collect();

        // Réseau : octets échangés depuis le rafraîchissement précédent, sur le temps écoulé.
        let elapsed = state.net_refreshed_at.elapsed();
        state.networks.refresh(true);
        state.net_refreshed_at = Instant::now();
        let interfaces: Vec<_> = state.networks.iter().collect();
        let kinds: Vec<_> = interfaces
            .iter()
            .map(|(name, _)| observe_interface(name))
            .collect();
        let counted: Vec<_> = interfaces
            .iter()
            .zip(throughput_interfaces(&kinds))
            .filter_map(|(entry, counts)| counts.then_some(*entry))
            .collect();
        let net = (elapsed >= MIN_NET_INTERVAL && !counted.is_empty()).then(|| {
            let seconds = elapsed.as_secs_f64();
            let per_second = |bytes: u64| (bytes as f64 / seconds).round() as u64;
            NetRate {
                up_bytes_per_s: per_second(
                    counted.iter().map(|(_, data)| data.transmitted()).sum(),
                ),
                down_bytes_per_s: per_second(counted.iter().map(|(_, data)| data.received()).sum()),
            }
        });

        state.disks.refresh_specifics(true, storage_only());
        if state.temps_refreshed_at.elapsed() >= TEMPERATURE_REFRESH {
            state.components.refresh(false);
            state.temps = temperatures(&state.components);
            state.temps_refreshed_at = Instant::now();
        }

        Ok(SystemSample {
            uptime_s: System::uptime(),
            cpu: percent(state.system.global_cpu_usage()),
            cores,
            mem: MemoryUsage {
                used_bytes: state.system.used_memory(),
                total_bytes: state.system.total_memory(),
            },
            disks: to_volumes(&state.disks)
                .into_iter()
                .map(|volume| DiskUsage {
                    used_bytes: volume.used_bytes(),
                    name: volume.name,
                    mount: volume.mount,
                    total_bytes: volume.total_bytes,
                })
                .collect(),
            net,
            temps: state.temps.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_loopback_is_recognised_by_its_name() {
        for name in ["lo", "lo0", "Loopback Pseudo-Interface 1"] {
            assert!(observe_interface(name).loopback, "{name}");
        }
        for name in ["eth0", "Ethernet", "tailscale0"] {
            assert!(!observe_interface(name).loopback, "{name}");
        }
        // Une interface qui n'existe pas n'est pas physique.
        assert!(!observe_interface("hearth-test-no-such-if").physical);
    }

    #[test]
    fn the_identity_describes_this_machine() {
        let identity = SysinfoProbe::new().identity();
        assert!(!identity.name.is_empty());
        assert!(!identity.os.name.is_empty());
        assert!(!identity.os.arch.is_empty());
        assert!(identity.cpu.logical_cores >= 1);
        assert!(identity.memory_total_bytes > 0);
        assert!(
            identity.gpus.is_empty(),
            "les cartes viennent d'une autre sonde"
        );
        for disk in &identity.disks {
            assert!(disk.total_bytes > 0);
        }
    }

    #[test]
    fn a_sample_is_complete_and_sane() {
        let probe = SysinfoProbe::new();
        let first = probe.sample().unwrap();
        let second = probe.sample().unwrap();
        for sample in [&first, &second] {
            assert!((0.0..=100.0).contains(&sample.cpu), "{}", sample.cpu);
            assert!(!sample.cores.is_empty());
            assert!(sample.cores.iter().all(|c| (0.0..=100.0).contains(c)));
            assert!(sample.mem.total_bytes > 0);
            assert!(sample.mem.used_bytes <= sample.mem.total_bytes);
            for disk in &sample.disks {
                assert!(disk.used_bytes <= disk.total_bytes);
            }
            for temp in &sample.temps {
                assert!(PLAUSIBLE_CELSIUS.contains(&temp.celsius));
            }
        }
        // Le premier passage attend l'intervalle minimal de sysinfo : charge et débit sont réels.
        assert!(first.net.is_some() || cfg!(not(any(target_os = "linux", windows))));
    }

    /// Budget de la conception : un échantillon coûte moins de 5 ms (cible Linux). C'est une
    /// **mesure journalisée**, pas une assertion : le coût dépend de la machine et de sa charge
    /// (sous Windows l'énumération des interfaces réseau coûte à elle seule une douzaine de ms).
    /// Visible avec `cargo test -- --nocapture a_sample_cost`.
    #[test]
    fn a_sample_cost_is_measured_and_reported() {
        let probe = SysinfoProbe::new();
        probe.sample().unwrap();
        let mut costs = Vec::new();
        for _ in 0..5 {
            std::thread::sleep(MINIMUM_CPU_UPDATE_INTERVAL + Duration::from_millis(20));
            let started = Instant::now();
            probe.sample().unwrap();
            costs.push(started.elapsed());
        }
        costs.sort();
        eprintln!(
            "coût médian d'un échantillon : {:?} (cible Linux < 5 ms) {costs:?}",
            costs[costs.len() / 2]
        );
    }
}

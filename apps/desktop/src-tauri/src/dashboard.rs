//! Ce que la coquille dit à l'interface sur la machine d'un serveur (tableau de bord) : types
//! sérialisés (typés par tauri-specta) et conversions depuis `hearth-proto`. Pur : sans E/S, sans
//! Tauri.
//!
//! Les SEUILS d'alerte sont appliqués ici, par les fonctions de `hearth_proto::thresholds`
//! (BR-DASH-003, BR-DASH-004) : l'interface ne reçoit que des niveaux (`normal`, `attention`,
//! `critical`) et ne connaît aucun seuil. Le niveau du processeur « tenu 30 s » se calcule sur la
//! série au pas de 1 s tenue par [`DashBook`] (le flux en direct), jamais sur la fenêtre `1h`.
//!
//! Une mesure illisible reste absente (`None`), jamais un zéro inventé (BR-DASH-008). Les dates
//! sont des millisecondes depuis l'époque (`f64`), les quantités des octets (`f64`, exacts
//! jusqu'à 9 Po) : un `u64` n'existe pas côté web.

use std::collections::{HashMap, VecDeque};

use hearth_proto::api::machine::MachineResponse;
use hearth_proto::api::metrics::Sample;
use hearth_proto::thresholds::{CpuPoint, Level, cpu_level, temperature_level, usage_level};
use serde::Serialize;
use specta::Type;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

/// Noms des événements Tauri du tableau de bord.
pub mod events {
    pub const SNAPSHOT: &str = "link://snapshot";
    pub const METRICS: &str = "link://metrics";
    pub const HISTORY: &str = "link://history";
}

/// Points du processeur gardés par serveur : de quoi tenir [`CPU_HOLD_MS`] à 1 Hz avec de la marge.
const CPU_SERIES_CAP: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum LevelDto {
    Normal,
    Attention,
    Critical,
}

impl From<Level> for LevelDto {
    fn from(level: Level) -> Self {
        match level {
            Level::Normal => Self::Normal,
            Level::Attention => Self::Attention,
            Level::Critical => Self::Critical,
        }
    }
}

fn number(value: u64) -> f64 {
    value as f64
}

/// Un pourcentage ou une température à une décimale : le flux porte des `f32`, que `serde_json`
/// élargit en `f64` (`37.3` deviendrait `37.29999923706055`).
fn round1(value: f32) -> f64 {
    (f64::from(value) * 10.0).round() / 10.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OsDto {
    pub name: String,
    pub version: Option<String>,
    pub kernel: Option<String>,
    pub arch: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CpuInfoDto {
    pub model: String,
    pub physical_cores: Option<u32>,
    pub logical_cores: u32,
    pub frequency_mhz: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DiskInfoDto {
    pub name: String,
    pub mount: String,
    pub fs: Option<String>,
    pub total_bytes: f64,
    pub removable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GpuInfoDto {
    pub name: String,
    pub memory_total_bytes: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CapabilitiesDto {
    pub gpu: bool,
    pub temps: bool,
}

/// Identité de la machine (BR-DASH-001, 005, 006).
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MachineDto {
    pub name: String,
    pub os: OsDto,
    pub cpu: CpuInfoDto,
    pub memory_total_bytes: f64,
    pub disks: Vec<DiskInfoDto>,
    pub gpus: Vec<GpuInfoDto>,
    pub capabilities: CapabilitiesDto,
}

impl From<&MachineResponse> for MachineDto {
    fn from(machine: &MachineResponse) -> Self {
        Self {
            name: machine.name.clone(),
            os: OsDto {
                name: machine.os.name.clone(),
                version: machine.os.version.clone(),
                kernel: machine.os.kernel.clone(),
                arch: machine.os.arch.clone(),
            },
            cpu: CpuInfoDto {
                model: machine.cpu.model.clone(),
                physical_cores: machine.cpu.physical_cores,
                logical_cores: machine.cpu.logical_cores,
                frequency_mhz: machine.cpu.frequency_mhz.map(number),
            },
            memory_total_bytes: number(machine.memory_total_bytes),
            disks: machine
                .disks
                .iter()
                .map(|disk| DiskInfoDto {
                    name: disk.name.clone(),
                    mount: disk.mount.clone(),
                    fs: disk.fs.clone(),
                    total_bytes: number(disk.total_bytes),
                    removable: disk.removable,
                })
                .collect(),
            gpus: machine
                .gpus
                .iter()
                .map(|gpu| GpuInfoDto {
                    name: gpu.name.clone(),
                    memory_total_bytes: gpu.memory_total_bytes.map(number),
                })
                .collect(),
            capabilities: CapabilitiesDto {
                gpu: machine.capabilities.gpu,
                temps: machine.capabilities.temps,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MemoryDto {
    pub used_bytes: f64,
    pub total_bytes: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DiskSampleDto {
    pub name: String,
    pub mount: String,
    pub used_bytes: f64,
    pub total_bytes: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NetDto {
    pub up_bytes_per_s: f64,
    pub down_bytes_per_s: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GpuSampleDto {
    pub name: String,
    pub load_percent: Option<f64>,
    pub memory_used_bytes: Option<f64>,
    pub memory_total_bytes: Option<f64>,
    /// Absente quand la carte n'expose pas sa température (BR-DASH-007).
    pub temp_c: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TempDto {
    pub label: String,
    pub celsius: f64,
}

/// Une seconde de la vie de la machine.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SampleDto {
    /// Instant de la mesure (millisecondes depuis l'époque).
    pub at: f64,
    pub uptime_s: f64,
    pub cpu: f64,
    pub cores: Vec<f64>,
    pub mem: MemoryDto,
    pub disks: Vec<DiskSampleDto>,
    pub net: Option<NetDto>,
    pub gpus: Vec<GpuSampleDto>,
    pub temps: Vec<TempDto>,
}

impl SampleDto {
    /// `at_ms` : l'instant de la mesure, déjà converti (voir [`sample_millis`]).
    fn new(sample: &Sample, at_ms: i64) -> Self {
        Self {
            at: at_ms as f64,
            uptime_s: number(sample.uptime_s),
            cpu: round1(sample.cpu),
            cores: sample.cores.iter().map(|core| round1(*core)).collect(),
            mem: MemoryDto {
                used_bytes: number(sample.mem.used_bytes),
                total_bytes: number(sample.mem.total_bytes),
            },
            disks: sample
                .disks
                .iter()
                .map(|disk| DiskSampleDto {
                    name: disk.name.clone(),
                    mount: disk.mount.clone(),
                    used_bytes: number(disk.used_bytes),
                    total_bytes: number(disk.total_bytes),
                })
                .collect(),
            net: sample.net.map(|net| NetDto {
                up_bytes_per_s: number(net.up_bytes_per_s),
                down_bytes_per_s: number(net.down_bytes_per_s),
            }),
            gpus: sample
                .gpus
                .iter()
                .map(|gpu| GpuSampleDto {
                    name: gpu.name.clone(),
                    load_percent: gpu.load_percent.map(round1),
                    memory_used_bytes: gpu.memory_used_bytes.map(number),
                    memory_total_bytes: gpu.memory_total_bytes.map(number),
                    temp_c: gpu.temp_c.map(round1),
                })
                .collect(),
            temps: sample
                .temps
                .iter()
                .map(|temp| TempDto {
                    label: temp.label.clone(),
                    celsius: round1(temp.celsius),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GpuLevelsDto {
    pub memory: LevelDto,
    pub temp: LevelDto,
}

/// Niveau d'alerte de chaque mesure d'un échantillon (BR-DASH-003). Mêmes positions que les
/// listes de l'échantillon (`disks[i]`, `gpus[i]`, `temps[i]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LevelsDto {
    pub cpu: LevelDto,
    pub mem: LevelDto,
    pub disks: Vec<LevelDto>,
    pub gpus: Vec<GpuLevelsDto>,
    pub temps: Vec<LevelDto>,
}

/// Niveaux d'un échantillon ; `cpu` : niveau déjà tenu sur la série (BR-DASH-004).
fn levels(sample: &Sample, cpu: Level) -> LevelsDto {
    LevelsDto {
        cpu: cpu.into(),
        mem: usage_level(sample.mem.used_bytes, sample.mem.total_bytes).into(),
        disks: sample
            .disks
            .iter()
            .map(|disk| usage_level(disk.used_bytes, disk.total_bytes).into())
            .collect(),
        gpus: sample
            .gpus
            .iter()
            .map(|gpu| GpuLevelsDto {
                memory: match (gpu.memory_used_bytes, gpu.memory_total_bytes) {
                    (Some(used), Some(total)) => usage_level(used, total).into(),
                    _ => LevelDto::Normal,
                },
                temp: gpu.temp_c.map_or(LevelDto::Normal, |celsius| {
                    temperature_level(celsius).into()
                }),
            })
            .collect(),
        temps: sample
            .temps
            .iter()
            .map(|temp| temperature_level(temp.celsius).into())
            .collect(),
    }
}

/// Instant d'un échantillon en millisecondes (RFC 3339) ; `fallback_ms` s'il est illisible.
pub fn sample_millis(sample: &Sample, fallback_ms: i64) -> i64 {
    OffsetDateTime::parse(&sample.at, &Rfc3339)
        .ok()
        .and_then(|date| i64::try_from(date.unix_timestamp_nanos() / 1_000_000).ok())
        .unwrap_or(fallback_ms)
}

/// Un échantillon en direct (`link://metrics`, chaque seconde).
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MetricsEvent {
    pub server_id: String,
    pub sample: SampleDto,
    pub levels: LevelsDto,
}

/// Identité et historique (`link://snapshot`, et lecture `get_dashboard`). `levels` : ceux du
/// dernier échantillon de `history`, s'il y en a un.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotEvent {
    pub server_id: String,
    pub machine: MachineDto,
    pub history: Vec<SampleDto>,
    pub levels: Option<LevelsDto>,
}

/// « Maintenant » en millisecondes qui ne recule jamais : l'horloge MURALE lue UNE seule fois à la construction,
/// puis l'horloge MONOTONE (`Instant`). La série du processeur et le repli d'un échantillon à la date illisible
/// ne suivent donc pas un saut de l'horloge murale (changement d'heure, synchronisation, réglage à la main).
/// FIX:01M4CRD60RKGZC2HTT52GK6P2T
#[derive(Debug, Clone, Copy)]
pub struct MonoMs {
    start: std::time::Instant,
    base_ms: i64,
}

impl MonoMs {
    /// `wall` n'est appelée qu'ici, une fois.
    pub fn new(wall: impl FnOnce() -> i64, start: std::time::Instant) -> Self {
        Self {
            start,
            base_ms: wall(),
        }
    }

    /// Les millisecondes à l'instant `now` : la base murale plus le temps écoulé sur l'horloge monotone.
    pub fn at(&self, now: std::time::Instant) -> i64 {
        let elapsed = now.saturating_duration_since(self.start).as_millis();
        self.base_ms
            .saturating_add(i64::try_from(elapsed).unwrap_or(i64::MAX))
    }
}

/// L'heure écoulée avant l'instantané (`link://history`), à 1 échantillon par 10 secondes : le tableau de bord
/// amorce sa courbe d'une heure dès l'ouverture (BR-DASH-010). Plus ancienne que l'instantané.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEvent {
    pub server_id: String,
    pub history: Vec<SampleDto>,
}

/// Les points de la série du processeur d'un historique : l'écart entre deux points est celui de
/// l'agent, mais la série est ramenée à l'instant de réception (le dernier point est « maintenant ») :
/// une horloge de l'agent qui recule ou qui diffère de celle du poste ne fausse pas les 30 s.
fn cpu_points(samples: &[Sample], at: &[i64], now_ms: i64) -> Vec<CpuPoint> {
    let shift = at.last().map_or(0, |last| now_ms.saturating_sub(*last));
    let mut out = Vec::with_capacity(samples.len());
    let mut last = i64::MIN;
    for (sample, at_ms) in samples.iter().zip(at) {
        let at_ms = at_ms.saturating_add(shift).max(last.saturating_add(1));
        last = at_ms;
        out.push(CpuPoint {
            at_ms,
            percent: sample.cpu,
        });
    }
    out
}

/// Instants des échantillons d'un historique, dans l'ordre.
fn millis_of(samples: &[Sample], fallback_ms: i64) -> Vec<i64> {
    samples
        .iter()
        .map(|sample| sample_millis(sample, fallback_ms))
        .collect()
}

/// Instantané d'après un historique (du plus ancien au plus récent) : sans état, pour la
/// lecture `get_dashboard` et pour l'instantané d'une connexion.
pub fn snapshot(
    server_id: &str,
    machine: &MachineResponse,
    history: &[Sample],
    now_ms: i64,
) -> (SnapshotEvent, Vec<CpuPoint>) {
    let at = millis_of(history, now_ms);
    let series = cpu_points(history, &at, now_ms);
    let last_levels = history
        .last()
        .map(|sample| levels(sample, cpu_level(&series)));
    let event = SnapshotEvent {
        server_id: server_id.to_owned(),
        machine: MachineDto::from(machine),
        history: history
            .iter()
            .zip(&at)
            .map(|(sample, at_ms)| SampleDto::new(sample, *at_ms))
            .collect(),
        levels: last_levels,
    };
    (event, series)
}

/// Série du processeur au pas de 1 s tenue par serveur (BR-DASH-004) : la seule mémoire de ce
/// module. Bornée ; un serveur retiré s'oublie.
#[derive(Debug, Default)]
pub struct DashBook {
    cpu: HashMap<String, VecDeque<CpuPoint>>,
    /// L'heure avant l'instantané, par serveur : rejouée avec la dernière vue à une interface qui s'ouvre
    /// après la connexion (un événement n'est qu'un signal). Jamais mêlée à la série du processeur.
    older: HashMap<String, Vec<SampleDto>>,
}

impl DashBook {
    /// L'instantané d'une connexion : la série repart de l'historique reçu.
    pub fn on_snapshot(
        &mut self,
        server_id: &str,
        machine: &MachineResponse,
        history: &[Sample],
        now_ms: i64,
    ) -> SnapshotEvent {
        let (event, series) = snapshot(server_id, machine, history, now_ms);
        let keep = series.len().saturating_sub(CPU_SERIES_CAP);
        self.cpu.insert(
            server_id.to_owned(),
            series.into_iter().skip(keep).collect(),
        );
        event
    }

    /// Un échantillon en direct : niveaux à jour, série du processeur prolongée.
    pub fn on_metrics(&mut self, server_id: &str, sample: &Sample, now_ms: i64) -> MetricsEvent {
        let series = self.cpu.entry(server_id.to_owned()).or_default();
        // Instant de RÉCEPTION : la série du processeur ne dépend pas de l'horloge de l'agent.
        let mut at_ms = now_ms;
        if let Some(last) = series.back() {
            at_ms = at_ms.max(last.at_ms.saturating_add(1));
        }
        series.push_back(CpuPoint {
            at_ms,
            percent: sample.cpu,
        });
        while series.len() > CPU_SERIES_CAP {
            series.pop_front();
        }
        let level = cpu_level(series.make_contiguous());
        MetricsEvent {
            server_id: server_id.to_owned(),
            sample: SampleDto::new(sample, sample_millis(sample, now_ms)),
            levels: levels(sample, level),
        }
    }

    /// L'heure écoulée avant l'instantané d'une connexion : retenue pour être rejouée, annoncée à l'interface.
    pub fn on_history(&mut self, server_id: &str, samples: &[Sample], now_ms: i64) -> HistoryEvent {
        let history: Vec<SampleDto> = samples
            .iter()
            .map(|sample| SampleDto::new(sample, sample_millis(sample, now_ms)))
            .collect();
        self.older.insert(server_id.to_owned(), history.clone());
        HistoryEvent {
            server_id: server_id.to_owned(),
            history,
        }
    }

    /// Ajoute à une vue l'heure retenue qui la précède (strictement plus ancienne que son premier échantillon).
    pub fn with_older(&self, mut view: SnapshotEvent) -> SnapshotEvent {
        let Some(older) = self.older.get(&view.server_id) else {
            return view;
        };
        let limit = view.history.first().map_or(f64::INFINITY, |first| first.at);
        let mut history: Vec<SampleDto> = older
            .iter()
            .filter(|sample| sample.at < limit)
            .cloned()
            .collect();
        history.append(&mut view.history);
        view.history = history;
        view
    }

    pub fn forget(&mut self, server_id: &str) {
        self.cpu.remove(server_id);
        self.older.remove(server_id);
    }
}

//! Mesures : l'échantillon, l'anneau d'une heure, les fenêtres d'historique et leur
//! rééchantillonnage (BR-DASH-008, BR-DASH-010, BR-DASH-011). Fonctions pures.
//!
//! Une mesure illisible est absente (`None`, liste vide) : jamais un zéro inventé, et les autres
//! mesures de l'échantillon ne sont pas touchées.

use std::collections::VecDeque;

use time::{Duration, OffsetDateTime};

/// Taille de l'anneau : une heure à un échantillon par seconde. Rien n'est écrit sur disque.
pub const RING_CAPACITY: usize = 3_600;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryUsage {
    pub used_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskUsage {
    pub name: String,
    pub mount: String,
    pub used_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetRate {
    pub up_bytes_per_s: u64,
    pub down_bytes_per_s: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GpuReading {
    pub name: String,
    pub load_percent: Option<f32>,
    pub memory_used_bytes: Option<u64>,
    pub memory_total_bytes: Option<u64>,
    pub temp_c: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TempReading {
    pub label: String,
    pub celsius: f32,
}

/// Ce que la sonde système rend à chaque passage (sans date ni carte graphique).
#[derive(Debug, Clone, PartialEq)]
pub struct SystemSample {
    pub uptime_s: u64,
    /// Charge globale du processeur, 0 à 100.
    pub cpu: f32,
    pub cores: Vec<f32>,
    pub mem: MemoryUsage,
    pub disks: Vec<DiskUsage>,
    pub net: Option<NetRate>,
    pub temps: Vec<TempReading>,
}

/// Une seconde de la vie de la machine.
#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    pub at: OffsetDateTime,
    pub uptime_s: u64,
    pub cpu: f32,
    pub cores: Vec<f32>,
    pub mem: MemoryUsage,
    pub disks: Vec<DiskUsage>,
    pub net: Option<NetRate>,
    pub gpus: Vec<GpuReading>,
    pub temps: Vec<TempReading>,
}

impl Sample {
    pub fn new(at: OffsetDateTime, system: SystemSample, gpus: Vec<GpuReading>) -> Self {
        Self {
            at,
            uptime_s: system.uptime_s,
            cpu: system.cpu,
            cores: system.cores,
            mem: system.mem,
            disks: system.disks,
            net: system.net,
            gpus,
            temps: system.temps,
        }
    }
}

/// Fenêtre d'historique : 1 min et 5 min à un échantillon par seconde, 1 h à un par 10 s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HistoryWindow {
    OneMinute,
    #[default]
    FiveMinutes,
    OneHour,
}

impl HistoryWindow {
    /// Valeur du paramètre `window` (`1m`, `5m`, `1h`).
    pub fn parse(label: &str) -> Option<Self> {
        match label {
            "1m" => Some(Self::OneMinute),
            "5m" => Some(Self::FiveMinutes),
            "1h" => Some(Self::OneHour),
            _ => None,
        }
    }

    pub fn span(self) -> Duration {
        match self {
            Self::OneMinute => Duration::minutes(1),
            Self::FiveMinutes => Duration::minutes(5),
            Self::OneHour => Duration::hours(1),
        }
    }

    /// Pas des échantillons rendus, en secondes.
    pub fn step_s(self) -> u32 {
        match self {
            Self::OneMinute | Self::FiveMinutes => 1,
            Self::OneHour => 10,
        }
    }
}

/// Les derniers échantillons, du plus ancien au plus récent ; le plus ancien est oublié quand
/// l'anneau est plein.
#[derive(Debug, Clone)]
pub struct Ring {
    samples: VecDeque<Sample>,
    capacity: usize,
}

impl Default for Ring {
    fn default() -> Self {
        Self::with_capacity(RING_CAPACITY)
    }
}

impl Ring {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(capacity.min(RING_CAPACITY)),
            capacity: capacity.max(1),
        }
    }

    pub fn push(&mut self, sample: Sample) {
        if self.samples.len() == self.capacity {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub fn latest(&self) -> Option<&Sample> {
        self.samples.back()
    }

    /// Les échantillons plus récents que `since` (exclu), bruts.
    pub fn since(&self, since: OffsetDateTime) -> Vec<Sample> {
        self.samples
            .iter()
            .filter(|sample| sample.at > since)
            .cloned()
            .collect()
    }

    /// L'historique d'une fenêtre à `now` : les échantillons des dernières `window.span()`,
    /// rééchantillonnés au pas de la fenêtre.
    pub fn history(&self, window: HistoryWindow, now: OffsetDateTime) -> Vec<Sample> {
        resample(&self.since(now - window.span()), window.step_s())
    }
}

/// Réduit une série (du plus ancien au plus récent) à un échantillon par pas de `step_s`
/// secondes, les pas étant alignés sur les multiples de `step_s` de l'horloge. Chaque pas devient
/// la moyenne de ses échantillons, daté de son dernier ; les listes (cœurs, disques, cartes
/// graphiques, sondes) suivent le dernier échantillon du pas. Une mesure absente de tout un pas
/// reste absente (BR-DASH-008). Un pas de 1 s ou moins rend la série telle quelle.
pub fn resample(samples: &[Sample], step_s: u32) -> Vec<Sample> {
    if step_s <= 1 {
        return samples.to_vec();
    }
    let step = i64::from(step_s);
    let mut out = Vec::new();
    let mut start = 0;
    while start < samples.len() {
        let bucket = samples[start].at.unix_timestamp().div_euclid(step);
        let end = samples[start..]
            .iter()
            .position(|sample| sample.at.unix_timestamp().div_euclid(step) != bucket)
            .map_or(samples.len(), |offset| start + offset);
        out.push(average(&samples[start..end]));
        start = end;
    }
    out
}

fn mean_f32(values: impl Iterator<Item = f32>) -> Option<f32> {
    let (sum, count) = values.fold((0.0_f64, 0_u32), |(sum, count), value| {
        (sum + f64::from(value), count + 1)
    });
    (count > 0).then(|| (sum / f64::from(count)) as f32)
}

fn mean_u64(values: impl Iterator<Item = u64>) -> Option<u64> {
    let (sum, count) = values.fold((0_u128, 0_u128), |(sum, count), value| {
        (sum + u128::from(value), count + 1)
    });
    (count > 0).then(|| u64::try_from(sum / count).unwrap_or(u64::MAX))
}

/// Moyenne d'un groupe non vide d'échantillons consécutifs.
fn average(group: &[Sample]) -> Sample {
    let Some(last) = group.last() else {
        // Les groupes viennent de `resample`, jamais vides.
        return Sample {
            at: OffsetDateTime::UNIX_EPOCH,
            uptime_s: 0,
            cpu: 0.0,
            cores: vec![],
            mem: MemoryUsage {
                used_bytes: 0,
                total_bytes: 0,
            },
            disks: vec![],
            net: None,
            gpus: vec![],
            temps: vec![],
        };
    };
    let cores = (0..last.cores.len())
        .map(|i| {
            mean_f32(group.iter().filter_map(|s| s.cores.get(i).copied())).unwrap_or(last.cores[i])
        })
        .collect();
    let disks = last
        .disks
        .iter()
        .map(|disk| DiskUsage {
            used_bytes: mean_u64(
                group
                    .iter()
                    .flat_map(|s| s.disks.iter())
                    .filter(|d| d.mount == disk.mount)
                    .map(|d| d.used_bytes),
            )
            .unwrap_or(disk.used_bytes),
            ..disk.clone()
        })
        .collect();
    let net = {
        let rates: Vec<_> = group.iter().filter_map(|s| s.net).collect();
        mean_u64(rates.iter().map(|r| r.up_bytes_per_s))
            .zip(mean_u64(rates.iter().map(|r| r.down_bytes_per_s)))
    }
    .map(|(up_bytes_per_s, down_bytes_per_s)| NetRate {
        up_bytes_per_s,
        down_bytes_per_s,
    });
    let gpus = last
        .gpus
        .iter()
        .enumerate()
        .map(|(i, gpu)| {
            let same = || {
                group
                    .iter()
                    .filter_map(move |s| s.gpus.get(i))
                    .filter(|g| g.name == gpu.name)
            };
            GpuReading {
                name: gpu.name.clone(),
                load_percent: mean_f32(same().filter_map(|g| g.load_percent)),
                memory_used_bytes: mean_u64(same().filter_map(|g| g.memory_used_bytes)),
                memory_total_bytes: gpu.memory_total_bytes,
                temp_c: mean_f32(same().filter_map(|g| g.temp_c)),
            }
        })
        .collect();
    let temps = last
        .temps
        .iter()
        .map(|temp| TempReading {
            label: temp.label.clone(),
            celsius: mean_f32(
                group
                    .iter()
                    .flat_map(|s| s.temps.iter())
                    .filter(|t| t.label == temp.label)
                    .map(|t| t.celsius),
            )
            .unwrap_or(temp.celsius),
        })
        .collect();
    Sample {
        at: last.at,
        uptime_s: last.uptime_s,
        cpu: mean_f32(group.iter().map(|s| s.cpu)).unwrap_or(last.cpu),
        cores,
        mem: MemoryUsage {
            used_bytes: mean_u64(group.iter().map(|s| s.mem.used_bytes))
                .unwrap_or(last.mem.used_bytes),
            total_bytes: last.mem.total_bytes,
        },
        disks,
        net,
        gpus,
        temps,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: i64) -> OffsetDateTime {
        OffsetDateTime::UNIX_EPOCH + Duration::seconds(1_790_000_000 + seconds)
    }

    fn sample(seconds: i64, cpu: f32) -> Sample {
        Sample {
            at: at(seconds),
            uptime_s: seconds as u64,
            cpu,
            cores: vec![cpu, cpu / 2.0],
            mem: MemoryUsage {
                used_bytes: 100 + seconds as u64,
                total_bytes: 1_000,
            },
            disks: vec![DiskUsage {
                name: "/dev/sda".into(),
                mount: "/".into(),
                used_bytes: 10,
                total_bytes: 100,
            }],
            net: Some(NetRate {
                up_bytes_per_s: 10,
                down_bytes_per_s: 20,
            }),
            gpus: vec![],
            temps: vec![],
        }
    }

    #[test]
    fn the_ring_keeps_the_last_hour_and_forgets_the_oldest() {
        let mut ring = Ring::default();
        for i in 0..(RING_CAPACITY as i64 + 5) {
            ring.push(sample(i, 1.0));
        }
        assert_eq!(ring.len(), RING_CAPACITY);
        assert_eq!(
            ring.since(at(-1)).first().map(|s| s.at),
            Some(at(5)),
            "les 5 plus anciens ont été oubliés"
        );
        assert_eq!(
            ring.latest().map(|s| s.at),
            Some(at(RING_CAPACITY as i64 + 4))
        );
    }

    #[test]
    fn an_empty_ring_has_no_history() {
        let ring = Ring::default();
        assert!(ring.is_empty());
        assert!(ring.history(HistoryWindow::OneHour, at(0)).is_empty());
        assert!(ring.latest().is_none());
    }

    #[test]
    fn window_labels_spans_and_steps() {
        assert_eq!(HistoryWindow::parse("1m"), Some(HistoryWindow::OneMinute));
        assert_eq!(HistoryWindow::parse("5m"), Some(HistoryWindow::FiveMinutes));
        assert_eq!(HistoryWindow::parse("1h"), Some(HistoryWindow::OneHour));
        for label in ["", "10m", "1H", "60m", "1 m"] {
            assert_eq!(HistoryWindow::parse(label), None, "{label:?}");
        }
        assert_eq!(HistoryWindow::default(), HistoryWindow::FiveMinutes);
        let steps: Vec<_> = [
            HistoryWindow::OneMinute,
            HistoryWindow::FiveMinutes,
            HistoryWindow::OneHour,
        ]
        .map(|w| (w.span().whole_seconds(), w.step_s()))
        .into();
        assert_eq!(steps, [(60, 1), (300, 1), (3_600, 10)]);
    }

    #[test]
    fn short_windows_return_one_sample_per_second_up_to_now() {
        let mut ring = Ring::default();
        for i in 0..400 {
            ring.push(sample(i, 1.0));
        }
        let now = at(399);
        let minute = ring.history(HistoryWindow::OneMinute, now);
        assert_eq!(minute.len(), 60);
        assert_eq!(minute.last().map(|s| s.at), Some(at(399)));
        assert_eq!(minute.first().map(|s| s.at), Some(at(340)));
        let five = ring.history(HistoryWindow::FiveMinutes, now);
        assert_eq!(five.len(), 300);
        assert_eq!(five.first().map(|s| s.at), Some(at(100)));
    }

    #[test]
    fn the_hour_is_averaged_by_ten_seconds() {
        let mut ring = Ring::default();
        // Cpu = numéro de seconde : la moyenne d'un pas [10k, 10k+9] vaut 10k + 4,5.
        for i in 0..100 {
            ring.push(sample(i, i as f32));
        }
        let hour = ring.history(HistoryWindow::OneHour, at(99));
        assert_eq!(hour.len(), 10);
        let base = at(0).unix_timestamp().rem_euclid(10);
        // Les pas sont alignés sur l'horloge : 1 790 000 000 est un multiple de 10.
        assert_eq!(base, 0);
        assert_eq!(hour[0].cpu, 4.5);
        assert_eq!(hour[1].cpu, 14.5);
        assert_eq!(hour[0].at, at(9), "daté de son dernier échantillon");
        assert_eq!(hour[0].cores[0], 4.5);
        assert_eq!(hour[0].cores[1], 2.25);
        assert_eq!(hour[0].mem.used_bytes, 104, "moyenne entière de 100..109");
    }

    #[test]
    fn a_partial_step_is_still_a_step() {
        let samples: Vec<_> = (3..8).map(|i| sample(i, 10.0)).collect();
        let out = resample(&samples, 10);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].at, at(7));
    }

    #[test]
    fn a_measure_absent_from_a_whole_step_stays_absent() {
        let mut samples: Vec<_> = (0..10).map(|i| sample(i, 10.0)).collect();
        for s in &mut samples {
            s.net = None;
        }
        let out = resample(&samples, 10);
        assert_eq!(out[0].net, None);
    }

    #[test]
    fn a_measure_present_in_part_of_a_step_averages_what_exists() {
        let mut samples: Vec<_> = (0..10).map(|i| sample(i, 10.0)).collect();
        for s in samples.iter_mut().take(5) {
            s.net = None;
        }
        samples[7].net = Some(NetRate {
            up_bytes_per_s: 20,
            down_bytes_per_s: 40,
        });
        let out = resample(&samples, 10);
        // 4 échantillons à 10/20 et 1 à 20/40 : 12 / 24.
        assert_eq!(
            out[0].net,
            Some(NetRate {
                up_bytes_per_s: 12,
                down_bytes_per_s: 24
            })
        );
    }

    #[test]
    fn lists_follow_the_last_sample_of_the_step() {
        let mut samples: Vec<_> = (0..10).map(|i| sample(i, 10.0)).collect();
        // Un disque monté en cours de pas, une sonde et une carte graphique qui apparaissent.
        samples[9].disks.push(DiskUsage {
            name: "/dev/sdb".into(),
            mount: "/mnt/usb".into(),
            used_bytes: 7,
            total_bytes: 70,
        });
        for s in &mut samples[5..] {
            s.temps.push(TempReading {
                label: "cpu".into(),
                celsius: 50.0,
            });
            s.gpus.push(GpuReading {
                name: "gpu".into(),
                load_percent: Some(30.0),
                memory_used_bytes: None,
                memory_total_bytes: Some(8),
                temp_c: None,
            });
        }
        samples[9].gpus[0].load_percent = Some(50.0);
        let out = resample(&samples, 10);
        assert_eq!(out[0].disks.len(), 2);
        assert_eq!(out[0].disks[1].used_bytes, 7);
        assert_eq!(out[0].temps.len(), 1);
        assert_eq!(out[0].temps[0].celsius, 50.0);
        assert_eq!(out[0].gpus[0].load_percent, Some(34.0));
        assert_eq!(out[0].gpus[0].memory_used_bytes, None);
        assert_eq!(out[0].gpus[0].temp_c, None);
    }

    #[test]
    fn a_step_of_one_second_is_the_series_itself() {
        let samples: Vec<_> = (0..5).map(|i| sample(i, i as f32)).collect();
        assert_eq!(resample(&samples, 1), samples);
        assert!(resample(&[], 10).is_empty());
    }
}

//! L'heure écoulée des mesures d'un serveur (BR-DASH-010, ADR-0015 §5) : ce que `GET /metrics/history?window=1h`
//! apporte EN PLUS de l'instantané du flux. Pur : aucune E/S.
//!
//! L'instantané du flux porte les 5 dernières minutes à 1 échantillon par seconde ; l'historique d'une heure
//! rend la même fin à 1 échantillon par 10 secondes (moyennes). Seuls les échantillons PLUS ANCIENS que le
//! premier de l'instantané sont gardés : l'heure ne remplace jamais le détail à la seconde.

use hearth_proto::api::metrics::{Sample, StepPeak};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

fn at_of(sample: &Sample) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(&sample.at, &Rfc3339).ok()
}

/// Ce que l'agent a rendu comme maxima par rapport aux échantillons de l'heure : de quoi décider d'un journal
/// visible quand la correction ne peut pas s'appliquer (un repli silencieux laisserait la courbe sur les
/// moyennes sans que personne le voie).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeaksFit {
    /// Rien à corriger : aucun échantillon.
    NoSamples,
    /// Un maximum par échantillon : appliqués.
    Matching,
    /// Aucun maximum (agent plus ancien, ou route qui a cessé d'en rendre).
    Absent { samples: usize },
    /// Un nombre de maxima différent du nombre d'échantillons : ignorés.
    Mismatch { samples: usize, peaks: usize },
}

pub fn peaks_fit(samples: &[Sample], peaks: &[StepPeak]) -> PeaksFit {
    match (samples.len(), peaks.len()) {
        (0, _) => PeaksFit::NoSamples,
        (n, p) if n == p => PeaksFit::Matching,
        (n, 0) => PeaksFit::Absent { samples: n },
        (n, p) => PeaksFit::Mismatch {
            samples: n,
            peaks: p,
        },
    }
}

/// Remplace, dans chaque échantillon de l'heure (une MOYENNE de 10 s), les mesures tracées par le MAXIMUM de son
/// pas (`peaks`, un par échantillon, même ordre) : le client trace le maximum sur la fenêtre d'une heure, comme
/// en direct, donc un pic d'une seconde ne disparaît pas quand il vieillit. Si l'agent ne rend pas de maxima
/// (agent plus ancien) ou pas autant que d'échantillons, les moyennes restent telles quelles.
// FIX:01M4CY8BVM3MV9769QWNDNW7VT
// FIX:01M4D6KNC8G4T68J25J7DXMMNM : `peaks_fit` dit si les maxima s'appliquent (journal de `read_hour`).
pub fn with_peaks(mut samples: Vec<Sample>, peaks: &[StepPeak]) -> Vec<Sample> {
    if peaks.len() != samples.len() {
        return samples;
    }
    for (sample, peak) in samples.iter_mut().zip(peaks) {
        sample.cpu = peak.cpu;
        sample.mem.used_bytes = peak.mem_used_bytes;
        if peak.net.is_some() {
            sample.net = peak.net;
        }
        for (gpu, peak) in sample.gpus.iter_mut().zip(&peak.gpus) {
            if peak.load_percent.is_some() {
                gpu.load_percent = peak.load_percent;
            }
            if peak.memory_used_bytes.is_some() {
                gpu.memory_used_bytes = peak.memory_used_bytes;
            }
            if peak.temp_c.is_some() {
                gpu.temp_c = peak.temp_c;
            }
        }
        // Les sondes sont rendues dans le même ordre que celles de l'échantillon (deux sondes homonymes
        // comprises) : on les apparie par rang, le nom ne servant qu'à refuser un décalage.
        // FIX:01M4D6KN655K009FC3JR9H70VN
        for (temp, peak) in sample.temps.iter_mut().zip(&peak.temps) {
            if temp.label == peak.label {
                temp.celsius = peak.celsius;
            }
        }
    }
    samples
}

/// Les échantillons de `hour` strictement plus anciens que le premier de `snapshot` (tous, si
/// l'instantané est vide), du plus ancien au plus récent. Un échantillon dont la date est illisible est
/// écarté : il ne peut pas être rangé.
pub fn older_than_snapshot(hour: Vec<Sample>, snapshot: &[Sample]) -> Vec<Sample> {
    let limit = snapshot.iter().find_map(at_of);
    hour.into_iter()
        .filter(|sample| match (at_of(sample), limit) {
            (Some(at), Some(limit)) => at < limit,
            (Some(_), None) => true,
            (None, _) => false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hearth_proto::api::metrics::MemorySample;

    fn at(text: &str) -> Sample {
        Sample {
            at: text.into(),
            uptime_s: 1,
            cpu: 1.0,
            cores: vec![],
            mem: MemorySample {
                used_bytes: 1,
                total_bytes: 2,
            },
            disks: vec![],
            net: None,
            gpus: vec![],
            temps: vec![],
        }
    }

    #[test]
    fn only_what_is_older_than_the_first_second_of_the_snapshot_is_kept() {
        let snapshot = vec![
            at("2026-10-08T10:00:00.250Z"),
            at("2026-10-08T10:00:01.250Z"),
        ];
        let hour = vec![
            at("2026-10-08T09:59:50Z"),
            at("2026-10-08T10:00:00.250Z"),
            at("2026-10-08T10:00:10Z"),
        ];
        let kept = older_than_snapshot(hour, &snapshot);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].at, "2026-10-08T09:59:50Z");
    }

    #[test]
    fn the_peaks_replace_the_means_so_a_spike_is_not_lost_and_a_missing_or_short_list_changes_nothing()
     {
        use hearth_proto::api::metrics::{GpuPeak, GpuSample, NetSample, TempPeak, TempSample};
        let mut mean = at("2026-10-08T09:00:00Z");
        mean.cpu = 19.0;
        mean.gpus = vec![GpuSample {
            name: "g".into(),
            load_percent: Some(20.0),
            memory_used_bytes: Some(5),
            memory_total_bytes: Some(10),
            temp_c: Some(50.0),
        }];
        mean.temps = vec![
            TempSample {
                label: "cpu".into(),
                celsius: 45.0,
            },
            TempSample {
                label: "ssd".into(),
                celsius: 30.0,
            },
        ];
        let peak = StepPeak {
            cpu: 100.0,
            mem_used_bytes: 7,
            net: Some(NetSample {
                up_bytes_per_s: 3,
                down_bytes_per_s: 4,
            }),
            gpus: vec![GpuPeak {
                load_percent: Some(90.0),
                memory_used_bytes: None,
                temp_c: Some(80.0),
            }],
            temps: vec![
                TempPeak {
                    label: "cpu".into(),
                    celsius: 95.0,
                },
                // Une sonde qui ne porte pas le même nom que celle de l'échantillon : ignorée.
                TempPeak {
                    label: "autre".into(),
                    celsius: 99.0,
                },
            ],
        };
        let out = with_peaks(vec![mean.clone()], std::slice::from_ref(&peak));
        assert_eq!(out[0].gpus[0].temp_c, Some(80.0), "température de la carte");
        assert_eq!(out[0].temps[0].celsius, 95.0, "température de la sonde");
        assert_eq!(
            out[0].temps[1].celsius, 30.0,
            "autre sonde : moyenne gardée"
        );
        assert_eq!(out[0].cpu, 100.0);
        assert_eq!(out[0].mem.used_bytes, 7);
        assert_eq!(out[0].gpus[0].load_percent, Some(90.0));
        assert_eq!(
            out[0].gpus[0].memory_used_bytes,
            Some(5),
            "sans maximum, la moyenne reste"
        );
        assert_eq!(out[0].net.map(|n| n.down_bytes_per_s), Some(4));
        // Aucun maximum (agent plus ancien) ou une liste d'une autre longueur : les moyennes, intactes.
        assert_eq!(with_peaks(vec![mean.clone()], &[])[0].cpu, 19.0);
        assert_eq!(with_peaks(vec![mean.clone(), mean], &[peak])[0].cpu, 19.0);
    }

    #[test]
    fn the_fit_of_the_peaks_tells_a_missing_or_short_list_so_it_can_be_logged() {
        let two = vec![at("2026-10-08T09:00:00Z"), at("2026-10-08T09:00:10Z")];
        let one = vec![StepPeak {
            cpu: 1.0,
            mem_used_bytes: 1,
            net: None,
            gpus: vec![],
            temps: vec![],
        }];
        assert_eq!(peaks_fit(&[], &[]), PeaksFit::NoSamples);
        assert_eq!(peaks_fit(&two, &[]), PeaksFit::Absent { samples: 2 });
        assert_eq!(
            peaks_fit(&two, &one),
            PeaksFit::Mismatch {
                samples: 2,
                peaks: 1
            }
        );
        assert_eq!(peaks_fit(&two[..1], &one), PeaksFit::Matching);
    }

    #[test]
    fn an_empty_snapshot_keeps_the_whole_hour_and_unreadable_dates_are_dropped() {
        let hour = vec![at("2026-10-08T09:59:50Z"), at("pas une date")];
        let kept = older_than_snapshot(hour, &[]);
        assert_eq!(kept.len(), 1);
    }

    #[test]
    fn dates_with_different_subsecond_digits_compare_as_dates_not_as_text() {
        // « …:15Z » est AVANT « …:15.25Z », alors que « Z » suit « . » dans l'ordre des caractères.
        let snapshot = vec![at("2026-10-08T10:00:15.25Z")];
        let kept = older_than_snapshot(vec![at("2026-10-08T10:00:15Z")], &snapshot);
        assert_eq!(kept.len(), 1);
    }
}

//! Conversions du tableau de bord : seuils appliqués par `hearth-proto` (jamais par l'interface),
//! niveau du processeur « tenu 30 s » sur la série en direct, mesures absentes laissées absentes
//! (BR-DASH-003, 004, 005, 006, 007, 008).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use hearth_desktop_lib::dashboard::{DashBook, LevelDto, sample_millis, snapshot};
use hearth_proto::api::machine::{
    Capabilities, CpuInfo, DiskInfo, GpuInfo, MachineResponse, OsInfo,
};
use hearth_proto::api::metrics::{
    DiskSample, GpuSample, MemorySample, NetSample, Sample, TempSample,
};

const GIB: u64 = 1 << 30;

fn machine(gpu: bool, temps: bool) -> MachineResponse {
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
            frequency_mhz: Some(4500),
        },
        memory_total_bytes: 64 * GIB,
        disks: vec![DiskInfo {
            name: "/dev/nvme0n1p2".into(),
            mount: "/".into(),
            fs: Some("ext4".into()),
            total_bytes: 1000 * GIB,
            removable: false,
        }],
        gpus: if gpu {
            vec![GpuInfo {
                name: "RTX 4090".into(),
                memory_total_bytes: Some(24 * GIB),
            }]
        } else {
            vec![]
        },
        capabilities: Capabilities { gpu, temps },
    }
}

/// Date RFC 3339 de la seconde `n` après le 2026-10-04 10:00:00.
fn at(second: u32) -> String {
    format!("2026-10-04T10:{:02}:{:02}.250Z", second / 60, second % 60)
}

fn sample(second: u32, cpu: f32) -> Sample {
    Sample {
        at: at(second),
        uptime_s: 266_400,
        cpu,
        cores: vec![cpu; 4],
        mem: MemorySample {
            used_bytes: 8 * GIB,
            total_bytes: 64 * GIB,
        },
        disks: vec![DiskSample {
            name: "/dev/nvme0n1p2".into(),
            mount: "/".into(),
            used_bytes: 400 * GIB,
            total_bytes: 1000 * GIB,
        }],
        net: Some(NetSample {
            up_bytes_per_s: 1200,
            down_bytes_per_s: 45_000,
        }),
        gpus: vec![],
        temps: vec![],
    }
}

#[test]
fn memory_disk_and_temperature_levels_follow_the_shared_thresholds() {
    let mut book = DashBook::default();
    let mut s = sample(0, 10.0);
    // 85 % exact de la mémoire : attention (borne incluse) ; 95 % exact du disque : critique.
    s.mem = MemorySample {
        used_bytes: 85 * GIB,
        total_bytes: 100 * GIB,
    };
    s.disks[0].used_bytes = 95 * GIB;
    s.disks[0].total_bytes = 100 * GIB;
    s.temps = vec![
        TempSample {
            label: "a".into(),
            celsius: 79.9,
        },
        TempSample {
            label: "b".into(),
            celsius: 80.0,
        },
        TempSample {
            label: "c".into(),
            celsius: 90.0,
        },
    ];
    let event = book.on_metrics("srv", &s, 0);
    assert_eq!(event.levels.mem, LevelDto::Attention);
    assert_eq!(event.levels.disks, vec![LevelDto::Critical]);
    assert_eq!(
        event.levels.temps,
        vec![LevelDto::Normal, LevelDto::Attention, LevelDto::Critical]
    );
    assert_eq!(event.levels.cpu, LevelDto::Normal);
}

#[test]
fn the_processor_level_counts_only_once_held_for_thirty_seconds() {
    let mut book = DashBook::default();
    let mut last = LevelDto::Normal;
    // 96 % à chaque seconde : normal jusqu'à la trentième seconde incluse, puis critique.
    for second in 0..=29 {
        last = book
            .on_metrics("srv", &sample(second, 96.0), i64::from(second) * 1000)
            .levels
            .cpu;
        assert_eq!(last, LevelDto::Normal, "seconde {second}");
    }
    assert_eq!(last, LevelDto::Normal);
    let held = book
        .on_metrics("srv", &sample(30, 96.0), i64::from(30) * 1000)
        .levels
        .cpu;
    assert_eq!(held, LevelDto::Critical);
    // Un creux sous le seuil remet le compte à zéro.
    let dip = book
        .on_metrics("srv", &sample(31, 10.0), i64::from(31) * 1000)
        .levels
        .cpu;
    assert_eq!(dip, LevelDto::Normal);
    let again = book
        .on_metrics("srv", &sample(32, 96.0), i64::from(32) * 1000)
        .levels
        .cpu;
    assert_eq!(again, LevelDto::Normal);
}

#[test]
fn a_gap_in_the_live_series_breaks_the_hold() {
    let mut book = DashBook::default();
    for second in 0..=40 {
        book.on_metrics("srv", &sample(second, 90.0), i64::from(second) * 1000);
    }
    // Le lien est resté coupé 20 s : on ne peut plus affirmer que la charge est restée là.
    let after = book
        .on_metrics("srv", &sample(61, 90.0), i64::from(61) * 1000)
        .levels
        .cpu;
    assert_eq!(after, LevelDto::Normal);
}

#[test]
fn servers_do_not_share_their_series() {
    let mut book = DashBook::default();
    for second in 0..=30 {
        book.on_metrics("a", &sample(second, 90.0), i64::from(second) * 1000);
    }
    assert_eq!(
        book.on_metrics("a", &sample(31, 90.0), i64::from(31) * 1000)
            .levels
            .cpu,
        LevelDto::Attention
    );
    assert_eq!(
        book.on_metrics("b", &sample(31, 90.0), i64::from(31) * 1000)
            .levels
            .cpu,
        LevelDto::Normal
    );
    book.forget("a");
    assert_eq!(
        book.on_metrics("a", &sample(32, 90.0), i64::from(32) * 1000)
            .levels
            .cpu,
        LevelDto::Normal
    );
}

#[test]
fn a_snapshot_resumes_the_series_so_the_hold_survives_a_reconnection() {
    let mut book = DashBook::default();
    let history: Vec<Sample> = (0..=30).map(|second| sample(second, 97.0)).collect();
    let event = book.on_snapshot("srv", &machine(false, false), &history, 30_000);
    assert_eq!(event.levels.unwrap().cpu, LevelDto::Critical);
    assert_eq!(event.history.len(), 31);
    // La suite en direct prolonge la même série.
    assert_eq!(
        book.on_metrics("srv", &sample(31, 97.0), i64::from(31) * 1000)
            .levels
            .cpu,
        LevelDto::Critical
    );
}

#[test]
fn the_hold_follows_the_reception_instant_not_the_clock_of_the_agent() {
    let mut book = DashBook::default();
    let mut level = LevelDto::Normal;
    // L'horloge de l'agent recule d'une heure en plein milieu : les 30 s se comptent à la réception.
    for second in 0..=30u32 {
        let mut s = sample(second, 96.0);
        if second >= 10 {
            s.at = "2026-10-04T09:00:00.000Z".into();
        }
        level = book
            .on_metrics("srv", &s, i64::from(second) * 1000)
            .levels
            .cpu;
    }
    assert_eq!(level, LevelDto::Critical);
}

#[test]
fn levels_and_measures_have_the_same_lengths() {
    let mut s = sample(0, 5.0);
    s.gpus = vec![GpuSample {
        name: "g".into(),
        load_percent: None,
        memory_used_bytes: None,
        memory_total_bytes: None,
        temp_c: None,
    }];
    s.temps = vec![TempSample {
        label: "t".into(),
        celsius: 40.0,
    }];
    let event = DashBook::default().on_metrics("srv", &s, 0);
    assert_eq!(event.levels.disks.len(), event.sample.disks.len());
    assert_eq!(event.levels.gpus.len(), event.sample.gpus.len());
    assert_eq!(event.levels.temps.len(), event.sample.temps.len());
}

#[test]
fn an_unreadable_date_falls_back_to_the_given_instant() {
    let mut s = sample(0, 1.0);
    s.at = "pas une date".into();
    assert_eq!(sample_millis(&s, 1234), 1234);
    assert_eq!(sample_millis(&sample(0, 1.0), 0), 1_791_108_000_250);
}

#[test]
fn a_machine_without_gpu_or_sensors_is_described_as_such() {
    let (event, _) = snapshot("srv", &machine(false, false), &[sample(0, 5.0)], 0);
    assert!(!event.machine.capabilities.gpu);
    assert!(!event.machine.capabilities.temps);
    assert!(event.machine.gpus.is_empty());
    assert!(event.history[0].gpus.is_empty());
    assert!(event.history[0].temps.is_empty());
}

#[test]
fn an_unreadable_measure_stays_absent_and_the_rest_is_kept() {
    let mut s = sample(0, 5.0);
    s.net = None;
    s.gpus = vec![GpuSample {
        name: "RTX 4090".into(),
        load_percent: Some(37.3),
        memory_used_bytes: None,
        memory_total_bytes: Some(24 * GIB),
        temp_c: None,
    }];
    let mut book = DashBook::default();
    let event = book.on_metrics("srv", &s, 0);
    assert!(event.sample.net.is_none());
    let gpu = &event.sample.gpus[0];
    assert_eq!(gpu.load_percent, Some(37.3));
    assert_eq!(gpu.memory_used_bytes, None);
    assert_eq!(gpu.temp_c, None);
    // Ce qu'on n'a pas ne déclenche aucune alerte.
    assert_eq!(event.levels.gpus[0].memory, LevelDto::Normal);
    assert_eq!(event.levels.gpus[0].temp, LevelDto::Normal);
}

#[test]
fn video_memory_and_gpu_temperature_have_their_own_levels() {
    let mut s = sample(0, 5.0);
    s.gpus = vec![GpuSample {
        name: "RTX 4090".into(),
        load_percent: Some(99.0),
        memory_used_bytes: Some(21 * GIB),
        memory_total_bytes: Some(24 * GIB),
        temp_c: Some(91.0),
    }];
    let mut book = DashBook::default();
    let levels = book.on_metrics("srv", &s, 0).levels;
    assert_eq!(levels.gpus[0].memory, LevelDto::Attention);
    assert_eq!(levels.gpus[0].temp, LevelDto::Critical);
}

#[test]
fn quantities_and_percentages_cross_the_bridge_readably() {
    let mut s = sample(0, 37.3);
    s.cores = vec![10.0, 15.0, 9.5, 14.8];
    let mut book = DashBook::default();
    let event = book.on_metrics("srv", &s, 0);
    let json = serde_json::to_value(&event).unwrap();
    // Pas de `37.29999923706055` : une décimale, camelCase.
    assert_eq!(json["sample"]["cpu"], 37.3);
    assert_eq!(json["sample"]["cores"][3], 14.8);
    assert_eq!(json["sample"]["mem"]["totalBytes"], (64 * GIB) as f64);
    assert_eq!(json["levels"]["cpu"], "normal");
    assert_eq!(json["serverId"], "srv");
}

/// L'heure écoulée avant l'instantané : retenue, rejouée avec la vue, strictement plus ancienne que son
/// premier échantillon, et jamais mêlée à la série du processeur (HRT-18, BR-DASH-010).
#[test]
fn the_hour_before_the_snapshot_is_kept_replayed_with_the_view_and_kept_out_of_the_cpu_series() {
    let mut book = DashBook::default();
    let machine = machine(false, false);
    let view_samples: Vec<Sample> = (600..=605).map(|s| sample(s, 10.0)).collect();
    // L'heure d'avant : un échantillon par 10 s, dont un chevauche l'instantané (à écarter à la relecture).
    let hour: Vec<Sample> = [0, 10, 20, 600, 610]
        .iter()
        .map(|s| sample(*s, 99.0))
        .collect();
    let event = book.on_history("srv", &hour, 0);
    assert_eq!(event.server_id, "srv");
    assert_eq!(event.history.len(), 5);

    let (view, _) = snapshot("srv", &machine, &view_samples, 0);
    let first = view.history[0].at;
    let joined = book.with_older(view);
    // 3 plus anciens (0, 10, 20 s), les échantillons à 600 s et après appartiennent à l'instantané.
    assert_eq!(joined.history.len(), 3 + 6);
    assert!(joined.history[..3].iter().all(|sample| sample.at < first));
    assert!(
        joined
            .history
            .windows(2)
            .all(|pair| pair[0].at < pair[1].at)
    );

    // La série du processeur (30 s à 1 Hz) ne voit pas l'heure : un snapshot de connexion la repart de lui seul.
    let snapshot_event = book.on_snapshot("srv", &machine, &view_samples, 605_000);
    assert_eq!(snapshot_event.history.len(), 6);
    let level = book
        .on_metrics("srv", &sample(606, 10.0), 606_000)
        .levels
        .cpu;
    assert_eq!(
        level,
        LevelDto::Normal,
        "99 % de l'heure d'avant ne tient pas 30 s"
    );

    // Un serveur oublié oublie aussi son heure.
    book.forget("srv");
    let (view, _) = snapshot("srv", &machine, &view_samples, 0);
    assert_eq!(book.with_older(view).history.len(), 6);
}

/// FIX:01M4CRD60RKGZC2HTT52GK6P2T : « maintenant » lit l'horloge murale UNE fois, puis avance sur l'horloge
/// monotone. Rougit si on revient à une lecture murale à chaque appel (la source serait appelée plusieurs fois et
/// le temps suivrait ses sauts).
#[test]
fn now_reads_the_wall_clock_once_then_only_follows_the_monotonic_clock() {
    use hearth_desktop_lib::dashboard::MonoMs;
    use std::cell::Cell;
    use std::time::{Duration, Instant};
    let reads = Cell::new(0);
    let start = Instant::now();
    let clock = MonoMs::new(
        || {
            reads.set(reads.get() + 1);
            1_790_000_000_000
        },
        start,
    );
    assert_eq!(clock.at(start), 1_790_000_000_000);
    assert_eq!(clock.at(start + Duration::from_secs(5)), 1_790_000_005_000);
    assert_eq!(reads.get(), 1, "l'horloge murale n'est lue qu'une fois");
}

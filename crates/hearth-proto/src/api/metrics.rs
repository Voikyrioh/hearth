//! Échantillon de mesures (un par seconde) et historique : `GET /api/v1/metrics/history`, et
//! message `metrics` du flux.
//!
//! Une mesure momentanément illisible est un champ absent (`null`) : jamais un zéro inventé, et
//! les autres champs de l'échantillon ne sont pas touchés (BR-DASH-008). Les quantités sont en
//! octets, les charges en pourcentage (0 à 100), les températures en degrés Celsius, les débits
//! en octets par seconde : le formatage est l'affaire du client (BR-DASH-014).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySample {
    pub used_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiskSample {
    pub name: String,
    pub mount: String,
    pub used_bytes: u64,
    pub total_bytes: u64,
}

/// Débit réseau agrégé sur les interfaces physiques.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetSample {
    pub up_bytes_per_s: u64,
    pub down_bytes_per_s: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GpuSample {
    pub name: String,
    pub load_percent: Option<f32>,
    pub memory_used_bytes: Option<u64>,
    pub memory_total_bytes: Option<u64>,
    /// Absente quand la carte n'expose pas sa température (BR-DASH-007).
    pub temp_c: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TempSample {
    /// Nom de la sonde tel que le système le donne (`coretemp Package id 0`).
    pub label: String,
    pub celsius: f32,
}

/// Une seconde de la vie de la machine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sample {
    /// Instant de la mesure, RFC 3339 en UTC.
    pub at: String,
    pub uptime_s: u64,
    /// Charge globale du processeur.
    pub cpu: f32,
    /// Charge par cœur logique, dans l'ordre du système.
    pub cores: Vec<f32>,
    pub mem: MemorySample,
    pub disks: Vec<DiskSample>,
    pub net: Option<NetSample>,
    /// Vide quand la machine n'a pas de carte graphique mesurable.
    pub gpus: Vec<GpuSample>,
    /// Vide quand le système n'expose aucune sonde.
    pub temps: Vec<TempSample>,
}

/// Fenêtre d'historique demandée (`window=1m|5m|1h`, BR-DASH-010).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HistoryWindow {
    #[serde(rename = "1m")]
    OneMinute,
    #[serde(rename = "5m")]
    FiveMinutes,
    #[serde(rename = "1h")]
    OneHour,
}

/// Réponse de `GET /metrics/history`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryResponse {
    pub window: HistoryWindow,
    /// Pas des échantillons rendus, en secondes (1 pour 1 min et 5 min, 10 pour 1 h).
    pub step_s: u32,
    /// Du plus ancien au plus récent.
    pub samples: Vec<Sample>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Sample {
        Sample {
            at: "2026-10-04T10:30:15.250Z".into(),
            uptime_s: 3_600,
            cpu: 12.5,
            cores: vec![10.0, 15.0],
            mem: MemorySample {
                used_bytes: 8 << 30,
                total_bytes: 16 << 30,
            },
            disks: vec![],
            net: Some(NetSample {
                up_bytes_per_s: 10,
                down_bytes_per_s: 20,
            }),
            gpus: vec![GpuSample {
                name: "RTX 4090".into(),
                load_percent: Some(40.0),
                memory_used_bytes: None,
                memory_total_bytes: Some(24 << 30),
                temp_c: None,
            }],
            temps: vec![],
        }
    }

    #[test]
    fn round_trips_through_json() {
        let text = serde_json::to_string(&sample()).expect("serialization");
        let back: Sample = serde_json::from_str(&text).expect("deserialization");
        assert_eq!(back, sample());
    }

    #[test]
    fn an_unreadable_measure_is_a_null_field_not_a_zero() {
        let json = serde_json::to_value(sample()).expect("serialization");
        assert!(json["gpus"][0]["temp_c"].is_null());
        assert!(json["gpus"][0]["memory_used_bytes"].is_null());
        assert_eq!(json["gpus"][0]["load_percent"], 40.0);
    }

    #[test]
    fn window_labels_are_the_documented_query_values() {
        for (window, label) in [
            (HistoryWindow::OneMinute, "\"1m\""),
            (HistoryWindow::FiveMinutes, "\"5m\""),
            (HistoryWindow::OneHour, "\"1h\""),
        ] {
            assert_eq!(serde_json::to_string(&window).expect("json"), label);
        }
    }
}

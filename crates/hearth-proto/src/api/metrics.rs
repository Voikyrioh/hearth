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

/// Le MAXIMUM de chaque mesure tracée sur un pas d'une fenêtre rééchantillonnée (`1h`, pas de 10 s), rendu à
/// côté de la moyenne : un pic d'une seconde ne disparaît pas quand il vieillit (BR-DASH-010). Les mesures
/// qui ont un maximum : processeur global, mémoire utilisée, débits, charge, mémoire et température de chaque
/// carte graphique, température de chaque sonde. Les cœurs et les disques ne sont pas tracés sur l'heure : ils
/// n'ont pas de maximum et gardent leur moyenne.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepPeak {
    /// Charge globale maximale du pas.
    pub cpu: f32,
    pub mem_used_bytes: u64,
    /// Débits maximaux (chacun son maximum) ; absent si aucune mesure de débit dans le pas.
    pub net: Option<NetSample>,
    /// Une entrée par carte graphique du dernier échantillon du pas, dans le même ordre.
    pub gpus: Vec<GpuPeak>,
    /// Une entrée par sonde du dernier échantillon du pas, dans le même ordre (champ ajouté : absent chez un
    /// agent plus ancien, ignoré par un client plus ancien).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub temps: Vec<TempPeak>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GpuPeak {
    pub load_percent: Option<f32>,
    pub memory_used_bytes: Option<u64>,
    /// Température maximale de la carte (champ ajouté).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temp_c: Option<f32>,
}

/// La température maximale d'une sonde sur le pas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TempPeak {
    pub label: String,
    pub celsius: f32,
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
    /// Les maxima de chaque pas, un par échantillon de `samples`, dans le même ordre : seulement pour la
    /// fenêtre `1h` (les autres sont à 1 échantillon par seconde, déjà au maximum de détail). Champ AJOUTÉ :
    /// un client plus ancien l'ignore et lit les moyennes de `samples` ; un agent plus ancien ne l'envoie
    /// pas (liste vide).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub peaks: Vec<StepPeak>,
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
    fn the_peaks_are_an_added_field_an_older_reader_ignores_and_an_older_agent_omits() {
        let mut response = HistoryResponse {
            window: HistoryWindow::OneHour,
            step_s: 10,
            samples: vec![sample()],
            peaks: vec![],
        };
        // Sans maxima (fenêtres à 1 s, agent plus ancien) : le champ n'est pas écrit.
        assert!(!serde_json::to_string(&response).unwrap().contains("peaks"));
        // Un agent plus ancien n'envoie pas le champ : il se lit comme une liste vide.
        let old = r#"{"window":"1h","step_s":10,"samples":[]}"#;
        assert!(
            serde_json::from_str::<HistoryResponse>(old)
                .unwrap()
                .peaks
                .is_empty()
        );
        response.peaks = vec![StepPeak {
            cpu: 100.0,
            mem_used_bytes: 1,
            net: None,
            gpus: vec![GpuPeak {
                load_percent: Some(90.0),
                memory_used_bytes: None,
                temp_c: Some(71.0),
            }],
            temps: vec![TempPeak {
                label: "coretemp Package id 0".into(),
                celsius: 88.0,
            }],
        }];
        let text = serde_json::to_string(&response).unwrap();
        assert_eq!(
            serde_json::from_str::<HistoryResponse>(&text).unwrap(),
            response
        );
        // Un lecteur plus ancien (qui ne connaît pas le champ) ignore `peaks` et lit les moyennes de `samples` :
        // on le joue pour de bon avec un type qui n'a que les anciens champs.
        #[derive(Deserialize)]
        struct OlderReader {
            window: HistoryWindow,
            step_s: u32,
            samples: Vec<Sample>,
        }
        let older: OlderReader = serde_json::from_str(&text).unwrap();
        assert_eq!(older.window, HistoryWindow::OneHour);
        assert_eq!(older.step_s, 10);
        assert_eq!(older.samples, response.samples);
        // Un pic sans les champs ajoutés (agent d'avant les températures) se lit : champs absents.
        let before: StepPeak = serde_json::from_str(
            r#"{"cpu":1.0,"mem_used_bytes":2,"net":null,"gpus":[{"load_percent":1.0,"memory_used_bytes":null}]}"#,
        )
        .unwrap();
        assert!(before.temps.is_empty() && before.gpus[0].temp_c.is_none());
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

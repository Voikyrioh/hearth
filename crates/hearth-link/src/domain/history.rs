//! L'heure écoulée des mesures d'un serveur (BR-DASH-010, ADR-0015 §5) : ce que `GET /metrics/history?window=1h`
//! apporte EN PLUS de l'instantané du flux. Pur : aucune E/S.
//!
//! L'instantané du flux porte les 5 dernières minutes à 1 échantillon par seconde ; l'historique d'une heure
//! rend la même fin à 1 échantillon par 10 secondes (moyennes). Seuls les échantillons PLUS ANCIENS que le
//! premier de l'instantané sont gardés : l'heure ne remplace jamais le détail à la seconde.

use hearth_proto::api::metrics::Sample;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

fn at_of(sample: &Sample) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(&sample.at, &Rfc3339).ok()
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

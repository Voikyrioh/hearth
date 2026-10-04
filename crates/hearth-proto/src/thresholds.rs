//! Seuils d'alerte : fonctions pures, partagées entre l'agent et le client (BR-DASH-003 et
//! BR-DASH-004). Trois niveaux : normal, attention, critique. Les bornes sont **incluses** : une
//! valeur égale au seuil est déjà dans le niveau supérieur.
//!
//! | Mesure | Attention | Critique |
//! |---|---|---|
//! | Processeur (tenu 30 s), mémoire, mémoire vidéo, disque | 85 % | 95 % |
//! | Température (processeur, carte graphique, sondes) | 80 °C | 90 °C |
//!
//! Une valeur illisible (`NaN`, quantité totale nulle) est normale : on n'alerte pas sur une
//! mesure qu'on n'a pas.

use serde::{Deserialize, Serialize};

pub const ATTENTION_PERCENT: u64 = 85;
pub const CRITICAL_PERCENT: u64 = 95;
pub const ATTENTION_CELSIUS: f32 = 80.0;
pub const CRITICAL_CELSIUS: f32 = 90.0;

/// Durée pendant laquelle la charge du processeur doit tenir un niveau avant qu'il ne compte
/// (BR-DASH-004), en millisecondes.
pub const CPU_HOLD_MS: i64 = 30_000;

/// Écart maximal entre deux points consécutifs d'une série pour qu'on la dise « tenue » : au-delà,
/// des points manquent (coupure du lien) et on ne peut plus affirmer que la charge est restée là.
pub const CPU_MAX_GAP_MS: i64 = 5_000;

/// Niveau d'alerte, du moins grave au plus grave (l'ordre sert aux comparaisons et à `max`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Normal,
    Attention,
    Critical,
}

/// Niveau d'un pourcentage (0 à 100) pris à l'instant, sans durée : mémoire, mémoire vidéo,
/// disque, et chaque point d'une série de processeur.
pub fn percent_level(percent: f32) -> Level {
    if percent.is_nan() {
        return Level::Normal;
    }
    if percent >= CRITICAL_PERCENT as f32 {
        Level::Critical
    } else if percent >= ATTENTION_PERCENT as f32 {
        Level::Attention
    } else {
        Level::Normal
    }
}

/// Niveau d'une occupation `used` sur `total` (octets), en calcul entier : aucune erreur
/// d'arrondi aux bornes. `total` nul : normal.
pub fn usage_level(used: u64, total: u64) -> Level {
    if total == 0 {
        return Level::Normal;
    }
    let scaled = u128::from(used) * 100;
    let total = u128::from(total);
    if scaled >= total * u128::from(CRITICAL_PERCENT) {
        Level::Critical
    } else if scaled >= total * u128::from(ATTENTION_PERCENT) {
        Level::Attention
    } else {
        Level::Normal
    }
}

/// Niveau d'une température en degrés Celsius.
pub fn temperature_level(celsius: f32) -> Level {
    if celsius.is_nan() {
        Level::Normal
    } else if celsius >= CRITICAL_CELSIUS {
        Level::Critical
    } else if celsius >= ATTENTION_CELSIUS {
        Level::Attention
    } else {
        Level::Normal
    }
}

/// Un point de la série de charge du processeur.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CpuPoint {
    /// Instant du point, en millisecondes (l'origine importe peu : seules les différences servent).
    pub at_ms: i64,
    pub percent: f32,
}

/// Niveau du processeur d'après sa série (du plus ancien au plus récent) : un niveau ne compte que
/// si **tous** les points depuis au moins [`CPU_HOLD_MS`] y sont (BR-DASH-004), sans trou de plus
/// de [`CPU_MAX_GAP_MS`]. Un pic bref reste normal ; une charge à 100 % depuis 10 s et à 90 %
/// avant est « attention », pas « critique ».
pub fn cpu_level(series: &[CpuPoint]) -> Level {
    let Some(latest) = series.last().map(|point| point.at_ms) else {
        return Level::Normal;
    };
    [Level::Critical, Level::Attention]
        .into_iter()
        .find(|level| held(series, latest, *level))
        .unwrap_or(Level::Normal)
}

/// Les points les plus récents sont-ils au moins au niveau `level` depuis [`CPU_HOLD_MS`] ?
fn held(series: &[CpuPoint], latest: i64, level: Level) -> bool {
    let mut run_start = None;
    let mut newer_at = latest;
    for point in series.iter().rev() {
        if percent_level(point.percent) < level || newer_at - point.at_ms > CPU_MAX_GAP_MS {
            break;
        }
        run_start = Some(point.at_ms);
        newer_at = point.at_ms;
    }
    run_start.is_some_and(|start| latest - start >= CPU_HOLD_MS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_bounds_are_inclusive() {
        let cases = [
            (0.0, Level::Normal),
            (84.99, Level::Normal),
            (85.0, Level::Attention),
            (94.99, Level::Attention),
            (95.0, Level::Critical),
            (100.0, Level::Critical),
            (140.0, Level::Critical),
            (f32::NAN, Level::Normal),
        ];
        for (percent, expected) in cases {
            assert_eq!(percent_level(percent), expected, "{percent}");
        }
    }

    #[test]
    fn usage_bounds_are_exact_in_integers() {
        // 85 % de 200 = 170, 95 % de 200 = 190.
        let cases = [
            (0, Level::Normal),
            (169, Level::Normal),
            (170, Level::Attention),
            (189, Level::Attention),
            (190, Level::Critical),
            (200, Level::Critical),
            (250, Level::Critical),
        ];
        for (used, expected) in cases {
            assert_eq!(usage_level(used, 200), expected, "{used}/200");
        }
        // Sur de très grandes quantités, une borne reste exacte là où un flottant arrondirait.
        let total = u64::MAX;
        assert_eq!(usage_level(total / 100 * 85 - 1, total), Level::Normal);
        assert_eq!(usage_level(total / 100 * 95, total), Level::Attention);
        assert_eq!(usage_level(total, total), Level::Critical);
    }

    #[test]
    fn usage_without_a_total_is_normal() {
        assert_eq!(usage_level(10, 0), Level::Normal);
        assert_eq!(usage_level(0, 0), Level::Normal);
    }

    #[test]
    fn temperature_bounds_are_inclusive() {
        let cases = [
            (-10.0, Level::Normal),
            (79.9, Level::Normal),
            (80.0, Level::Attention),
            (89.9, Level::Attention),
            (90.0, Level::Critical),
            (110.0, Level::Critical),
            (f32::NAN, Level::Normal),
        ];
        for (celsius, expected) in cases {
            assert_eq!(temperature_level(celsius), expected, "{celsius}");
        }
    }

    #[test]
    fn levels_are_ordered_by_severity() {
        assert!(Level::Normal < Level::Attention);
        assert!(Level::Attention < Level::Critical);
    }

    /// Série à 1 point par seconde : `count` points, à `percent`, finissant à `end_ms`.
    fn flat(end_ms: i64, count: i64, percent: f32) -> Vec<CpuPoint> {
        (0..count)
            .map(|i| CpuPoint {
                at_ms: end_ms - (count - 1 - i) * 1_000,
                percent,
            })
            .collect()
    }

    #[test]
    fn the_processor_must_hold_a_level_for_thirty_seconds() {
        // 31 points = 30 s d'écart entre le premier et le dernier : tenu.
        assert_eq!(cpu_level(&flat(100_000, 31, 90.0)), Level::Attention);
        // 30 points = 29 s : pas encore.
        assert_eq!(cpu_level(&flat(100_000, 30, 90.0)), Level::Normal);
        assert_eq!(cpu_level(&flat(100_000, 31, 99.0)), Level::Critical);
        assert_eq!(cpu_level(&flat(100_000, 30, 99.0)), Level::Normal);
        assert_eq!(cpu_level(&flat(100_000, 600, 40.0)), Level::Normal);
    }

    #[test]
    fn a_short_peak_stays_normal() {
        let mut series = flat(100_000, 60, 20.0);
        for point in series.iter_mut().rev().take(10) {
            point.percent = 100.0;
        }
        assert_eq!(cpu_level(&series), Level::Normal);
    }

    #[test]
    fn a_dip_below_the_threshold_restarts_the_hold() {
        let mut series = flat(100_000, 60, 90.0);
        series[40].percent = 50.0; // il y a 19 s
        assert_eq!(cpu_level(&series), Level::Normal);
        series[40].percent = 85.0; // pile au seuil : tient
        assert_eq!(cpu_level(&series), Level::Attention);
    }

    #[test]
    fn the_level_is_the_highest_one_held_long_enough() {
        // 20 s à 99 % après 40 s à 90 % : critique pas tenu, attention tenu.
        let mut series = flat(100_000, 61, 90.0);
        for point in series.iter_mut().rev().take(21) {
            point.percent = 99.0;
        }
        assert_eq!(cpu_level(&series), Level::Attention);
        // 35 s à 99 % : critique.
        let mut series = flat(100_000, 61, 90.0);
        for point in series.iter_mut().rev().take(36) {
            point.percent = 99.0;
        }
        assert_eq!(cpu_level(&series), Level::Critical);
    }

    #[test]
    fn a_gap_in_the_series_breaks_the_hold() {
        let mut series = flat(100_000, 31, 90.0);
        // Retire 10 points au milieu : un trou de 11 s, on ne sait plus ce qui s'est passé.
        series.drain(10..20);
        assert_eq!(cpu_level(&series), Level::Normal);
        // Un trou de 5 s exactement reste toléré.
        let mut series = flat(100_000, 31, 90.0);
        series.drain(10..14);
        assert_eq!(cpu_level(&series), Level::Attention);
    }

    #[test]
    fn empty_and_single_point_series_are_normal() {
        assert_eq!(cpu_level(&[]), Level::Normal);
        assert_eq!(cpu_level(&flat(1_000, 1, 100.0)), Level::Normal);
    }
}

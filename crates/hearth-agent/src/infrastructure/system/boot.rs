//! Identifiant de démarrage du noyau et temps écoulé depuis le démarrage (HRT-25, ADR-0025).
//!
//! Linux : `/proc/sys/kernel/random/boot_id` (un UUID tiré à chaque démarrage du noyau) et
//! `/proc/uptime` (la première valeur : secondes depuis le démarrage, horloge qui ne recule pas et que
//! l'heure murale ne touche pas). L'unité systemd de l'agent (ADR-0012) ne pose ni
//! `ProtectKernelTunables` ni `ProcSubset=pid` : les deux fichiers sont lisibles. Hors Linux, rien
//! n'est lisible : aucune fenêtre n'est jamais ouverte.

use std::path::{Path, PathBuf};

use time::Duration;

use crate::application::ports::BootInfo;

const BOOT_ID_PATH: &str = "/proc/sys/kernel/random/boot_id";
const UPTIME_PATH: &str = "/proc/uptime";

/// Longueur maximale retenue d'un identifiant de démarrage : celle d'un UUID écrit.
const MAX_BOOT_ID_LEN: usize = 64;

pub struct ProcBootInfo {
    boot_id: PathBuf,
    uptime: PathBuf,
}

impl ProcBootInfo {
    /// Les fichiers du noyau.
    pub fn new() -> Self {
        Self::at(Path::new(BOOT_ID_PATH), Path::new(UPTIME_PATH))
    }

    /// D'autres fichiers (tests).
    pub fn at(boot_id: &Path, uptime: &Path) -> Self {
        Self {
            boot_id: boot_id.to_owned(),
            uptime: uptime.to_owned(),
        }
    }
}

impl Default for ProcBootInfo {
    fn default() -> Self {
        Self::new()
    }
}

impl BootInfo for ProcBootInfo {
    fn boot_id(&self) -> Option<String> {
        parse_boot_id(&std::fs::read_to_string(&self.boot_id).ok()?)
    }

    fn uptime(&self) -> Duration {
        std::fs::read_to_string(&self.uptime)
            .ok()
            .and_then(|text| parse_uptime(&text))
            .unwrap_or(Duration::MAX)
    }
}

/// Un identifiant de démarrage : un UUID (hexadécimal et tirets), sans espace autour. Tout autre
/// contenu est illisible.
pub fn parse_boot_id(text: &str) -> Option<String> {
    let id = text.trim();
    let valid = !id.is_empty()
        && id.len() <= MAX_BOOT_ID_LEN
        && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
    valid.then(|| id.to_ascii_lowercase())
}

/// `/proc/uptime` : « 12345.67 54321.00 » ; la première valeur, en secondes (virgule flottante).
pub fn parse_uptime(text: &str) -> Option<Duration> {
    let seconds: f64 = text.split_whitespace().next()?.parse().ok()?;
    (seconds.is_finite() && (0.0..1e12).contains(&seconds)).then(|| Duration::seconds_f64(seconds))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_boot_id_is_an_uuid_without_spaces() {
        assert_eq!(
            parse_boot_id("6f1b2c3d-0a1b-4c5d-8e9f-001122334455\n").as_deref(),
            Some("6f1b2c3d-0a1b-4c5d-8e9f-001122334455")
        );
        assert_eq!(parse_boot_id("ABC-DEF").as_deref(), Some("abc-def"));
        for bad in [
            "",
            "   ",
            "\n",
            "pas un uuid",
            "12 34",
            &"a".repeat(65),
            "é",
        ] {
            assert_eq!(parse_boot_id(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn the_uptime_is_the_first_field_in_seconds() {
        assert_eq!(
            parse_uptime("12345.67 54321.00\n"),
            Some(Duration::seconds_f64(12345.67))
        );
        assert_eq!(parse_uptime("0.00 0.00"), Some(Duration::ZERO));
        for bad in ["", "abc 1", "-5.0 1", "NaN 1", "inf 1", "1e30 1"] {
            assert_eq!(parse_uptime(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn unreadable_files_mean_no_identifier_and_an_endless_uptime() {
        let dir = tempfile::tempdir().unwrap();
        let boot = ProcBootInfo::at(&dir.path().join("absent-1"), &dir.path().join("absent-2"));
        assert_eq!(boot.boot_id(), None);
        assert_eq!(boot.uptime(), Duration::MAX);
    }

    /// Le vrai noyau (Linux, y compris dans un conteneur) : les deux fichiers sont lisibles, l'identifiant
    /// ne change pas d'une lecture à l'autre (un redémarrage du service n'ouvre aucune fenêtre) et le temps
    /// écoulé ne recule pas.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_real_kernel_files_are_readable_stable_and_monotonic() {
        let boot = ProcBootInfo::new();
        let first = boot.boot_id().expect("identifiant de démarrage lisible");
        let up_first = boot.uptime();
        assert!(up_first > Duration::ZERO && up_first < Duration::MAX);
        let up_second = boot.uptime();
        assert_eq!(boot.boot_id().as_deref(), Some(first.as_str()));
        assert!(up_second >= up_first);
        assert_eq!(first.len(), 36, "un UUID : {first}");
    }

    #[test]
    fn readable_files_give_the_identifier_and_the_uptime() {
        let dir = tempfile::tempdir().unwrap();
        let id = dir.path().join("boot_id");
        let up = dir.path().join("uptime");
        std::fs::write(&id, "11111111-2222-3333-4444-555555555555\n").unwrap();
        std::fs::write(&up, "90.50 10.00\n").unwrap();
        let boot = ProcBootInfo::at(&id, &up);
        assert_eq!(
            boot.boot_id().as_deref(),
            Some("11111111-2222-3333-4444-555555555555")
        );
        assert_eq!(boot.uptime(), Duration::seconds_f64(90.5));
    }
}

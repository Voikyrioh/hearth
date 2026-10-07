//! Cartes AMD et Intel par les fichiers du noyau (`/sys/class/drm/card*/device`), sans rien lancer
//! ni charger. Les cartes NVIDIA (fournisseur `0x10de`) sont laissées à `nvidia-smi`.
//!
//! Chaque fichier est indépendant : absent ou illisible, il rend un champ absent. Une machine
//! sans ce dossier (Windows, conteneur) n'a simplement aucune carte ici.

use std::fs;
use std::path::{Path, PathBuf};

use super::round1;
use crate::application::ports::GpuProbe;
use crate::domain::machine::GpuIdentity;
use crate::domain::metrics::GpuReading;

const DRM_ROOT: &str = "/sys/class/drm";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Vendor {
    Amd,
    Intel,
}

impl Vendor {
    fn from_id(id: &str) -> Option<Self> {
        match id.trim().to_ascii_lowercase().as_str() {
            "0x1002" => Some(Self::Amd),
            "0x8086" => Some(Self::Intel),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Amd => "AMD",
            Self::Intel => "Intel",
        }
    }
}

pub struct SysfsGpuProbe {
    root: PathBuf,
}

impl Default for SysfsGpuProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl SysfsGpuProbe {
    pub fn new() -> Self {
        Self::with_root(DRM_ROOT)
    }

    /// Racine différente de `/sys/class/drm` (tests).
    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Les cartes AMD et Intel, dans l'ordre de leur numéro.
    fn cards(&self) -> Vec<(Vendor, PathBuf)> {
        let Ok(entries) = fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut cards: Vec<(u32, Vendor, PathBuf)> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                // `card0`, jamais `card0-DP-1` (connecteur) ni `renderD128`.
                let name = entry.file_name().into_string().ok()?;
                let number = name.strip_prefix("card")?.parse::<u32>().ok()?;
                let device = entry.path().join("device");
                let vendor = Vendor::from_id(&fs::read_to_string(device.join("vendor")).ok()?)?;
                Some((number, vendor, device))
            })
            .collect();
        cards.sort_by_key(|(number, _, _)| *number);
        cards
            .into_iter()
            .map(|(_, vendor, device)| (vendor, device))
            .collect()
    }
}

fn read(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|text| text.trim().to_owned())
}

fn read_u64(path: &Path) -> Option<u64> {
    read(path)?.parse().ok()
}

/// Température de la première sonde `hwmon` de la carte, en degrés (le noyau la donne en
/// millièmes de degré).
fn read_temperature(device: &Path) -> Option<f32> {
    let mut hwmons: Vec<PathBuf> = fs::read_dir(device.join("hwmon"))
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    hwmons.sort();
    hwmons.iter().find_map(|hwmon| {
        let millidegrees = read(&hwmon.join("temp1_input"))?.parse::<i64>().ok()?;
        let celsius = millidegrees as f32 / 1000.0;
        (-50.0..=150.0).contains(&celsius).then(|| round1(celsius))
    })
}

fn name_of(vendor: Vendor, device: &Path) -> String {
    let model = read(&device.join("product_name")).filter(|name| !name.is_empty());
    match (model, read(&device.join("device"))) {
        (Some(model), _) => format!("{} {model}", vendor.label()),
        (None, Some(id)) => format!("{} GPU ({id})", vendor.label()),
        (None, None) => format!("{} GPU", vendor.label()),
    }
}

/// Lit une carte. Les champs que le pilote n'expose pas restent absents (Intel n'a ni charge ni
/// mémoire vidéo dédiée dans ces fichiers).
fn read_card(vendor: Vendor, device: &Path) -> GpuReading {
    GpuReading {
        name: name_of(vendor, device),
        load_percent: read_u64(&device.join("gpu_busy_percent"))
            .map(|percent| percent.min(100) as f32),
        memory_used_bytes: read_u64(&device.join("mem_info_vram_used")),
        memory_total_bytes: read_u64(&device.join("mem_info_vram_total")),
        temp_c: read_temperature(device),
    }
}

impl GpuProbe for SysfsGpuProbe {
    fn detect(&self) -> Vec<GpuIdentity> {
        self.cards()
            .into_iter()
            .map(|(vendor, device)| {
                let reading = read_card(vendor, &device);
                GpuIdentity {
                    name: reading.name,
                    memory_total_bytes: reading.memory_total_bytes,
                }
            })
            .collect()
    }

    fn sample(&self) -> Vec<GpuReading> {
        self.cards()
            .into_iter()
            .map(|(vendor, device)| read_card(vendor, &device))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, content: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn card(root: &Path, name: &str, vendor: &str) -> PathBuf {
        let device = root.join(name).join("device");
        write(&device.join("vendor"), vendor);
        device
    }

    #[test]
    fn an_amd_card_is_read_field_by_field() {
        let root = crate::test_tmp::tempdir().unwrap();
        let device = card(root.path(), "card0", "0x1002\n");
        write(&device.join("device"), "0x73bf\n");
        write(&device.join("gpu_busy_percent"), "42\n");
        write(&device.join("mem_info_vram_used"), "1073741824\n");
        write(&device.join("mem_info_vram_total"), "8589934592\n");
        write(
            &device.join("hwmon").join("hwmon3").join("temp1_input"),
            "61500\n",
        );

        let probe = SysfsGpuProbe::with_root(root.path());
        let readings = probe.sample();
        assert_eq!(readings.len(), 1);
        assert_eq!(readings[0].name, "AMD GPU (0x73bf)");
        assert_eq!(readings[0].load_percent, Some(42.0));
        assert_eq!(readings[0].memory_used_bytes, Some(1 << 30));
        assert_eq!(readings[0].memory_total_bytes, Some(8 << 30));
        assert_eq!(readings[0].temp_c, Some(61.5));
        let identity = probe.detect();
        assert_eq!(identity[0].memory_total_bytes, Some(8 << 30));
    }

    #[test]
    fn an_intel_card_without_files_has_absent_fields_not_zeros() {
        let root = crate::test_tmp::tempdir().unwrap();
        let device = card(root.path(), "card1", "0x8086");
        write(&device.join("product_name"), "Arc A380");
        let readings = SysfsGpuProbe::with_root(root.path()).sample();
        assert_eq!(readings.len(), 1);
        assert_eq!(readings[0].name, "Intel Arc A380");
        assert_eq!(readings[0].load_percent, None);
        assert_eq!(readings[0].memory_used_bytes, None);
        assert_eq!(readings[0].memory_total_bytes, None);
        assert_eq!(readings[0].temp_c, None);
    }

    #[test]
    fn one_unreadable_file_does_not_affect_the_others() {
        let root = crate::test_tmp::tempdir().unwrap();
        let device = card(root.path(), "card0", "0x1002");
        write(&device.join("gpu_busy_percent"), "pas un nombre");
        write(&device.join("mem_info_vram_total"), "100");
        write(
            &device.join("hwmon").join("hwmon0").join("temp1_input"),
            "999999999",
        );
        let readings = SysfsGpuProbe::with_root(root.path()).sample();
        assert_eq!(readings[0].load_percent, None);
        assert_eq!(readings[0].memory_total_bytes, Some(100));
        assert_eq!(readings[0].temp_c, None, "température aberrante ignorée");
    }

    #[test]
    fn connectors_render_nodes_and_nvidia_cards_are_not_listed() {
        let root = crate::test_tmp::tempdir().unwrap();
        card(root.path(), "card0", "0x10de");
        card(root.path(), "card0-DP-1", "0x1002");
        card(root.path(), "renderD128", "0x1002");
        card(root.path(), "card2", "0x1002");
        card(root.path(), "card10", "0x8086");
        card(root.path(), "card3", "0x1234");
        let names: Vec<_> = SysfsGpuProbe::with_root(root.path())
            .sample()
            .into_iter()
            .map(|reading| reading.name)
            .collect();
        assert_eq!(names, ["AMD GPU", "Intel GPU"], "carte 2 puis carte 10");
    }

    #[test]
    fn a_missing_root_means_no_card() {
        let probe = SysfsGpuProbe::with_root("/hearth-test/no/such/dir");
        assert!(probe.detect().is_empty());
        assert!(probe.sample().is_empty());
    }
}

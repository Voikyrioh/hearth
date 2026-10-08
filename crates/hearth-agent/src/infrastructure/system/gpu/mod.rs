//! Cartes graphiques. NVIDIA par l'outil `nvidia-smi` en sous-processus (pas de NVML : le binaire
//! est statique, ADR-0003), AMD et Intel par les fichiers du noyau, ailleurs rien.

pub mod none;
pub mod nvidia_smi;
pub mod sysfs;

use std::sync::Arc;

use crate::application::ports::GpuProbe;
use crate::domain::machine::GpuIdentity;
use crate::domain::metrics::GpuReading;

pub use none::NoGpuProbe;
pub use nvidia_smi::NvidiaSmiProbe;
pub use sysfs::SysfsGpuProbe;

/// Plusieurs sondes à la suite : les cartes de chacune, dans l'ordre.
pub struct CompositeGpuProbe(pub Vec<Arc<dyn GpuProbe>>);

impl GpuProbe for CompositeGpuProbe {
    fn detect(&self) -> Vec<GpuIdentity> {
        self.0.iter().flat_map(|probe| probe.detect()).collect()
    }

    fn sample(&self) -> Vec<GpuReading> {
        self.0.iter().flat_map(|probe| probe.sample()).collect()
    }
}

/// La sonde de la plateforme : Linux = `nvidia-smi` (lancé ici, une seule instance) + fichiers du
/// noyau ; Windows (mode dev) et autres : aucune carte. À appeler dans un runtime Tokio.
pub fn platform_probe() -> Arc<dyn GpuProbe> {
    if cfg!(target_os = "linux") {
        Arc::new(CompositeGpuProbe(vec![
            Arc::new(NvidiaSmiProbe::start()),
            Arc::new(SysfsGpuProbe::new()),
        ]))
    } else {
        Arc::new(NoGpuProbe)
    }
}

/// Tronque une mesure à une décimale (vers le bas) : pas de bruit inutile sur le fil, et la même règle que
/// l'affichage, qui tronque (« 99 % » pour 99,96). Arrondir ferait afficher ou classer « 100 % » une mesure à
/// 99,96 (FIX:01M4CRD4NX3A34B7Z31A7RWE1B). La tolérance absorbe le bruit des flottants (0,7 lu « 0,69999999 »).
pub(crate) fn trunc1(value: f32) -> f32 {
    (((f64::from(value) * 10.0) + 1e-4).floor() / 10.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed(&'static str);

    impl GpuProbe for Fixed {
        fn detect(&self) -> Vec<GpuIdentity> {
            vec![GpuIdentity {
                name: self.0.into(),
                memory_total_bytes: None,
            }]
        }

        fn sample(&self) -> Vec<GpuReading> {
            vec![GpuReading {
                name: self.0.into(),
                load_percent: None,
                memory_used_bytes: None,
                memory_total_bytes: None,
                temp_c: None,
            }]
        }
    }

    #[test]
    fn a_composite_lists_the_cards_of_each_probe_in_order() {
        let composite = CompositeGpuProbe(vec![
            Arc::new(Fixed("nvidia")),
            Arc::new(NoGpuProbe),
            Arc::new(Fixed("amd")),
        ]);
        let names: Vec<_> = composite.detect().into_iter().map(|g| g.name).collect();
        assert_eq!(names, ["nvidia", "amd"]);
        assert_eq!(composite.sample().len(), 2);
    }

    #[test]
    fn truncation_keeps_one_decimal_like_the_display() {
        assert_eq!(trunc1(12.345), 12.3);
        assert_eq!(trunc1(99.96), 99.9, "99,96 ne devient jamais 100");
        assert_eq!(trunc1(0.04), 0.0);
        assert_eq!(
            trunc1(0.7),
            0.7,
            "le bruit des flottants ne retire pas un dixième"
        );
        assert_eq!(
            trunc1(-3.25),
            -3.3,
            "vers le bas, comme Math.floor de l'affichage"
        );
    }
}

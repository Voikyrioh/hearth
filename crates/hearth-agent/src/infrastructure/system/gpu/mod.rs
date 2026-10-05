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

/// Arrondit une mesure à une décimale : pas de bruit inutile sur le fil.
pub(crate) fn round1(value: f32) -> f32 {
    (value * 10.0).round() / 10.0
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
    fn rounding_keeps_one_decimal() {
        assert_eq!(round1(12.345), 12.3);
        assert_eq!(round1(99.96), 100.0);
        assert_eq!(round1(0.04), 0.0);
    }
}

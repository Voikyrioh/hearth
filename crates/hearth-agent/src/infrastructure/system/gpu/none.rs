use crate::application::ports::GpuProbe;
use crate::domain::machine::GpuIdentity;
use crate::domain::metrics::GpuReading;

/// Aucune carte graphique mesurable : sous Windows (mode dev), ou quand rien n'est branché.
#[derive(Debug, Default)]
pub struct NoGpuProbe;

impl GpuProbe for NoGpuProbe {
    fn detect(&self) -> Vec<GpuIdentity> {
        Vec::new()
    }

    fn sample(&self) -> Vec<GpuReading> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_detects_nothing_and_measures_nothing() {
        assert!(NoGpuProbe.detect().is_empty());
        assert!(NoGpuProbe.sample().is_empty());
    }
}

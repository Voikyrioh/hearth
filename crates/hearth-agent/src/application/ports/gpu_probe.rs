use crate::domain::machine::GpuIdentity;
use crate::domain::metrics::GpuReading;

/// Cartes graphiques de la machine. Matériel absent = listes vides, jamais une erreur ni une
/// valeur inventée ; une mesure momentanément illisible est un champ absent de la lecture.
pub trait GpuProbe: Send + Sync {
    /// Cartes détectées à cet instant.
    fn detect(&self) -> Vec<GpuIdentity>;

    /// Dernières mesures de chaque carte détectée.
    fn sample(&self) -> Vec<GpuReading>;
}

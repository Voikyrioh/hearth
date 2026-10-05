//! Architecture prise en charge (BR-INSTALL-012).

/// Les architectures que l'installation accepte, telles qu'affichées dans le message de refus.
pub const SUPPORTED_ARCHITECTURES: &str = "x86_64, arm64";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    X86_64,
    Arm64,
}

/// Reconnaît le nom d'une architecture tel que le donnent Rust (`x86_64`, `aarch64`), `uname -m`
/// (`x86_64`, `aarch64`, `arm64`) ou les distributions (`amd64`). Tout le reste est refusé.
pub fn parse_arch(name: &str) -> Option<Arch> {
    match name.trim().to_ascii_lowercase().as_str() {
        "x86_64" | "amd64" => Some(Arch::X86_64),
        "aarch64" | "arm64" => Some(Arch::Arm64),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_usual_names_of_the_two_supported_architectures_are_accepted() {
        assert_eq!(parse_arch("x86_64"), Some(Arch::X86_64));
        assert_eq!(parse_arch("amd64"), Some(Arch::X86_64));
        assert_eq!(parse_arch("aarch64"), Some(Arch::Arm64));
        assert_eq!(parse_arch("ARM64\n"), Some(Arch::Arm64));
    }

    #[test]
    fn every_other_architecture_is_refused() {
        for name in ["i686", "armv7l", "riscv64", "ppc64le", "s390x", ""] {
            assert_eq!(parse_arch(name), None, "{name}");
        }
    }
}

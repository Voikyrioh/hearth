//! Architecture prise en charge (BR-INSTALL-012) : x86_64 seulement pour l'instant ; arm64 viendra
//! avec un binaire arm64 (aucun n'est construit aujourd'hui).

/// Les architectures que l'installation accepte, telles qu'affichées dans le message de refus.
pub const SUPPORTED_ARCHITECTURES: &str = "x86_64";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    X86_64,
}

/// Reconnaît le nom d'une architecture tel que le donnent Rust (`x86_64`), `uname -m` (`x86_64`)
/// ou les distributions (`amd64`). Tout le reste, `aarch64` et `arm64` compris, est refusé.
pub fn parse_arch(name: &str) -> Option<Arch> {
    match name.trim().to_ascii_lowercase().as_str() {
        "x86_64" | "amd64" => Some(Arch::X86_64),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_usual_names_of_x86_64_are_accepted() {
        assert_eq!(parse_arch("x86_64"), Some(Arch::X86_64));
        assert_eq!(parse_arch("amd64"), Some(Arch::X86_64));
        assert_eq!(parse_arch("AMD64\n"), Some(Arch::X86_64));
    }

    #[test]
    fn arm64_and_every_other_architecture_are_refused_for_now() {
        for name in [
            "aarch64", "arm64", "i686", "armv7l", "riscv64", "ppc64le", "s390x", "",
        ] {
            assert_eq!(parse_arch(name), None, "{name}");
        }
    }
}

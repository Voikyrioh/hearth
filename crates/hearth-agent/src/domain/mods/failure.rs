//! Pourquoi un serveur ne démarre pas avec ses nouveaux mods : la cause reconnue dans les dernières lignes du jeu, en
//! liste FERMÉE, avec sa phrase en clair (HRT-259, BR-MOD-027).
//!
//! Les lignes viennent d'un conteneur hostile : elles ne sont jamais crues au-delà de sous-chaînes LITTÉRALES connues
//! (aucune expression régulière, aucune interprétation), jamais tracées, jamais écrites au journal d'activité. Seule la
//! cause (un code fermé) en sort. Du pur : aucune E/S.

/// Les causes reconnues, de la plus précise à la plus générale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureReason {
    /// Un même mod est présent deux fois (deux fichiers, même identifiant).
    DuplicateMod,
    /// Un mod demande un autre mod qui est absent (ou d'une autre version).
    MissingDependency,
    /// Un mod n'est pas fait pour cette version du jeu, ou deux mods s'excluent.
    Incompatible,
    /// Le jeu manque de mémoire.
    OutOfMemory,
}

impl FailureReason {
    pub const ALL: [FailureReason; 4] = [
        Self::DuplicateMod,
        Self::MissingDependency,
        Self::Incompatible,
        Self::OutOfMemory,
    ];

    /// Le code stable, lu par le client (qui met le texte en phrase).
    pub const fn code(self) -> &'static str {
        match self {
            Self::DuplicateMod => "duplicate_mod",
            Self::MissingDependency => "missing_dependency",
            Self::Incompatible => "incompatible",
            Self::OutOfMemory => "out_of_memory",
        }
    }

    /// La phrase en clair (vouvoiement, phrase complète).
    pub const fn sentence(self) -> &'static str {
        match self {
            Self::DuplicateMod => {
                "Un même mod est installé deux fois : retirez l'un des deux fichiers."
            }
            Self::MissingDependency => {
                "Un des mods a besoin d'un autre mod qui est absent : ajoutez-le, ou retirez le mod qui le demande."
            }
            Self::Incompatible => {
                "Un des mods n'est pas fait pour cette version du jeu, ou ne s'entend pas avec un autre mod."
            }
            Self::OutOfMemory => {
                "Le jeu a manqué de mémoire au démarrage : donnez-lui plus de mémoire."
            }
        }
    }

    /// Les sous-chaînes littérales (en minuscules) qui désignent cette cause dans les lignes du jeu.
    const fn needles(self) -> &'static [&'static str] {
        match self {
            Self::DuplicateMod => &[
                "duplicate mod",
                "duplicate mods",
                "is present twice",
                "shares the same id",
                "mod id is already",
                "found duplicate",
            ],
            Self::MissingDependency => &[
                "requires any version of",
                "which is missing",
                "could not find required mod",
                "missing or unsupported mandatory dependencies",
                "unmet dependency",
                "depends on mod",
                "missing dependency",
            ],
            Self::Incompatible => &[
                "incompatible mods found",
                "incompatible with the game",
                "incompatible with minecraft",
                "are incompatible with",
                "is incompatible with",
                "some of your mods are incompatible",
            ],
            Self::OutOfMemory => &["java.lang.outofmemoryerror", "outofmemoryerror"],
        }
    }
}

/// La cause que ces lignes disent, s'il y en a une. La plus précise d'abord : « deux fois » avant « absent » avant
/// « incompatible » (un mod en double est souvent aussi vu comme incompatible).
pub fn reason_of<'a>(lines: impl IntoIterator<Item = &'a str>) -> Option<FailureReason> {
    let lowered: Vec<String> = lines
        .into_iter()
        .map(|line| line.to_ascii_lowercase())
        .collect();
    FailureReason::ALL.into_iter().find(|reason| {
        lowered
            .iter()
            .any(|line| reason.needles().iter().any(|needle| line.contains(needle)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(lines: &[&str]) -> Option<FailureReason> {
        reason_of(lines.iter().copied())
    }

    #[test]
    fn a_missing_dependency_is_recognised_from_the_fabric_report() {
        assert_eq!(
            read(&[
                "[main/ERROR]: Incompatible mods found!",
                "\t - Mod 'Sodium Extra' (sodium-extra) 0.5.0 requires any version of mod sodium, which is missing!"
            ]),
            Some(FailureReason::MissingDependency)
        );
    }

    #[test]
    fn a_mod_present_twice_wins_over_the_other_causes() {
        assert_eq!(
            read(&[
                "Incompatible mods found!",
                "Duplicate mod 'lithium' found in lithium-1.jar and lithium-2.jar"
            ]),
            Some(FailureReason::DuplicateMod)
        );
    }

    #[test]
    fn an_incompatibility_and_a_lack_of_memory_are_recognised() {
        assert_eq!(
            read(&["Mod 'x' (x) is incompatible with minecraft 1.19"]),
            Some(FailureReason::Incompatible)
        );
        assert_eq!(
            read(&["Exception in thread \"main\" java.lang.OutOfMemoryError: Java heap space"]),
            Some(FailureReason::OutOfMemory)
        );
    }

    #[test]
    fn an_unknown_report_has_no_reason_and_never_a_guessed_one() {
        assert_eq!(read(&["Loading...", "something odd happened"]), None);
        assert_eq!(read(&[]), None);
    }

    #[test]
    fn every_reason_has_a_distinct_code_and_a_full_sentence() {
        let mut codes: Vec<&str> = FailureReason::ALL.iter().map(|r| r.code()).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), FailureReason::ALL.len());
        for reason in FailureReason::ALL {
            assert!(reason.sentence().ends_with('.'), "{reason:?}");
            assert!(!reason.sentence().contains('—'), "pas de tiret cadratin");
        }
    }
}

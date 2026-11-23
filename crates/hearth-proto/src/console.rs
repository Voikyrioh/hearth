//! La console d'un serveur de jeu : forme d'une ligne et classe d'une commande (HRT-267, ADR-0067, BR-CONSOLE-003, 004).
//!
//! Du domaine PUR, partagé par l'agent (qui reclasse et décide, il ne croit jamais le client) et par la liaison cliente
//! (qui classe pour l'écran : confirmation, mot de passe). Aucune E/S. Le texte d'une ligne est du contenu sensible :
//! `ConsoleLine` n'a pas de `Debug` qui le montre.

use std::collections::BTreeMap;

/// Au plus ce nombre de points de code dans une ligne.
pub const MAX_LINE_CHARS: usize = 256;

/// Pourquoi une ligne n'a pas la forme d'une commande.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineError {
    /// Vide (ou une seule barre oblique).
    Empty,
    /// Plus de [`MAX_LINE_CHARS`] points de code.
    TooLong,
    /// Un caractère de contrôle (`\n`, `\r`, `\t`, `\0`...), un séparateur de ligne, un invisible ou un renversement du
    /// sens d'écriture.
    ForbiddenChar,
}

/// Un caractère qui n'a rien à faire dans une commande : contrôle, séparateur de ligne Unicode, invisible, ou
/// renversement du sens d'écriture (la même définition que le chargeur de fiches).
pub fn is_risky_char(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{2028}' | '\u{2029}' | '\u{feff}' | '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}'
                | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{2069}'
        )
}

/// Une ligne de console normalisée : elle ne se construit que par [`normalize`]. Son texte n'est jamais montré par
/// `Debug`.
#[derive(Clone, PartialEq, Eq)]
pub struct ConsoleLine(String);

impl ConsoleLine {
    /// Le texte envoyé : la ligne normalisée, sans autre changement.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Le nombre de points de code.
    pub fn chars(&self) -> usize {
        self.0.chars().count()
    }

    /// Le premier mot, tel qu'écrit.
    pub fn first_word(&self) -> &str {
        self.0.split(' ').next().unwrap_or_default()
    }

    /// Le deuxième mot, s'il existe (l'argument pris pour `{joueur}`).
    pub fn first_arg(&self) -> Option<&str> {
        self.0.split(' ').filter(|word| !word.is_empty()).nth(1)
    }

    /// Le verbe jugé : le premier mot en minuscules ASCII, sans le préfixe `minecraft:`.
    pub fn verb(&self) -> String {
        let lower = self.first_word().to_ascii_lowercase();
        lower
            .strip_prefix("minecraft:")
            .map_or_else(|| lower.clone(), str::to_owned)
    }
}

impl std::fmt::Debug for ConsoleLine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ConsoleLine({} caractères)", self.chars())
    }
}

/// Retire les espaces de tête et de fin et UNE barre oblique de tête ; refuse vide, plus de 256 points de code, tout
/// caractère de contrôle et tout caractère à risque.
pub fn normalize(raw: &str) -> Result<ConsoleLine, LineError> {
    if raw.chars().any(is_risky_char) {
        return Err(LineError::ForbiddenChar);
    }
    let trimmed = raw.trim();
    let line = trimmed.strip_prefix('/').unwrap_or(trimmed).trim_start();
    if line.is_empty() {
        return Err(LineError::Empty);
    }
    if line.chars().count() > MAX_LINE_CHARS {
        return Err(LineError::TooLong);
    }
    Ok(ConsoleLine(line.to_owned()))
}

/// Pourquoi un verbe est refusé (liste fermée de la fiche).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RefusalReason {
    /// L'arrêt se demande par Hearth (il enregistre le monde et dit ce qu'il fait).
    UseHearthStop,
    /// Le redémarrage se demande par Hearth.
    UseHearthRestart,
    /// La commande casse le serveur.
    BreaksServer,
}

impl RefusalReason {
    pub const ALL: [RefusalReason; 3] = [
        Self::UseHearthStop,
        Self::UseHearthRestart,
        Self::BreaksServer,
    ];

    pub const fn code(self) -> &'static str {
        match self {
            Self::UseHearthStop => "use_hearth_stop",
            Self::UseHearthRestart => "use_hearth_restart",
            Self::BreaksServer => "breaks_server",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|reason| reason.code() == text)
    }
}

/// La classe d'une commande.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleClass {
    /// Sans conséquence durable particulière.
    Free,
    /// Donne un pouvoir, change qui peut entrer, ou casse la sauvegarde : le mot de passe est exigé.
    Sensitive,
    /// Jamais envoyée par Hearth.
    Refused(RefusalReason),
}

impl ConsoleClass {
    /// Le code signé de la classe : `1` libre, `2` sensible. Une commande refusée n'a pas d'acte.
    pub const fn signed_code(self) -> Option<u8> {
        match self {
            Self::Free => Some(1),
            Self::Sensitive => Some(2),
            Self::Refused(_) => None,
        }
    }
}

/// Ce que la fiche dit des verbes (jamais écrit dans le client ni dans l'agent).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConsolePolicy {
    refused: BTreeMap<String, RefusalReason>,
    sensitive: Vec<String>,
}

impl ConsolePolicy {
    pub fn new(
        refused: impl IntoIterator<Item = (String, RefusalReason)>,
        sensitive: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            refused: refused.into_iter().collect(),
            sensitive: sensitive.into_iter().collect(),
        }
    }

    pub fn refusal(&self, verb: &str) -> Option<RefusalReason> {
        self.refused.get(verb).copied()
    }

    pub fn is_sensitive(&self, verb: &str) -> bool {
        self.sensitive.iter().any(|v| v == verb)
    }
}

/// La classe d'une ligne : le verbe (premier mot en minuscules ASCII, préfixe `minecraft:` retiré) est refusé,
/// sensible ou libre selon la politique de la fiche ; un verbe à espace de noms inconnu (`x:y`) est sensible.
pub fn classify(policy: &ConsolePolicy, line: &ConsoleLine) -> ConsoleClass {
    let verb = line.verb();
    if let Some(reason) = policy.refusal(&verb) {
        return ConsoleClass::Refused(reason);
    }
    // Un verbe d'un autre espace de noms (`mod:commande`, hors `minecraft:`) est celui d'un mod : Hearth ne sait pas ce
    // qu'il fait, le mot de passe est exigé (ADR-0067).
    if policy.is_sensitive(&verb) || verb.contains(':') {
        ConsoleClass::Sensitive
    } else {
        ConsoleClass::Free
    }
}

/// L'empreinte signée d'une ligne : SHA-256 de ses octets UTF-8.
pub fn line_digest(line: &ConsoleLine) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    Sha256::digest(line.as_str().as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> ConsolePolicy {
        ConsolePolicy::new(
            [
                ("stop".to_owned(), RefusalReason::UseHearthStop),
                ("restart".to_owned(), RefusalReason::UseHearthRestart),
                ("reload".to_owned(), RefusalReason::BreaksServer),
            ],
            [
                "op",
                "deop",
                "ban",
                "kick",
                "whitelist",
                "save-off",
                "execute",
            ]
            .map(str::to_owned),
        )
    }

    fn class(text: &str) -> ConsoleClass {
        classify(&policy(), &normalize(text).expect("ligne"))
    }

    #[test]
    fn a_line_is_trimmed_loses_one_leading_slash_and_keeps_its_text() {
        assert_eq!(
            normalize("  /time set day  ").unwrap().as_str(),
            "time set day"
        );
        assert_eq!(normalize("//say x").unwrap().as_str(), "/say x");
        assert_eq!(
            normalize("say  deux  espaces").unwrap().as_str(),
            "say  deux  espaces"
        );
    }

    #[test]
    fn an_empty_too_long_or_hostile_line_is_refused() {
        assert_eq!(normalize("").unwrap_err(), LineError::Empty);
        assert_eq!(normalize("   ").unwrap_err(), LineError::Empty);
        assert_eq!(normalize("/").unwrap_err(), LineError::Empty);
        assert!(normalize(&"a".repeat(256)).is_ok());
        assert_eq!(normalize(&"a".repeat(257)).unwrap_err(), LineError::TooLong);
        // Compté en points de code, pas en octets.
        assert!(normalize(&"é".repeat(256)).is_ok());
        assert_eq!(normalize(&"é".repeat(257)).unwrap_err(), LineError::TooLong);
        for hostile in [
            "say a\nstop",
            "say a\rstop",
            "say a\tb",
            "say a\0b",
            "say a\u{200b}b",
            "say a\u{202e}b",
            "say a\u{2028}b",
            "say a\u{feff}b",
            "\u{7f}say",
        ] {
            assert_eq!(
                normalize(hostile).unwrap_err(),
                LineError::ForbiddenChar,
                "{hostile:?}"
            );
        }
    }

    #[test]
    fn the_verb_is_judged_lowercase_without_the_minecraft_prefix() {
        assert_eq!(class("time set day"), ConsoleClass::Free);
        assert_eq!(class("/minecraft:op Bram"), ConsoleClass::Sensitive);
        // Un verbe de mod (autre espace de noms) est sensible par défaut ; `minecraft:` est retiré avant.
        assert_eq!(class("coolmod:wipe all"), ConsoleClass::Sensitive);
        assert_eq!(class("minecraft:list"), ConsoleClass::Free);
        assert_eq!(class("OP Bram"), ConsoleClass::Sensitive);
        assert_eq!(
            class("STOP"),
            ConsoleClass::Refused(RefusalReason::UseHearthStop)
        );
        assert_eq!(
            class("minecraft:reload"),
            ConsoleClass::Refused(RefusalReason::BreaksServer)
        );
        assert_eq!(class("save-off"), ConsoleClass::Sensitive);
        assert_eq!(
            class("say op"),
            ConsoleClass::Free,
            "seul le premier mot juge"
        );
        assert_eq!(
            class("opx Bram"),
            ConsoleClass::Free,
            "un verbe n'est pas un préfixe"
        );
    }

    #[test]
    fn the_first_argument_is_the_second_word() {
        let line = normalize("op Bram extra").unwrap();
        assert_eq!(line.first_word(), "op");
        assert_eq!(line.first_arg(), Some("Bram"));
        assert_eq!(normalize("list").unwrap().first_arg(), None);
    }

    #[test]
    fn the_text_of_a_line_is_never_in_its_debug_form() {
        let line = normalize("op Secret-Pseudo").unwrap();
        assert!(!format!("{line:?}").contains("Secret"));
    }

    #[test]
    fn the_signed_class_codes_are_one_and_two_and_a_refusal_has_none() {
        assert_eq!(ConsoleClass::Free.signed_code(), Some(1));
        assert_eq!(ConsoleClass::Sensitive.signed_code(), Some(2));
        assert_eq!(
            ConsoleClass::Refused(RefusalReason::BreaksServer).signed_code(),
            None
        );
    }

    #[test]
    fn the_refusal_reasons_round_trip() {
        for reason in RefusalReason::ALL {
            assert_eq!(RefusalReason::parse(reason.code()), Some(reason));
        }
        assert_eq!(RefusalReason::parse("autre"), None);
    }
}

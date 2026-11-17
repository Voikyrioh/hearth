//! Les identifiants que le fil et le client reconnaissent, définis UNE fois (agent, liaison et coquille s'y réfèrent).

/// Longueur maximale d'un identifiant stable (comme `MAX_FICHE_TOKEN_CHARS`).
pub const STABLE_ID_MAX_LEN: usize = 64;

/// Un identifiant stable (de fiche, de question, de contrat, de port) est un mot court : minuscules, chiffres, tiret et
/// souligné ; tout autre texte n'est pas un identifiant, ne part pas et n'est jamais répété.
pub fn is_stable_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= STABLE_ID_MAX_LEN
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stable_id_is_a_short_lowercase_word() {
        assert!(is_stable_id("terraria"));
        assert!(is_stable_id("a_b-9"));
        assert!(!is_stable_id(""));
        assert!(!is_stable_id("Terraria"));
        assert!(!is_stable_id("a/b"));
        assert!(!is_stable_id(&"a".repeat(65)));
        assert!(is_stable_id(&"a".repeat(64)));
    }
}

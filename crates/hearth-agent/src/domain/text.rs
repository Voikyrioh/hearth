//! Texte saisi par un client (identifiant, nom du poste) avant qu'il n'entre dans une clé, une
//! trace ou un événement du journal : rien qui puisse forger une ligne ou en retourner l'affichage.

/// Le caractère peut-il fausser une ligne de journal ou son affichage ?
///
/// Retire : les caractères de contrôle (Cc), les séparateurs de ligne et de paragraphe Unicode
/// (U+2028, U+2029) et les caractères de format (Cf : marques et isolats bidirectionnels,
/// espaces de largeur nulle, marque d'ordre des octets, étiquettes).
pub fn is_unsafe_char(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{2028}' | '\u{2029}'
            // Cf
            | '\u{00AD}'
            | '\u{0600}'..='\u{0605}'
            | '\u{061C}'
            | '\u{06DD}'
            | '\u{070F}'
            | '\u{0890}'..='\u{0891}'
            | '\u{08E2}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{206F}'
            | '\u{FEFF}'
            | '\u{FFF9}'..='\u{FFFB}'
            | '\u{110BD}'
            | '\u{110CD}'
            | '\u{1BCA0}'..='\u{1BCA3}'
            | '\u{1D173}'..='\u{1D17A}'
            | '\u{E0001}'
            | '\u{E0020}'..='\u{E007F}'
        )
}

/// Octets en hexadécimal minuscule (une seule copie pour la somme et l'empreinte).
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Le texte sans caractère de `is_unsafe_char`.
pub fn strip_unsafe(text: &str) -> String {
    text.chars().filter(|&c| !is_unsafe_char(c)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_line_separators_and_format_characters_are_removed() {
        for unsafe_char in [
            '\n', '\r', '\t', '\u{1f}', '\u{85}', '\u{2028}', '\u{2029}', '\u{202E}', '\u{202A}',
            '\u{2066}', '\u{2069}', '\u{200E}', '\u{200F}', '\u{200B}', '\u{FEFF}', '\u{061C}',
        ] {
            assert!(is_unsafe_char(unsafe_char), "{unsafe_char:?}");
            assert_eq!(strip_unsafe(&format!("a{unsafe_char}b")), "ab");
        }
    }

    #[test]
    fn ordinary_text_is_kept_including_accents_spaces_and_scripts() {
        let text = "poste de Zoé 01 - 日本語 مرحبا";
        assert_eq!(strip_unsafe(text), text);
    }
}

//! Bornes de tout ce qui vient d'un conteneur ou du moteur avant d'aller sur le fil (HRT-61).
//!
//! Un conteneur est créé par n'importe qui sur le serveur : son nom, son image, ses étiquettes, ses
//! chemins de montage et le nombre de ses ports ne sont pas de confiance. Chaque texte est débarrassé
//! des caractères de contrôle (retours à la ligne, échappements) puis coupé à une longueur
//! plafonnée ; les listes sont plafonnées en nombre. Ces bornes sont des règles du domaine, pas des
//! détails de l'adaptateur : elles valent pour tout moteur (`BR-SVC-021` : jusqu'à 255 caractères restent lisibles en entier).

/// Longueur maximale (en caractères) du nom d'un service.
pub const MAX_NAME_CHARS: usize = 255;
/// Longueur maximale de la référence d'une image (sans la version).
pub const MAX_IMAGE_REF_CHARS: usize = 255;
/// Longueur maximale de la version d'une image (le moteur limite déjà à 128).
pub const MAX_IMAGE_TAG_CHARS: usize = 128;
/// Longueur maximale d'un chemin de montage ou du nom d'un volume.
pub const MAX_PATH_CHARS: usize = 1024;
/// Longueur maximale du projet Compose.
pub const MAX_LABEL_CHARS: usize = 255;
/// Longueur maximale du nom et de la version annoncés par le moteur.
pub const MAX_ENGINE_TEXT_CHARS: usize = 64;
/// Nombre maximal de ports publiés gardés par service (un serveur de jeu en publie des dizaines ; au
/// delà, les suivants sont omis).
pub const MAX_PORTS: usize = 128;
/// Nombre maximal de montages gardés par service.
pub const MAX_MOUNTS: usize = 64;

/// Marque d'un texte coupé.
const ELLIPSIS: char = '…';

/// Un caractère qui n'a rien à faire dans un nom affiché : contrôle (retour à la ligne, échappement)
/// ou formatage Unicode invisible qui trompe l'œil (renversement du sens d'écriture U+202A à U+202E
/// et U+2066 à U+2069, espaces et joints de largeur nulle U+200B à U+200F, U+2060 à U+2064, U+061C,
/// U+FEFF), plus le trait d'union conditionnel U+00AD et les séparateurs de ligne et de paragraphe
/// U+2028 et U+2029 (hors catégorie « contrôle » : un nom pouvait casser une ligne d'affichage).
/// FIX:01M4SFCSXW2P3QXP6FY8CS6ZMQ (docs/bugs/FIX-01M4SFCSXW2P3QXP6FY8CS6ZMQ.md). Sans eux, un chemin de montage ou une étiquette pourrait se lire à l'envers.
fn is_unsafe_char(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{00AD}'
                | '\u{061C}'
                | '\u{200B}'..='\u{200F}'
                | '\u{2028}'..='\u{2029}'
                | '\u{202A}'..='\u{202E}'
                | '\u{2060}'..='\u{2064}'
                | '\u{2066}'..='\u{2069}'
                | '\u{FEFF}'
        )
}

/// Le texte sans caractère de contrôle ni de formatage invisible, coupé à `max` caractères (la marque « … » compte dans la
/// limite). Un texte que la coupe ne touche pas est rendu tel quel.
pub fn bounded(text: &str, max: usize) -> String {
    let mut cleaned: String = text.chars().filter(|c| !is_unsafe_char(*c)).collect();
    if cleaned.chars().count() > max {
        cleaned = cleaned
            .chars()
            .take(max.saturating_sub(1))
            .chain(std::iter::once(ELLIPSIS))
            .collect();
    }
    cleaned
}

/// Comme [`bounded`] pour un texte facultatif : un texte vide après nettoyage devient `None`.
pub fn bounded_opt(text: Option<&str>, max: usize) -> Option<String> {
    text.map(|text| bounded(text, max))
        .filter(|text| !text.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_characters_are_removed() {
        assert_eq!(bounded("web\n\r\t\u{1b}[31mx\u{0}", 64), "web[31mx");
        assert_eq!(bounded("tout va bien", 64), "tout va bien");
    }

    #[test]
    fn bidirectional_and_invisible_formatting_characters_are_removed() {
        // U+202E renverse l'affichage : ce chemin se lirait « /data/exe.jpg ».
        assert_eq!(bounded("/data/\u{202E}gpj.exe", 64), "/data/gpj.exe");
        assert_eq!(bounded("a\u{200B}b\u{2066}c\u{FEFF}d\u{061C}", 64), "abcd");
        // Séparateurs de ligne et de paragraphe, trait d'union conditionnel (hors « contrôle »).
        assert_eq!(bounded("web\u{2028}x\u{2029}y\u{00AD}z", 64), "webxyz");
        // Les lettres accentuées et l'arabe ordinaire restent.
        assert_eq!(bounded("é مرحبا", 64), "é مرحبا");
    }

    #[test]
    fn a_long_text_is_cut_to_the_limit_with_a_mark() {
        let cut = bounded(&"é".repeat(500), MAX_NAME_CHARS);
        assert_eq!(cut.chars().count(), MAX_NAME_CHARS);
        assert!(cut.ends_with('…'));
        // Exactement à la limite : intact.
        let exact = "a".repeat(MAX_NAME_CHARS);
        assert_eq!(bounded(&exact, MAX_NAME_CHARS), exact);
    }

    #[test]
    fn an_optional_text_empty_after_cleaning_is_absent() {
        assert_eq!(bounded_opt(Some("\n\t"), 10), None);
        assert_eq!(bounded_opt(None, 10), None);
        assert_eq!(bounded_opt(Some("a"), 10).as_deref(), Some("a"));
    }
}

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

/// Le texte sans caractère de contrôle, coupé à `max` caractères (la marque « … » compte dans la
/// limite). Un texte que la coupe ne touche pas est rendu tel quel.
pub fn bounded(text: &str, max: usize) -> String {
    let mut cleaned: String = text.chars().filter(|c| !c.is_control()).collect();
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

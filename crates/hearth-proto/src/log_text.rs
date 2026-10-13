//! Nettoyage du texte d'une ligne de journal de service (HRT-66, BR-SLOG-013). UNE implémentation,
//! appliquée par l'agent avant l'envoi : l'interface affiche le texte tel que reçu, sans rien
//! interpréter.
//!
//! Un programme écrit ce qu'il veut dans ses journaux. Sont retirés :
//! - les séquences d'échappement du terminal (couleurs et mise en forme `ESC [ ... m`, positionnement
//!   du curseur, titres de fenêtre `ESC ] ... BEL`, jeux de caractères `ESC ( B`, et leurs formes à un
//!   seul caractère `U+009B`) ;
//! - tout autre caractère de contrôle, `U+0000` compris (la tabulation reste : sans elle les sorties en
//!   colonnes se collent) ;
//! - les caractères de formatage Unicode invisibles ou qui renversent l'écriture (U+202A à U+202E,
//!   U+2066 à U+2069, espaces et joints de largeur nulle, U+FEFF, trait d'union conditionnel, séparateurs
//!   de ligne et de paragraphe U+2028 et U+2029, étiquettes U+E0000 à U+E007F, caractères de remplissage
//!   U+115F, U+1160, U+3164, U+FFA0, U+180E, annotations U+FFF9 à U+FFFB, formatage musical U+1D173 à
//!   U+1D17A). Les joints U+200C et U+200D sont retirés aussi : un émoji composé se défait et le persan
//!   perd ses séparations, choix assumé (BR-SLOG-013). La liste est la même que celle des textes de conteneur
//!   de l'agent (`domain::services::limits`).
//!
//! La coupe des lignes de plus de [`MAX_LINE_CHARS`] caractères est ici aussi ([`bound`]) : l'agent et
//! les tests ne comptent pas chacun à leur façon. Le retour chariot seul vaut une fin de ligne : c'est
//! l'assembleur de lignes de l'agent qui le découpe, avant ce nettoyage (une chaîne nettoyée est
//! une seule ligne).

/// Longueur maximale (en caractères) du texte d'une ligne envoyée. Au-delà, le texte est coupé et le
/// total est annoncé (`cut`).
pub const MAX_LINE_CHARS: usize = 10_000;

/// Un caractère de formatage Unicode qui n'a rien à faire dans un texte affiché : invisible, ou qui
/// renverse le sens d'écriture, ou qui casse une ligne d'affichage (hors catégorie « contrôle »).
pub fn is_invisible_format(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}'
            | '\u{061C}'
            // Remplissages Hangul et séparateur de voyelles mongol : invisibles, mais « pleins ».
            | '\u{115F}'..='\u{1160}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{2028}'..='\u{2029}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{2069}'
            | '\u{3164}'
            | '\u{FEFF}'
            | '\u{FFA0}'
            // Annotations interlinéaires, formatage musical, étiquettes (texte invisible qui part avec un
            // copier-coller).
            | '\u{FFF9}'..='\u{FFFB}'
            | '\u{1D173}'..='\u{1D17A}'
            | '\u{E0000}'..='\u{E007F}'
    )
}

/// Ce caractère est-il retiré d'une ligne de journal ? Les contrôles (tabulation exceptée) et les
/// caractères de formatage invisibles.
fn is_removed(c: char) -> bool {
    (c.is_control() && c != '\t') || is_invisible_format(c)
}

/// Le texte d'UNE ligne, sans séquence d'échappement ni caractère de contrôle ni caractère de
/// formatage invisible. Total : aucun texte ne la fait échouer.
pub fn clean(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\u{1b}' => skip_escape(&mut chars),
            // Introducteur de séquence de contrôle à un seul caractère (C1).
            '\u{9b}' => skip_csi_body(&mut chars),
            c if is_removed(c) => {}
            c => out.push(c),
        }
    }
    out
}

/// Ce qui suit un `ESC` : le consomme (`ESC` déjà lu).
fn skip_escape(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    match chars.peek().copied() {
        // CSI : `ESC [` paramètres, intermédiaires, un octet final (0x40 à 0x7E).
        Some('[') => {
            chars.next();
            skip_csi_body(chars);
        }
        // OSC, DCS, SOS, PM, APC : une chaîne jusqu'à BEL ou au terminateur `ESC \`.
        Some(']' | 'P' | 'X' | '^' | '_') => {
            chars.next();
            while let Some(c) = chars.next() {
                if c == '\u{7}' || c == '\u{9c}' {
                    return;
                }
                if c == '\u{1b}' {
                    if chars.peek() == Some(&'\\') {
                        chars.next();
                    }
                    return;
                }
            }
        }
        // `ESC` et des intermédiaires (0x20 à 0x2F : `ESC ( B`) puis un octet final (0x30 à 0x7E).
        Some(c) if ('\u{20}'..='\u{2f}').contains(&c) => {
            while chars
                .peek()
                .is_some_and(|next| ('\u{20}'..='\u{2f}').contains(next))
            {
                chars.next();
            }
            if chars
                .peek()
                .is_some_and(|next| ('\u{30}'..='\u{7e}').contains(next))
            {
                chars.next();
            }
        }
        Some(c) if ('\u{30}'..='\u{7e}').contains(&c) => {
            chars.next();
        }
        // `ESC` seul, ou suivi d'autre chose : le caractère suivant n'appartient pas à la séquence.
        _ => {}
    }
}

/// Le corps d'une séquence CSI (après `ESC [` ou `U+009B`) : paramètres (0x30 à 0x3F), intermédiaires
/// (0x20 à 0x2F), octet final (0x40 à 0x7E). Un caractère hors de cette grammaire interrompt la
/// séquence sans être consommé.
fn skip_csi_body(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    while let Some(&c) = chars.peek() {
        match c {
            '\u{20}'..='\u{3f}' => {
                chars.next();
            }
            '\u{40}'..='\u{7e}' => {
                chars.next();
                return;
            }
            _ => return,
        }
    }
}

/// Coupe un texte déjà nettoyé à [`MAX_LINE_CHARS`] caractères. Rend le texte gardé et, s'il a été
/// coupé, le nombre total de caractères de la ligne BRUTE (`dropped` : caractères déjà écartés avant, comptés
/// dans le total sans avoir été gardés ; les codes d'échappement et les caractères retirés par [`clean`] y
/// comptent, à un caractère près au bord de la coupe). `None` : la ligne est entière.
pub fn bound(text: &str, dropped: usize) -> (String, Option<u32>) {
    let kept = text.chars().count();
    if kept <= MAX_LINE_CHARS && dropped == 0 {
        return (text.to_owned(), None);
    }
    let total = u32::try_from(kept.saturating_add(dropped)).unwrap_or(u32::MAX);
    let cut: String = text.chars().take(MAX_LINE_CHARS).collect();
    (cut, Some(total))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_codes_are_removed_and_the_text_stays() {
        assert_eq!(
            clean("\u{1b}[31merreur\u{1b}[0m : disque plein"),
            "erreur : disque plein"
        );
        assert_eq!(clean("\u{1b}[1;38;5;208mx\u{1b}[0m"), "x");
        assert_eq!(clean("\u{1b}[2K\u{1b}[1Gligne"), "ligne");
        assert_eq!(clean("a\u{9b}31mb"), "ab");
    }

    #[test]
    fn window_titles_and_charset_switches_are_removed() {
        assert_eq!(clean("\u{1b}]0;titre\u{7}après"), "après");
        assert_eq!(
            clean("\u{1b}]8;;http://x\u{1b}\\lien\u{1b}]8;;\u{1b}\\"),
            "lien"
        );
        assert_eq!(clean("\u{1b}(Bok"), "ok");
        assert_eq!(clean("\u{1b}cok"), "ok");
    }

    #[test]
    fn a_broken_sequence_never_eats_the_text_around_it() {
        // `ESC` seul à la fin, `ESC [` coupé, `ESC` suivi d'un caractère de contrôle.
        assert_eq!(clean("fin\u{1b}"), "fin");
        assert_eq!(clean("a\u{1b}[b"), "a");
        assert_eq!(clean("a\u{1b}\u{7}b"), "ab");
        assert_eq!(clean("a\u{1b}[31\nb"), "ab");
    }

    #[test]
    fn control_characters_are_removed_but_the_tab_stays() {
        assert_eq!(clean("a\u{0}b\u{7}c\u{8}d\u{7f}e\u{85}f"), "abcdef");
        assert_eq!(clean("a\tb"), "a\tb");
        // Aucun saut de ligne ne survit : une ligne reste une ligne.
        assert_eq!(clean("a\nb\rc"), "abc");
    }

    #[test]
    fn invisible_and_direction_changing_characters_are_removed() {
        assert_eq!(clean("/data/\u{202E}gpj.exe"), "/data/gpj.exe");
        assert_eq!(
            clean("a\u{200B}b\u{2066}c\u{FEFF}d\u{061C}e\u{2028}f\u{2029}g\u{00AD}h"),
            "abcdefgh"
        );
        assert_eq!(clean("é مرحبا 日本語 🙂"), "é مرحبا 日本語 🙂");
    }

    #[test]
    fn tag_characters_and_invisible_fillers_are_removed_too() {
        // Un texte écrit en étiquettes (U+E0000...) est invisible mais part avec un copier-coller.
        let hidden: String = "ignore"
            .chars()
            .map(|c| char::from_u32(0xE0000 + u32::from(c)).unwrap_or('?'))
            .collect();
        assert_eq!(clean(&format!("ok{hidden}")), "ok");
        assert_eq!(
            clean("a\u{180E}b\u{FFF9}c\u{FFFB}d\u{1D173}e\u{115F}f\u{1160}g\u{3164}h\u{FFA0}i"),
            "abcdefghi"
        );
    }

    #[test]
    fn a_text_without_anything_to_remove_is_unchanged() {
        let plain = "2026-10-13 10:00:00 INFO  listening on :8080 (pid=42)";
        assert_eq!(clean(plain), plain);
        assert_eq!(clean(""), "");
    }

    #[test]
    fn a_long_line_is_cut_at_the_limit_and_its_total_is_given() {
        let long = "é".repeat(MAX_LINE_CHARS + 25);
        let (kept, cut) = bound(&long, 0);
        assert_eq!(kept.chars().count(), MAX_LINE_CHARS);
        assert_eq!(cut, Some(u32::try_from(MAX_LINE_CHARS + 25).unwrap()));
        // Exactement à la limite : entière.
        let exact = "a".repeat(MAX_LINE_CHARS);
        assert_eq!(bound(&exact, 0), (exact.clone(), None));
        // Des caractères déjà écartés avant : la ligne est annoncée coupée, total compris.
        let (kept, cut) = bound("abc", 500);
        assert_eq!(kept, "abc");
        assert_eq!(cut, Some(503));
    }
}

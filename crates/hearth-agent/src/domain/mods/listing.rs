//! Les noms de fichiers des mods (HRT-254, BR-MOD-020, 022, 024, 032) : domaine pur.
//!
//! Un nom vient du volume du service, écrit par le jeu et ses mods : il est HOSTILE. Seul un nom de l'alphabet sûr
//! est rendu tel quel ou manipulé ; tout autre est montré par son rang et ne peut être ni retiré ni désactivé.

/// Longueur maximale d'un nom de fichier gardé.
pub const MAX_NAME: usize = 128;
/// L'extension d'un mod actif et celle d'un mod désactivé.
pub const JAR: &str = ".jar";
pub const DISABLED: &str = ".jar.disabled";
/// Au plus ce numéro dans « Mods retirés » : `-2` … `-99`.
pub const MAX_RETIRED_SUFFIX: u32 = 99;

/// Lettres, chiffres, espaces et `._+()-`, 5 à 128 octets, jamais un nom qui commence par un point ni par un espace.
pub fn is_safe_file_name(name: &str) -> bool {
    name.len() >= 5
        && name.len() <= MAX_NAME
        && !name.starts_with('.')
        && !name.starts_with(' ')
        && !name.ends_with(' ')
        && name.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b' ' | b'.' | b'_' | b'+' | b'(' | b')' | b'-')
        })
}

/// La fin se compare par OCTETS : jamais une découpe de texte (panique UTF-8).
fn ends_with_ignore_case(name: &str, suffix: &str) -> bool {
    name.len() >= suffix.len()
        && name.as_bytes()[name.len() - suffix.len()..].eq_ignore_ascii_case(suffix.as_bytes())
}

pub fn is_active_jar(name: &str) -> bool {
    ends_with_ignore_case(name, JAR)
}

pub fn is_disabled_jar(name: &str) -> bool {
    ends_with_ignore_case(name, DISABLED)
}

/// Le nom sans `.disabled` (identité du fichier pour l'origine et les doublons).
pub fn active_name(name: &str) -> &str {
    if is_disabled_jar(name) {
        &name[..name.len() - ".disabled".len()]
    } else {
        name
    }
}

/// Le nom désactivé d'un nom actif.
pub fn disabled_name(active: &str) -> String {
    format!("{active}.disabled")
}

/// Le nom sous lequel Hearth pose un jar déposé : `{identifiant}-{version}.jar` (la version est omise si le jar
/// ne la dit pas : `{identifiant}.jar`). Jamais le nom envoyé.
pub fn placed_name(id: &str, version: Option<&str>) -> String {
    match version {
        Some(version) => format!("{id}-{version}{JAR}"),
        None => format!("{id}{JAR}"),
    }
}

/// Le nom dans « Mods retirés » pour la `n`-ième tentative : `n` = 1 donne le nom lui-même, puis `-2`, `-3`…
/// (avant l'extension, `.jar` ou `.jar.disabled`). `None` au-delà de 99.
pub fn retired_name(name: &str, n: u32) -> Option<String> {
    if n == 0 || n > MAX_RETIRED_SUFFIX {
        return None;
    }
    if n == 1 {
        return Some(name.to_owned());
    }
    let ext = if is_disabled_jar(name) {
        DISABLED.len()
    } else if is_active_jar(name) {
        JAR.len()
    } else {
        return None;
    };
    let stem = &name[..name.len() - ext];
    let suffix = &name[name.len() - ext..];
    Some(format!("{stem}-{n}{suffix}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_safe_alphabet() {
        for ok in [
            "sodium-0.5.8+mc1.20.1.jar",
            "Mod Menu (1.0).jar",
            "a_b-c.jar.disabled",
        ] {
            assert!(is_safe_file_name(ok), "{ok}");
        }
        for bad in [
            ".jar",
            ".hidden.jar",
            "a/b.jar",
            "a\\b.jar",
            "é.jar",
            "a\u{202e}.jar",
            "a\0.jar",
            "x.jar\n",
            " lead.jar",
            "trail.jar ",
            "",
            "ab",
        ] {
            assert!(!is_safe_file_name(bad), "{bad:?}");
        }
        assert!(!is_safe_file_name(&format!("{}.jar", "a".repeat(MAX_NAME))));
    }

    #[test]
    fn states_and_names() {
        assert!(is_active_jar("a.JAR") && !is_active_jar("a.jar.disabled"));
        assert!(is_disabled_jar("a.jar.disabled") && !is_disabled_jar("a.jar"));
        assert_eq!(active_name("a.jar.disabled"), "a.jar");
        assert_eq!(active_name("a.jar"), "a.jar");
        assert_eq!(disabled_name("a.jar"), "a.jar.disabled");
        // Texte hostile : aucune panique au milieu d'un caractère.
        assert!(!is_active_jar("é"));
        assert!(!is_disabled_jar("ééééééééééééé"));
        assert_eq!(active_name("éé"), "éé");
    }

    #[test]
    fn the_name_hearth_gives_to_a_placed_jar() {
        assert_eq!(placed_name("sodium", Some("0.5.8")), "sodium-0.5.8.jar");
        assert_eq!(placed_name("sodium", None), "sodium.jar");
        assert!(is_safe_file_name(&placed_name(
            "a.b-c_d",
            Some("1.0+mc1.20")
        )));
    }

    #[test]
    fn a_retired_name_gets_a_number_when_taken() {
        assert_eq!(retired_name("a.jar", 1).as_deref(), Some("a.jar"));
        assert_eq!(retired_name("a.jar", 2).as_deref(), Some("a-2.jar"));
        assert_eq!(
            retired_name("a.jar.disabled", 3).as_deref(),
            Some("a-3.jar.disabled")
        );
        assert_eq!(retired_name("a.jar", 99).as_deref(), Some("a-99.jar"));
        assert_eq!(retired_name("a.jar", 100), None);
        assert_eq!(retired_name("a.jar", 0), None);
        assert_eq!(retired_name("a.txt", 2), None);
    }
}

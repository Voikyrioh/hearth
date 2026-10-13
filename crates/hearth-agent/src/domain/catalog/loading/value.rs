//! Textes à alphabet fermé : identifiants, chemins relatifs d'un canal, valeurs d'argument, adresses
//! affichées, version du jeu. Fonctions pures ; chaque refus nomme un caractère, jamais une valeur.

use std::cmp::Ordering;

use serde::Deserialize;

use super::limits::{
    MAX_ARG_VALUE_CHARS, MAX_GAME_VERSION_CHARS, MAX_ORDER_CHARS, MAX_PATH_CHARS,
    MAX_PATH_COMPONENT_CHARS, MAX_TARGET_CHARS, MAX_URL_CHARS,
};

/// Jeu de caractères d'un réglage de type `text`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Charset {
    /// Une ligne de texte : aucun caractère de contrôle ni caractère Unicode qui trompe l'œil.
    Line,
    /// Un nom de fichier sans extension : lettres ASCII, chiffres, tiret, souligné.
    FileStem,
    /// Valeur d'argument de commande : lettres ASCII, chiffres, point, tiret, souligné, deux-points.
    ArgSafe,
}

/// Un caractère qui n'a rien à faire dans un texte affiché ou écrit dans un fichier : contrôle, séparateur
/// de ligne Unicode, invisible, ou renversement du sens d'écriture.
pub fn is_risky_char(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{2028}' | '\u{2029}' | '\u{feff}' | '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}'
                | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{2069}'
        )
}

/// Premier caractère de `text` hors du jeu de caractères, ou rien. Pour `ArgSafe`, un début par `-` ou `+`
/// est aussi refusé (un argument ne doit pas pouvoir devenir un autre drapeau).
pub fn first_refused_char(charset: Charset, text: &str) -> Option<char> {
    match charset {
        Charset::Line => text.chars().find(|c| is_risky_char(*c)),
        Charset::FileStem => text
            .chars()
            .find(|c| !(c.is_ascii_alphanumeric() || *c == '_' || *c == '-')),
        Charset::ArgSafe => {
            if let Some(first) = text.chars().next()
                && (first == '-' || first == '+')
            {
                return Some(first);
            }
            text.chars().find(|c| !is_arg_safe_char(*c))
        }
    }
}

pub fn is_arg_safe_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':')
}

/// Identifiant : minuscules ASCII, chiffres, tiret (et souligné si `underscore`), commence par une lettre ou
/// un chiffre.
pub fn ident_ok(text: &str, min: usize, max: usize, underscore: bool) -> bool {
    let len = text.chars().count();
    if len < min || len > max {
        return false;
    }
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
        return false;
    }
    text.chars().all(|c| {
        c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || (underscore && c == '_')
    })
}

/// Raison pour laquelle un chemin est refusé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathRefused {
    Empty,
    Absolute,
    DotDot,
    EmptyComponent,
    Backslash,
    BadChar(char),
    TooLong,
}

/// Chemin relatif au dossier d'un canal : l'équivalent pur du `RelPath` de l'accès aux fichiers (HRT-93).
/// Ni absolu, ni `..`, ni composant vide ou `.`, ni antislash, caractères `[A-Za-z0-9._-]` et `/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelPath(String);

impl ChannelPath {
    pub fn new(text: &str) -> Result<Self, PathRefused> {
        if text.is_empty() {
            return Err(PathRefused::Empty);
        }
        if text.chars().count() > MAX_PATH_CHARS {
            return Err(PathRefused::TooLong);
        }
        if text.starts_with('/') {
            return Err(PathRefused::Absolute);
        }
        if text.contains('\\') {
            return Err(PathRefused::Backslash);
        }
        for component in text.split('/') {
            if component.is_empty() {
                return Err(PathRefused::EmptyComponent);
            }
            if component == ".." {
                return Err(PathRefused::DotDot);
            }
            if component == "." {
                return Err(PathRefused::EmptyComponent);
            }
            if component.chars().count() > MAX_PATH_COMPONENT_CHARS {
                return Err(PathRefused::TooLong);
            }
            if let Some(bad) = component
                .chars()
                .find(|c| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')))
            {
                return Err(PathRefused::BadChar(bad));
            }
        }
        Ok(Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Cible d'un dossier DANS le conteneur : absolue, sans `..`, sans composant vide, alphabet fermé
/// `[A-Za-z0-9._-]` par composant. Jamais `:`, `,`, `=`, espace ni rien qui ait un sens dans une description
/// de montage : le format est sûr quelle que soit la forme que prendra l'adaptateur. C'est un chemin du
/// service, jamais du serveur.
pub fn container_target_ok(text: &str) -> bool {
    if text.is_empty() || text.chars().count() > MAX_TARGET_CHARS || !text.starts_with('/') {
        return false;
    }
    if text == "/" {
        return false;
    }
    let trimmed = text.strip_suffix('/').unwrap_or(text);
    trimmed[1..].split('/').all(|component| {
        !component.is_empty()
            && component != "."
            && component != ".."
            && component
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    })
}

/// Adresse affichée (jamais contactée) : `https://`, un hôte, rien qui ressemble à des identifiants.
pub fn https_url_ok(text: &str) -> UrlCheck {
    if text.chars().count() > MAX_URL_CHARS {
        return UrlCheck::Invalid;
    }
    let Some(rest) = text.strip_prefix("https://") else {
        return UrlCheck::NotHttps;
    };
    if rest.is_empty() || rest.chars().any(|c| c.is_whitespace() || is_risky_char(c)) {
        return UrlCheck::Invalid;
    }
    if rest
        .chars()
        .any(|c| matches!(c, '"' | '<' | '>' | '\\' | '`'))
    {
        return UrlCheck::Invalid;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty()
        || authority.contains('@')
        || !authority
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'))
    {
        return UrlCheck::Invalid;
    }
    UrlCheck::Ok
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlCheck {
    Ok,
    NotHttps,
    Invalid,
}

/// Un ordre envoyé au jeu par l'entrée standard : ASCII imprimable, 64 caractères au plus, non vide.
pub fn order_refusal(line: &str) -> Option<&'static str> {
    if line.is_empty() {
        return Some("ordre vide");
    }
    if line.chars().count() > MAX_ORDER_CHARS {
        return Some("plus de 64 caractères");
    }
    if !line.chars().all(|c| (' '..='~').contains(&c)) {
        return Some("caractère hors de l'ASCII imprimable");
    }
    if line.trim() != line {
        return Some("espace en début ou en fin d'ordre");
    }
    None
}

/// Nom d'un argument de commande : un tiret ou un plus, une lettre, puis lettres, chiffres, point.
pub fn arg_flag_ok(flag: &str) -> bool {
    let mut chars = flag.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if first != '-' && first != '+' {
        return false;
    }
    let rest: Vec<char> = chars.collect();
    if rest.is_empty() || rest.len() > MAX_ARG_VALUE_CHARS {
        return false;
    }
    let mut it = rest.into_iter();
    // Un second tiret est permis (`--option`), jamais un second plus.
    let mut head = it.next();
    if head == Some('-') && first == '-' {
        head = it.next();
    }
    let Some(letter) = head else {
        return false;
    };
    letter.is_ascii_alphabetic() && it.all(|c| c.is_ascii_alphanumeric() || c == '.')
}

/// Texte littéral cherché dans un journal ou écrit tel quel : une ligne, non vide, bornée.
pub fn literal_ok(text: &str, max: usize) -> bool {
    !text.is_empty() && text.chars().count() <= max && !text.chars().any(is_risky_char)
}

/// Version `dotted_numeric` : un à quatre nombres de six chiffres au plus, séparés par des points.
pub fn parse_dotted(text: &str) -> Option<Vec<u32>> {
    let parts: Vec<&str> = text.split('.').collect();
    if parts.is_empty() || parts.len() > 4 {
        return None;
    }
    parts
        .iter()
        .map(|part| {
            if part.is_empty() || part.len() > 6 || !part.bytes().all(|b| b.is_ascii_digit()) {
                None
            } else {
                part.parse::<u32>().ok()
            }
        })
        .collect()
}

/// Compare deux versions `dotted_numeric` (une partie absente vaut zéro). `None` si l'une est illisible.
pub fn compare_versions(a: &str, b: &str) -> Option<Ordering> {
    let (a, b) = (parse_dotted(a)?, parse_dotted(b)?);
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (
            a.get(i).copied().unwrap_or(0),
            b.get(i).copied().unwrap_or(0),
        );
        match x.cmp(&y) {
            Ordering::Equal => continue,
            other => return Some(other),
        }
    }
    Some(Ordering::Equal)
}

/// La version du jeu lue dans une ligne de journal (BR-FICHE-049) : le texte compris entre le littéral
/// `after` et le premier des caractères de `until_any` (ou la fin de la ligne). 32 caractères au plus,
/// alphabet `[0-9A-Za-z._-]`. Recherche littérale linéaire : aucune expression régulière (I6).
pub fn extract_game_version(line: &str, after: &str, until_any: &[char]) -> Option<String> {
    if after.is_empty() {
        return None;
    }
    let start = line.find(after)? + after.len();
    let rest = &line[start..];
    let end = rest
        .char_indices()
        .find(|(_, c)| until_any.contains(c))
        .map_or(rest.len(), |(i, _)| i);
    let candidate = &rest[..end];
    let chars = candidate.chars().count();
    if chars == 0
        || chars > MAX_GAME_VERSION_CHARS
        || !candidate
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        return None;
    }
    Some(candidate.to_owned())
}

/// Un signe de journal est une sous-chaîne LITTÉRALE (I6) : `.*` ne cherche pas « tout », il cherche ces
/// deux caractères.
pub fn log_line_contains(line: &str, needle: &str) -> bool {
    !needle.is_empty() && line.contains(needle)
}

/// Format de fichier de réglages (énumération fermée, du code de l'agent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileFormat {
    KvLines,
    XmlProperties,
}

/// Ce qu'une valeur ne peut pas contenir dans un fichier d'un format donné : le premier caractère fautif.
/// `kv_lines` : fin de ligne, octet nul, et le guillemet quand les valeurs sont entre guillemets.
/// `xml_properties` : caractères de contrôle sauf la tabulation.
pub fn forbidden_in_file(format: FileFormat, double_quoted: bool, value: &str) -> Option<char> {
    match format {
        // Une seule règle, pour la validation ET pour l'écriture : fin de ligne, octet nul, guillemet (fichier
        // entre guillemets) et espace ou tabulation de bord (perdus à la relecture d'une valeur nue).
        FileFormat::KvLines => value
            .chars()
            .find(|c| matches!(c, '\n' | '\r' | '\0') || (double_quoted && *c == '"'))
            .or_else(|| {
                [value.chars().next(), value.chars().last()]
                    .into_iter()
                    .flatten()
                    .find(|c| matches!(c, ' ' | '\t'))
            }),
        FileFormat::XmlProperties => value.chars().find(|c| c.is_control() && *c != '\t'),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_are_lowercase_ascii_and_bounded() {
        assert!(ident_ok("world_name", 1, 48, true));
        assert!(!ident_ok("World", 1, 48, true));
        assert!(!ident_ok("a_b", 2, 32, false));
        assert!(!ident_ok("-a", 1, 48, true));
        assert!(!ident_ok("", 1, 48, true));
        assert!(!ident_ok(&"a".repeat(49), 1, 48, true));
    }

    #[test]
    fn channel_paths_refuse_every_escape() {
        assert!(ChannelPath::new("serverconfig.txt").is_ok());
        assert!(ChannelPath::new("a/b.cfg").is_ok());
        assert_eq!(ChannelPath::new(""), Err(PathRefused::Empty));
        assert_eq!(ChannelPath::new("/etc/passwd"), Err(PathRefused::Absolute));
        assert_eq!(ChannelPath::new("../x"), Err(PathRefused::DotDot));
        assert_eq!(ChannelPath::new("a/../x"), Err(PathRefused::DotDot));
        assert_eq!(ChannelPath::new("a//b"), Err(PathRefused::EmptyComponent));
        assert_eq!(ChannelPath::new("a/"), Err(PathRefused::EmptyComponent));
        assert_eq!(ChannelPath::new("./a"), Err(PathRefused::EmptyComponent));
        assert_eq!(ChannelPath::new("a\\b"), Err(PathRefused::Backslash));
        assert_eq!(ChannelPath::new("a b"), Err(PathRefused::BadChar(' ')));
        assert_eq!(ChannelPath::new("a\0b"), Err(PathRefused::BadChar('\0')));
        assert_eq!(
            ChannelPath::new(&"a".repeat(129)),
            Err(PathRefused::TooLong)
        );
    }

    #[test]
    fn container_targets_are_absolute_clean_paths() {
        assert!(container_target_ok("/config"));
        assert!(container_target_ok("/root/.local/share/Terraria/Worlds"));
        assert!(!container_target_ok("/"));
        assert!(!container_target_ok("config"));
        assert!(!container_target_ok("/a/../b"));
        assert!(!container_target_ok("/a//b"));
        assert!(!container_target_ok("/a\nb"));
        for bad in ["/d:rw", "/d,x=y", "/a b", "/a=b", "/a$b", "/a\"b", "/é"] {
            assert!(!container_target_ok(bad), "{bad:?}");
        }
    }

    #[test]
    fn the_argument_alphabet_has_no_character_an_interpreter_reads() {
        for bad in [
            ' ', '\t', ';', '$', '|', '&', '<', '>', '(', ')', '\\', '*', '?', '"', '\'', '`', '{',
            '}', '[', ']', '!', '#', '~', '=', ',', '/', '%', '\n',
        ] {
            assert_eq!(
                first_refused_char(Charset::ArgSafe, &format!("ab{bad}c")),
                Some(bad),
                "caractère {bad:?}"
            );
        }
        assert_eq!(first_refused_char(Charset::ArgSafe, "Monde-1_2.3:4"), None);
        assert_eq!(first_refused_char(Charset::ArgSafe, "-flag"), Some('-'));
        assert_eq!(first_refused_char(Charset::ArgSafe, "+flag"), Some('+'));
        assert_eq!(first_refused_char(Charset::ArgSafe, "é"), Some('é'));
    }

    #[test]
    fn line_text_refuses_controls_and_invisible_characters() {
        assert_eq!(
            first_refused_char(Charset::Line, "Bonjour tout le monde é"),
            None
        );
        assert_eq!(first_refused_char(Charset::Line, "a\nb"), Some('\n'));
        assert_eq!(
            first_refused_char(Charset::Line, "a\u{202e}b"),
            Some('\u{202e}')
        );
        assert_eq!(
            first_refused_char(Charset::Line, "a\u{2028}b"),
            Some('\u{2028}')
        );
        assert_eq!(
            first_refused_char(Charset::FileStem, "Mon monde"),
            Some(' ')
        );
        assert_eq!(first_refused_char(Charset::FileStem, "Monde_1-a"), None);
    }

    #[test]
    fn addresses_are_https_only_with_a_plain_host() {
        assert_eq!(https_url_ok("https://example.org/eula"), UrlCheck::Ok);
        assert_eq!(https_url_ok("https://example.org:8443/x?y=1"), UrlCheck::Ok);
        assert_eq!(https_url_ok("http://example.org"), UrlCheck::NotHttps);
        assert_eq!(https_url_ok("ftp://example.org"), UrlCheck::NotHttps);
        assert_eq!(https_url_ok("https://"), UrlCheck::Invalid);
        assert_eq!(https_url_ok("https://u:p@example.org"), UrlCheck::Invalid);
        assert_eq!(https_url_ok("https://exa mple.org"), UrlCheck::Invalid);
        assert_eq!(https_url_ok("https://example.org/\"x"), UrlCheck::Invalid);
    }

    #[test]
    fn orders_are_short_printable_ascii() {
        assert_eq!(order_refusal("save"), None);
        assert!(order_refusal("").is_some());
        assert!(order_refusal(&"a".repeat(65)).is_some());
        assert!(order_refusal("sauvé").is_some());
        assert!(order_refusal("save\nexit").is_some());
        assert!(order_refusal(" save").is_some());
        assert_eq!(order_refusal(&"a".repeat(64)), None);
    }

    #[test]
    fn argument_names_are_literal_flags() {
        for good in ["-nom", "-disableannouncementbox", "--opt", "+server.seed"] {
            assert!(arg_flag_ok(good), "{good}");
        }
        for bad in [
            "nom", "-", "--", "-a b", "-a;b", "-$x", "- x", "-1a", "", "-a/b",
        ] {
            assert!(!arg_flag_ok(bad), "{bad}");
        }
    }

    #[test]
    fn versions_compare_numerically_not_textually() {
        assert_eq!(compare_versions("3.0", "2.6"), Some(Ordering::Greater));
        assert_eq!(compare_versions("10.0", "9.9"), Some(Ordering::Greater));
        assert_eq!(compare_versions("3", "3.0.0"), Some(Ordering::Equal));
        assert_eq!(compare_versions("3.x", "3.0"), None);
        assert_eq!(parse_dotted("1.2.3.4.5"), None);
    }

    #[test]
    fn the_game_version_is_the_text_between_two_literals() {
        assert_eq!(
            extract_game_version("INF Version: 3.1.4 Compatibility", "Version: ", &[' ']),
            Some("3.1.4".into())
        );
        assert_eq!(extract_game_version("v=1.2", "v=", &[]), Some("1.2".into()));
        // Texte de plus de 32 caractères, alphabet hors liste, vide, repère absent.
        assert_eq!(
            extract_game_version(&format!("v={}", "1".repeat(33)), "v=", &[]),
            None
        );
        assert_eq!(extract_game_version("v=1;rm -rf", "v=", &[]), None);
        assert_eq!(extract_game_version("v= 1", "v=", &[' ']), None);
        assert_eq!(extract_game_version("nothing", "v=", &[]), None);
        assert_eq!(extract_game_version("v=1", "", &[]), None);
    }

    #[test]
    fn a_log_sign_is_searched_literally_and_in_linear_time() {
        // `.*` et `[a-z]+` sont des caractères, pas des motifs.
        assert!(!log_line_contains("Server started", ".*"));
        assert!(log_line_contains("pattern .* here", ".*"));
        assert!(!log_line_contains("abc", "[a-z]+"));
        assert!(log_line_contains("x[a-z]+y", "[a-z]+"));
        assert!(!log_line_contains("abc", ""));
        // Ligne de 64 Kio : un seul passage, pas d'explosion.
        let line = "a".repeat(64 * 1024);
        let needle = format!("{}b", "a".repeat(1000));
        let start = std::time::Instant::now();
        assert!(!log_line_contains(&line, &needle));
        assert!(start.elapsed() < std::time::Duration::from_secs(2));
    }

    #[test]
    fn file_values_cannot_carry_a_second_line() {
        assert_eq!(
            forbidden_in_file(FileFormat::KvLines, false, "a\nb=c"),
            Some('\n')
        );
        assert_eq!(
            forbidden_in_file(FileFormat::KvLines, false, "a\rb"),
            Some('\r')
        );
        assert_eq!(
            forbidden_in_file(FileFormat::KvLines, true, "a\"b"),
            Some('"')
        );
        assert_eq!(
            forbidden_in_file(FileFormat::KvLines, false, "a\"b=c"),
            None
        );
        assert_eq!(
            forbidden_in_file(FileFormat::XmlProperties, false, "a\u{1}b"),
            Some('\u{1}')
        );
        assert_eq!(
            forbidden_in_file(FileFormat::XmlProperties, false, "a\tb<&>"),
            None
        );
    }
}

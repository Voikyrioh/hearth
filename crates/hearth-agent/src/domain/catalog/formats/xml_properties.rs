//! `xml_properties` : lignes `<property name="…" value="…" />` sous une racine (7 Days to Die).
//!
//! Aucun arbre XML, aucune bibliothèque : un balayage qui ne reconnaît que quatre choses (commentaires,
//! instructions `<?…?>`, balises `property` auto-fermantes sous la racine déclarée, balise fermante de la
//! racine) et laisse tout le reste en octets bruts. Ni entité externe, ni développement d'entité, ni encodage
//! exotique ne peuvent donc exister : `<!DOCTYPE`, `<!ENTITY`, un encodage autre qu'UTF-8 et toute référence
//! d'entité hors des cinq prédéfinies et des formes numériques refusent le fichier entier.

use super::{
    ConfigFormat, Doc, EntryInfo, MAX_FILE_BYTES, MAX_LINE_BYTES, MAX_LINES, MAX_VALUE_BYTES,
    Piece, Refused, Unreadable, UnreadableReason, check_input,
};

pub struct XmlProperties {
    root: String,
    element: String,
    key_attr: String,
    value_attr: String,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum RootState {
    Before,
    Inside,
    After,
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn is_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b':' | b'-' | b'.')
}

/// Décode les cinq entités prédéfinies et les références numériques. Toute autre référence est refusée.
fn decode(raw: &str) -> Option<String> {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        let semi = after.find(';')?;
        if semi > 10 {
            return None;
        }
        let name = &after[..semi];
        let decoded = match name {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            _ => {
                let digits = name.strip_prefix('#')?;
                let code = match digits.strip_prefix('x') {
                    Some(hex) if !hex.is_empty() && hex.bytes().all(|b| b.is_ascii_hexdigit()) => {
                        u32::from_str_radix(hex, 16).ok()?
                    }
                    Some(_) => return None,
                    None if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) => {
                        digits.parse::<u32>().ok()?
                    }
                    None => return None,
                };
                char::from_u32(code)?
            }
        };
        if decoded.is_control() && decoded != '\t' {
            return None;
        }
        out.push(decoded);
        rest = &after[semi + 1..];
    }
    out.push_str(rest);
    Some(out)
}

fn encode(value: &str, quote: u8) -> String {
    let mut out = String::with_capacity(value.len() + 8);
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if quote == b'"' => out.push_str("&quot;"),
            '\'' if quote == b'\'' => out.push_str("&apos;"),
            '\t' => out.push_str("&#9;"),
            other => out.push(other),
        }
    }
    out
}

impl XmlProperties {
    pub fn new(root: &str, element: &str, key_attr: &str, value_attr: &str) -> Self {
        Self {
            root: root.to_owned(),
            element: element.to_owned(),
            key_attr: key_attr.to_owned(),
            value_attr: value_attr.to_owned(),
        }
    }

    fn check_declaration(decl: &str) -> Result<(), Unreadable> {
        if !decl
            .get(..5)
            .is_some_and(|p| p.eq_ignore_ascii_case("<?xml"))
        {
            return Ok(());
        }
        let Some(at) = decl.find("encoding") else {
            return Ok(());
        };
        let rest = decl[at + "encoding".len()..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else {
            return Err(Unreadable::new(UnreadableReason::XmlEncoding, None));
        };
        let rest = rest.trim_start();
        let mut chars = rest.chars();
        let quote = chars.next().filter(|c| *c == '"' || *c == '\'');
        let Some(quote) = quote else {
            return Err(Unreadable::new(UnreadableReason::XmlEncoding, None));
        };
        let name: String = chars.take_while(|c| *c != quote).collect();
        if name.eq_ignore_ascii_case("utf-8") {
            Ok(())
        } else {
            Err(Unreadable::new(UnreadableReason::XmlEncoding, None))
        }
    }

    /// Fin (exclue de `>` comprise) d'une balise ouvrante, en respectant les guillemets des attributs.
    fn tag_end(text: &str, lt: usize) -> Option<usize> {
        let bytes = text.as_bytes();
        let mut quote = 0u8;
        let mut i = lt + 1;
        while i < bytes.len() {
            let b = bytes[i];
            if quote != 0 {
                if b == quote {
                    quote = 0;
                }
            } else if b == b'"' || b == b'\'' {
                quote = b;
            } else if b == b'>' {
                return Some(i);
            }
            i += 1;
        }
        None
    }

    fn parse_property(&self, tag: &str, line: usize) -> Result<EntryInfo, Unreadable> {
        let attr_err = || Unreadable::new(UnreadableReason::XmlAttribute, Some(line));
        let b = tag.as_bytes();
        let end = b.len() - 2; // avant « /> »
        let mut p = 1 + self.element.len();
        let mut attrs: Vec<(&str, usize, usize, u8)> = Vec::new();
        loop {
            let ws_start = p;
            while p < end && is_ws(b[p]) {
                p += 1;
            }
            if p >= end {
                break;
            }
            if p == ws_start {
                return Err(attr_err());
            }
            let name_start = p;
            while p < end && is_name_byte(b[p]) {
                p += 1;
            }
            if p == name_start {
                return Err(attr_err());
            }
            let name = &tag[name_start..p];
            while p < end && is_ws(b[p]) {
                p += 1;
            }
            if p >= end || b[p] != b'=' {
                return Err(attr_err());
            }
            p += 1;
            while p < end && is_ws(b[p]) {
                p += 1;
            }
            if p >= end || (b[p] != b'"' && b[p] != b'\'') {
                return Err(attr_err());
            }
            let quote = b[p];
            p += 1;
            let value_start = p;
            while p < end && b[p] != quote {
                if b[p] == b'<' {
                    return Err(attr_err());
                }
                p += 1;
            }
            if p >= end {
                return Err(attr_err());
            }
            attrs.push((name, value_start, p, quote));
            p += 1;
        }
        let mut seen = std::collections::BTreeSet::new();
        for (name, ..) in &attrs {
            if !seen.insert(*name) {
                return Err(attr_err());
            }
        }
        let find = |wanted: &str| attrs.iter().find(|a| a.0 == wanted).copied();
        let (Some(key_attr), Some(value_attr)) = (find(&self.key_attr), find(&self.value_attr))
        else {
            return Err(attr_err());
        };
        let raw_key = &tag[key_attr.1..key_attr.2];
        let raw_value = &tag[value_attr.1..value_attr.2];
        for raw in [raw_key, raw_value] {
            if raw
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\t' | '\n' | '\r'))
            {
                return Err(attr_err());
            }
        }
        let entity_err = || Unreadable::new(UnreadableReason::XmlEntity, Some(line));
        let key = decode(raw_key).ok_or_else(entity_err)?;
        let value = decode(raw_value).ok_or_else(entity_err)?;
        Ok(EntryInfo {
            key,
            value,
            start: value_attr.1,
            end: value_attr.2,
            quote: value_attr.3,
        })
    }

    fn clean_key(key: &str) -> bool {
        !key.is_empty() && key.chars().count() <= 256 && !key.chars().any(char::is_control)
    }

    fn check_value(value: &str) -> Result<(), Refused> {
        if value.len() > MAX_VALUE_BYTES {
            return Err(Refused::TooLong);
        }
        match value.chars().find(|c| c.is_control() && *c != '\t') {
            Some(bad) => Err(Refused::Value(bad)),
            None => Ok(()),
        }
    }

    fn tag_for(&self, key: &str, value: &str) -> (String, usize, usize) {
        let head = format!(
            "<{} {}=\"{}\" {}=\"",
            self.element,
            self.key_attr,
            encode(key, b'"'),
            self.value_attr
        );
        let encoded = encode(value, b'"');
        let start = head.len();
        let end = start + encoded.len();
        (format!("{head}{encoded}\" />"), start, end)
    }

    fn dominant_eol(doc: &Doc) -> &'static str {
        let (mut crlf, mut lf) = (0usize, 0usize);
        for piece in &doc.pieces {
            let bytes = &piece.bytes;
            for (i, b) in bytes.iter().enumerate() {
                if *b == b'\n' {
                    if i > 0 && bytes[i - 1] == b'\r' {
                        crlf += 1;
                    } else {
                        lf += 1;
                    }
                }
            }
        }
        if crlf > lf { "\r\n" } else { "\n" }
    }

    /// Octets d'indentation de la ligne d'un morceau « autre » : ce qui suit son dernier saut de ligne,
    /// à condition que ce ne soit que des espaces et des tabulations.
    fn trailing_indent(bytes: &[u8]) -> Option<&[u8]> {
        let nl = bytes.iter().rposition(|b| *b == b'\n')?;
        let tail = &bytes[nl + 1..];
        tail.iter()
            .all(|b| *b == b' ' || *b == b'\t')
            .then_some(tail)
    }
}

impl ConfigFormat for XmlProperties {
    fn parse(&self, bytes: &[u8]) -> Result<Doc, Unreadable> {
        check_input(bytes)?;
        let text = std::str::from_utf8(bytes)
            .map_err(|_| Unreadable::new(UnreadableReason::InvalidUtf8, None))?;
        let mut lines = 0usize;
        for line in text.split('\n') {
            lines += 1;
            if line.len() > MAX_LINE_BYTES {
                return Err(Unreadable::new(UnreadableReason::LineTooLong, Some(lines)));
            }
            if lines > MAX_LINES {
                return Err(Unreadable::new(UnreadableReason::TooManyLines, Some(lines)));
            }
        }
        let lowered = text.to_ascii_lowercase();
        if lowered.contains("<!doctype") || lowered.contains("<!entity") {
            return Err(Unreadable::new(UnreadableReason::XmlDoctypeOrEntity, None));
        }
        drop(lowered);

        // Numéro de ligne d'une position, compté depuis la dernière position demandée (le balayage avance
        // toujours) : le total reste linéaire dans la taille du fichier, pas quadratique.
        let cursor = std::cell::Cell::new((0usize, 1usize));
        let line_of = |pos: usize| {
            let (from, line) = cursor.get();
            let (from, line) = if pos >= from { (from, line) } else { (0, 1) };
            let line = line + text[from..pos].matches('\n').count();
            cursor.set((pos, line));
            line
        };
        let unterminated =
            |pos: usize| Unreadable::new(UnreadableReason::XmlUnterminated, Some(line_of(pos)));

        let mut pieces: Vec<Piece> = Vec::new();
        let mut other_start = 0usize;
        let mut i = 0usize;
        let mut state = RootState::Before;
        let mut closing_index = None;
        let mut entries = 0usize;
        while let Some(rel) = text[i..].find('<') {
            let lt = i + rel;
            let rest = &text[lt..];
            if let Some(body) = rest.strip_prefix("<!--") {
                let Some(end) = body.find("-->") else {
                    return Err(unterminated(lt));
                };
                i = lt + 4 + end + 3;
                continue;
            }
            if rest.starts_with("<![CDATA[") {
                let Some(end) = rest.find("]]>") else {
                    return Err(unterminated(lt));
                };
                i = lt + end + 3;
                continue;
            }
            if rest.starts_with("<?") {
                let Some(end) = rest.find("?>") else {
                    return Err(unterminated(lt));
                };
                Self::check_declaration(&rest[..end + 2])?;
                i = lt + end + 2;
                continue;
            }
            if rest.starts_with("</") {
                let Some(end) = rest.find('>') else {
                    return Err(unterminated(lt));
                };
                if rest[2..end].trim() == self.root {
                    match state {
                        RootState::Inside => {
                            if lt > other_start {
                                pieces.push(Piece::other(bytes[other_start..lt].to_vec()));
                            }
                            other_start = lt;
                            closing_index = Some(pieces.len());
                            state = RootState::After;
                        }
                        RootState::After => {
                            return Err(Unreadable::new(
                                UnreadableReason::XmlRootClosedTwice,
                                Some(line_of(lt)),
                            ));
                        }
                        RootState::Before => {
                            return Err(Unreadable::new(
                                UnreadableReason::XmlRootMissing,
                                Some(line_of(lt)),
                            ));
                        }
                    }
                }
                i = lt + end + 1;
                continue;
            }
            let Some(tag_end) = Self::tag_end(text, lt) else {
                return Err(unterminated(lt));
            };
            let tag = &text[lt..=tag_end];
            let name_len = tag[1..]
                .find(|c: char| c.is_ascii_whitespace() || c == '/' || c == '>')
                .unwrap_or(tag.len() - 1);
            let name = &tag[1..1 + name_len];
            let self_closing = tag.ends_with("/>");
            if name == self.element {
                if state != RootState::Inside {
                    return Err(Unreadable::new(
                        UnreadableReason::XmlPropertyOutsideRoot,
                        Some(line_of(lt)),
                    ));
                }
                if !self_closing || tag.len() < 2 + 1 + self.element.len() {
                    return Err(Unreadable::new(
                        UnreadableReason::XmlPropertyShape,
                        Some(line_of(lt)),
                    ));
                }
                let entry = self.parse_property(tag, line_of(lt))?;
                if lt > other_start {
                    pieces.push(Piece::other(bytes[other_start..lt].to_vec()));
                }
                pieces.push(Piece {
                    bytes: tag.as_bytes().to_vec(),
                    entry: Some(entry),
                    closing: false,
                });
                other_start = tag_end + 1;
                entries += 1;
                if entries > MAX_LINES {
                    return Err(Unreadable::new(
                        UnreadableReason::TooManyLines,
                        Some(line_of(lt)),
                    ));
                }
            } else if name == self.root && state == RootState::Before {
                if self_closing {
                    return Err(Unreadable::new(
                        UnreadableReason::XmlRootNotClosed,
                        Some(line_of(lt)),
                    ));
                }
                state = RootState::Inside;
            }
            i = tag_end + 1;
        }
        match state {
            RootState::Before => {
                return Err(Unreadable::new(UnreadableReason::XmlRootMissing, None));
            }
            RootState::Inside => {
                return Err(Unreadable::new(UnreadableReason::XmlRootNotClosed, None));
            }
            RootState::After => {}
        }
        let mut last = Piece::other(bytes[other_start..].to_vec());
        last.closing = true;
        debug_assert_eq!(closing_index, Some(pieces.len()));
        pieces.push(last);
        Ok(Doc { pieces })
    }

    fn set(&self, doc: &mut Doc, key: &str, value: &str) -> Result<(), Refused> {
        if !Self::clean_key(key) {
            return Err(Refused::Key);
        }
        Self::check_value(value)?;
        let existing = doc
            .pieces
            .iter()
            .rposition(|p| p.entry.as_ref().is_some_and(|e| e.key == key));
        let size_before = doc.byte_len();
        if let Some(index) = existing {
            let piece = &mut doc.pieces[index];
            let Some(entry) = piece.entry.as_mut() else {
                return Err(Refused::Key);
            };
            let encoded = encode(value, entry.quote);
            if size_before - (entry.end - entry.start) + encoded.len() > MAX_FILE_BYTES {
                return Err(Refused::TooLong);
            }
            piece.bytes.splice(entry.start..entry.end, encoded.bytes());
            entry.end = entry.start + encoded.len();
            entry.value = value.to_owned();
            return Ok(());
        }
        let Some(closing) = doc.pieces.iter().position(|p| p.closing) else {
            return Err(Refused::Key);
        };
        let eol = Self::dominant_eol(doc);
        let indent: Vec<u8> = doc
            .pieces
            .iter()
            .enumerate()
            .rev()
            .find(|(_, p)| p.entry.is_some())
            .and_then(|(i, _)| i.checked_sub(1))
            .and_then(|prev| Self::trailing_indent(&doc.pieces[prev].bytes))
            .map_or_else(|| b"  ".to_vec(), <[u8]>::to_vec);
        let ends_with_newline = closing
            .checked_sub(1)
            .and_then(|prev| doc.pieces[prev].bytes.last())
            .is_none_or(|b| *b == b'\n');
        let mut before = Vec::new();
        if !ends_with_newline {
            before.extend_from_slice(eol.as_bytes());
        }
        before.extend_from_slice(&indent);
        let (tag, start, end) = self.tag_for(key, value);
        let entry_count = doc.pieces.iter().filter(|p| p.entry.is_some()).count();
        if entry_count >= MAX_LINES
            || size_before + tag.len() + before.len() + eol.len() > MAX_FILE_BYTES
        {
            return Err(Refused::TooLong);
        }
        let inserted = [
            Piece::other(before),
            Piece {
                bytes: tag.into_bytes(),
                entry: Some(EntryInfo {
                    key: key.to_owned(),
                    value: value.to_owned(),
                    start,
                    end,
                    quote: b'"',
                }),
                closing: false,
            },
            Piece::other(eol.as_bytes().to_vec()),
        ];
        let mut at = closing;
        for piece in inserted {
            if piece.bytes.is_empty() {
                continue;
            }
            doc.pieces.insert(at, piece);
            at += 1;
        }
        Ok(())
    }

    fn unset(&self, doc: &mut Doc, key: &str) {
        let matching: Vec<usize> = doc
            .pieces
            .iter()
            .enumerate()
            .filter(|(_, p)| p.entry.as_ref().is_some_and(|e| e.key == key))
            .map(|(i, _)| i)
            .collect();
        for index in matching.into_iter().rev() {
            let indent_len = index
                .checked_sub(1)
                .filter(|prev| doc.pieces[*prev].entry.is_none())
                .and_then(|prev| Self::trailing_indent(&doc.pieces[prev].bytes).map(<[u8]>::len));
            let eol_len = doc
                .pieces
                .get(index + 1)
                .filter(|n| n.entry.is_none() && !n.closing)
                .and_then(|n| {
                    if n.bytes.starts_with(b"\r\n") {
                        Some(2)
                    } else if n.bytes.starts_with(b"\n") {
                        Some(1)
                    } else {
                        None
                    }
                });
            if let (Some(indent_len), Some(eol_len)) = (indent_len, eol_len) {
                let prev = &mut doc.pieces[index - 1];
                let keep = prev.bytes.len() - indent_len;
                prev.bytes.truncate(keep);
                doc.pieces[index + 1].bytes.drain(..eol_len);
            }
            doc.pieces.remove(index);
        }
    }

    fn create(&self, pairs: &[(String, String)]) -> Result<Vec<u8>, Refused> {
        let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        out.push_str(&format!("<{}>\n", self.root));
        for (key, value) in pairs {
            if !Self::clean_key(key) {
                return Err(Refused::Key);
            }
            Self::check_value(value)?;
            out.push_str("  ");
            out.push_str(&self.tag_for(key, value).0);
            out.push('\n');
        }
        out.push_str(&format!("</{}>\n", self.root));
        Ok(out.into_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn format() -> XmlProperties {
        XmlProperties::new("ServerSettings", "property", "name", "value")
    }

    const SAMPLE: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<!-- commentaire -->\r\n<ServerSettings>\r\n\t<property name=\"ServerName\" value=\"Mon serveur\"/>\r\n\t<!-- <property name=\"Hidden\" value=\"1\"/> -->\r\n\t<property name=\"Port\" value=\"26900\" />\r\n\t<other attr=\"x\">texte</other>\r\n</ServerSettings>\r\n";

    #[test]
    fn a_file_passes_unchanged_and_a_commented_property_is_inactive() {
        let format = format();
        let doc = format.parse(SAMPLE.as_bytes()).expect("lisible");
        assert_eq!(doc.render(), SAMPLE.as_bytes());
        assert_eq!(doc.get("ServerName"), Some("Mon serveur"));
        assert_eq!(doc.get("Port"), Some("26900"));
        assert_eq!(doc.get("Hidden"), None);
        assert_eq!(doc.entries().len(), 2);
    }

    #[test]
    fn a_duplicate_key_reads_the_last_and_a_key_is_case_sensitive() {
        let format = format();
        let text = "<ServerSettings>
<property name=\"Port\" value=\"1\"/>
<property name=\"Port\" value=\"2\"/>
<property name=\"PORT\" value=\"3\"/>
</ServerSettings>
";
        let mut doc = format.parse(text.as_bytes()).expect("lisible");
        assert_eq!(doc.get("Port"), Some("2"), "la dernière l'emporte");
        assert_eq!(
            doc.get("PORT"),
            Some("3"),
            "la casse compte : clé distincte"
        );
        assert_eq!(doc.get("port"), None);
        assert_eq!(doc.duplicate_keys(), 1);
        format.set(&mut doc, "Port", "9").expect("ok");
        assert_eq!(doc.get("Port"), Some("9"));
        assert_eq!(doc.get("PORT"), Some("3"));
        format.unset(&mut doc, "Port");
        assert_eq!(doc.get("Port"), None, "unset les retire toutes");
        assert_eq!(doc.get("PORT"), Some("3"));
    }

    #[test]
    fn nineteen_thousand_properties_are_read_in_linear_time() {
        let format = format();
        let mut text = String::from(
            "<ServerSettings>
",
        );
        for i in 0..19_000 {
            text.push_str(&format!(
                "<property name=\"k{i}\" value=\"v\"/>
"
            ));
        }
        text.push_str(
            "</ServerSettings>
",
        );
        let started = std::time::Instant::now();
        let doc = format.parse(text.as_bytes()).expect("lisible");
        assert_eq!(doc.entries().len(), 19_000);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "lecture quadratique ?"
        );
    }

    #[test]
    fn setting_a_value_changes_only_its_bytes() {
        let format = format();
        let mut doc = format.parse(SAMPLE.as_bytes()).expect("lisible");
        format.set(&mut doc, "Port", "26910").expect("ok");
        assert_eq!(doc.render(), SAMPLE.replace("26900", "26910").as_bytes());
    }

    #[test]
    fn a_new_property_goes_before_the_closing_root_with_the_indentation_and_line_ending() {
        let format = format();
        let mut doc = format.parse(SAMPLE.as_bytes()).expect("lisible");
        format.set(&mut doc, "Added", "1").expect("ok");
        let expected = SAMPLE.replace(
            "</ServerSettings>",
            "\t<property name=\"Added\" value=\"1\" />\r\n</ServerSettings>",
        );
        assert_eq!(String::from_utf8(doc.render()).expect("utf8"), expected);
        assert_eq!(doc.get("Added"), Some("1"));
    }

    #[test]
    fn special_characters_come_back_identical_or_are_refused() {
        let format = format();
        for value in [
            "a&b",
            "a<b",
            "a>b",
            "a\"b",
            "a'b",
            "]]>",
            "-->",
            "a=b",
            "&amp;",
            "&#60;",
            "tab\there",
            "é€",
        ] {
            let mut doc = format.parse(SAMPLE.as_bytes()).expect("lisible");
            let before = doc.entries().len();
            format.set(&mut doc, "Port", value).expect("permise");
            let reparsed = format.parse(&doc.render()).expect("relisible");
            assert_eq!(reparsed.get("Port"), Some(value), "{value:?}");
            assert_eq!(
                reparsed.entries().len(),
                before,
                "pas de second élément pour {value:?}"
            );
            assert_eq!(reparsed.get("ServerName"), Some("Mon serveur"));
        }
        let mut doc = format.parse(SAMPLE.as_bytes()).expect("lisible");
        for bad in ["a\nb", "a\rb", "a\u{1}b", "a\0b"] {
            assert!(
                matches!(format.set(&mut doc, "Port", bad), Err(Refused::Value(_))),
                "{bad:?}"
            );
        }
        assert_eq!(doc.render(), SAMPLE.as_bytes(), "rien n'a été écrit");
    }

    #[test]
    fn a_value_with_markup_cannot_close_the_attribute_or_the_element() {
        let format = format();
        let mut doc = format.parse(SAMPLE.as_bytes()).expect("lisible");
        let hostile = "\" /><property name=\"Evil\" value=\"1";
        format.set(&mut doc, "Port", hostile).expect("encodée");
        let reparsed = format.parse(&doc.render()).expect("relisible");
        assert_eq!(reparsed.get("Evil"), None, "aucune seconde propriété");
        assert_eq!(reparsed.get("Port"), Some(hostile));
    }

    #[test]
    fn entities_and_doctype_and_encodings_are_refused_whole() {
        let format = format();
        let wrap = |body: &str| {
            format!("<?xml version=\"1.0\"?>\n<ServerSettings>\n{body}\n</ServerSettings>\n")
        };
        for (name, text, reason) in [
            ("doctype", format!("<!DOCTYPE x [<!ENTITY a \"b\">]>\n{}", wrap("")), UnreadableReason::XmlDoctypeOrEntity),
            ("entity", wrap("<!ENTITY a \"b\">"), UnreadableReason::XmlDoctypeOrEntity),
            ("entité inconnue", wrap("<property name=\"a\" value=\"&xxe;\"/>"), UnreadableReason::XmlEntity),
            ("esperluette nue", wrap("<property name=\"a\" value=\"a & b\"/>"), UnreadableReason::XmlEntity),
            ("numérique invalide", wrap("<property name=\"a\" value=\"&#0;\"/>"), UnreadableReason::XmlEntity),
            ("encodage", "<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?>\n<ServerSettings>\n</ServerSettings>\n".into(), UnreadableReason::XmlEncoding),
            ("attribut en double", wrap("<property name=\"a\" name=\"b\" value=\"1\"/>"), UnreadableReason::XmlAttribute),
            ("attribut absent", wrap("<property name=\"a\"/>"), UnreadableReason::XmlAttribute),
            ("sans guillemets", wrap("<property name=a value=\"1\"/>"), UnreadableReason::XmlAttribute),
            ("pas auto-fermante", wrap("<property name=\"a\" value=\"1\"></property>"), UnreadableReason::XmlPropertyShape),
            ("hors racine", "<property name=\"a\" value=\"1\"/>\n<ServerSettings>\n</ServerSettings>\n".into(), UnreadableReason::XmlPropertyOutsideRoot),
            ("racine non refermée", "<ServerSettings>\n<property name=\"a\" value=\"1\"/>\n".into(), UnreadableReason::XmlRootNotClosed),
            ("racine absente", "<Other>\n</Other>\n".into(), UnreadableReason::XmlRootMissing),
            ("commentaire ouvert", "<ServerSettings>\n<!-- jamais fermé\n</ServerSettings>\n".into(), UnreadableReason::XmlUnterminated),
            ("balise ouverte", "<ServerSettings>\n<property name=\"a\" value=\"1\n</ServerSettings>\n".into(), UnreadableReason::XmlUnterminated),
            ("racine fermée deux fois", "<ServerSettings>\n</ServerSettings>\n</ServerSettings>\n".into(), UnreadableReason::XmlRootClosedTwice),
        ] {
            assert_eq!(format.parse(text.as_bytes()).err().map(|e| e.reason), Some(reason), "{name}");
        }
        assert_eq!(
            format
                .parse(b"<ServerSettings>\xff</ServerSettings>")
                .err()
                .map(|e| e.reason),
            Some(UnreadableReason::InvalidUtf8)
        );
        assert_eq!(
            format
                .parse(b"<ServerSettings>\0</ServerSettings>")
                .err()
                .map(|e| e.reason),
            Some(UnreadableReason::NulByte)
        );
    }

    #[test]
    fn numeric_references_and_both_quote_styles_are_read() {
        let format = format();
        let doc = format
            .parse(b"<ServerSettings>\n<property name='a' value='x&#65;&#x42;&quot;'/>\n</ServerSettings>\n")
            .expect("lisible");
        assert_eq!(doc.get("a"), Some("xAB\""));
        let mut doc = doc;
        format.set(&mut doc, "a", "it's").expect("ok");
        let out = String::from_utf8(doc.render()).expect("utf8");
        assert!(out.contains("value='it&apos;s'"), "{out}");
    }

    #[test]
    fn unsetting_removes_the_whole_line_and_nothing_else() {
        let format = format();
        let mut doc = format.parse(SAMPLE.as_bytes()).expect("lisible");
        format.unset(&mut doc, "Port");
        assert_eq!(
            String::from_utf8(doc.render()).expect("utf8"),
            SAMPLE.replace("\t<property name=\"Port\" value=\"26900\" />\r\n", "")
        );
        format.unset(&mut doc, "absent");
        assert_eq!(doc.entries().len(), 1);
    }

    #[test]
    fn a_file_without_entries_gets_a_property_with_the_default_indentation() {
        let format = format();
        let mut doc = format
            .parse(b"<ServerSettings>\n</ServerSettings>\n")
            .expect("lisible");
        format.set(&mut doc, "a", "1").expect("ok");
        assert_eq!(
            String::from_utf8(doc.render()).expect("utf8"),
            "<ServerSettings>\n  <property name=\"a\" value=\"1\" />\n</ServerSettings>\n"
        );
    }

    #[test]
    fn a_created_file_is_readable_back() {
        let format = format();
        let bytes = format
            .create(&[("A".into(), "1".into()), ("B".into(), "x&y".into())])
            .expect("ok");
        let doc = format.parse(&bytes).expect("relisible");
        assert_eq!(doc.get("A"), Some("1"));
        assert_eq!(doc.get("B"), Some("x&y"));
        assert!(format.create(&[("a\nb".into(), "1".into())]).is_err());
    }

    #[test]
    fn a_key_with_markup_is_encoded_not_interpreted() {
        let format = format();
        let mut doc = format
            .parse(b"<ServerSettings>\n</ServerSettings>\n")
            .expect("lisible");
        format.set(&mut doc, "a\"/><x", "1").expect("encodée");
        let reparsed = format.parse(&doc.render()).expect("relisible");
        assert_eq!(reparsed.get("a\"/><x"), Some("1"));
        assert_eq!(reparsed.entries().len(), 1);
    }
}

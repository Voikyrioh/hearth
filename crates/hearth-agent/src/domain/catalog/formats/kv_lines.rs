//! `kv_lines` : lignes `clé=valeur` (Terraria, réglages de l'outil d'administration de 7 Days to Die).
//!
//! Un morceau par ligne, avec sa propre fin de ligne (`\n` ou `\r\n`, ou aucune pour la dernière), plus un
//! éventuel BOM UTF-8 en premier morceau. Une ligne vide, commentée (`#secure=1` est INACTIVE, jamais
//! « réactivée »), ou sans séparateur est gardée telle quelle. Régler une valeur ne change que ses octets.

use super::{
    ConfigFormat, Doc, EntryInfo, MAX_FILE_BYTES, MAX_LINE_BYTES, MAX_LINES, MAX_VALUE_BYTES,
    Piece, Refused, Unreadable, UnreadableReason, check_input,
};
use crate::domain::catalog::loading::value::{FileFormat, forbidden_in_file};

const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

pub struct KvLines {
    separator: u8,
    comment: u8,
    double_quoted: bool,
}

impl KvLines {
    pub fn new(separator: u8, comment: u8, double_quoted: bool) -> Self {
        Self {
            separator,
            comment,
            double_quoted,
        }
    }

    fn is_blank(b: u8) -> bool {
        b == b' ' || b == b'\t'
    }

    /// Analyse une ligne (sans sa fin de ligne). `None` : ligne gardée telle quelle.
    fn parse_line(&self, line: &[u8], line_no: usize) -> Result<Option<EntryInfo>, Unreadable> {
        let first = line.iter().position(|b| !Self::is_blank(*b));
        let Some(first) = first else { return Ok(None) };
        if line[first] == self.comment {
            return Ok(None);
        }
        let Some(sep) = line.iter().position(|b| *b == self.separator) else {
            return Ok(None);
        };
        // Clé : à gauche du séparateur, espaces de bord retirés (pour la comparaison seulement).
        let key_end = line[..sep]
            .iter()
            .rposition(|b| !Self::is_blank(*b))
            .map_or(first, |p| p + 1);
        if key_end <= first {
            // « =valeur » : pas de clé, la ligne reste telle quelle.
            return Ok(None);
        }
        let key = String::from_utf8_lossy(&line[first..key_end]).into_owned();
        // Valeur : après le séparateur, espaces de bord retirés.
        let region = &line[sep + 1..];
        let (mut start, mut end) = match region.iter().position(|b| !Self::is_blank(*b)) {
            None => (sep + 1, sep + 1),
            Some(s) => {
                let e = region
                    .iter()
                    .rposition(|b| !Self::is_blank(*b))
                    .map_or(s, |p| p + 1);
                (sep + 1 + s, sep + 1 + e)
            }
        };
        let mut quote = 0u8;
        if self.double_quoted && start < end && line[start] == b'"' {
            if end - start < 2 || line[end - 1] != b'"' {
                return Err(Unreadable::new(
                    UnreadableReason::UnbalancedQuote,
                    Some(line_no),
                ));
            }
            start += 1;
            end -= 1;
            quote = b'"';
        }
        let value = String::from_utf8_lossy(&line[start..end]).into_owned();
        Ok(Some(EntryInfo {
            key,
            value,
            start,
            end,
            quote,
        }))
    }

    fn clean_key(&self, key: &str) -> bool {
        !key.is_empty()
            && key.len() <= 256
            && key == key.trim()
            && !key
                .bytes()
                .any(|b| b == self.separator || b < 0x20 || b == 0x7f)
            && key.as_bytes().first() != Some(&self.comment)
    }

    fn dominant_eol(doc: &Doc) -> &'static [u8] {
        let (mut crlf, mut lf) = (0usize, 0usize);
        for piece in &doc.pieces {
            if piece.bytes.ends_with(b"\r\n") {
                crlf += 1;
            } else if piece.bytes.ends_with(b"\n") {
                lf += 1;
            }
        }
        if crlf > lf { b"\r\n" } else { b"\n" }
    }

    fn encode(&self, value: &str) -> Result<Vec<u8>, Refused> {
        if value.len() > MAX_VALUE_BYTES {
            return Err(Refused::TooLong);
        }
        if let Some(bad) = forbidden_in_file(FileFormat::KvLines, self.double_quoted, value) {
            return Err(Refused::Value(bad));
        }
        Ok(value.as_bytes().to_vec())
    }

    fn new_line(&self, key: &str, value: &str, eol: &[u8]) -> Result<Vec<u8>, Refused> {
        if !self.clean_key(key) {
            return Err(Refused::Key);
        }
        let value_bytes = self.encode(value)?;
        let mut line = key.as_bytes().to_vec();
        line.push(self.separator);
        if self.double_quoted {
            line.push(b'"');
        }
        line.extend_from_slice(&value_bytes);
        if self.double_quoted {
            line.push(b'"');
        }
        line.extend_from_slice(eol);
        Ok(line)
    }
}

impl ConfigFormat for KvLines {
    fn parse(&self, bytes: &[u8]) -> Result<Doc, Unreadable> {
        check_input(bytes)?;
        if std::str::from_utf8(bytes).is_err() {
            return Err(Unreadable::new(UnreadableReason::InvalidUtf8, None));
        }
        let mut pieces = Vec::new();
        let mut rest = bytes;
        if rest.starts_with(BOM) {
            pieces.push(Piece::other(BOM.to_vec()));
            rest = &rest[BOM.len()..];
        }
        let mut line_no = 0usize;
        while !rest.is_empty() {
            line_no += 1;
            if line_no > MAX_LINES {
                return Err(Unreadable::new(
                    UnreadableReason::TooManyLines,
                    Some(line_no),
                ));
            }
            let end = rest
                .iter()
                .position(|b| *b == b'\n')
                .map_or(rest.len(), |p| p + 1);
            let raw = &rest[..end];
            if raw.len() > MAX_LINE_BYTES {
                return Err(Unreadable::new(
                    UnreadableReason::LineTooLong,
                    Some(line_no),
                ));
            }
            let body_end = if raw.ends_with(b"\r\n") {
                raw.len() - 2
            } else if raw.ends_with(b"\n") {
                raw.len() - 1
            } else {
                raw.len()
            };
            let body = &raw[..body_end];
            if body.contains(&b'\r') {
                return Err(Unreadable::new(
                    UnreadableReason::LoneCarriageReturn,
                    Some(line_no),
                ));
            }
            let entry = self.parse_line(body, line_no)?;
            pieces.push(Piece {
                bytes: raw.to_vec(),
                entry,
                closing: false,
            });
            rest = &rest[end..];
        }
        Ok(Doc { pieces })
    }

    fn set(&self, doc: &mut Doc, key: &str, value: &str) -> Result<(), Refused> {
        if !self.clean_key(key) {
            return Err(Refused::Key);
        }
        let size_after = doc.byte_len();
        let existing = doc
            .pieces
            .iter()
            .rposition(|p| p.entry.as_ref().is_some_and(|e| e.key == key));
        if let Some(index) = existing {
            let piece = &mut doc.pieces[index];
            let Some(entry) = piece.entry.as_mut() else {
                return Err(Refused::Key);
            };
            let encoded = self.encode(value)?;
            if size_after - (entry.end - entry.start) + encoded.len() > MAX_FILE_BYTES {
                return Err(Refused::TooLong);
            }
            piece
                .bytes
                .splice(entry.start..entry.end, encoded.iter().copied());
            entry.end = entry.start + encoded.len();
            entry.value = value.to_owned();
            return Ok(());
        }
        let eol = Self::dominant_eol(doc);
        let line = self.new_line(key, value, eol)?;
        if doc.pieces.len() >= MAX_LINES || size_after + line.len() + eol.len() > MAX_FILE_BYTES {
            return Err(Refused::TooLong);
        }
        // Le dernier morceau doit finir par une fin de ligne avant qu'on ajoute une ligne (un BOM seul
        // n'en a pas besoin).
        if let Some(last) = doc.pieces.last_mut()
            && !last.bytes.is_empty()
            && last.bytes.as_slice() != BOM
            && !last.bytes.ends_with(b"\n")
        {
            last.bytes.extend_from_slice(eol);
        }
        let start = key.len() + 1 + usize::from(self.double_quoted);
        let end = line.len() - eol.len() - usize::from(self.double_quoted);
        doc.pieces.push(Piece {
            bytes: line,
            entry: Some(EntryInfo {
                key: key.to_owned(),
                value: value.to_owned(),
                start,
                end,
                quote: if self.double_quoted { b'"' } else { 0 },
            }),
            closing: false,
        });
        Ok(())
    }

    fn unset(&self, doc: &mut Doc, key: &str) {
        doc.pieces
            .retain(|p| p.entry.as_ref().is_none_or(|e| e.key != key));
    }

    fn create(&self, pairs: &[(String, String)]) -> Result<Vec<u8>, Refused> {
        let mut out = Vec::new();
        for (key, value) in pairs {
            out.extend(self.new_line(key, value, b"\n")?);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn format() -> KvLines {
        KvLines::new(b'=', b'#', false)
    }

    /// Premier test du ticket : un fichier à la Terraria passe intact, puis une retouche ne change que la valeur.
    #[test]
    fn identite_octets_d_un_fichier_a_la_terraria() {
        let input = b"\xEF\xBB\xBFworldname=Monde\r\n# commentaire\n#secure=1\n\nmotd = Bonjour tout le monde";
        let format = format();
        let mut doc = format.parse(input).expect("lisible");
        assert_eq!(doc.render(), input, "identité octet pour octet");
        assert_eq!(doc.get("motd"), Some("Bonjour tout le monde"));
        assert_eq!(doc.get("secure"), None, "une ligne commentée est inactive");
        format
            .set(&mut doc, "motd", "Salut")
            .expect("valeur permise");
        let after = doc.render();
        let (start, _) = (input.len() - "Bonjour tout le monde".len(), ());
        let mut expected = input[..start].to_vec();
        expected.extend_from_slice(b"Salut");
        assert_eq!(after, expected, "seuls les octets de la valeur ont changé");
        assert_eq!(doc.get("motd"), Some("Salut"));
    }

    #[test]
    fn a_new_key_adds_one_line_with_the_dominant_line_ending() {
        let format = format();
        let mut doc = format.parse(b"a=1\r\nb=2").expect("lisible");
        format.set(&mut doc, "c", "3").expect("ok");
        assert_eq!(doc.render(), b"a=1\r\nb=2\r\nc=3\r\n");
        let mut doc = format.parse(b"a=1\nb=2\n").expect("lisible");
        format.set(&mut doc, "c", "3").expect("ok");
        assert_eq!(doc.render(), b"a=1\nb=2\nc=3\n");
        let mut empty = format.parse(b"").expect("lisible");
        format.set(&mut empty, "a", "1").expect("ok");
        assert_eq!(empty.render(), b"a=1\n");
    }

    #[test]
    fn a_commented_line_is_never_reactivated() {
        let format = format();
        let mut doc = format.parse(b"#secure=1\n").expect("lisible");
        format.set(&mut doc, "secure", "0").expect("ok");
        assert_eq!(doc.render(), b"#secure=1\nsecure=0\n");
    }

    #[test]
    fn the_last_duplicate_is_read_and_retouched_and_counted() {
        let format = format();
        let mut doc = format.parse(b"a=1\na=2\n").expect("lisible");
        assert_eq!(doc.get("a"), Some("2"));
        assert_eq!(doc.duplicate_keys(), 1);
        format.set(&mut doc, "a", "9").expect("ok");
        assert_eq!(doc.render(), b"a=1\na=9\n");
        format.unset(&mut doc, "a");
        assert_eq!(doc.render(), b"");
    }

    #[test]
    fn a_key_is_case_sensitive_so_a_differently_cased_line_stays_a_distinct_unknown_key() {
        let format = format();
        let mut doc = format
            .parse(
                b"secure=0
SECURE=1
",
            )
            .expect("lisible");
        assert_eq!(doc.get("secure"), Some("0"));
        assert_eq!(doc.get("SECURE"), Some("1"));
        format.set(&mut doc, "secure", "1").expect("ok");
        assert_eq!(
            doc.render(),
            b"secure=1
SECURE=1
"
        );
        assert_eq!(doc.duplicate_keys(), 0);
    }

    #[test]
    fn an_equal_sign_inside_a_value_is_kept() {
        let format = format();
        let mut doc = format.parse(b"motd=a=b=c\n").expect("lisible");
        assert_eq!(doc.get("motd"), Some("a=b=c"));
        format.set(&mut doc, "motd", "x=y").expect("ok");
        assert_eq!(doc.render(), b"motd=x=y\n");
    }

    #[test]
    fn a_value_cannot_inject_a_second_line() {
        let format = format();
        let mut doc = format.parse(b"a=1\n").expect("lisible");
        for bad in ["x\ny=2", "x\r\ny=2", "x\ry=2", "x\0y"] {
            let err = format.set(&mut doc, "a", bad).expect_err("refusée");
            assert!(matches!(err, Refused::Value(_)));
        }
        assert_eq!(doc.render(), b"a=1\n", "rien n'a été écrit");
        assert!(format.create(&[("k".into(), "v\nw=1".into())]).is_err());
    }

    #[test]
    fn a_key_with_the_separator_or_a_line_break_is_refused() {
        let format = format();
        let mut doc = format.parse(b"").expect("lisible");
        for bad in ["", "a=b", "a\nb", " a", "a ", "#a", "\ta"] {
            assert_eq!(format.set(&mut doc, bad, "1"), Err(Refused::Key), "{bad:?}");
        }
    }

    #[test]
    fn leading_and_trailing_blanks_of_a_value_are_kept_around_it() {
        let format = format();
        let mut doc = format.parse(b"  motd  =   salut   \n").expect("lisible");
        assert_eq!(doc.get("motd"), Some("salut"));
        format.set(&mut doc, "motd", "ok").expect("ok");
        assert_eq!(doc.render(), b"  motd  =   ok   \n");
        assert!(
            format.set(&mut doc, "motd", " ok").is_err(),
            "perdrait ses espaces à la relecture"
        );
    }

    #[test]
    fn an_empty_value_is_a_value() {
        let format = format();
        let mut doc = format.parse(b"password=\nmotd=   \n").expect("lisible");
        assert_eq!(doc.get("password"), Some(""));
        assert_eq!(doc.get("motd"), Some(""));
        format.set(&mut doc, "password", "x").expect("ok");
        format.set(&mut doc, "motd", "y").expect("ok");
        assert_eq!(doc.render(), b"password=x\nmotd=y   \n");
    }

    #[test]
    fn the_double_quoted_variant_keeps_the_quotes() {
        let format = KvLines::new(b'=', b'#', true);
        let mut doc = format
            .parse(b"querymode=\"1\"\nother=2\n")
            .expect("lisible");
        assert_eq!(doc.get("querymode"), Some("1"));
        assert_eq!(doc.get("other"), Some("2"));
        format.set(&mut doc, "querymode", "0").expect("ok");
        format.set(&mut doc, "new", "a b").expect("ok");
        assert_eq!(doc.render(), b"querymode=\"0\"\nother=2\nnew=\"a b\"\n");
        assert_eq!(
            format.set(&mut doc, "querymode", "a\"b"),
            Err(Refused::Value('"'))
        );
        assert!(
            format.parse(b"a=\"unclosed\n").is_err(),
            "guillemet non refermé : fichier refusé"
        );
        assert!(format.parse(b"a=\"\n").is_err());
    }

    #[test]
    fn hostile_files_are_refused_whole_and_never_allocated_first() {
        let format = format();
        assert_eq!(
            format.parse(b"a=1\0b").err().map(|e| e.reason),
            Some(UnreadableReason::NulByte)
        );
        assert_eq!(
            format.parse(b"a=\xff\xfe\n").err().map(|e| e.reason),
            Some(UnreadableReason::InvalidUtf8)
        );
        assert_eq!(
            format.parse(b"a=1\rb=2\n").err().map(|e| e.reason),
            Some(UnreadableReason::LoneCarriageReturn)
        );
        let long = format!("a={}\n", "x".repeat(MAX_LINE_BYTES));
        assert_eq!(
            format.parse(long.as_bytes()).err().map(|e| e.reason),
            Some(UnreadableReason::LineTooLong)
        );
        let many = "a=1\n".repeat(MAX_LINES + 1);
        assert_eq!(
            format.parse(many.as_bytes()).err().map(|e| e.reason),
            Some(UnreadableReason::TooManyLines)
        );
        let huge = vec![b'a'; super::super::MAX_FILE_BYTES + 1];
        assert_eq!(
            format.parse(&huge).err().map(|e| e.reason),
            Some(UnreadableReason::TooLarge)
        );
        assert!(
            format
                .parse(&"a=1\n".repeat(MAX_LINES).into_bytes())
                .is_ok(),
            "20 000 lignes passent"
        );
    }

    #[test]
    fn a_utf16_file_is_refused_not_mangled() {
        let format = format();
        let utf16: Vec<u8> = "a=1\n".encode_utf16().flat_map(u16::to_le_bytes).collect();
        let mut with_bom = vec![0xFF, 0xFE];
        with_bom.extend(utf16);
        assert!(format.parse(&with_bom).is_err());
    }

    #[test]
    fn ten_successive_settings_keep_every_other_byte() {
        let format = format();
        let original: &[u8] = b"\xEF\xBB\xBF# en-tete\r\nworldname=Monde\r\nunknown_thing=garde moi\r\n#secure=1\r\n\r\nmotd=Salut\r\nlast=x";
        let mut doc = format.parse(original).expect("lisible");
        for i in 0..10 {
            format
                .set(&mut doc, "motd", &format!("valeur {i}"))
                .expect("ok");
            format.set(&mut doc, "extra", &format!("{i}")).expect("ok");
        }
        let out = doc.render();
        let text = String::from_utf8(out).expect("utf8");
        assert!(text.starts_with("\u{feff}# en-tete\r\nworldname=Monde\r\nunknown_thing=garde moi\r\n#secure=1\r\n\r\nmotd=valeur 9\r\nlast=x\r\n"));
        assert!(text.ends_with("extra=9\r\n"));
        assert_eq!(doc.get("unknown_thing"), Some("garde moi"));
    }

    #[test]
    fn a_line_without_a_key_or_a_separator_is_kept_as_it_is() {
        let format = format();
        let input = b"=value\njust text\n  \t\n";
        let doc = format.parse(input).expect("lisible");
        assert!(doc.entries().is_empty());
        assert_eq!(doc.render(), input);
    }
}

//! Suite de contrat des formats de fichier de réglages : UNE suite, passée par chaque format, sans qu'on
//! l'assouplisse pour l'un d'eux (conception 11.1, section 15). Générateur déterministe (graine fixe), cas
//! bornés : aucune boucle de charge.
//!
//! P1 identité, P2 localité, P3 refus plutôt que perte, doublons, fins de ligne mêlées, BOM, fichier sans fin
//! de ligne finale, valeur vide, 20 000 lignes, valeurs qui tentent d'injecter une seconde ligne ou une
//! seconde propriété, dix réglages successifs qui épargnent commentaires et lignes inconnues.

use super::kv_lines::KvLines;
use super::xml_properties::XmlProperties;
use super::{ConfigFormat, Doc, MAX_LINES, Refused};

/// Générateur xorshift64* : même suite à chaque exécution.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }

    fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }
}

struct Fixture {
    name: &'static str,
    format: Box<dyn ConfigFormat>,
    generate: fn(&mut Rng) -> Vec<u8>,
    keys: &'static [&'static str],
    /// Valeurs que le format doit savoir écrire et relire à l'identique.
    valid_values: &'static [&'static str],
    /// Fichiers que le format doit refuser entiers.
    hostile_files: Vec<Vec<u8>>,
}

/// Valeurs qui tentent d'abîmer un fichier : écrites puis relues identiques, ou refusées sans rien écrire.
const SPECIAL_VALUES: &[&str] = &[
    "a\nb=c",
    "a\r\nb=c",
    "a\rb",
    "a\0b",
    "a\"b",
    "a<b",
    "a&b",
    "]]>",
    "-->",
    "a=b",
    " lead",
    "trail ",
    "\u{2028}x",
    "tab\tx",
    "x\u{1}y",
    "\"/><property name=\"Evil\" value=\"1",
    "&amp;",
    "&#10;",
    "\\n",
    "# not a comment",
    "é€\u{1f600}",
];

fn kv_generate(rng: &mut Rng) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    if rng.chance(20) {
        out.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    }
    let lines = rng.below(14);
    for n in 0..lines {
        let eol: &[u8] = if rng.chance(40) { b"\r\n" } else { b"\n" };
        let line = match rng.below(10) {
            0 => format!("k{}={}", rng.below(4), rng.below(100)),
            1 => format!("motd = Bonjour {n}"),
            2 => format!("  password  =  s{n}  "),
            3 => "secure=".to_owned(),
            4 => format!("# commentaire {n}"),
            5 => format!("#secure={n}"),
            6 => String::new(),
            7 => format!("pas de separateur {n}"),
            8 => format!("k{}=a=b=c", rng.below(4)),
            _ => format!("=sans cle {n}"),
        };
        out.extend_from_slice(line.as_bytes());
        // La dernière ligne peut n'avoir aucune fin de ligne.
        if n + 1 < lines || rng.chance(70) {
            out.extend_from_slice(eol);
        }
    }
    out
}

fn xml_generate(rng: &mut Rng) -> Vec<u8> {
    let eol = if rng.chance(40) { "\r\n" } else { "\n" };
    let mut out = String::new();
    if rng.chance(20) {
        out.push('\u{feff}');
    }
    if rng.chance(70) {
        out.push_str(&format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>{eol}"));
    }
    if rng.chance(30) {
        out.push_str(&format!("<!-- en-tête -->{eol}"));
    }
    out.push_str(&format!("<ServerSettings>{eol}"));
    let lines = rng.below(12);
    for n in 0..lines {
        let indent = *rng.pick(&["", "  ", "\t", "    "]);
        let line = match rng.below(8) {
            0 => format!(
                "<property name=\"k{}\" value=\"{}\"/>",
                rng.below(4),
                rng.below(100)
            ),
            1 => format!("<property name=\"ServerName\" value=\"Mon serveur {n}\" />"),
            2 => format!("<property name='Port' value='{}'/>", 26900 + n),
            3 => "<property name=\"Name&amp;Co\" value=\"a &lt; b &amp; c\" extra=\"1\"/>"
                .to_string(),
            4 => format!("<!-- <property name=\"k1\" value=\"commented{n}\"/> -->"),
            5 => format!("<other attr=\"x\">texte {n}</other>"),
            6 => String::new(),
            _ => format!("<property name=\"k{}\" value=\"\"/>", rng.below(4)),
        };
        out.push_str(indent);
        out.push_str(&line);
        out.push_str(eol);
    }
    out.push_str("</ServerSettings>");
    if rng.chance(70) {
        out.push_str(eol);
    }
    out.into_bytes()
}

fn fixtures() -> Vec<Fixture> {
    let kv_keys: &[&str] = &[
        "k0", "k1", "k2", "k3", "motd", "password", "secure", "absent",
    ];
    let xml_keys: &[&str] = &[
        "k0",
        "k1",
        "k2",
        "k3",
        "ServerName",
        "Port",
        "Name&Co",
        "absent",
    ];
    let common: &[&str] = &[
        "",
        "x",
        "Salut tout le monde",
        "a=b",
        "é€",
        "0",
        "42",
        "k=v",
        "#hash",
        ";semi",
    ];
    let xml_only: &[&str] = &[
        "",
        "x",
        "Salut tout le monde",
        "a=b",
        "é€",
        "0",
        "42",
        "a&b",
        "a<b",
        "a>b",
        "\"",
        "'",
        "]]>",
        "-->",
    ];
    let mut kv_hostile: Vec<Vec<u8>> = vec![
        b"a=1\0b=2\n".to_vec(),
        b"a=\xff\xfe\n".to_vec(),
        b"a=1\rb=2\n".to_vec(),
        format!("a={}\n", "x".repeat(70_000)).into_bytes(),
        "a=1\n".repeat(MAX_LINES + 1).into_bytes(),
        vec![b'a'; super::MAX_FILE_BYTES + 1],
    ];
    kv_hostile.push({
        let mut utf16 = vec![0xFF, 0xFE];
        utf16.extend("a=1\n".encode_utf16().flat_map(u16::to_le_bytes));
        utf16
    });
    let xml_hostile: Vec<Vec<u8>> = vec![
        b"<ServerSettings>\0</ServerSettings>".to_vec(),
        b"<ServerSettings>\xff</ServerSettings>".to_vec(),
        b"<!DOCTYPE x><ServerSettings></ServerSettings>".to_vec(),
        b"<ServerSettings><!ENTITY a \"b\"></ServerSettings>".to_vec(),
        b"<?xml version=\"1.0\" encoding=\"UTF-16\"?><ServerSettings></ServerSettings>".to_vec(),
        b"<ServerSettings><property name=\"a\" value=\"&bogus;\"/></ServerSettings>".to_vec(),
        b"<ServerSettings><property name=\"a\" value=\"1\"/>".to_vec(),
        b"<ServerSettings><!-- ouvert </ServerSettings>".to_vec(),
        format!("<ServerSettings>{}</ServerSettings>", "x".repeat(70_000)).into_bytes(),
        vec![b'a'; super::MAX_FILE_BYTES + 1],
    ];
    vec![
        Fixture {
            name: "kv_lines",
            format: Box::new(KvLines::new(b'=', b'#', false)),
            generate: kv_generate,
            keys: kv_keys,
            valid_values: common,
            hostile_files: kv_hostile,
        },
        Fixture {
            name: "xml_properties",
            format: Box::new(XmlProperties::new(
                "ServerSettings",
                "property",
                "name",
                "value",
            )),
            generate: xml_generate,
            keys: xml_keys,
            valid_values: xml_only,
            hostile_files: xml_hostile,
        },
    ]
}

const CASES: usize = 300;

fn entries_except(doc: &Doc, key: &str) -> Vec<(String, String)> {
    doc.entries()
        .into_iter()
        .filter(|e| e.key != key)
        .map(|e| (e.key.to_owned(), e.value.to_owned()))
        .collect()
}

#[test]
fn p1_identity_render_of_parse_gives_the_same_bytes() {
    for fixture in fixtures() {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        let mut accepted = 0usize;
        for case in 0..CASES {
            let bytes = (fixture.generate)(&mut rng);
            let Ok(doc) = fixture.format.parse(&bytes) else {
                continue;
            };
            accepted += 1;
            assert_eq!(doc.render(), bytes, "{} cas {case}", fixture.name);
        }
        assert!(
            accepted > CASES / 2,
            "{} : le générateur doit produire surtout des fichiers lisibles ({accepted})",
            fixture.name
        );
    }
}

#[test]
fn p2_locality_setting_a_value_changes_only_its_bytes() {
    for fixture in fixtures() {
        let mut rng = Rng(0xD1B5_4A32_D192_ED03);
        for case in 0..CASES {
            let bytes = (fixture.generate)(&mut rng);
            let Ok(doc) = fixture.format.parse(&bytes) else {
                continue;
            };
            let key = *rng.pick(fixture.keys);
            let value = *rng.pick(fixture.valid_values);
            let range = doc.value_range(key);
            let others_before = entries_except(&doc, key);
            let count_before = doc.entries().len();
            let mut edited = doc.clone();
            fixture
                .format
                .set(&mut edited, key, value)
                .unwrap_or_else(|e| {
                    panic!("{} cas {case} : {value:?} refusée ({e:?})", fixture.name)
                });
            let after = edited.render();
            assert_eq!(edited.get(key), Some(value), "{} cas {case}", fixture.name);
            assert_eq!(
                entries_except(&edited, key),
                others_before,
                "{} cas {case} : les autres entrées",
                fixture.name
            );
            match range {
                Some((start, end)) => {
                    assert_eq!(
                        &after[..start],
                        &bytes[..start],
                        "{} cas {case} : avant la valeur",
                        fixture.name
                    );
                    let tail = bytes.len() - end;
                    assert_eq!(
                        &after[after.len() - tail..],
                        &bytes[end..],
                        "{} cas {case} : après la valeur",
                        fixture.name
                    );
                    assert_eq!(edited.entries().len(), count_before);
                }
                None => {
                    // Une seule ligne ajoutée, le reste intact (une fin de ligne peut être ajoutée au dernier morceau).
                    assert_eq!(
                        edited.entries().len(),
                        count_before + 1,
                        "{} cas {case}",
                        fixture.name
                    );
                    assert!(after.len() > bytes.len(), "{} cas {case}", fixture.name);
                }
            }
            // Le fichier écrit est relisible et redonne ses octets.
            let again = fixture
                .format
                .parse(&after)
                .unwrap_or_else(|e| panic!("{} cas {case} : relecture {e}", fixture.name));
            assert_eq!(again.render(), after);
            assert_eq!(again.get(key), Some(value));
        }
    }
}

#[test]
fn a_special_value_is_read_back_identical_or_refused_without_writing_anything() {
    for fixture in fixtures() {
        let mut rng = Rng(0x2545_F491_4F6C_DD1D);
        for case in 0..60 {
            let bytes = (fixture.generate)(&mut rng);
            let Ok(doc) = fixture.format.parse(&bytes) else {
                continue;
            };
            for key in ["k0", "ServerName", "motd", "new_key"] {
                for value in SPECIAL_VALUES {
                    let mut edited = doc.clone();
                    let others = entries_except(&doc, key);
                    let count_before = doc.entries().len();
                    match fixture.format.set(&mut edited, key, value) {
                        Err(Refused::Value(_) | Refused::TooLong | Refused::Key) => {
                            assert_eq!(
                                edited.render(),
                                bytes,
                                "{} cas {case} : refus sans écriture",
                                fixture.name
                            );
                        }
                        Ok(()) => {
                            let again =
                                fixture.format.parse(&edited.render()).unwrap_or_else(|e| {
                                    panic!(
                                        "{} cas {case} {value:?} : fichier écrit illisible ({e})",
                                        fixture.name
                                    )
                                });
                            assert_eq!(again.get(key), Some(*value), "{} {value:?}", fixture.name);
                            assert_eq!(
                                entries_except(&again, key),
                                others,
                                "{} {value:?} : une autre entrée a bougé",
                                fixture.name
                            );
                            assert!(
                                again.entries().len() <= count_before + 1,
                                "{} {value:?} : entrée en trop",
                                fixture.name
                            );
                            assert_eq!(
                                again.duplicate_keys(),
                                doc.duplicate_keys(),
                                "{} {value:?}",
                                fixture.name
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn p3_a_file_the_format_cannot_split_is_refused_whole() {
    for fixture in fixtures() {
        for (n, hostile) in fixture.hostile_files.iter().enumerate() {
            assert!(
                fixture.format.parse(hostile).is_err(),
                "{} fichier hostile {n}",
                fixture.name
            );
        }
    }
}

#[test]
fn parse_never_panics_and_accepted_bytes_come_back_identical_on_random_input() {
    for fixture in fixtures() {
        let mut rng = Rng(0xA076_1D64_78BD_642F);
        let alphabet: &[u8] = b"<>/=\"' &;#\r\n\tabk01!-?[]\xc3\xa9";
        for case in 0..CASES {
            let len = rng.below(120);
            let bytes: Vec<u8> = (0..len).map(|_| *rng.pick(alphabet)).collect();
            if let Ok(doc) = fixture.format.parse(&bytes) {
                assert_eq!(doc.render(), bytes, "{} cas {case}", fixture.name);
            }
        }
    }
}

#[test]
fn twenty_thousand_lines_pass_and_one_more_is_refused() {
    let kv = KvLines::new(b'=', b'#', false);
    let at_limit = "a=1\n".repeat(MAX_LINES).into_bytes();
    let mut doc = kv.parse(&at_limit).expect("20 000 lignes");
    assert_eq!(doc.render(), at_limit);
    assert_eq!(
        kv.set(&mut doc, "new", "1"),
        Err(Refused::TooLong),
        "le fichier écrit ne serait plus relisible"
    );
    assert!(
        kv.parse(&"a=1\n".repeat(MAX_LINES + 1).into_bytes())
            .is_err()
    );
    kv.set(&mut doc, "a", "2")
        .expect("retoucher une entrée existante reste permis");
    assert!(kv.parse(&doc.render()).is_ok());
}

#[test]
fn a_value_too_long_to_read_back_is_refused() {
    for fixture in fixtures() {
        let bytes = (fixture.generate)(&mut Rng(7));
        let Ok(mut doc) = fixture.format.parse(&bytes) else {
            continue;
        };
        let huge = "x".repeat(super::MAX_VALUE_BYTES + 1);
        assert_eq!(
            fixture.format.set(&mut doc, "k0", &huge),
            Err(Refused::TooLong),
            "{}",
            fixture.name
        );
        let ok = "x".repeat(super::MAX_VALUE_BYTES);
        fixture.format.set(&mut doc, "k0", &ok).expect("à la borne");
        assert!(
            fixture.format.parse(&doc.render()).is_ok(),
            "{}",
            fixture.name
        );
    }
}

#[test]
fn ten_successive_settings_keep_comments_unknown_lines_commented_lines_and_line_endings() {
    for fixture in fixtures() {
        let mut rng = Rng(0xE703_7ED1_A0B4_28DB);
        for case in 0..60 {
            let bytes = (fixture.generate)(&mut rng);
            let Ok(original) = fixture.format.parse(&bytes) else {
                continue;
            };
            // Les morceaux sans entrée (commentaires, lignes inconnues, lignes commentées, fins de ligne) :
            // chacun doit se retrouver tel quel après dix réglages.
            let kept: Vec<Vec<u8>> = original
                .pieces
                .iter()
                .filter(|p| p.entry.is_none() && !p.closing)
                .map(|p| {
                    let mut b = p.bytes.clone();
                    while b.ends_with(b"\n") || b.ends_with(b"\r") {
                        b.pop();
                    }
                    b
                })
                .filter(|b| !b.is_empty())
                .collect();
            let mut doc = original.clone();
            for i in 0..10 {
                let key = fixture.keys[(i + case) % fixture.keys.len()];
                let value = fixture.valid_values[(i * 3 + case) % fixture.valid_values.len()];
                fixture
                    .format
                    .set(&mut doc, key, value)
                    .expect("valeur permise");
            }
            let out = doc.render();
            for piece in &kept {
                assert!(
                    out.windows(piece.len()).any(|w| w == piece.as_slice()),
                    "{} cas {case} : un morceau inconnu a disparu : {:?}",
                    fixture.name,
                    String::from_utf8_lossy(piece)
                );
            }
            assert!(fixture.format.parse(&out).is_ok());
        }
    }
}

#[test]
fn duplicates_are_counted_and_the_last_one_is_read() {
    let kv = KvLines::new(b'=', b'#', false);
    let doc = kv.parse(b"a=1\nb=2\na=3\n").expect("lisible");
    assert_eq!(doc.duplicate_keys(), 1);
    assert_eq!(doc.get("a"), Some("3"));
    let xml = XmlProperties::new("ServerSettings", "property", "name", "value");
    let doc = xml
        .parse(b"<ServerSettings>\n<property name=\"a\" value=\"1\"/>\n<property name=\"a\" value=\"3\"/>\n</ServerSettings>\n")
        .expect("lisible");
    assert_eq!(doc.duplicate_keys(), 1);
    assert_eq!(doc.get("a"), Some("3"));
}

#[test]
fn an_empty_value_is_a_value_in_both_formats() {
    let kv = KvLines::new(b'=', b'#', false);
    let mut doc = kv.parse(b"a=\n").expect("lisible");
    assert_eq!(doc.get("a"), Some(""));
    kv.set(&mut doc, "a", "").expect("vide permise");
    assert_eq!(doc.render(), b"a=\n");
    let xml = XmlProperties::new("ServerSettings", "property", "name", "value");
    let mut doc = xml
        .parse(b"<ServerSettings>\n<property name=\"a\" value=\"\"/>\n</ServerSettings>\n")
        .expect("lisible");
    assert_eq!(doc.get("a"), Some(""));
    xml.set(&mut doc, "a", "").expect("vide permise");
    assert_eq!(doc.get("a"), Some(""));
}

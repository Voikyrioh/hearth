//! Une fiche chargée : les deux fichiers analysés, les préréglages résolus, l'empreinte. Et la validation
//! d'UNE valeur de réglage contre son type (la même fonction sert au chargement, pour les valeurs
//! proposées, et à l'exécution, pour celles de l'utilisateur).

use sha2::{Digest, Sha256};

use super::schema::fiche::{Channel, FicheFile, Port};
use super::schema::settings::{
    ArgSpec, ChangeWhen, Choice, Imposed, ItemCharset, Level, Retired, SecretKind, Source, Theme,
    Versions,
};
use super::value::{Charset, first_refused_char, is_risky_char};

/// Un type de réglage avec ses paramètres.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueSpec {
    Text {
        charset: Charset,
        min_len: usize,
        max_len: usize,
    },
    Secret {
        kind: SecretKind,
        min_len: usize,
        max_len: usize,
        obtain_url: Option<String>,
    },
    Int {
        min: Option<i64>,
        max: Option<i64>,
    },
    Bool {
        on: String,
        off: String,
    },
    Choice {
        choices: Vec<Choice>,
    },
    IdList {
        item_charset: ItemCharset,
        separator: char,
        max_items: usize,
    },
    Opaque {
        max_len: usize,
    },
}

/// Un réglage résolu (préréglage appliqué, champs obligatoires présents).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Setting {
    pub id: String,
    pub key: String,
    pub channel: String,
    pub arg: Option<ArgSpec>,
    pub theme: String,
    pub level: Level,
    pub label: String,
    pub help: String,
    pub spec: ValueSpec,
    pub game_default: Option<String>,
    pub proposed: Option<String>,
    pub change: ChangeWhen,
    pub imposed: Option<Imposed>,
    pub versions: Option<Versions>,
    pub source: Source,
    pub only_variants: Option<Vec<String>>,
}

impl Setting {
    pub fn is_secret(&self) -> bool {
        matches!(self.spec, ValueSpec::Secret { .. })
    }
}

/// Une fiche acceptée par le chargeur.
#[derive(Debug, Clone)]
pub struct Fiche {
    pub file: FicheFile,
    pub themes: Vec<Theme>,
    pub settings: Vec<Setting>,
    pub retired: Vec<Retired>,
    pub digest: [u8; 32],
}

impl Fiche {
    pub fn id(&self) -> &str {
        &self.file.id
    }

    pub fn setting(&self, id: &str) -> Option<&Setting> {
        self.settings.iter().find(|s| s.id == id)
    }

    pub fn channel(&self, id: &str) -> Option<&Channel> {
        self.file.channel.iter().find(|c| c.id == id)
    }

    pub fn port(&self, id: &str) -> Option<&Port> {
        self.file.port.iter().find(|p| p.id == id)
    }

    pub fn retired_setting(&self, id: &str) -> Option<&Retired> {
        self.retired.iter().find(|r| r.id == id)
    }

    pub fn digest_hex(&self) -> String {
        hex(&self.digest)
    }
}

/// Empreinte d'une fiche : SHA-256 de `u32(longueur) || fiche.toml || u32(longueur) || settings.toml`,
/// longueurs sur quatre octets en gros-boutiste (conception technique 5.1).
pub fn fiche_digest(fiche_toml: &[u8], settings_toml: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for part in [fiche_toml, settings_toml] {
        hasher.update((part.len() as u32).to_be_bytes());
        hasher.update(part);
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(hasher.finalize().as_slice());
    out
}

pub fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Pourquoi une valeur ne passe pas son type. Ne porte jamais la valeur : au plus UN caractère fautif
/// (et aucun pour un secret).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueRefusal {
    /// Pas un entier écrit sous sa forme canonique, ou hors des bornes.
    Bounds,
    /// Pas l'une des valeurs permises.
    Choice,
    Charset(Option<char>),
    Length,
}

/// Valide `text` pour `setting` (type, bornes, jeu de caractères). Ne regarde ni le canal, ni `imposed`,
/// ni `change` : ce sont des règles de `settings_state`.
pub fn validate_value(spec: &ValueSpec, text: &str) -> Result<(), ValueRefusal> {
    match spec {
        ValueSpec::Text {
            charset,
            min_len,
            max_len,
        } => {
            check_len(text, *min_len, *max_len)?;
            match first_refused_char(*charset, text) {
                Some(c) => Err(ValueRefusal::Charset(Some(c))),
                None => Ok(()),
            }
        }
        ValueSpec::Secret {
            min_len, max_len, ..
        } => {
            check_len(text, *min_len, *max_len)?;
            if text.chars().any(is_risky_char) {
                return Err(ValueRefusal::Charset(None));
            }
            Ok(())
        }
        ValueSpec::Int { min, max } => {
            let Ok(number) = text.parse::<i64>() else {
                return Err(ValueRefusal::Bounds);
            };
            if number.to_string() != text {
                return Err(ValueRefusal::Bounds);
            }
            if min.is_some_and(|m| number < m) || max.is_some_and(|m| number > m) {
                return Err(ValueRefusal::Bounds);
            }
            Ok(())
        }
        ValueSpec::Bool { on, off } => {
            if text == on || text == off {
                Ok(())
            } else {
                Err(ValueRefusal::Choice)
            }
        }
        ValueSpec::Choice { choices } => {
            if choices.iter().any(|c| c.value == text) {
                Ok(())
            } else {
                Err(ValueRefusal::Choice)
            }
        }
        ValueSpec::IdList {
            item_charset,
            separator,
            max_items,
        } => {
            if text.is_empty() {
                return Ok(());
            }
            let mut count = 0usize;
            for item in text.split(*separator) {
                count += 1;
                if count > *max_items {
                    return Err(ValueRefusal::Length);
                }
                if item.is_empty() {
                    return Err(ValueRefusal::Length);
                }
                let bad = match item_charset {
                    ItemCharset::Digits => item.chars().find(|c| !c.is_ascii_digit()),
                    ItemCharset::Slug => item.chars().find(|c| {
                        !(c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_'))
                    }),
                };
                if let Some(c) = bad {
                    return Err(ValueRefusal::Charset(Some(c)));
                }
                if item.len() > 64 {
                    return Err(ValueRefusal::Length);
                }
            }
            Ok(())
        }
        ValueSpec::Opaque { max_len } => {
            check_len(text, 0, *max_len)?;
            if text.chars().any(is_risky_char) {
                return Err(ValueRefusal::Charset(None));
            }
            Ok(())
        }
    }
}

fn check_len(text: &str, min: usize, max: usize) -> Result<(), ValueRefusal> {
    let len = text.chars().count();
    if len < min || len > max {
        Err(ValueRefusal::Length)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(charset: Charset, max: usize) -> ValueSpec {
        ValueSpec::Text {
            charset,
            min_len: 0,
            max_len: max,
        }
    }

    #[test]
    fn a_text_value_is_bounded_and_cites_the_character_not_the_value() {
        let spec = text(Charset::ArgSafe, 8);
        assert_eq!(validate_value(&spec, "Monde-1"), Ok(()));
        assert_eq!(
            validate_value(&spec, "Mon mnd"),
            Err(ValueRefusal::Charset(Some(' ')))
        );
        assert_eq!(
            validate_value(&spec, "123456789"),
            Err(ValueRefusal::Length)
        );
        assert_eq!(
            validate_value(&spec, "a;rm"),
            Err(ValueRefusal::Charset(Some(';')))
        );
    }

    #[test]
    fn a_secret_refusal_never_names_a_character() {
        let spec = ValueSpec::Secret {
            kind: SecretKind::GamePassword,
            min_len: 0,
            max_len: 10,
            obtain_url: None,
        };
        assert_eq!(
            validate_value(&spec, "abc\ndef"),
            Err(ValueRefusal::Charset(None))
        );
        assert_eq!(validate_value(&spec, "ok"), Ok(()));
        assert_eq!(
            validate_value(&spec, "01234567890"),
            Err(ValueRefusal::Length)
        );
    }

    #[test]
    fn an_integer_is_canonical_and_within_bounds() {
        let spec = ValueSpec::Int {
            min: Some(-1),
            max: Some(255),
        };
        for ok in ["-1", "0", "8", "255"] {
            assert_eq!(validate_value(&spec, ok), Ok(()), "{ok}");
        }
        for bad in [
            "", "+8", "08", "8 ", "-2", "256", "1e3", "0x10", "٣", "--1", "1.0",
        ] {
            assert_eq!(
                validate_value(&spec, bad),
                Err(ValueRefusal::Bounds),
                "{bad:?}"
            );
        }
        assert_eq!(
            validate_value(
                &ValueSpec::Int {
                    min: Some(-1),
                    max: None
                },
                "999999"
            ),
            Ok(())
        );
    }

    #[test]
    fn a_choice_or_a_bool_is_one_of_the_listed_values() {
        let choices = ValueSpec::Choice {
            choices: vec![Choice {
                value: "1".into(),
                label: "a".into(),
            }],
        };
        assert_eq!(validate_value(&choices, "1"), Ok(()));
        assert_eq!(validate_value(&choices, "2"), Err(ValueRefusal::Choice));
        let boolean = ValueSpec::Bool {
            on: "YES".into(),
            off: "NO".into(),
        };
        assert_eq!(validate_value(&boolean, "NO"), Ok(()));
        assert_eq!(validate_value(&boolean, "yes"), Err(ValueRefusal::Choice));
    }

    #[test]
    fn an_id_list_has_bounded_clean_items() {
        let spec = ValueSpec::IdList {
            item_charset: ItemCharset::Digits,
            separator: ',',
            max_items: 3,
        };
        assert_eq!(validate_value(&spec, ""), Ok(()));
        assert_eq!(validate_value(&spec, "1,22,333"), Ok(()));
        assert_eq!(validate_value(&spec, "1,2,3,4"), Err(ValueRefusal::Length));
        assert_eq!(validate_value(&spec, "1,,2"), Err(ValueRefusal::Length));
        assert_eq!(
            validate_value(&spec, "1,a"),
            Err(ValueRefusal::Charset(Some('a')))
        );
    }

    #[test]
    fn the_digest_of_a_fiche_covers_both_files_with_their_lengths() {
        let a = fiche_digest(b"ab", b"c");
        let b = fiche_digest(b"a", b"bc");
        assert_ne!(a, b, "les longueurs séparent les deux fichiers");
        assert_eq!(a, fiche_digest(b"ab", b"c"));
        assert_eq!(hex(&a).len(), 64);
    }
}

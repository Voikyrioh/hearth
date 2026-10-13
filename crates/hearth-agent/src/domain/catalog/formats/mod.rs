//! Formats de fichier de réglages, écrits à la main, sans bibliothèque (ADR-0051, conception 11.1).
//!
//! Le fichier du jeu est la vérité de ses octets. Un format le découpe en MORCEAUX dont la concaténation
//! redonne exactement les octets d'origine ; une retouche ne change que les octets de la valeur visée.
//! Trois propriétés, passées par une suite de contrat unique (`contract.rs`) :
//!
//! - **P1, identité** : `render(parse(octets)) == octets`, pour tout fichier que `parse` accepte ;
//! - **P2, localité** : après `set(clé, valeur)`, les octets hors de la valeur de cette entrée sont intacts
//!   (ou, pour une clé nouvelle, une seule ligne est ajoutée en fin de fichier) ;
//! - **P3, refus plutôt que perte** : un fichier que le format ne sait pas découper est refusé ENTIER, jamais
//!   réécrit. Un fichier de jeu est un contenu hostile : les bornes sont vérifiées avant toute allocation.
//!
//! Aucune E/S : les octets viennent de l'appelant (`SafeFs::read_text`, HRT-93, plus tard).

pub mod kv_lines;
pub mod xml_properties;

#[cfg(test)]
mod contract;

use std::fmt;

use super::loading::schema::fiche::{Channel, ChannelKind, Quote};
use super::loading::value::FileFormat;

/// Taille maximale d'un fichier de réglages lu (2 Mio).
pub const MAX_FILE_BYTES: usize = 2 * 1024 * 1024;
/// Longueur maximale d'une ligne (64 Kio).
pub const MAX_LINE_BYTES: usize = 64 * 1024;
/// Nombre maximal de lignes (ou d'entrées pour XML).
pub const MAX_LINES: usize = 20_000;
/// Taille maximale d'une valeur écrite (16 Kio) : une ligne écrite ne dépasse jamais ce que `parse` accepte.
pub const MAX_VALUE_BYTES: usize = 16 * 1024;

/// Pourquoi un fichier est refusé entier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnreadableReason {
    TooLarge,
    NulByte,
    InvalidUtf8,
    LineTooLong,
    TooManyLines,
    /// Un retour chariot isolé : deux lectures possibles de la ligne, donc aucune retouche sûre.
    LoneCarriageReturn,
    UnbalancedQuote,
    XmlEncoding,
    XmlDoctypeOrEntity,
    XmlUnterminated,
    XmlRootMissing,
    XmlRootNotClosed,
    XmlRootClosedTwice,
    XmlPropertyShape,
    XmlPropertyOutsideRoot,
    XmlAttribute,
    XmlEntity,
}

impl UnreadableReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TooLarge => "fichier de plus de 2 Mio",
            Self::NulByte => "octet nul",
            Self::InvalidUtf8 => "encodage invalide (UTF-8 attendu)",
            Self::LineTooLong => "ligne de plus de 64 Kio",
            Self::TooManyLines => "plus de 20 000 lignes",
            Self::LoneCarriageReturn => "retour chariot isolé",
            Self::UnbalancedQuote => "guillemet non refermé",
            Self::XmlEncoding => "encodage XML déclaré autre que UTF-8",
            Self::XmlDoctypeOrEntity => "déclaration DOCTYPE ou ENTITY",
            Self::XmlUnterminated => "commentaire, balise ou section non terminé",
            Self::XmlRootMissing => "élément racine absent",
            Self::XmlRootNotClosed => "élément racine non refermé",
            Self::XmlRootClosedTwice => "élément racine refermé deux fois",
            Self::XmlPropertyShape => "élément de réglage qui n'est pas une balise auto-fermante",
            Self::XmlPropertyOutsideRoot => "élément de réglage hors de la racine",
            Self::XmlAttribute => "attribut absent, en double, mal formé ou non délimité",
            Self::XmlEntity => "référence d'entité inconnue ou invalide",
        }
    }
}

/// Fichier que le format ne sait pas découper : refusé entier (`SETTINGS_FILE_UNREADABLE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unreadable {
    pub reason: UnreadableReason,
    /// Numéro de ligne (à partir de 1) quand il est connu. Jamais le contenu de la ligne.
    pub line: Option<usize>,
}

impl Unreadable {
    pub(crate) fn new(reason: UnreadableReason, line: Option<usize>) -> Self {
        Self { reason, line }
    }
}

impl fmt::Display for Unreadable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.reason.as_str())?;
        if let Some(line) = self.line {
            write!(f, " (ligne {line})")?;
        }
        Ok(())
    }
}

impl std::error::Error for Unreadable {}

/// Pourquoi une clé ou une valeur ne peut pas être écrite. Cite UN caractère, jamais la valeur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// Caractère que le format ne sait pas écrire dans une valeur.
    Value(char),
    /// Clé vide ou qui contient le séparateur, une fin de ligne ou un caractère de contrôle.
    Key,
    /// Valeur de plus de 16 Kio, ou fichier qui dépasserait ses bornes (20 000 lignes, 2 Mio) : le fichier
    /// écrit ne serait plus relisible.
    TooLong,
}

/// Une entrée vue dans le fichier, dans l'ordre du fichier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryView<'a> {
    pub key: &'a str,
    pub value: &'a str,
    /// Rang de l'entrée parmi les entrées du fichier.
    pub index: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct EntryInfo {
    pub key: String,
    /// Valeur décodée (XML : entités résolues).
    pub value: String,
    /// Octets de la valeur DANS le morceau : `[start, end)`.
    pub start: usize,
    pub end: usize,
    /// Guillemet qui délimite la valeur (XML), ou `b'"'` pour un `kv_lines` entre guillemets, sinon 0.
    pub quote: u8,
}

#[derive(Debug, Clone)]
pub(crate) struct Piece {
    pub bytes: Vec<u8>,
    pub entry: Option<EntryInfo>,
    /// XML : ce morceau commence par la balise fermante de la racine.
    pub closing: bool,
}

impl Piece {
    pub(crate) fn other(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            entry: None,
            closing: false,
        }
    }
}

/// Un fichier découpé en morceaux. La concaténation des morceaux est le fichier.
#[derive(Debug, Clone)]
pub struct Doc {
    pub(crate) pieces: Vec<Piece>,
}

impl Doc {
    /// Les octets du fichier : la concaténation des morceaux.
    pub fn render(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.pieces.iter().map(|p| p.bytes.len()).sum());
        for piece in &self.pieces {
            out.extend_from_slice(&piece.bytes);
        }
        out
    }

    /// Taille du fichier rendu, en octets.
    pub fn byte_len(&self) -> usize {
        self.pieces.iter().map(|p| p.bytes.len()).sum()
    }

    pub fn entries(&self) -> Vec<EntryView<'_>> {
        self.pieces
            .iter()
            .filter_map(|p| p.entry.as_ref())
            .enumerate()
            .map(|(index, e)| EntryView {
                key: &e.key,
                value: &e.value,
                index,
            })
            .collect()
    }

    /// Valeur de la DERNIÈRE entrée de cette clé (hypothèse sur le comportement du jeu devant un doublon,
    /// non vérifiée ; `duplicate_keys` le signale).
    pub fn get(&self, key: &str) -> Option<&str> {
        self.pieces
            .iter()
            .filter_map(|p| p.entry.as_ref())
            .rev()
            .find(|e| e.key == key)
            .map(|e| e.value.as_str())
    }

    /// Nombre de clés qui apparaissent plus d'une fois.
    pub fn duplicate_keys(&self) -> usize {
        let mut seen = std::collections::BTreeMap::<&str, usize>::new();
        for entry in self.pieces.iter().filter_map(|p| p.entry.as_ref()) {
            *seen.entry(entry.key.as_str()).or_default() += 1;
        }
        seen.values().filter(|n| **n > 1).count()
    }

    /// Décalage, dans le fichier, de la valeur de la dernière entrée de cette clé : `[début, fin)`.
    pub fn value_range(&self, key: &str) -> Option<(usize, usize)> {
        let mut offset = 0usize;
        let mut found = None;
        for piece in &self.pieces {
            if let Some(entry) = &piece.entry
                && entry.key == key
            {
                found = Some((offset + entry.start, offset + entry.end));
            }
            offset += piece.bytes.len();
        }
        found
    }
}

/// Interface commune. Chaque format est du code de l'agent, une énumération fermée.
pub trait ConfigFormat {
    fn parse(&self, bytes: &[u8]) -> Result<Doc, Unreadable>;
    /// Retouche la valeur de la dernière entrée de `key`, ou ajoute une ligne en fin de fichier.
    fn set(&self, doc: &mut Doc, key: &str, value: &str) -> Result<(), Refused>;
    /// Retire les entrées de `key`, rien d'autre.
    fn unset(&self, doc: &mut Doc, key: &str);
    /// Un fichier neuf (il n'existe pas) : seulement quand rien n'est à préserver.
    fn create(&self, pairs: &[(String, String)]) -> Result<Vec<u8>, Refused>;
}

/// Le format d'un canal `file` de la fiche.
pub fn format_for(channel: &Channel) -> Option<Box<dyn ConfigFormat>> {
    if channel.kind != ChannelKind::File {
        return None;
    }
    match channel.format? {
        FileFormat::KvLines => {
            let kv = channel.kv.as_ref()?;
            let separator = u8::try_from(kv.separator.chars().next()?).ok()?;
            let comment = u8::try_from(kv.comment.chars().next()?).ok()?;
            Some(Box::new(kv_lines::KvLines::new(
                separator,
                comment,
                kv.quote == Quote::Double,
            )))
        }
        FileFormat::XmlProperties => {
            let xml = channel.xml.as_ref()?;
            Some(Box::new(xml_properties::XmlProperties::new(
                &xml.root,
                &xml.element,
                &xml.key_attr,
                &xml.value_attr,
            )))
        }
    }
}

/// Bornes communes, avant toute allocation proportionnelle au contenu.
pub(crate) fn check_input(bytes: &[u8]) -> Result<(), Unreadable> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(Unreadable::new(UnreadableReason::TooLarge, None));
    }
    if bytes.contains(&0) {
        let line = bytes
            .iter()
            .take_while(|b| **b != 0)
            .filter(|b| **b == b'\n')
            .count()
            + 1;
        return Err(Unreadable::new(UnreadableReason::NulByte, Some(line)));
    }
    Ok(())
}

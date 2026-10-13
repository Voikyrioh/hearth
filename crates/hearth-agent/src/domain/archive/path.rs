//! Noms d'entrées : jugement d'un nom brut (ADR-0055 points 6, 8 et 9) et registre des noms déjà vus
//! (doublons, collisions de casse et de forme Unicode, fichier contre dossier).

use std::collections::HashMap;

use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

use super::limits::ArchiveLimits;
use super::refusal::ArchiveRefusal;

/// Chemin relatif jugé : UTF-8 valide, `/` pour seul séparateur, aucun composant vide, `.` ni `..`,
/// aucun caractère de contrôle ni de format (`is_forbidden_char`), aucun nom réservé de Windows, au plus `max_depth` niveaux.
/// Seul `judge` en fabrique : posséder un `ArchivePath`, c'est que le nom a passé tous les refus.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ArchivePath(String);

impl ArchivePath {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn components(&self) -> impl Iterator<Item = &str> {
        self.0.split('/')
    }

    pub fn depth(&self) -> usize {
        self.components().count()
    }
}

/// Un nom brut jugé : le chemin et, si le nom finissait par `/`, l'indication « dossier ».
pub(super) struct JudgedName {
    pub path: ArchivePath,
    pub trailing_slash: bool,
}

/// Caractères de FORMAT d'Unicode (catégorie générale `Cf`, Unicode 16.0) : ils ne s'impriment pas mais
/// changent l'affichage (inversion bidirectionnelle, largeur nulle, marques de langue) et permettent de
/// faire passer un nom pour un autre. Le standard de Rust n'expose pas les catégories : la table est
/// écrite ici, testée caractère par caractère, et à relire à chaque montée d'Unicode.
fn is_format_char(c: char) -> bool {
    matches!(
        u32::from(c),
        0x00AD
            | 0x0600..=0x0605
            | 0x061C
            | 0x06DD
            | 0x070F
            | 0x0890..=0x0891
            | 0x08E2
            | 0x180E
            | 0x200B..=0x200F
            | 0x202A..=0x202E
            | 0x2060..=0x2064
            | 0x2066..=0x206F
            | 0xFEFF
            | 0xFFF9..=0xFFFB
            | 0x110BD
            | 0x110CD
            | 0x1_3430..=0x1_343F
            | 0x1_BCA0..=0x1_BCA3
            | 0x1_D173..=0x1_D17A
            | 0xE0001
            | 0xE0020..=0xE007F
    )
}

/// Un nom ne contient AUCUN caractère de catégorie `Cc` (contrôles C0, DEL et C1), `Cf` (format),
/// `Zl` (U+2028) ni `Zp` (U+2029) : voir `BR-ARCH-003`.
pub(super) fn is_forbidden_char(c: char) -> bool {
    c.is_control() || is_format_char(c) || matches!(c, '\u{2028}' | '\u{2029}')
}

const RESERVED_DEVICES: [&str; 6] = ["con", "prn", "aux", "nul", "conin$", "conout$"];

fn is_reserved_device(component: &str) -> bool {
    let stem = component
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end_matches(' ')
        .to_lowercase();
    if RESERVED_DEVICES.contains(&stem.as_str()) {
        return true;
    }
    let bytes = stem.as_bytes();
    bytes.len() == 4
        && (stem.starts_with("com") || stem.starts_with("lpt"))
        && matches!(bytes.get(3), Some(b'1'..=b'9'))
}

/// Juge un nom brut tel que lu dans l'archive. Aucun encodage n'est deviné.
pub(super) fn judge(raw: &[u8], limits: &ArchiveLimits) -> Result<JudgedName, ArchiveRefusal> {
    if raw.is_empty() {
        return Err(ArchiveRefusal::EmptyComponent);
    }
    if raw.len() > limits.max_name_bytes {
        return Err(ArchiveRefusal::NameTooLong {
            max: limits.max_name_bytes,
        });
    }
    if raw.contains(&0) {
        return Err(ArchiveRefusal::ControlCharacter);
    }
    let name = std::str::from_utf8(raw).map_err(|_| ArchiveRefusal::NonUtf8Name)?;
    if name.chars().any(is_forbidden_char) {
        return Err(ArchiveRefusal::ControlCharacter);
    }
    if name.contains('\\') {
        return Err(ArchiveRefusal::Backslash);
    }
    if name.starts_with('/') {
        return Err(ArchiveRefusal::AbsolutePath);
    }
    if name.contains(':') {
        return Err(ArchiveRefusal::DriveLetter);
    }
    let (body, trailing_slash) = match name.strip_suffix('/') {
        Some(body) => (body, true),
        None => (name, false),
    };
    let mut depth = 0usize;
    for component in body.split('/') {
        if component.is_empty() {
            return Err(ArchiveRefusal::EmptyComponent);
        }
        if component == "." || component == ".." {
            return Err(ArchiveRefusal::ParentComponent);
        }
        if component.len() > limits.max_component_bytes {
            return Err(ArchiveRefusal::NameTooLong {
                max: limits.max_component_bytes,
            });
        }
        if component.ends_with('.') || component.ends_with(' ') || is_reserved_device(component) {
            return Err(ArchiveRefusal::ReservedName);
        }
        depth += 1;
        if depth > limits.max_depth {
            return Err(ArchiveRefusal::TooDeep {
                max: limits.max_depth,
            });
        }
    }
    Ok(JudgedName {
        path: ArchivePath(body.to_string()),
        trailing_slash,
    })
}

/// Clé de comparaison : forme NFC, minuscules, forme NFC de nouveau. Deux noms de même clé désignent
/// le même fichier sur un système qui ne distingue pas la casse ou la forme Unicode.
fn fold(name: &str) -> String {
    let composed: String = name.nfc().collect();
    composed.to_lowercase().nfc().collect()
}

type Key = [u8; 16];

fn key_of(text: &str) -> Key {
    let digest = Sha256::digest(text.as_bytes());
    let mut key = [0u8; 16];
    key.copy_from_slice(&digest[..16]);
    key
}

fn exact_of(text: &str) -> u64 {
    let digest = Sha256::digest(text.as_bytes());
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest[16..24]);
    u64::from_le_bytes(bytes)
}

struct Node {
    is_dir: bool,
    /// Le dossier a-t-il sa propre entrée (et pas seulement un enfant qui l'implique) ?
    explicit: bool,
    exact: u64,
}

/// Les noms déjà vus. Les clés sont des empreintes de 16 octets : la mémoire ne dépend pas de la
/// longueur des noms, et le nombre de noeuds est borné.
pub(super) struct NameRegistry {
    nodes: HashMap<Key, Node>,
    max_nodes: usize,
}

impl NameRegistry {
    pub(super) fn new(max_nodes: usize) -> Self {
        Self {
            nodes: HashMap::new(),
            max_nodes,
        }
    }

    /// Enregistre un chemin (et les dossiers qu'il implique). Refuse un doublon exact, deux noms qui
    /// ne diffèrent que par la casse ou la forme Unicode, un fichier et un dossier du même nom, un
    /// fichier dont un préfixe est un fichier.
    pub(super) fn add(&mut self, path: &ArchivePath, is_dir: bool) -> Result<(), ArchiveRefusal> {
        let text = path.as_str();
        let mut end = 0usize;
        let components: Vec<&str> = text.split('/').collect();
        for (index, component) in components.iter().enumerate() {
            end += component.len() + usize::from(index > 0);
            let prefix = &text[..end];
            let last = index + 1 == components.len();
            let key = key_of(&fold(prefix));
            let exact = exact_of(prefix);
            let wants_dir = !last || is_dir;
            match self.nodes.get_mut(&key) {
                Some(node) => {
                    if node.exact != exact {
                        return Err(ArchiveRefusal::NameCollision);
                    }
                    if node.is_dir != wants_dir {
                        return Err(ArchiveRefusal::FileDirConflict);
                    }
                    if last {
                        if node.explicit || !node.is_dir {
                            return Err(ArchiveRefusal::DuplicateName);
                        }
                        node.explicit = true;
                    }
                }
                None => {
                    if self.nodes.len() >= self.max_nodes {
                        return Err(ArchiveRefusal::TooManyEntries {
                            max: self.max_nodes,
                        });
                    }
                    self.nodes.insert(
                        key,
                        Node {
                            is_dir: wants_dir,
                            explicit: last,
                            exact,
                        },
                    );
                }
            }
        }
        Ok(())
    }
}

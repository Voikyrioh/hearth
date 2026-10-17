//! `RelPath` : un chemin relatif à une racine, jugé AVANT tout appel au système (`BR-FILES-003` d et a,
//! ADR-0046 point 2).

use std::fmt;

/// Longueur maximale d'un composant, en octets (limite des systèmes de fichiers courants).
pub const MAX_COMPONENT_BYTES: usize = 255;
/// Longueur maximale du chemin entier, en octets.
pub const MAX_PATH_BYTES: usize = 4096;
/// Profondeur maximale (nombre de composants).
pub const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelPathError {
    Empty,
    Absolute,
    NulByte,
    EmptyComponent,
    DotComponent,
    DotDotComponent,
    ComponentTooLong,
    PathTooLong,
    TooDeep,
}

impl fmt::Display for RelPathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Jamais le chemin : il vient du client.
        f.write_str(match self {
            Self::Empty => "empty path",
            Self::Absolute => "absolute path",
            Self::NulByte => "nul byte in path",
            Self::EmptyComponent => "empty path component",
            Self::DotComponent => "`.` path component",
            Self::DotDotComponent => "`..` path component",
            Self::ComponentTooLong => "path component too long",
            Self::PathTooLong => "path too long",
            Self::TooDeep => "path too deep",
        })
    }
}

impl std::error::Error for RelPathError {}

/// Composants validés, séparés par `/` : ni vide, ni absolu, ni `.` ni `..`, sans octet nul, bornés.
/// `RelPath::root()` désigne la racine elle-même (zéro composant) : seule `parse` refuse le vide.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RelPath(Vec<String>);

impl RelPath {
    /// La racine elle-même.
    pub fn root() -> Self {
        Self(Vec::new())
    }

    pub fn parse(text: &str) -> Result<Self, RelPathError> {
        if text.is_empty() {
            return Err(RelPathError::Empty);
        }
        if text.contains('\0') {
            return Err(RelPathError::NulByte);
        }
        if text.starts_with('/') {
            return Err(RelPathError::Absolute);
        }
        if text.len() > MAX_PATH_BYTES {
            return Err(RelPathError::PathTooLong);
        }
        let mut parts = Vec::new();
        for part in text.split('/') {
            Self::check_component(part)?;
            parts.push(part.to_owned());
        }
        if parts.len() > MAX_DEPTH {
            return Err(RelPathError::TooDeep);
        }
        Ok(Self(parts))
    }

    /// Un seul nom, sans séparateur.
    pub fn check_component(part: &str) -> Result<(), RelPathError> {
        match part {
            "" => Err(RelPathError::EmptyComponent),
            "." => Err(RelPathError::DotComponent),
            ".." => Err(RelPathError::DotDotComponent),
            _ if part.contains('\0') => Err(RelPathError::NulByte),
            // Un composant qui contient un séparateur de l'hôte ne désigne plus un seul niveau.
            _ if part.contains('/') => Err(RelPathError::Absolute),
            _ if part.len() > MAX_COMPONENT_BYTES => Err(RelPathError::ComponentTooLong),
            _ => Ok(()),
        }
    }

    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    pub fn components(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(String::as_str)
    }

    pub fn depth(&self) -> usize {
        self.0.len()
    }

    /// Le dernier composant et les dossiers qui le précèdent ; `None` pour la racine.
    pub fn split_last(&self) -> Option<(&str, &[String])> {
        self.0
            .split_last()
            .map(|(last, rest)| (last.as_str(), rest))
    }
}

impl fmt::Display for RelPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.join("/"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_paths_are_accepted() {
        let path = RelPath::parse("world/saves/a.json").unwrap();
        assert_eq!(path.depth(), 3);
        assert_eq!(path.to_string(), "world/saves/a.json");
        assert!(!path.is_root());
        assert!(RelPath::root().is_root());
    }

    #[test]
    fn every_bad_shape_is_refused() {
        let cases = [
            ("", RelPathError::Empty),
            ("/etc/passwd", RelPathError::Absolute),
            ("a/../b", RelPathError::DotDotComponent),
            ("..", RelPathError::DotDotComponent),
            ("a/./b", RelPathError::DotComponent),
            (".", RelPathError::DotComponent),
            ("a//b", RelPathError::EmptyComponent),
            ("a/", RelPathError::EmptyComponent),
            ("a\0b", RelPathError::NulByte),
            ("a/b\0", RelPathError::NulByte),
        ];
        for (text, expected) in cases {
            assert_eq!(RelPath::parse(text), Err(expected), "{text:?}");
        }
    }

    #[test]
    fn bounds_are_enforced() {
        let long = "a".repeat(MAX_COMPONENT_BYTES + 1);
        assert_eq!(RelPath::parse(&long), Err(RelPathError::ComponentTooLong));
        assert!(RelPath::parse(&"a".repeat(MAX_COMPONENT_BYTES)).is_ok());
        let deep = vec!["a"; MAX_DEPTH + 1].join("/");
        assert_eq!(RelPath::parse(&deep), Err(RelPathError::TooDeep));
        let wide = vec!["a".repeat(200); 21].join("/");
        assert_eq!(RelPath::parse(&wide), Err(RelPathError::PathTooLong));
    }

    #[test]
    fn a_dot_dot_inside_a_longer_name_is_just_a_name() {
        assert!(RelPath::parse("..a/b..").is_ok());
        assert!(RelPath::parse(".hidden").is_ok());
    }

    #[test]
    fn a_component_cannot_smuggle_a_separator() {
        assert!(RelPath::check_component("a/b").is_err());
        assert!(RelPath::check_component("ok").is_ok());
    }
}

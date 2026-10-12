//! L'identité stable d'un service (ADR-0037).
//!
//! Opaque pour le client, calculée par l'agent, indépendante de l'identifiant du conteneur quand
//! un service est recréé. Trois formes :
//! - `h_<ULID>` : service créé par Hearth (tranche 5) ;
//! - `u_<unité>` : service supervisé par une unité systemd (tranche 2) ;
//! - `c_<identifiant du conteneur>` : tout autre conteneur.
//!
//! Cette tranche ne PRODUIT que `c_` ; `h_` et `u_` sont seulement analysés (une route future les
//! recevra de l'extérieur).

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyKind {
    Hearth,
    Unit,
    Container,
}

#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ServiceKey(String);

/// Longueur maximale du corps d'une clé (identifiant ou nom d'unité).
const MAX_BODY: usize = 256;

impl ServiceKey {
    /// La clé d'un conteneur que personne d'autre ne désigne : `c_<identifiant complet>`.
    pub fn for_container(container_id: &str) -> Option<Self> {
        Self::parse(&format!("c_{container_id}"))
    }

    /// Lit une clé reçue de l'extérieur ; `None` si elle n'a aucune des trois formes.
    pub fn parse(text: &str) -> Option<Self> {
        let (prefix, body) = text.split_once('_')?;
        if body.is_empty() || body.len() > MAX_BODY {
            return None;
        }
        let valid = match prefix {
            "c" => body.chars().all(|c| c.is_ascii_alphanumeric()),
            "h" => body.len() == 26 && body.chars().all(is_ulid_char),
            // Les caractères permis dans un nom d'unité systemd (`systemd.unit(5)`).
            "u" => body.chars().all(|c| {
                c.is_ascii_alphanumeric() || matches!(c, ':' | '-' | '_' | '.' | '\\' | '@')
            }),
            _ => false,
        };
        valid.then(|| Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn kind(&self) -> KeyKind {
        match self.0.as_bytes().first() {
            Some(b'h') => KeyKind::Hearth,
            Some(b'u') => KeyKind::Unit,
            _ => KeyKind::Container,
        }
    }
}

/// Alphabet de Crockford, sans I, L, O, U.
fn is_ulid_char(c: char) -> bool {
    c.is_ascii_digit() || (c.is_ascii_uppercase() && !matches!(c, 'I' | 'L' | 'O' | 'U'))
}

impl fmt::Debug for ServiceKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ServiceKey({})", self.0)
    }
}

impl fmt::Display for ServiceKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_container_key_is_c_then_the_full_id() {
        let id = "ab12".repeat(16);
        let key = ServiceKey::for_container(&id).expect("key");
        assert_eq!(key.as_str(), format!("c_{id}"));
        assert_eq!(key.kind(), KeyKind::Container);
    }

    #[test]
    fn the_three_forms_parse_and_anything_else_does_not() {
        assert_eq!(
            ServiceKey::parse("h_01J9ZY0G3Q8M2K6W4T7V5N1B9D").map(|k| k.kind()),
            Some(KeyKind::Hearth)
        );
        assert_eq!(
            ServiceKey::parse("u_docker-terraria.service").map(|k| k.kind()),
            Some(KeyKind::Unit)
        );
        assert_eq!(
            ServiceKey::parse("c_0123abcd").map(|k| k.kind()),
            Some(KeyKind::Container)
        );
        for bad in [
            "",
            "c_",
            "x_abc",
            "h_short",
            "h_01J9ZY0G3Q8M2K6W4T7V5N1B9U",
            "c_a b",
            "c_../etc",
            "u_a;rm",
            "u_a b",
            "nopre",
            "c_é",
        ] {
            assert!(ServiceKey::parse(bad).is_none(), "{bad:?}");
        }
        assert!(ServiceKey::parse(&format!("c_{}", "a".repeat(MAX_BODY + 1))).is_none());
    }
}

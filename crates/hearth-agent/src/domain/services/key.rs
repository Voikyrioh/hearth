//! L'identité stable d'un service (ADR-0037).
//!
//! Opaque pour le client, calculée par l'agent, indépendante de l'identifiant du conteneur quand
//! un service est recréé. Trois formes :
//! - `h_<ULID>` : service créé par Hearth (tranche 5) ;
//! - `u_<unité>` : service supervisé par une unité systemd (tranche 2, HRT-63) ; le nom de l'unité
//!   est celui de systemd, suffixe `.service` compris (`u_docker-terraria.service`) ;
//! - `c_<identifiant du conteneur>` : tout autre conteneur.
//!
//! Les tranches 1 et 2 PRODUISENT `c_` et `u_` ; `h_` est seulement analysé (tranche 5).

use std::fmt;

use super::unit::UnitName;

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

    /// La clé d'un service créé par Hearth : `h_<ULID>` (HRT-68). `None` si l'identifiant n'est pas un ULID.
    pub fn for_hearth(ulid: &str) -> Option<Self> {
        Self::parse(&format!("h_{ulid}"))
    }

    /// La clé d'un service supervisé : `u_<unité>`. Le nom d'unité est déjà validé ; la clé ne dépend
    /// pas du conteneur, que le système recrée à chaque démarrage (BR-SVC-016).
    pub fn for_unit(unit: &UnitName) -> Self {
        Self(format!("u_{unit}"))
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

    /// L'unité d'une clé `u_<unité>` ; `None` pour toute autre forme.
    pub fn unit(&self) -> Option<UnitName> {
        self.0.strip_prefix("u_").and_then(UnitName::parse)
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
    fn a_supervised_service_key_is_u_then_the_unit_and_does_not_depend_on_the_container() {
        let unit = UnitName::parse("docker-x.service").expect("unit");
        let key = ServiceKey::for_unit(&unit);
        assert_eq!(key.as_str(), "u_docker-x.service");
        assert_eq!(key.kind(), KeyKind::Unit);
        assert_eq!(ServiceKey::parse(key.as_str()), Some(key));
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

//! Valeur secrète (mot de passe, hachage) : jamais affichée, effacée de la mémoire à la libération.
//!
//! Pas de `Display`, pas de `Clone`, pas de `PartialEq` : la lecture passe par `expose`, qui
//! rend l'usage explicite et facile à repérer en revue (BR-ACCT-006).

use std::fmt;

use zeroize::Zeroize;

pub struct Secret(String);

impl Secret {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    /// Donne accès à la valeur en clair. À n'utiliser que pour hacher ou vérifier.
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<String> for Secret {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for Secret {
    fn from(value: &str) -> Self {
        Self::new(value.to_owned())
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_reveals_the_value() {
        let secret = Secret::from("Tr0ub4dor-horse-3");
        assert_eq!(format!("{secret:?}"), "Secret(***)");
        assert_eq!(format!("{secret:#?}"), "Secret(***)");
    }

    #[test]
    fn expose_gives_the_value_back() {
        assert_eq!(Secret::from("abc").expose(), "abc");
    }

    #[test]
    fn a_secret_inside_a_struct_stays_hidden() {
        #[derive(Debug)]
        #[allow(dead_code)]
        struct Holder {
            password: Secret,
        }
        let holder = Holder {
            password: Secret::from("hunter2-Hunter2"),
        };
        assert!(!format!("{holder:?}").contains("hunter2"));
    }

    #[test]
    fn zeroize_clears_the_buffer() {
        let mut value = String::from("secret-value");
        value.zeroize();
        assert!(value.is_empty());
    }
}

//! Texte secret (mot de passe, jeton) : effacé de la mémoire à la libération, jamais affiché.

use std::fmt;

use zeroize::Zeroize;

/// Secret en mémoire. `Debug` ne montre rien ; `expose` est le seul accès au contenu et ne doit
/// servir qu'à l'envoyer au serveur ou au coffre.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<&str> for Secret {
    fn from(value: &str) -> Self {
        Self::new(value)
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
    fn debug_never_shows_the_content() {
        let secret = Secret::new("Correct-Horse-9");
        assert!(!format!("{secret:?}").contains("Correct"));
        assert!(!format!("{:?}", Some(&secret)).contains("Correct"));
    }

    #[test]
    fn expose_gives_the_content_back() {
        assert_eq!(Secret::from("abc").expose(), "abc");
        assert!(Secret::new("").is_empty());
    }
}

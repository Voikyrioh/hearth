//! Une valeur secrète lue de l'environnement (mot de passe ou clé de signature de l'agent, HRT-173) :
//! son `Debug` ne l'écrit jamais, et elle n'est lue que par `expose`.

pub struct Secret(String);

impl Secret {
    #[cfg(test)]
    pub fn new(value: String) -> Self {
        Self(value)
    }

    /// Lit une variable d'environnement ; absente, c'est un secret vide.
    pub fn from_env(name: &str) -> Self {
        Self(std::env::var(name).unwrap_or_default())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_blank(&self) -> bool {
        self.0.trim().is_empty()
    }
}

/// La valeur est effacée de la mémoire quand le secret disparaît.
impl Drop for Secret {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.0);
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(<masqué>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_shows_the_value() {
        let secret = Secret::new("valeur-sentinelle-42".to_owned());
        let text = format!("{secret:?} {:?}", Some(&secret));
        assert!(!text.contains("sentinelle"), "{text}");
        assert!(text.contains("masqué"));
    }

    #[test]
    fn a_blank_or_absent_secret_is_blank() {
        assert!(Secret::new("  \n".to_owned()).is_blank());
        assert!(Secret::from_env("HEARTH_VARIABLE_QUI_N_EXISTE_PAS").is_blank());
        assert!(!Secret::new("x".to_owned()).is_blank());
    }
}

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

/// Efface les textes d'un corps JSON (un mot de passe d'action) : à appeler quand la requête qui le
/// porte est libérée.
pub(crate) fn wipe_body(body: &mut Option<serde_json::Value>) {
    fn wipe_value(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::String(text) => text.zeroize(),
            serde_json::Value::Array(items) => items.iter_mut().for_each(wipe_value),
            serde_json::Value::Object(map) => map.values_mut().for_each(wipe_value),
            _ => {}
        }
    }
    if let Some(value) = body {
        wipe_value(value);
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

    #[test]
    fn a_request_body_is_wiped_in_every_text_however_deep() {
        let mut body = Some(serde_json::json!({
            "current": "Ancien-1",
            "list": ["a", { "deep": "b" }],
            "reauth": { "password": "Correct-Horse-9", "device": { "signature": "sig" } },
            "keep_address": true,
            "n": 3
        }));
        wipe_body(&mut body);
        // Chaque texte est vide, les clés, les booléens et les nombres sont intacts.
        assert_eq!(
            body.unwrap(),
            serde_json::json!({
                "current": "",
                "list": ["", { "deep": "" }],
                "reauth": { "password": "", "device": { "signature": "" } },
                "keep_address": true,
                "n": 3
            })
        );
        let mut none: Option<serde_json::Value> = None;
        wipe_body(&mut none);
    }
}

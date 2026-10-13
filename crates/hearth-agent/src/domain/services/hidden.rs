//! Un texte qui ne s'affiche jamais : nom d'une variable d'environnement, argument de la commande de
//! lancement d'un service.
//!
//! Ces textes sont écrits par n'importe qui sur le serveur et peuvent contenir un secret
//! (`--token=...`). Le type n'a ni `Display`, ni conversion silencieuse vers un texte : `Debug` ne
//! montre rien, et la lecture passe par [`Hidden::expose`], facile à repérer en revue. Il n'entre dans
//! aucune trace, aucun message d'erreur, aucune entrée du journal d'activité ni la base (ADR-0037).
//! Les VALEURS des variables, elles, sont des `domain::secret::Secret` (effacées à la libération).

use std::fmt;

#[derive(Clone, PartialEq, Eq)]
pub struct Hidden(String);

impl Hidden {
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    /// Le texte en clair. À n'utiliser que pour le mettre sur le fil.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Hidden {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Hidden(***)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_shows_the_text() {
        let hidden = Hidden::new("--token=SENTINEL");
        assert_eq!(format!("{hidden:?}"), "Hidden(***)");
        assert_eq!(format!("{hidden:#?}"), "Hidden(***)");
        assert_eq!(format!("{:?}", vec![hidden.clone()]), "[Hidden(***)]");
        assert_eq!(hidden.expose(), "--token=SENTINEL");
    }
}

//! Qui voit une opération (HRT-144, ADR-0045, BR-ADOPT-004, BR-ADOPT-015, BR-GRANT-003).
//!
//! Une opération n'est lisible que par qui peut voir le SERVICE qu'elle touche, et une opération sur un
//! service qu'on ne voit pas a EXACTEMENT la réponse d'une opération inconnue (`404 OPERATION_NOT_FOUND`) :
//! rien ne dit qu'elle existe. Même règle pour la lecture, le message `operation` du flux, la décision et
//! l'annulation. Un seul point de décision : [`may_see`].
//!
//! - un service créé par Hearth (`h_`) : tous les comptes authentifiés (ses fiches sont publiées à tous,
//!   BR-ADOPT-016) ;
//! - un service adopté : seulement le droit « gérer les services » ;
//! - tout autre service (conteneur non adopté, adoption relâchée depuis) : personne.

use crate::domain::services::key::{KeyKind, ServiceKey};

/// Ce que l'agent sait du service d'une opération au moment de juger : l'origine que lui donne le filtre
/// de publication (`visibility_of`), jamais un nom ni un état.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceOrigin {
    /// La clé est celle d'un service créé par Hearth.
    Hearth,
    /// Un service adopté.
    Adopted,
    /// Ni l'un ni l'autre : jamais publié.
    Hidden,
}

/// L'origine d'après la clé et le fait que le service soit adopté à cet instant.
pub fn origin_of(key: &ServiceKey, adopted: bool) -> ServiceOrigin {
    if key.kind() == KeyKind::Hearth {
        ServiceOrigin::Hearth
    } else if adopted {
        ServiceOrigin::Adopted
    } else {
        ServiceOrigin::Hidden
    }
}

/// Ce compte voit-il l'opération de ce service ?
pub fn may_see(origin: ServiceOrigin, may_manage_services: bool) -> bool {
    match origin {
        ServiceOrigin::Hearth => true,
        ServiceOrigin::Adopted => may_manage_services,
        ServiceOrigin::Hidden => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(text: &str) -> ServiceKey {
        ServiceKey::parse(text).unwrap()
    }

    #[test]
    fn who_sees_what_hearth_to_all_adopted_to_the_right_hidden_to_nobody() {
        for (origin, right, seen) in [
            (ServiceOrigin::Hearth, false, true),
            (ServiceOrigin::Hearth, true, true),
            (ServiceOrigin::Adopted, false, false),
            (ServiceOrigin::Adopted, true, true),
            (ServiceOrigin::Hidden, false, false),
            (ServiceOrigin::Hidden, true, false),
        ] {
            assert_eq!(may_see(origin, right), seen, "{origin:?} droit={right}");
        }
    }

    #[test]
    fn the_origin_comes_from_the_key_and_the_adoption_and_a_hearth_key_wins() {
        let hearth = key("h_01J9ZY0G3Q8M2K6W4T7V5N1B9D");
        assert_eq!(origin_of(&hearth, false), ServiceOrigin::Hearth);
        assert_eq!(origin_of(&hearth, true), ServiceOrigin::Hearth);
        for text in ["c_0123abcd", "u_docker-x.service"] {
            assert_eq!(origin_of(&key(text), true), ServiceOrigin::Adopted);
            assert_eq!(origin_of(&key(text), false), ServiceOrigin::Hidden);
        }
    }
}

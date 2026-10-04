//! Suppression de son propre compte : confirmation par l'identifiant (BR-ACCT-012).

use thiserror::Error;

use super::username::Username;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("L'identifiant ne correspond pas, réessaye")]
pub struct ConfirmationMismatch;

/// L'administrateur qui supprime son propre compte doit retaper son identifiant.
/// La comparaison ignore la casse et les espaces autour, comme l'identifiant lui-même.
pub fn confirm_self_deletion(target: &Username, typed: &str) -> Result<(), ConfirmationMismatch> {
    if typed.trim().eq_ignore_ascii_case(target.as_str()) {
        Ok(())
    } else {
        Err(ConfirmationMismatch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marie() -> Username {
        Username::parse("marie").unwrap()
    }

    #[test]
    fn the_exact_username_confirms() {
        assert_eq!(confirm_self_deletion(&marie(), "marie"), Ok(()));
    }

    #[test]
    fn case_and_surrounding_spaces_are_ignored() {
        assert_eq!(confirm_self_deletion(&marie(), " MARIE "), Ok(()));
    }

    #[test]
    fn another_text_is_refused() {
        assert_eq!(
            confirm_self_deletion(&marie(), "marie2"),
            Err(ConfirmationMismatch)
        );
        assert_eq!(
            confirm_self_deletion(&marie(), ""),
            Err(ConfirmationMismatch)
        );
        assert_eq!(
            confirm_self_deletion(&marie(), "mar ie"),
            Err(ConfirmationMismatch)
        );
    }
}

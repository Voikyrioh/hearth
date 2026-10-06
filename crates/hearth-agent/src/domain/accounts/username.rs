//! Identifiant d'un compte (BR-ACCT-002 et BR-ACCT-003).

use std::fmt;

use hearth_proto::account_rules::{USERNAME_MAX_LEN, USERNAME_MIN_LEN, check_username};

pub use hearth_proto::account_rules::UsernameProblem as UsernameError;

pub const MIN_LEN: usize = USERNAME_MIN_LEN;
pub const MAX_LEN: usize = USERNAME_MAX_LEN;

/// Identifiant normalisé : minuscules, chiffres, tiret, underscore, 3 à 32 caractères.
///
/// La saisie est ramenée en minuscules avant contrôle, ce qui rend l'unicité insensible à la
/// casse (« MARIE » et « marie » sont le même identifiant). Les règles vivent dans
/// `hearth-proto` (une seule source, partagée avec le client).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Username(String);

impl Username {
    /// BR-ACCT-002 : contrôle du format. BR-ACCT-003 : normalisation en minuscules.
    pub fn parse(raw: &str) -> Result<Self, UsernameError> {
        check_username(raw).map(Self)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Username {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_letters_digits_dash_and_underscore() {
        for ok in [
            "abc",
            "marie",
            "jean-paul",
            "jean_paul",
            "user42",
            "a-b",
            "_-_",
        ] {
            assert_eq!(
                Username::parse(ok).map(|u| u.to_string()),
                Ok(ok.to_owned())
            );
        }
    }

    #[test]
    fn length_limits_are_inclusive() {
        assert!(Username::parse(&"a".repeat(3)).is_ok());
        assert!(Username::parse(&"a".repeat(32)).is_ok());
        assert_eq!(Username::parse("ab"), Err(UsernameError::TooShort));
        assert_eq!(Username::parse("a"), Err(UsernameError::TooShort));
        assert_eq!(
            Username::parse(&"a".repeat(33)),
            Err(UsernameError::TooLong)
        );
    }

    #[test]
    fn empty_is_required_not_too_short() {
        assert_eq!(Username::parse(""), Err(UsernameError::Empty));
    }

    #[test]
    fn spaces_and_special_characters_are_refused() {
        for bad in [
            "a b",
            " abc",
            "abc ",
            "ab.c",
            "ab@c",
            "ab/c",
            "é-é",
            "日本語",
            "a\tb",
            "ab\n",
        ] {
            assert_eq!(
                Username::parse(bad),
                Err(UsernameError::InvalidChars),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn invalid_characters_win_over_length() {
        assert_eq!(Username::parse("a b"), Err(UsernameError::InvalidChars));
        assert_eq!(Username::parse("é"), Err(UsernameError::InvalidChars));
    }

    #[test]
    fn uppercase_is_normalized_so_uniqueness_ignores_case() {
        let upper = Username::parse("MARIE").unwrap();
        let lower = Username::parse("marie").unwrap();
        assert_eq!(upper, lower);
        assert_eq!(upper.as_str(), "marie");
        assert_eq!(Username::parse("Ab"), Err(UsernameError::TooShort));
    }

    #[test]
    fn messages_are_the_ones_of_the_specification() {
        assert_eq!(UsernameError::Empty.to_string(), "L'identifiant est requis");
        assert_eq!(
            UsernameError::TooShort.to_string(),
            "L'identifiant doit contenir au moins 3 caractères"
        );
        assert_eq!(
            UsernameError::InvalidChars.to_string(),
            "L'identifiant contient des caractères non autorisés"
        );
    }
}

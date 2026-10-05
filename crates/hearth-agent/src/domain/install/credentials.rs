//! Le premier compte administrateur (BR-INSTALL-002, 009, 010) : mêmes règles que les comptes
//! (`domain::accounts`), avec les messages propres à l'installation.

use thiserror::Error;

use crate::domain::accounts::password::unmet_rules;
use crate::domain::accounts::{Username, UsernameError};

/// Texte d'aide affiché sous la demande du nom (BR-INSTALL-009).
pub const NAME_HELP: &str = "Utilise des lettres minuscules, des chiffres, des tirets ou des underscores. Minimum 3 caractères.";

/// Texte d'aide affiché sous la demande du mot de passe (BR-INSTALL-010).
pub const PASSWORD_HELP: &str =
    "Minimum 12 caractères, au moins 1 majuscule, 1 minuscule et 1 chiffre.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum AdminNameError {
    #[error("Le nom du compte est requis.")]
    Required,
    /// Format refusé : longueur ou caractères (le texte d'aide dit lesquels).
    #[error("Le nom du compte contient des caractères non autorisés.")]
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum AdminPasswordError {
    #[error("Le mot de passe est requis.")]
    Required,
    /// Une règle de complexité n'est pas respectée (le texte d'aide dit lesquelles).
    #[error("Le mot de passe est trop court.")]
    Weak,
}

/// BR-INSTALL-009 : l'identifiant du premier compte (3 à 32 caractères, minuscules, chiffres,
/// tirets, underscores). Même règle que BR-ACCT-002.
pub fn parse_admin_name(raw: &str) -> Result<Username, AdminNameError> {
    match Username::parse(raw) {
        Ok(name) => Ok(name),
        Err(UsernameError::Empty) => Err(AdminNameError::Required),
        Err(_) => Err(AdminNameError::Invalid),
    }
}

/// BR-INSTALL-010 : 12 caractères au moins, une majuscule, une minuscule, un chiffre, et pas
/// l'identifiant. Même règle que BR-ACCT-004 et BR-ACCT-005.
pub fn check_admin_password(password: &str, name: &Username) -> Result<(), AdminPasswordError> {
    if password.is_empty() {
        return Err(AdminPasswordError::Required);
    }
    if unmet_rules(password, name).is_empty() {
        Ok(())
    } else {
        Err(AdminPasswordError::Weak)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum HashFormatError {
    #[error(
        "HEARTH_ADMIN_PASSWORD_HASH n'est pas un haché Argon2id au format PHC (`$argon2id$v=19$m=…,t=…,p=…$sel$haché`)."
    )]
    NotArgon2id,
}

/// Forme d'un haché Argon2id au format PHC : `$argon2id$v=19$m=…,t=…,p=…$sel$haché`. Contrôle
/// de forme seulement ; l'adaptateur de hachage fait la lecture complète.
pub fn check_password_hash_format(hash: &str) -> Result<(), HashFormatError> {
    let parts: Vec<&str> = hash.split('$').collect();
    let well_formed = parts.len() == 6
        && parts[0].is_empty()
        && parts[1] == "argon2id"
        && parts[2].starts_with("v=")
        && parts[3].starts_with("m=")
        && parts[4..].iter().all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/')
        });
    if well_formed {
        Ok(())
    } else {
        Err(HashFormatError::NotArgon2id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_name_is_required_and_a_bad_one_is_refused() {
        assert_eq!(parse_admin_name(""), Err(AdminNameError::Required));
        for raw in ["ab", "Marie Dupont", "marie!", &"a".repeat(33), "é_é_é"] {
            assert_eq!(parse_admin_name(raw), Err(AdminNameError::Invalid), "{raw}");
        }
    }

    #[test]
    fn a_name_is_three_to_thirty_two_lowercase_digits_dash_underscore() {
        for raw in ["abc", "marie_d-2", &"a".repeat(32)] {
            assert!(parse_admin_name(raw).is_ok(), "{raw}");
        }
        assert_eq!(parse_admin_name("MARIE").unwrap().as_str(), "marie");
    }

    #[test]
    fn the_messages_are_the_ones_of_the_specification() {
        assert_eq!(
            AdminNameError::Required.to_string(),
            "Le nom du compte est requis."
        );
        assert_eq!(
            AdminNameError::Invalid.to_string(),
            "Le nom du compte contient des caractères non autorisés."
        );
        assert_eq!(
            AdminPasswordError::Required.to_string(),
            "Le mot de passe est requis."
        );
        assert_eq!(
            AdminPasswordError::Weak.to_string(),
            "Le mot de passe est trop court."
        );
    }

    #[test]
    fn a_password_needs_twelve_characters_upper_lower_digit_and_not_the_name() {
        let name = Username::parse("marie").unwrap();
        assert_eq!(
            check_admin_password("", &name),
            Err(AdminPasswordError::Required)
        );
        for weak in [
            "Court1a",
            "pasdemajuscule12",
            "PASDEMINUSCULE12",
            "PasDeChiffreIci",
            "Xx-marie-Xx-12345",
        ] {
            assert_eq!(
                check_admin_password(weak, &name),
                Err(AdminPasswordError::Weak),
                "{weak}"
            );
        }
        assert_eq!(check_admin_password("Cheval-Agrafe-42", &name), Ok(()));
        // Exactement douze caractères.
        assert_eq!(check_admin_password("Abcdefghij12", &name), Ok(()));
    }

    #[test]
    fn only_an_argon2id_phc_hash_is_accepted() {
        let good = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHQ$aGFzaGhhc2hoYXNoaGFzaA";
        assert_eq!(check_password_hash_format(good), Ok(()));
        for bad in [
            "",
            "Cheval-Agrafe-42",
            "$argon2i$v=19$m=19456,t=2,p=1$c29tZXNhbHQ$aGFzaA",
            "$2b$12$abcdefghijklmnopqrstuuABCDEFGHIJKLMNOPQRSTUVWXYZ01234",
            "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHQ",
            "$argon2id$v=19$m=19456,t=2,p=1$$aGFzaA",
            "$argon2id$v=19$m=19456,t=2,p=1$sel$ha sh",
        ] {
            assert!(check_password_hash_format(bad).is_err(), "{bad}");
        }
    }
}

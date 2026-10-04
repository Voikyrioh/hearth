//! Rôle d'un compte et ce qu'il permet (BR-ACCT-001, BR-ACCT-013, BR-ACCT-014).

use std::fmt;
use std::str::FromStr;

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    Admin,
    ReadOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("Le rôle doit être « admin » ou « readonly »")]
pub struct UnknownRole;

impl Role {
    /// Forme stockée en base et saisie sur la ligne de commande.
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::ReadOnly => "readonly",
        }
    }

    /// Libellé affiché dans les messages.
    pub fn label(self) -> &'static str {
        match self {
            Role::Admin => "Administrateur",
            Role::ReadOnly => "Lecture seule",
        }
    }

    /// BR-ACCT-013 et BR-ACCT-014 : seul un administrateur gère les comptes (création,
    /// suppression, rôle, mot de passe d'autrui, révocation). Les routes de HRT-04 s'appuient
    /// sur cette fonction : le contrôle est fait par l'agent, jamais par le client.
    pub fn can_manage_accounts(self) -> bool {
        matches!(self, Role::Admin)
    }
}

impl FromStr for Role {
    type Err = UnknownRole;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "admin" => Ok(Role::Admin),
            "readonly" => Ok(Role::ReadOnly),
            _ => Err(UnknownRole),
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_form_round_trips() {
        for role in [Role::Admin, Role::ReadOnly] {
            assert_eq!(role.as_str().parse::<Role>(), Ok(role));
        }
    }

    #[test]
    fn parsing_ignores_case_and_surrounding_spaces() {
        assert_eq!(" ADMIN ".parse::<Role>(), Ok(Role::Admin));
        assert_eq!("ReadOnly".parse::<Role>(), Ok(Role::ReadOnly));
    }

    #[test]
    fn unknown_role_is_refused() {
        assert_eq!("root".parse::<Role>(), Err(UnknownRole));
        assert_eq!("".parse::<Role>(), Err(UnknownRole));
    }

    #[test]
    fn labels_are_the_ones_of_the_specification() {
        assert_eq!(Role::Admin.to_string(), "Administrateur");
        assert_eq!(Role::ReadOnly.to_string(), "Lecture seule");
    }

    #[test]
    fn only_an_administrator_manages_accounts() {
        assert!(Role::Admin.can_manage_accounts());
        assert!(!Role::ReadOnly.can_manage_accounts());
    }
}

//! Saisie du mot de passe au terminal, sans écho, avec confirmation ; ou valeur fournie par la
//! variable d'environnement `HEARTH_ACCOUNT_PASSWORD` pour l'automatisation.

use super::account::{AccountCliError, PasswordInput};
use crate::domain::secret::Secret;

pub const ENV_ACCOUNT_PASSWORD: &str = "HEARTH_ACCOUNT_PASSWORD";

pub struct TerminalPasswords {
    from_env: Option<Secret>,
}

impl TerminalPasswords {
    /// `env` donne accès aux variables d'environnement (injectable pour les tests). Une valeur
    /// vide est ignorée : on retombe sur la saisie au terminal.
    pub fn from_env(env: &dyn Fn(&str) -> Option<String>) -> Self {
        Self {
            from_env: env(ENV_ACCOUNT_PASSWORD)
                .filter(|value| !value.is_empty())
                .map(Secret::new),
        }
    }
}

impl PasswordInput for TerminalPasswords {
    fn new_password(&self) -> Result<Secret, AccountCliError> {
        if let Some(secret) = &self.from_env {
            return Ok(Secret::from(secret.expose()));
        }
        let first = prompt("Mot de passe : ")?;
        let second = prompt("Confirme le mot de passe : ")?;
        confirmed(first, second)
    }
}

fn prompt(label: &str) -> Result<Secret, AccountCliError> {
    rpassword::prompt_password(label)
        .map(Secret::new)
        .map_err(|error| AccountCliError::PasswordInput(error.to_string()))
}

/// Les deux saisies doivent être identiques.
fn confirmed(first: Secret, second: Secret) -> Result<Secret, AccountCliError> {
    if first.expose() == second.expose() {
        Ok(first)
    } else {
        Err(AccountCliError::PasswordMismatch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_entries_are_confirmed() {
        let secret = confirmed(Secret::from("Abcdefghij12"), Secret::from("Abcdefghij12")).unwrap();
        assert_eq!(secret.expose(), "Abcdefghij12");
    }

    #[test]
    fn different_entries_are_refused_with_the_specified_message() {
        let error =
            confirmed(Secret::from("Abcdefghij12"), Secret::from("Abcdefghij13")).unwrap_err();
        assert_eq!(
            error.to_string(),
            "Les deux mots de passe ne correspondent pas"
        );
    }

    #[test]
    fn the_environment_variable_provides_the_password_without_a_terminal() {
        let env = |name: &str| (name == ENV_ACCOUNT_PASSWORD).then(|| "Abcdefghij12".to_owned());
        let passwords = TerminalPasswords::from_env(&env);
        assert_eq!(passwords.new_password().unwrap().expose(), "Abcdefghij12");
        assert_eq!(passwords.new_password().unwrap().expose(), "Abcdefghij12");
    }

    #[test]
    fn an_empty_variable_is_ignored() {
        let env = |_: &str| Some(String::new());
        assert!(TerminalPasswords::from_env(&env).from_env.is_none());
    }
}

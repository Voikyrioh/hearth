//! Règles du mot de passe d'un compte (BR-ACCT-004 et BR-ACCT-005).

use thiserror::Error;

use super::username::Username;
use crate::domain::secret::Secret;

pub const MIN_LEN: usize = 12;

/// Une règle de mot de passe non respectée. Le message est celui de la spécification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum PasswordRule {
    #[error("Le mot de passe est requis")]
    Required,
    #[error("Le mot de passe doit contenir au moins 12 caractères")]
    MinLength,
    #[error("Le mot de passe doit contenir au moins un chiffre")]
    Digit,
    #[error("Le mot de passe doit contenir au moins une minuscule")]
    Lowercase,
    #[error("Le mot de passe doit contenir au moins une majuscule")]
    Uppercase,
    #[error("Le mot de passe ne doit pas contenir l'identifiant")]
    ContainsUsername,
}

/// Liste des règles non respectées par `password` pour le compte `username`.
/// Vide si le mot de passe est conforme. Un mot de passe vide ne rend que `Required`.
pub fn unmet_rules(password: &str, username: &Username) -> Vec<PasswordRule> {
    if password.is_empty() {
        return vec![PasswordRule::Required];
    }
    let mut unmet = Vec::new();
    if password.chars().count() < MIN_LEN {
        unmet.push(PasswordRule::MinLength);
    }
    if !password.chars().any(|c| c.is_ascii_digit()) {
        unmet.push(PasswordRule::Digit);
    }
    if !password.chars().any(char::is_lowercase) {
        unmet.push(PasswordRule::Lowercase);
    }
    if !password.chars().any(char::is_uppercase) {
        unmet.push(PasswordRule::Uppercase);
    }
    if contains_ignore_ascii_case(password, username.as_str()) {
        unmet.push(PasswordRule::ContainsUsername);
    }
    unmet
}

/// L'identifiant est en ASCII : la comparaison se fait octet par octet, sans copie (donc sans
/// copie en clair du mot de passe à effacer).
fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    let needle = needle.as_bytes();
    if needle.is_empty() {
        return false;
    }
    haystack
        .as_bytes()
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

/// Mot de passe refusé : toutes les règles non respectées, une par ligne à l'affichage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasswordRejected {
    pub rules: Vec<PasswordRule>,
}

impl std::fmt::Display for PasswordRejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (index, rule) in self.rules.iter().enumerate() {
            if index > 0 {
                f.write_str("\n")?;
            }
            write!(f, "{rule}")?;
        }
        Ok(())
    }
}

impl std::error::Error for PasswordRejected {}

/// Mot de passe en clair qui respecte les règles : seul moyen d'en fournir un à hacher.
#[derive(Debug)]
pub struct PlainPassword(Secret);

impl PlainPassword {
    pub fn new(password: Secret, username: &Username) -> Result<Self, PasswordRejected> {
        let rules = unmet_rules(password.expose(), username);
        if rules.is_empty() {
            Ok(Self(password))
        } else {
            Err(PasswordRejected { rules })
        }
    }

    pub fn expose(&self) -> &str {
        self.0.expose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marie() -> Username {
        Username::parse("marie").unwrap()
    }

    #[test]
    fn a_compliant_password_has_no_unmet_rule() {
        assert_eq!(unmet_rules("Abcdefghij12", &marie()), vec![]);
        assert_eq!(unmet_rules("Correct-Horse-Battery-9", &marie()), vec![]);
    }

    #[test]
    fn empty_password_is_required_only() {
        assert_eq!(unmet_rules("", &marie()), vec![PasswordRule::Required]);
    }

    #[test]
    fn length_limit_is_twelve_characters_inclusive() {
        assert_eq!(
            unmet_rules("Abcdefghi12", &marie()),
            vec![PasswordRule::MinLength]
        );
        assert_eq!(unmet_rules("Abcdefghij12", &marie()), vec![]);
    }

    #[test]
    fn length_counts_characters_not_bytes() {
        // 11 caractères mais plus de 12 octets.
        assert_eq!(
            unmet_rules("Éééééééééé1", &marie()),
            vec![PasswordRule::MinLength]
        );
    }

    #[test]
    fn each_missing_class_is_reported() {
        assert_eq!(
            unmet_rules("Abcdefghijkl", &marie()),
            vec![PasswordRule::Digit]
        );
        assert_eq!(
            unmet_rules("ABCDEFGHIJ12", &marie()),
            vec![PasswordRule::Lowercase]
        );
        assert_eq!(
            unmet_rules("abcdefghij12", &marie()),
            vec![PasswordRule::Uppercase]
        );
    }

    #[test]
    fn every_unmet_rule_is_listed_in_the_order_of_the_specification() {
        assert_eq!(
            unmet_rules("abc", &marie()),
            vec![
                PasswordRule::MinLength,
                PasswordRule::Digit,
                PasswordRule::Uppercase
            ]
        );
        assert_eq!(
            unmet_rules("123456", &marie()),
            vec![
                PasswordRule::MinLength,
                PasswordRule::Lowercase,
                PasswordRule::Uppercase
            ]
        );
    }

    #[test]
    fn password_must_not_contain_the_username_whatever_the_case() {
        assert_eq!(
            unmet_rules("xxMarieXXxx12", &marie()),
            vec![PasswordRule::ContainsUsername]
        );
        assert_eq!(
            unmet_rules("Xxxxxxxxxx1marie", &marie()),
            vec![PasswordRule::ContainsUsername]
        );
        assert_eq!(
            unmet_rules("MARIE-Marie-1234", &marie()),
            vec![PasswordRule::ContainsUsername]
        );
    }

    #[test]
    fn a_password_equal_to_the_username_is_refused() {
        let user = Username::parse("administrateur1").unwrap();
        assert!(unmet_rules("administrateur1", &user).contains(&PasswordRule::ContainsUsername));
    }

    #[test]
    fn a_similar_but_different_word_is_accepted() {
        assert_eq!(unmet_rules("Mari-e-Mari-e-12", &marie()), vec![]);
    }

    #[test]
    fn plain_password_accepts_a_compliant_secret() {
        let password = PlainPassword::new(Secret::from("Abcdefghij12"), &marie()).unwrap();
        assert_eq!(password.expose(), "Abcdefghij12");
    }

    #[test]
    fn plain_password_rejects_with_the_whole_list() {
        let error = PlainPassword::new(Secret::from("abc"), &marie()).unwrap_err();
        assert_eq!(
            error.rules,
            vec![
                PasswordRule::MinLength,
                PasswordRule::Digit,
                PasswordRule::Uppercase
            ]
        );
        assert_eq!(
            error.to_string(),
            "Le mot de passe doit contenir au moins 12 caractères\n\
             Le mot de passe doit contenir au moins un chiffre\n\
             Le mot de passe doit contenir au moins une majuscule"
        );
    }

    #[test]
    fn debug_of_a_plain_password_does_not_reveal_it() {
        let password = PlainPassword::new(Secret::from("Abcdefghij12"), &marie()).unwrap();
        assert!(!format!("{password:?}").contains("Abcdefghij12"));
    }

    #[test]
    fn messages_are_the_ones_of_the_specification() {
        assert_eq!(
            PasswordRule::Required.to_string(),
            "Le mot de passe est requis"
        );
        assert_eq!(
            PasswordRule::ContainsUsername.to_string(),
            "Le mot de passe ne doit pas contenir l'identifiant"
        );
    }
}

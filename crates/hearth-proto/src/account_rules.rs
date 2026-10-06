//! Règles de format d'un identifiant et d'un mot de passe de compte (BR-ACCT-002 à BR-ACCT-005).
//!
//! UNE seule source : l'agent les applique (c'est lui l'arbitre), le client les évalue en direct
//! pendant la saisie par la commande `check_account_input`, la ligne de commande de l'agent les
//! reprend telles quelles. Pur : aucune E/S, jamais de copie d'un mot de passe.

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const USERNAME_MIN_LEN: usize = 3;
pub const USERNAME_MAX_LEN: usize = 32;
pub const PASSWORD_MIN_LEN: usize = 12;

/// Pourquoi un identifiant est refusé. Le message est celui de la spécification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsernameProblem {
    #[error("L'identifiant est requis")]
    Empty,
    #[error("L'identifiant doit contenir au moins 3 caractères")]
    TooShort,
    #[error("L'identifiant doit contenir au plus 32 caractères")]
    TooLong,
    #[error("L'identifiant contient des caractères non autorisés")]
    InvalidChars,
}

/// BR-ACCT-002 : contrôle du format ; BR-ACCT-003 : la saisie est ramenée en minuscules, ce qui
/// rend l'unicité insensible à la casse. Rend l'identifiant normalisé.
pub fn check_username(raw: &str) -> Result<String, UsernameProblem> {
    if raw.is_empty() {
        return Err(UsernameProblem::Empty);
    }
    let normalized = raw.to_ascii_lowercase();
    let allowed = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_';
    if !normalized.chars().all(allowed) {
        return Err(UsernameProblem::InvalidChars);
    }
    let len = normalized.chars().count();
    if len < USERNAME_MIN_LEN {
        return Err(UsernameProblem::TooShort);
    }
    if len > USERNAME_MAX_LEN {
        return Err(UsernameProblem::TooLong);
    }
    Ok(normalized)
}

/// Une règle de mot de passe non respectée. Le message est celui de la spécification ; `code` est
/// le code stable de `details.rules` de `WEAK_PASSWORD`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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

impl PasswordRule {
    /// Les règles, dans l'ordre de la spécification.
    pub const ALL: [Self; 6] = [
        Self::Required,
        Self::MinLength,
        Self::Digit,
        Self::Lowercase,
        Self::Uppercase,
        Self::ContainsUsername,
    ];

    pub const fn code(self) -> &'static str {
        match self {
            Self::Required => "required",
            Self::MinLength => "min_length",
            Self::Digit => "digit",
            Self::Lowercase => "lowercase",
            Self::Uppercase => "uppercase",
            Self::ContainsUsername => "contains_username",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|rule| rule.code() == code)
    }
}

/// Règles non respectées par `password` pour le compte `username` (identifiant déjà normalisé ;
/// vide : la règle « contient l'identifiant » ne joue pas). Vide si le mot de passe est conforme ;
/// un mot de passe vide ne rend que `Required`.
pub fn unmet_password_rules(password: &str, username: &str) -> Vec<PasswordRule> {
    if password.is_empty() {
        return vec![PasswordRule::Required];
    }
    let mut unmet = Vec::new();
    if password.chars().count() < PASSWORD_MIN_LEN {
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
    if contains_ignore_ascii_case(password, username) {
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

/// Verdict d'une saisie de création ou de changement, évalué en direct par le client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputCheck {
    /// `None` : l'identifiant est valide.
    pub username: Option<UsernameProblem>,
    /// Règles non respectées par le mot de passe. La règle « contient l'identifiant » ne joue que
    /// si l'identifiant est valide (comme à l'agent, qui refuse l'identifiant d'abord).
    pub password: Vec<PasswordRule>,
}

pub fn check_input(raw_username: &str, password: &str) -> InputCheck {
    match check_username(raw_username) {
        Ok(username) => InputCheck {
            username: None,
            password: unmet_password_rules(password, &username),
        },
        Err(problem) => InputCheck {
            username: Some(problem),
            password: unmet_password_rules(password, ""),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_codes_round_trip() {
        for rule in PasswordRule::ALL {
            assert_eq!(PasswordRule::from_code(rule.code()), Some(rule));
        }
        assert_eq!(PasswordRule::from_code("inconnue"), None);
    }

    #[test]
    fn the_username_is_normalized_and_bounded() {
        assert_eq!(check_username("MARIE").as_deref(), Ok("marie"));
        assert_eq!(check_username(""), Err(UsernameProblem::Empty));
        assert_eq!(check_username("ab"), Err(UsernameProblem::TooShort));
        assert_eq!(check_username("a b"), Err(UsernameProblem::InvalidChars));
        assert_eq!(
            check_username(&"a".repeat(33)),
            Err(UsernameProblem::TooLong)
        );
        assert!(check_username(&"a".repeat(32)).is_ok());
    }

    #[test]
    fn the_username_rule_is_skipped_while_the_username_is_invalid() {
        let check = check_input("ma", "xxMAxxxxxxxx1A");
        assert_eq!(check.username, Some(UsernameProblem::TooShort));
        assert!(!check.password.contains(&PasswordRule::ContainsUsername));
        let check = check_input("marie", "xxMarieXXxx12");
        assert_eq!(check.username, None);
        assert_eq!(check.password, vec![PasswordRule::ContainsUsername]);
    }

    #[test]
    fn rules_come_out_in_the_order_of_the_specification() {
        assert_eq!(
            unmet_password_rules("abc", "marie"),
            vec![
                PasswordRule::MinLength,
                PasswordRule::Digit,
                PasswordRule::Uppercase
            ]
        );
    }

    #[test]
    fn serialized_rules_use_the_stable_codes() {
        for rule in PasswordRule::ALL {
            assert_eq!(
                serde_json::to_value(rule).unwrap(),
                serde_json::json!(rule.code())
            );
        }
    }

    /// Les mêmes vecteurs servent le simulateur du client (Vitest) : une divergence casse l'un des
    /// deux tests.
    #[test]
    fn the_shared_vectors_hold() {
        let text = include_str!("../tests/vectors/account-input.json");
        let cases: Vec<serde_json::Value> = serde_json::from_str(text).unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let username = case["username"].as_str().unwrap();
            let password = case["password"].as_str().unwrap();
            let check = check_input(username, password);
            let expected_username = case["usernameProblem"].as_str();
            assert_eq!(
                check.username.map(|p| serde_json::to_value(p).unwrap()),
                expected_username.map(|s| serde_json::json!(s)),
                "{case}"
            );
            let rules: Vec<&str> = check.password.iter().map(|r| r.code()).collect();
            let expected: Vec<&str> = case["passwordRules"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect();
            assert_eq!(rules, expected, "{case}");
        }
    }
}

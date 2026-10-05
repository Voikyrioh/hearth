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
        "HEARTH_ADMIN_PASSWORD_HASH n'est pas un haché Argon2id au format PHC (`$argon2id$v=19$m=…,t=…,p=…$sel$haché`). Fabrique-le avec `hearth-agent hash-password`."
    )]
    NotArgon2id,
    #[error(
        "Les paramètres du haché sont hors des bornes acceptées : mémoire de 19 Mio à 256 Mio (m=19456 à 262144), 2 à 10 itérations, parallélisme de 1 à 4, sel d'au moins 16 octets, haché d'au moins 32 octets. Un haché trop faible se casse ; un haché trop gourmand épuiserait la mémoire du serveur à chaque connexion."
    )]
    OutOfBounds,
}

/// Mémoire minimale (OWASP, 19 Mio) et maximale (256 Mio) d'un haché fourni, en Kio.
pub const MIN_MEMORY_KIB: u32 = 19 * 1024;
pub const MAX_MEMORY_KIB: u32 = 256 * 1024;

fn is_b64(part: &str) -> bool {
    !part.is_empty()
        && part
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/')
}

/// Nombre d'octets que représente un texte base64 sans remplissage.
fn b64_bytes(text: &str) -> usize {
    text.len() * 6 / 8
}

/// Haché Argon2id au format PHC, `$argon2id$v=19$m=…,t=…,p=…$sel$haché`, **strictement** : version
/// 19, paramètres entre des bornes basses et hautes, sel et sortie de longueur suffisante. Le
/// haché fourni ne doit être ni trop faible ni capable d'allouer la mémoire du serveur à chaque
/// connexion. Contrôle de forme et de bornes ; l'adaptateur de hachage fait la lecture complète.
pub fn check_password_hash_format(hash: &str) -> Result<(), HashFormatError> {
    let parts: Vec<&str> = hash.split('$').collect();
    let shaped = parts.len() == 6
        && parts[0].is_empty()
        && parts[1] == "argon2id"
        && parts[2] == "v=19"
        && is_b64(parts[4])
        && is_b64(parts[5]);
    if !shaped {
        return Err(HashFormatError::NotArgon2id);
    }
    let mut memory = None;
    let mut time = None;
    let mut lanes = None;
    let params: Vec<&str> = parts[3].split(',').collect();
    if params.len() != 3 {
        return Err(HashFormatError::NotArgon2id);
    }
    for (param, slot) in params.iter().zip([&mut memory, &mut time, &mut lanes]) {
        let (_, value) = param.split_once('=').ok_or(HashFormatError::NotArgon2id)?;
        if !value.bytes().all(|b| b.is_ascii_digit()) || value.is_empty() || value.len() > 9 {
            return Err(HashFormatError::NotArgon2id);
        }
        *slot = value.parse::<u32>().ok();
    }
    if !(parts[3].starts_with("m=") && parts[3].contains(",t=") && parts[3].contains(",p=")) {
        return Err(HashFormatError::NotArgon2id);
    }
    let (Some(memory), Some(time), Some(lanes)) = (memory, time, lanes) else {
        return Err(HashFormatError::NotArgon2id);
    };
    let in_bounds = (MIN_MEMORY_KIB..=MAX_MEMORY_KIB).contains(&memory)
        && (2..=10).contains(&time)
        && (1..=4).contains(&lanes)
        && (16..=64).contains(&b64_bytes(parts[4]))
        && (32..=128).contains(&b64_bytes(parts[5]));
    if in_bounds {
        Ok(())
    } else {
        Err(HashFormatError::OutOfBounds)
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

    const SALT: &str = "c29tZXNhbHRzb21lc2FsdA";
    const OUT: &str = "aGFzaGhhc2hoYXNoaGFzaGhhc2hoYXNoaGFzaGhhc2g";

    fn hash(params: &str) -> String {
        format!("$argon2id$v=19${params}${SALT}${OUT}")
    }

    #[test]
    fn only_an_argon2id_phc_hash_is_accepted() {
        assert_eq!(check_password_hash_format(&hash("m=19456,t=2,p=1")), Ok(()));
        for bad in [
            "",
            "Cheval-Agrafe-42",
            "$argon2i$v=19$m=19456,t=2,p=1$c29tZXNhbHRzb21lc2FsdA$aGFzaA",
            "$argon2id$v=16$m=19456,t=2,p=1$c29tZXNhbHRzb21lc2FsdA$aGFzaA",
            "$2b$12$abcdefghijklmnopqrstuuABCDEFGHIJKLMNOPQRSTUVWXYZ01234",
            "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRzb21lc2FsdA",
            "$argon2id$v=19$m=19456,t=2,p=1$$aGFzaA",
            "$argon2id$v=19$m=19456,t=2,p=1$sel$ha sh",
            "$argon2id$v=19$m=19456,t=2$c29tZXNhbHRzb21lc2FsdA$aGFzaA",
            "$argon2id$v=19$t=2,m=19456,p=1$c29tZXNhbHRzb21lc2FsdA$aGFzaA",
            "$argon2id$v=19$m=-1,t=2,p=1$c29tZXNhbHRzb21lc2FsdA$aGFzaA",
        ] {
            assert_eq!(
                check_password_hash_format(bad),
                Err(HashFormatError::NotArgon2id),
                "{bad}"
            );
        }
    }

    #[test]
    fn parameters_too_weak_or_too_greedy_are_refused() {
        for params in [
            "m=8,t=1,p=1",
            "m=19455,t=2,p=1",
            "m=19456,t=1,p=1",
            "m=19456,t=11,p=1",
            "m=19456,t=2,p=0",
            "m=19456,t=2,p=5",
            "m=262145,t=2,p=1",
            "m=4194304,t=2,p=1",
        ] {
            assert_eq!(
                check_password_hash_format(&hash(params)),
                Err(HashFormatError::OutOfBounds),
                "{params}"
            );
        }
        for params in ["m=19456,t=2,p=1", "m=262144,t=10,p=4", "m=65536,t=3,p=2"] {
            assert_eq!(
                check_password_hash_format(&hash(params)),
                Ok(()),
                "{params}"
            );
        }
    }

    #[test]
    fn a_salt_or_an_output_too_short_or_too_long_is_refused() {
        let short_salt = format!("$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHQ${OUT}");
        let short_out = format!("$argon2id$v=19$m=19456,t=2,p=1${SALT}$aGFzaA");
        let long_out = format!("$argon2id$v=19$m=19456,t=2,p=1${SALT}${}", "A".repeat(200));
        for bad in [short_salt, short_out, long_out] {
            assert_eq!(
                check_password_hash_format(&bad),
                Err(HashFormatError::OutOfBounds),
                "{bad}"
            );
        }
    }
}

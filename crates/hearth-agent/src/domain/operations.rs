//! Suivi des opérations par clé (BR-RESIL-010, côté agent).
//!
//! Le client donne à chaque requête qui modifie une clé (`Idempotency-Key`). L'agent la retient
//! avec le résultat : rejouer la même clé rend le premier résultat sans ré-exécuter ; une clé
//! dont l'exécution n'est pas finie répond « en cours » ; une clé qui appartient à un autre
//! compte est refusée (un compte ne lit jamais l'opération d'un autre).

use std::fmt;

use thiserror::Error;
use time::{Duration, OffsetDateTime};

use super::accounts::AccountId;

/// Durée de conservation d'une opération (le client la relit au retour du lien).
pub const RETENTION: Duration = Duration::hours(24);

const MAX_KEY_LEN: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error(
    "La clé d'opération est invalide : 1 à 64 caractères, lettres, chiffres, tiret ou souligné"
)]
pub struct InvalidKey;

/// Clé d'opération choisie par le client (un ULID en pratique).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OperationKey(String);

impl OperationKey {
    pub fn parse(raw: &str) -> Result<Self, InvalidKey> {
        let valid = !raw.is_empty()
            && raw.len() <= MAX_KEY_LEN
            && raw
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if valid {
            Ok(Self(raw.to_owned()))
        } else {
            Err(InvalidKey)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OperationKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationStatus {
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("statut d'opération inconnu")]
pub struct UnknownStatus;

impl OperationStatus {
    /// Forme stockée en base.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }

    pub fn from_stored(value: &str) -> Result<Self, UnknownStatus> {
        match value {
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            _ => Err(UnknownStatus),
        }
    }
}

/// Une opération telle que conservée. Le résultat est le JSON de la réponse (statut HTTP et
/// corps), opaque pour le domaine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operation {
    pub key: OperationKey,
    pub account: AccountId,
    /// Requête d'origine, `MÉTHODE /chemin`, pour le diagnostic.
    pub kind: String,
    pub status: OperationStatus,
    pub result_json: Option<String>,
    pub created_at: OffsetDateTime,
    pub finished_at: Option<OffsetDateTime>,
}

/// Que faire d'une requête qui porte la clé `key` quand `existing` est ce qu'on a en mémoire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Replay<'a> {
    /// Clé inconnue : exécuter la requête.
    Execute,
    /// Clé déjà terminée pour ce compte : rendre le premier résultat.
    Return(&'a Operation),
    /// Clé déjà reçue, exécution pas finie : répondre « en cours ».
    InProgress,
    /// Clé déjà utilisée par un autre compte.
    ForeignKey,
}

pub fn classify<'a>(existing: Option<&'a Operation>, account: &AccountId) -> Replay<'a> {
    match existing {
        None => Replay::Execute,
        Some(operation) if &operation.account != account => Replay::ForeignKey,
        Some(operation) => match operation.status {
            OperationStatus::Running => Replay::InProgress,
            OperationStatus::Succeeded | OperationStatus::Failed => Replay::Return(operation),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn operation(account: &str, status: OperationStatus) -> Operation {
        Operation {
            key: OperationKey::parse("01J0KEY").unwrap(),
            account: AccountId::new(account),
            kind: "PUT /me/password".into(),
            status,
            result_json: None,
            created_at: OffsetDateTime::UNIX_EPOCH,
            finished_at: None,
        }
    }

    #[test]
    fn a_ulid_is_a_valid_key() {
        assert!(OperationKey::parse("01J9ZY0G3Q8M2K6W4T7V5N1B9D").is_ok());
    }

    #[test]
    fn empty_long_or_odd_keys_are_refused() {
        for bad in ["", &"a".repeat(65), "a b", "a/b", "é", "a\n"] {
            assert_eq!(OperationKey::parse(bad), Err(InvalidKey), "{bad:?}");
        }
        assert!(OperationKey::parse(&"a".repeat(64)).is_ok());
    }

    #[test]
    fn an_unknown_key_is_executed() {
        assert_eq!(classify(None, &AccountId::new("A")), Replay::Execute);
    }

    #[test]
    fn a_finished_operation_of_the_same_account_is_returned() {
        for status in [OperationStatus::Succeeded, OperationStatus::Failed] {
            let op = operation("A", status);
            assert_eq!(
                classify(Some(&op), &AccountId::new("A")),
                Replay::Return(&op)
            );
        }
    }

    #[test]
    fn a_running_operation_answers_in_progress() {
        let op = operation("A", OperationStatus::Running);
        assert_eq!(
            classify(Some(&op), &AccountId::new("A")),
            Replay::InProgress
        );
    }

    #[test]
    fn another_accounts_key_is_refused_whatever_its_status() {
        for status in [
            OperationStatus::Running,
            OperationStatus::Succeeded,
            OperationStatus::Failed,
        ] {
            let op = operation("A", status);
            assert_eq!(
                classify(Some(&op), &AccountId::new("B")),
                Replay::ForeignKey
            );
        }
    }

    #[test]
    fn statuses_round_trip_through_their_stored_form() {
        for status in [
            OperationStatus::Running,
            OperationStatus::Succeeded,
            OperationStatus::Failed,
        ] {
            assert_eq!(OperationStatus::from_stored(status.as_str()), Ok(status));
        }
        assert_eq!(OperationStatus::from_stored("x"), Err(UnknownStatus));
    }
}

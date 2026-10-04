//! Suivi des opérations par clé (BR-RESIL-010, côté agent).
//!
//! Le client donne à chaque requête qui modifie une clé (`Idempotency-Key`). La clé est celle
//! d'un compte (deux comptes peuvent choisir la même sans se voir) et est liée à une requête
//! précise : méthode, chemin et corps. L'agent la retient avec le résultat : rejouer la même clé
//! avec la même requête rend le premier résultat sans ré-exécuter ; une clé dont l'exécution
//! n'est pas finie répond « en cours » ; une exécution interrompue par un arrêt de l'agent
//! répond « résultat inconnu » ; la même clé avec une autre requête est refusée.

use std::fmt;

use sha2::{Digest, Sha256};
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

/// Empreinte de la requête liée à une clé : SHA-256 de la méthode, du chemin et du corps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestFingerprint(String);

impl RequestFingerprint {
    pub fn of(method: &str, path: &str, body: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        // Les séparateurs (octet nul) empêchent de déplacer une frontière entre les parties.
        hasher.update(method.as_bytes());
        hasher.update([0]);
        hasher.update(path.as_bytes());
        hasher.update([0]);
        hasher.update(body);
        let digest = hasher.finalize();
        let mut text = String::with_capacity(64);
        for byte in digest {
            text.push_str(&format!("{byte:02x}"));
        }
        Self(text)
    }

    /// Relit une empreinte stockée.
    pub fn from_stored(text: String) -> Self {
        Self(text)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationStatus {
    Running,
    Succeeded,
    Failed,
    /// L'agent s'est arrêté pendant l'exécution : on ne sait pas si elle a eu lieu.
    Interrupted,
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
            Self::Interrupted => "interrupted",
        }
    }

    pub fn from_stored(value: &str) -> Result<Self, UnknownStatus> {
        match value {
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "interrupted" => Ok(Self::Interrupted),
            _ => Err(UnknownStatus),
        }
    }
}

/// Une opération telle que conservée. Le résultat est le JSON de la réponse, opaque pour le
/// domaine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operation {
    pub key: OperationKey,
    pub account: AccountId,
    /// Requête d'origine, `MÉTHODE /chemin`, pour le diagnostic.
    pub kind: String,
    pub request: RequestFingerprint,
    pub status: OperationStatus,
    pub result_json: Option<String>,
    pub created_at: OffsetDateTime,
    pub finished_at: Option<OffsetDateTime>,
}

/// Que faire d'une requête qui porte une clé.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Replay {
    /// Clé inconnue de ce compte : enregistrer et exécuter.
    Execute,
    /// Clé déjà terminée, même requête : rendre ce premier résultat.
    Return(Operation),
    /// Clé déjà reçue, exécution pas finie.
    InProgress,
    /// Exécution interrompue par un arrêt de l'agent : résultat inconnu, ne pas rejouer.
    Interrupted,
    /// Même clé, autre requête : refus, sans exécuter.
    KeyReused,
}

/// `existing` : l'opération déjà connue pour cette clé et ce compte ; `request` : la requête
/// qui arrive.
pub fn classify(existing: Option<Operation>, request: &RequestFingerprint) -> Replay {
    let Some(operation) = existing else {
        return Replay::Execute;
    };
    if operation.request != *request {
        return Replay::KeyReused;
    }
    match operation.status {
        OperationStatus::Running => Replay::InProgress,
        OperationStatus::Interrupted => Replay::Interrupted,
        OperationStatus::Succeeded | OperationStatus::Failed => Replay::Return(operation),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fingerprint(body: &str) -> RequestFingerprint {
        RequestFingerprint::of("PUT", "/me/password", body.as_bytes())
    }

    fn operation(status: OperationStatus) -> Operation {
        Operation {
            key: OperationKey::parse("01J0KEY").unwrap(),
            account: AccountId::new("A"),
            kind: "PUT /me/password".into(),
            request: fingerprint("{}"),
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
    fn the_request_fingerprint_covers_method_path_and_body() {
        let base = RequestFingerprint::of("PUT", "/a", b"x");
        assert_eq!(base, RequestFingerprint::of("PUT", "/a", b"x"));
        assert_ne!(base, RequestFingerprint::of("DELETE", "/a", b"x"));
        assert_ne!(base, RequestFingerprint::of("PUT", "/b", b"x"));
        assert_ne!(base, RequestFingerprint::of("PUT", "/a", b"y"));
        // Une frontière déplacée entre les parties ne donne pas la même empreinte.
        assert_ne!(
            RequestFingerprint::of("PUT", "/ab", b""),
            RequestFingerprint::of("PUT", "/a", b"b")
        );
        assert_eq!(base.as_str().len(), 64);
    }

    #[test]
    fn an_unknown_key_is_executed() {
        assert_eq!(classify(None, &fingerprint("{}")), Replay::Execute);
    }

    #[test]
    fn a_finished_operation_with_the_same_request_is_returned() {
        for status in [OperationStatus::Succeeded, OperationStatus::Failed] {
            let op = operation(status);
            assert_eq!(
                classify(Some(op.clone()), &fingerprint("{}")),
                Replay::Return(op)
            );
        }
    }

    #[test]
    fn a_running_operation_answers_in_progress() {
        let op = operation(OperationStatus::Running);
        assert_eq!(classify(Some(op), &fingerprint("{}")), Replay::InProgress);
    }

    #[test]
    fn an_interrupted_operation_is_never_replayed() {
        let op = operation(OperationStatus::Interrupted);
        assert_eq!(classify(Some(op), &fingerprint("{}")), Replay::Interrupted);
    }

    #[test]
    fn the_same_key_with_another_request_is_refused_whatever_the_status() {
        for status in [
            OperationStatus::Running,
            OperationStatus::Succeeded,
            OperationStatus::Failed,
            OperationStatus::Interrupted,
        ] {
            let op = operation(status);
            assert_eq!(
                classify(Some(op), &fingerprint(r#"{"autre":1}"#)),
                Replay::KeyReused
            );
        }
    }

    #[test]
    fn statuses_round_trip_through_their_stored_form() {
        for status in [
            OperationStatus::Running,
            OperationStatus::Succeeded,
            OperationStatus::Failed,
            OperationStatus::Interrupted,
        ] {
            assert_eq!(OperationStatus::from_stored(status.as_str()), Ok(status));
        }
        assert_eq!(OperationStatus::from_stored("x"), Err(UnknownStatus));
    }
}

//! Suivi des opérations par clé (BR-RESIL-010, côté agent).
//!
//! Le client donne à chaque requête qui modifie une clé (`Idempotency-Key`). La clé est celle
//! d'un compte (deux comptes peuvent choisir la même sans se voir) et est liée à une requête
//! précise : méthode, chemin et corps. L'agent la retient avec le résultat : rejouer la même clé
//! avec la même requête rend le premier résultat sans ré-exécuter ; une clé dont l'exécution
//! n'est pas finie répond « en cours » ; une exécution interrompue par un arrêt de l'agent
//! répond « résultat inconnu » ; la même clé avec une autre requête est refusée.
//!
//! L'empreinte de la requête est un HMAC-SHA-256 clé par un secret propre à l'installation
//! (HRT-32, ADR-0034) : le corps d'un `POST /accounts` ou d'un `PUT …/password` contient un mot de
//! passe, et un haché sans clé de ce corps, lu dans une copie de la base, se devine hors ligne à
//! pleine vitesse.

use std::fmt;

use subtle::ConstantTimeEq;
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

/// Longueur d'une empreinte : un HMAC-SHA-256, 32 octets.
pub const FINGERPRINT_LEN: usize = 32;

/// Étiquette de domaine et de version de l'encodage (HRT-32).
const DOMAIN_LABEL: &[u8] = b"hearth/request-fingerprint/v1";

/// Les trois parties d'une requête suivie, en une suite d'octets sans ambiguïté : chaque partie est
/// précédée de sa longueur (8 octets, grand-boutiste). Déplacer une frontière entre la méthode, le
/// chemin et le corps change la suite (`"a"` + `"bc"` n'est pas `"ab"` + `"c"`), quel que soit le
/// contenu, y compris des octets nuls. C'est ce que l'adaptateur donne au HMAC (jamais à un
/// haché sans clé). La suite commence par une étiquette de domaine et de version : un autre usage
/// du même secret, un jour, ne produirait pas de codes interchangeables avec ceux-ci.
pub fn canonical_request(method: &str, path: &str, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(48 + method.len() + path.len() + body.len());
    for part in [DOMAIN_LABEL, method.as_bytes(), path.as_bytes(), body] {
        out.extend_from_slice(&(part.len() as u64).to_be_bytes());
        out.extend_from_slice(part);
    }
    out
}

/// Empreinte de la requête liée à une clé : HMAC-SHA-256 de `canonical_request`, clé par le secret
/// de l'installation (`FingerprintSecret`). Sans le secret, rien ne permet de la calculer ni de
/// tester un mot de passe candidat contre elle : une copie de la base ne la livre pas (HRT-32).
///
/// Deux états : calculée, ou **effacée** (`purged`) : la migration `0008` efface les empreintes
/// d'avant la clé. Une empreinte effacée n'est égale à aucune empreinte calculée ; `classify` la
/// traite à part avant toute comparaison (`Replay::Unverifiable`). L'égalité de deux empreintes
/// calculées est en temps constant.
#[derive(Debug, Clone)]
pub struct RequestFingerprint(Repr);

#[derive(Clone)]
enum Repr {
    Keyed([u8; FINGERPRINT_LEN]),
    Purged,
}

// Le contenu d'une empreinte n'est pas un secret (sans la clé, elle ne dit rien) mais n'a pas
// besoin d'apparaître au journal : seule sa forme y est.
impl std::fmt::Debug for Repr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Keyed(_) => f.write_str("Keyed"),
            Self::Purged => f.write_str("Purged"),
        }
    }
}

impl RequestFingerprint {
    /// Le HMAC calculé par l'adaptateur.
    pub fn from_mac(mac: [u8; FINGERPRINT_LEN]) -> Self {
        Self(Repr::Keyed(mac))
    }

    /// Relit une empreinte stockée : 64 chiffres hexadécimaux, sinon (vide après la migration
    /// `0008`, ou illisible) « effacée ».
    pub fn from_stored(text: &str) -> Self {
        decode_hex(text).map_or(Self(Repr::Purged), |mac| Self(Repr::Keyed(mac)))
    }

    /// L'empreinte d'avant la clé, effacée par la migration `0008`.
    pub fn purged() -> Self {
        Self(Repr::Purged)
    }

    pub fn is_purged(&self) -> bool {
        matches!(self.0, Repr::Purged)
    }

    /// Forme stockée : 64 chiffres hexadécimaux, ou vide si effacée.
    pub fn to_stored(&self) -> String {
        match &self.0 {
            Repr::Keyed(mac) => {
                let mut text = String::with_capacity(FINGERPRINT_LEN * 2);
                for byte in mac {
                    text.push_str(&format!("{byte:02x}"));
                }
                text
            }
            Repr::Purged => String::new(),
        }
    }
}

impl Eq for RequestFingerprint {}

impl PartialEq for RequestFingerprint {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Repr::Keyed(a), Repr::Keyed(b)) => a.ct_eq(b).into(),
            (Repr::Purged, Repr::Purged) => true,
            _ => false,
        }
    }
}

fn decode_hex(text: &str) -> Option<[u8; FINGERPRINT_LEN]> {
    let bytes = text.as_bytes();
    if bytes.len() != FINGERPRINT_LEN * 2 {
        return None;
    }
    let mut out = [0_u8; FINGERPRINT_LEN];
    for (slot, pair) in out.iter_mut().zip(bytes.chunks_exact(2)) {
        let high = char::from(pair[0]).to_digit(16)?;
        let low = char::from(pair[1]).to_digit(16)?;
        *slot = u8::try_from(high * 16 + low).ok()?;
    }
    Some(out)
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
    /// Clé connue dont l'empreinte a été effacée par la migration `0008` (HRT-32) : on ne peut pas
    /// dire si c'est la même requête. Refus, sans exécuter, quel que soit l'état de l'opération ;
    /// son résultat reste lisible par `GET /operations/{id}`.
    Unverifiable,
}

/// `existing` : l'opération déjà connue pour cette clé et ce compte ; `request` : la requête
/// qui arrive.
pub fn classify(existing: Option<Operation>, request: &RequestFingerprint) -> Replay {
    let Some(operation) = existing else {
        return Replay::Execute;
    };
    // FIX:01M4BZN31A8Z8WN0WKNTCRTFFN : sans empreinte à comparer, jamais de ré-exécution « au cas où ».
    if operation.request.is_purged() {
        return Replay::Unverifiable;
    }
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

    fn secret(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    /// Calcul de test : le même HMAC que l'adaptateur (`infrastructure::fingerprint`), refait ici
    /// avec `sha2` pour que le domaine reste testable sans lui.
    fn mac(key: [u8; 32], data: &[u8]) -> [u8; 32] {
        use sha2::{Digest, Sha256};
        let mut inner = [0x36_u8; 64];
        let mut outer = [0x5c_u8; 64];
        for (i, k) in key.iter().enumerate() {
            inner[i] ^= k;
            outer[i] ^= k;
        }
        let first = Sha256::new()
            .chain_update(inner)
            .chain_update(data)
            .finalize();
        Sha256::new()
            .chain_update(outer)
            .chain_update(first)
            .finalize()
            .into()
    }

    fn of(key: u8, method: &str, path: &str, body: &[u8]) -> RequestFingerprint {
        RequestFingerprint::from_mac(mac(secret(key), &canonical_request(method, path, body)))
    }

    fn fingerprint(body: &str) -> RequestFingerprint {
        of(1, "PUT", "/me/password", body.as_bytes())
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
        let base = of(1, "PUT", "/a", b"x");
        assert_eq!(base, of(1, "PUT", "/a", b"x"));
        assert_ne!(base, of(1, "DELETE", "/a", b"x"));
        assert_ne!(base, of(1, "PUT", "/b", b"x"));
        assert_ne!(base, of(1, "PUT", "/a", b"y"));
        assert_eq!(base.to_stored().len(), 64);
    }

    #[test]
    fn a_boundary_moved_between_the_parts_changes_the_fingerprint() {
        type Parts<'a> = (&'a str, &'a str, &'a [u8]);
        let pairs: [(Parts, Parts); 5] = [
            (("PUT", "/ab", b""), ("PUT", "/a", b"b")),
            (("a", "bc", b""), ("ab", "c", b"")),
            (("PUT", "/a", b" b"), ("PUT", "/a ", b"b")),
            (("", "", b"x"), ("", "x", b"")),
            (("PUT", "", b"/a"), ("PUT/a", "", b"")),
        ];
        for ((m1, p1, b1), (m2, p2, b2)) in pairs {
            assert_ne!(canonical_request(m1, p1, b1), canonical_request(m2, p2, b2));
            assert_ne!(
                of(1, m1, p1, b1),
                of(1, m2, p2, b2),
                "{m1}|{p1} / {m2}|{p2}"
            );
        }
    }

    #[test]
    fn two_secrets_give_two_fingerprints_for_the_same_request() {
        assert_ne!(of(1, "PUT", "/a", b"x"), of(2, "PUT", "/a", b"x"));
    }

    #[test]
    fn a_fingerprint_is_stored_as_64_hex_digits_and_read_back() {
        let fingerprint = of(1, "PUT", "/a", b"x");
        let stored = fingerprint.to_stored();
        assert!(stored.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_eq!(RequestFingerprint::from_stored(&stored), fingerprint);
    }

    #[test]
    fn an_empty_or_unreadable_stored_value_is_a_purged_fingerprint_equal_to_no_computed_one() {
        for stored in ["", "abc", &"z".repeat(64), &"a".repeat(63), &"a".repeat(65)] {
            let purged = RequestFingerprint::from_stored(stored);
            assert!(purged.is_purged(), "{stored}");
            assert_eq!(purged, RequestFingerprint::purged());
            assert_ne!(purged, of(1, "PUT", "/a", b"x"));
            assert_eq!(purged.to_stored(), "");
        }
    }

    #[test]
    fn debug_shows_the_shape_not_the_bytes() {
        let shown = format!("{:?}", of(1, "PUT", "/a", b"x"));
        assert_eq!(shown, "RequestFingerprint(Keyed)");
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

    /// FIX:01M4BZN31A8Z8WN0WKNTCRTFFN : après la migration `0008`, une clé connue dont l'empreinte est
    /// effacée n'est jamais exécutée de nouveau, quel que soit son état ni la requête qui arrive.
    #[test]
    fn a_key_whose_fingerprint_was_purged_is_never_executed_again() {
        for status in [
            OperationStatus::Running,
            OperationStatus::Succeeded,
            OperationStatus::Failed,
            OperationStatus::Interrupted,
        ] {
            let mut op = operation(status);
            op.request = RequestFingerprint::from_stored("");
            assert_eq!(
                classify(Some(op), &fingerprint("{}")),
                Replay::Unverifiable,
                "{status:?}"
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

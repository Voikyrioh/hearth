//! Postes de confiance d'un compte (BR-TRUST-004, 022 à 026) : ce qui décide d'inscrire une clé,
//! combien de temps un poste est gardé, quel nom il porte.
//!
//! La clé ne change **aucune décision d'accès** dans cette version (HRT-24 y branchera la règle
//! « 2 critères sur 3 ») : elle est enregistrée, prouvée et listée.

use std::fmt;

use hearth_proto::api::devices::MAX_DEVICES_PER_ACCOUNT;
use time::{Duration, OffsetDateTime};

use crate::domain::accounts::AccountId;
use crate::domain::audit::ClientName;

/// Un poste est oublié 90 jours après sa dernière preuve valide (la durée de conservation du
/// journal).
pub const RETENTION: Duration = Duration::days(90);

/// Instant avant lequel la dernière preuve d'un poste est trop ancienne (purge).
pub fn cutoff(now: OffsetDateTime) -> OffsetDateTime {
    now - RETENTION
}

/// Identifiant technique d'un poste (ULID).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DeviceId(String);

impl DeviceId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Un poste inscrit, tel que lu. La clé publique n'y figure pas : on ne la relit jamais, on ne
/// fait que la retrouver par son empreinte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedDevice {
    pub id: DeviceId,
    pub account: AccountId,
    /// Empreinte de la clé publique (`hearth_proto::device_proof::key_id`).
    pub key_id: String,
    /// Nom annoncé à l'inscription, nettoyé (jamais un texte libre au journal).
    pub name: String,
    pub created_at: OffsetDateTime,
    pub last_proved_at: OffsetDateTime,
    pub last_addr: String,
}

/// Un poste à inscrire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewDevice {
    pub id: DeviceId,
    pub account: AccountId,
    pub key_id: String,
    pub public_key: [u8; hearth_proto::device_proof::PUBLIC_KEY_LEN],
    pub name: String,
    pub now: OffsetDateTime,
    pub addr: String,
}

/// Le nom du poste d'après `X-Hearth-Client`, nettoyé comme pour le journal ; « inconnu » s'il n'en
/// reste rien.
pub fn device_name(raw: Option<&str>) -> String {
    raw.and_then(ClientName::parse)
        .map_or_else(|| "inconnu".to_owned(), |name| name.as_str().to_owned())
}

/// Ce que la connexion fait de la clé dont la preuve est valide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Enrollment {
    /// La clé est déjà inscrite pour ce compte : seule sa dernière preuve est datée.
    Proven,
    /// La clé est inscrite maintenant.
    Enroll,
    /// Le compte a déjà 8 postes : rien n'est inscrit, aucune éviction (BR-TRUST-022).
    Limit,
    /// L'inscription est gelée (mode attaque) : la clé sera inscrite à une connexion suivante.
    Deferred,
    /// La clé appartient à un autre compte : jamais confiée à deux comptes.
    Foreign,
}

/// BR-TRUST-004, 022. `owned_here` : la clé est déjà inscrite pour CE compte ; `owned_elsewhere` :
/// pour un autre ; `count` : les postes du compte ; `frozen` : le mode attaque est actif.
///
/// L'ordre compte : un poste déjà inscrit reste « prouvé » même quand l'inscription est gelée ou
/// que le compte est plein (la preuve n'inscrit rien) ; une clé d'un autre compte n'est jamais
/// inscrite ici ; le gel prime sur la limite (le client réessaie après le mode attaque, la limite
/// se dirait alors).
pub fn judge_enrollment(
    owned_here: bool,
    owned_elsewhere: bool,
    count: usize,
    frozen: bool,
) -> Enrollment {
    if owned_here {
        Enrollment::Proven
    } else if owned_elsewhere {
        Enrollment::Foreign
    } else if frozen {
        Enrollment::Deferred
    } else if count >= MAX_DEVICES_PER_ACCOUNT {
        Enrollment::Limit
    } else {
        Enrollment::Enroll
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_truth_table_of_the_enrollment_is_exhaustive() {
        // (déjà ici, ailleurs, nombre de postes, gelé) -> issue
        let cases = [
            ((false, false, 0, false), Enrollment::Enroll),
            ((false, false, 7, false), Enrollment::Enroll),
            ((false, false, 8, false), Enrollment::Limit),
            ((false, false, 12, false), Enrollment::Limit),
            ((false, false, 0, true), Enrollment::Deferred),
            ((false, false, 8, true), Enrollment::Deferred),
            ((true, false, 0, false), Enrollment::Proven),
            ((true, false, 8, false), Enrollment::Proven),
            ((true, false, 8, true), Enrollment::Proven),
            ((false, true, 0, false), Enrollment::Foreign),
            ((false, true, 8, true), Enrollment::Foreign),
            // Impossible en base (la clé n'est qu'à un compte) mais défini : « ici » prime.
            ((true, true, 3, false), Enrollment::Proven),
        ];
        for ((here, elsewhere, count, frozen), expected) in cases {
            assert_eq!(
                judge_enrollment(here, elsewhere, count, frozen),
                expected,
                "ici={here} ailleurs={elsewhere} postes={count} gelé={frozen}"
            );
        }
    }

    #[test]
    fn the_limit_is_eight_devices() {
        assert_eq!(MAX_DEVICES_PER_ACCOUNT, 8);
    }

    #[test]
    fn a_device_is_forgotten_ninety_days_after_its_last_proof() {
        let now = OffsetDateTime::UNIX_EPOCH + Duration::days(20_000);
        assert_eq!(cutoff(now), now - Duration::days(90));
    }

    #[test]
    fn the_device_name_is_cleaned_bounded_and_never_empty() {
        assert_eq!(
            device_name(Some("  poste-de-marie/1.2 ")),
            "poste-de-marie/1.2"
        );
        assert_eq!(device_name(None), "inconnu");
        assert_eq!(device_name(Some("   ")), "inconnu");
        assert_eq!(device_name(Some("a\nb\u{202e}c")), "abc");
        assert_eq!(
            device_name(Some(&"x".repeat(500))).chars().count(),
            crate::domain::audit::MAX_CLIENT_NAME
        );
    }
}

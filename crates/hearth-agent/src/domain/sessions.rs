//! Sessions : ce qui est vrai quel que soit le stockage. Une session s'ouvre à la connexion, dure
//! 30 jours sans activité (expiration glissante), et se ferme par expiration, déconnexion ou
//! révocation (changement de mot de passe, suppression du compte, révocation explicite).

use std::fmt;

use time::{Duration, OffsetDateTime};

use super::accounts::AccountId;
use super::session_token::TokenHash;

/// Durée de vie d'une session sans activité (BR-RESIL-012).
pub const LIFETIME: Duration = Duration::days(30);

/// Intervalle minimal entre deux renouvellements : l'activité repousse l'expiration, mais pas
/// à chaque requête (une écriture par requête serait inutilement coûteuse).
pub const RENEWAL_INTERVAL: Duration = Duration::minutes(5);

/// Identifiant technique d'une session (ULID).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionId(String);

impl SessionId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Une session ouverte, telle que conservée. Le jeton n'y figure pas : seule son empreinte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub id: SessionId,
    pub account: AccountId,
    pub token_hash: TokenHash,
    pub client_name: String,
    pub client_addr: String,
    pub created_at: OffsetDateTime,
    pub last_seen_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
}

/// Pourquoi une session n'est plus utilisable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionEnd {
    /// Trop longtemps sans activité (ou jeton inconnu : session déjà purgée).
    Expired,
    /// Fermée par un changement de mot de passe, une suppression de compte ou une révocation.
    Revoked,
}

/// Date d'expiration d'une session active à `now`.
pub fn expiry_from(now: OffsetDateTime) -> OffsetDateTime {
    now + LIFETIME
}

/// BR-RESIL-012 : une session présentée est utilisable si elle existe et n'est pas expirée.
/// `revoked` dit si le jeton figure parmi les sessions révoquées (BR-RESIL-014). Un jeton inconnu
/// et non révoqué est une session purgée après expiration : « expirée ».
pub fn check(
    found: Option<&Session>,
    revoked: bool,
    now: OffsetDateTime,
) -> Result<&Session, SessionEnd> {
    match found {
        Some(session) if is_open(session.expires_at, now) => Ok(session),
        Some(_) => Err(SessionEnd::Expired),
        None if revoked => Err(SessionEnd::Revoked),
        None => Err(SessionEnd::Expired),
    }
}

/// Expiration glissante : nouvelle date d'expiration si l'activité doit la repousser maintenant
/// (dernière activité vue il y a au moins `RENEWAL_INTERVAL`), sinon rien à écrire.
pub fn renewed_expiry(last_seen_at: OffsetDateTime, now: OffsetDateTime) -> Option<OffsetDateTime> {
    (now - last_seen_at >= RENEWAL_INTERVAL).then(|| expiry_from(now))
}

/// Une session est ouverte tant que son expiration est dans le futur.
pub fn is_open(expires_at: OffsetDateTime, now: OffsetDateTime) -> bool {
    expires_at > now
}

/// Quelles sessions d'un compte l'opération ferme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionClosure {
    /// Toutes les sessions du compte.
    All,
    /// Toutes sauf celle qui a demandé l'opération.
    AllExcept(SessionId),
}

/// Qui change le mot de passe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasswordChange {
    /// Un administrateur (ou la ligne de commande) change le mot de passe d'un compte.
    ByAdmin,
    /// Le titulaire change son propre mot de passe depuis `current_session`, s'il en a une.
    Own { current_session: Option<SessionId> },
}

/// BR-ACCT-008 : le changement par un administrateur ferme toutes les sessions du compte.
/// BR-ACCT-009 : le titulaire garde sa session courante, les autres sont fermées.
pub fn closure_on_password_change(change: PasswordChange) -> SessionClosure {
    match change {
        PasswordChange::ByAdmin => SessionClosure::All,
        PasswordChange::Own {
            current_session: Some(current),
        } => SessionClosure::AllExcept(current),
        PasswordChange::Own {
            current_session: None,
        } => SessionClosure::All,
    }
}

/// BR-ACCT-010 : supprimer un compte ferme toutes ses sessions.
pub fn closure_on_account_deletion() -> SessionClosure {
    SessionClosure::All
}

/// BR-ACCT-011 : la révocation ferme toutes les sessions sans toucher au mot de passe.
pub fn closure_on_revocation() -> SessionClosure {
    SessionClosure::All
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::session_token::SessionToken;

    fn now() -> OffsetDateTime {
        OffsetDateTime::UNIX_EPOCH + Duration::days(20_000)
    }

    #[test]
    fn a_session_is_open_until_its_expiry() {
        assert!(is_open(now() + Duration::seconds(1), now()));
        assert!(!is_open(now(), now()));
        assert!(!is_open(now() - Duration::seconds(1), now()));
    }

    #[test]
    fn admin_password_change_closes_every_session() {
        assert_eq!(
            closure_on_password_change(PasswordChange::ByAdmin),
            SessionClosure::All
        );
    }

    #[test]
    fn own_password_change_keeps_the_current_session() {
        let current = SessionId::new("S1");
        assert_eq!(
            closure_on_password_change(PasswordChange::Own {
                current_session: Some(current.clone())
            }),
            SessionClosure::AllExcept(current)
        );
    }

    #[test]
    fn own_password_change_without_current_session_closes_everything() {
        assert_eq!(
            closure_on_password_change(PasswordChange::Own {
                current_session: None
            }),
            SessionClosure::All
        );
    }

    #[test]
    fn deletion_and_revocation_close_every_session() {
        assert_eq!(closure_on_account_deletion(), SessionClosure::All);
        assert_eq!(closure_on_revocation(), SessionClosure::All);
    }

    fn session(expires_at: OffsetDateTime) -> Session {
        Session {
            id: SessionId::new("S1"),
            account: AccountId::new("A1"),
            token_hash: SessionToken::from_bytes([1; 32]).hash(),
            client_name: "poste/1".into(),
            client_addr: "10.0.0.1".into(),
            created_at: now(),
            last_seen_at: now(),
            expires_at,
        }
    }

    #[test]
    fn a_new_session_lasts_thirty_days() {
        assert_eq!(expiry_from(now()), now() + Duration::days(30));
    }

    #[test]
    fn an_open_session_is_usable() {
        let open = session(now() + Duration::days(1));
        assert_eq!(check(Some(&open), false, now()), Ok(&open));
    }

    #[test]
    fn an_expired_session_ends_as_expired() {
        let old = session(now());
        assert_eq!(check(Some(&old), false, now()), Err(SessionEnd::Expired));
        let older = session(now() - Duration::days(3));
        assert_eq!(check(Some(&older), false, now()), Err(SessionEnd::Expired));
    }

    #[test]
    fn a_revoked_token_ends_as_revoked() {
        assert_eq!(check(None, true, now()), Err(SessionEnd::Revoked));
    }

    #[test]
    fn an_unknown_token_is_a_purged_expired_session() {
        assert_eq!(check(None, false, now()), Err(SessionEnd::Expired));
    }

    #[test]
    fn activity_pushes_the_expiry_back_only_after_the_renewal_interval() {
        let seen = now();
        assert_eq!(renewed_expiry(seen, seen + Duration::minutes(4)), None);
        assert_eq!(
            renewed_expiry(seen, seen + RENEWAL_INTERVAL),
            Some(seen + RENEWAL_INTERVAL + LIFETIME)
        );
        let later = seen + Duration::days(10);
        assert_eq!(renewed_expiry(seen, later), Some(later + LIFETIME));
    }

    #[test]
    fn a_clock_that_went_back_renews_nothing() {
        assert_eq!(renewed_expiry(now(), now() - Duration::hours(1)), None);
    }

    #[test]
    fn a_session_unused_for_thirty_days_expires_but_daily_use_keeps_it_alive() {
        let mut seen = now();
        let mut expires = expiry_from(seen);
        for _ in 0..90 {
            let tomorrow = seen + Duration::days(1);
            assert!(is_open(expires, tomorrow));
            if let Some(renewed) = renewed_expiry(seen, tomorrow) {
                expires = renewed;
            }
            seen = tomorrow;
        }
        assert!(!is_open(expires, seen + Duration::days(31)));
    }
}

//! Sessions : ce qui est vrai quel que soit le stockage. HRT-04 y ajoute le jeton et
//! l'expiration glissante ; HRT-03 n'y met que ce dont les comptes ont besoin.

use std::fmt;

use time::OffsetDateTime;

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
    use time::Duration;

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
}

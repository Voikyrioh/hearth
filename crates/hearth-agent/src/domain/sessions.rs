//! Sessions : ce qui est vrai quel que soit le stockage. Une session s'ouvre à la connexion, dure
//! 30 jours sans activité (expiration glissante), et se ferme par expiration, déconnexion ou
//! révocation (changement de mot de passe, suppression du compte, révocation explicite).

use std::fmt;

use time::{Duration, OffsetDateTime};

use super::accounts::{AccountId, Authority, ServicesRight, Username};
use super::session_token::TokenHash;

/// Durée de vie d'une session sans activité (BR-RESIL-012).
pub const LIFETIME: Duration = Duration::days(30);

/// Durée pendant laquelle l'empreinte d'un jeton révoqué est retenue (BR-RESIL-014).
pub const REVOCATION_RETENTION: Duration = Duration::days(90);

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

/// Ce que la fermeture des sessions d'un compte fait des postes de confiance et des adresses retenues.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustAfterRevocation {
    /// Tout est oublié, postes et adresses : le geste pour un poste volé (ADR-0022, ADR-0023).
    ForgetAll,
    /// Les postes à clé restent, avec leur adresse ; les adresses apprises sans clé sont oubliées
    /// (comme le changement de son propre mot de passe, BR-ACCT-009).
    ForgetWithoutDevice,
    /// Rien n'est oublié : le geste ne ferme que des sessions (BR-ACCT-018).
    Keep,
}

/// Le plan d'une fermeture de sessions demandée : quelles sessions, et ce que deviennent postes et adresses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Revocation {
    pub closure: SessionClosure,
    pub trust: TrustAfterRevocation,
}

/// BR-ACCT-017 à BR-ACCT-019 : ce que fait la fermeture des sessions de `target` (qui a le droit
/// `target_right`) demandée par `authority`.
/// - Un administrateur SANS le droit ferme un compte qui l'a : les sessions, et rien d'autre (le mot de
///   passe n'est de toute façon jamais touché ; ni les postes de confiance, ni les adresses retenues).
/// - Un compte ferme ses propres sessions : celle d'où il agit reste ouverte, comme à BR-ACCT-009.
/// - Tous les autres cas (ligne de commande du serveur, administrateur qui a le droit, cible sans le
///   droit) gardent l'effet d'avant : toutes les sessions, postes et adresses oubliés.
pub fn plan_revocation(
    authority: &Authority,
    target: &AccountId,
    target_right: ServicesRight,
    current_session: Option<&SessionId>,
) -> Revocation {
    let everything = Revocation {
        closure: closure_on_revocation(),
        trust: TrustAfterRevocation::ForgetAll,
    };
    let Authority::Network { account, right } = authority else {
        return everything;
    };
    if account == target {
        return match current_session {
            Some(current) => Revocation {
                closure: SessionClosure::AllExcept(current.clone()),
                trust: TrustAfterRevocation::ForgetWithoutDevice,
            },
            None => everything,
        };
    }
    if target_right.can_manage_services() && !right.can_manage_services() {
        return Revocation {
            closure: closure_on_revocation(),
            trust: TrustAfterRevocation::Keep,
        };
    }
    everything
}

/// BR-ACCT-019 : ce que le compte dont les sessions ont été fermées lit, une fois, à sa connexion suivante.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosureNotice {
    /// L'identifiant de l'administrateur qui a fermé les sessions.
    pub by: Username,
    pub at: OffsetDateTime,
}

/// BR-ACCT-019 : un avis n'est laissé que si un AUTRE compte a fermé au moins une session par le réseau
/// (jamais pour soi-même, jamais pour la ligne de commande qui n'a pas d'identifiant, jamais pour zéro session).
pub fn notice_for(
    authority: &Authority,
    by: Option<&Username>,
    target: &AccountId,
    closed: u64,
    at: OffsetDateTime,
) -> Option<ClosureNotice> {
    let Authority::Network { account, .. } = authority else {
        return None;
    };
    if account == target || closed == 0 {
        return None;
    }
    by.map(|by| ClosureNotice { by: by.clone(), at })
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

    fn network(account: &str, right: ServicesRight) -> Authority {
        Authority::Network {
            account: AccountId::new(account),
            right,
        }
    }

    #[test]
    fn an_administrator_without_the_right_closes_a_holder_and_nothing_else() {
        let plan = plan_revocation(
            &network("ADMIN", ServicesRight::None),
            &AccountId::new("HOLDER"),
            ServicesRight::Manage,
            Some(&SessionId::new("S1")),
        );
        assert_eq!(plan.closure, SessionClosure::All);
        assert_eq!(plan.trust, TrustAfterRevocation::Keep);
    }

    #[test]
    fn every_other_case_keeps_the_effect_it_had_before() {
        let target = AccountId::new("T");
        let everything = Revocation {
            closure: SessionClosure::All,
            trust: TrustAfterRevocation::ForgetAll,
        };
        // La ligne de commande, même sur un titulaire.
        assert_eq!(
            plan_revocation(&Authority::Server, &target, ServicesRight::Manage, None),
            everything
        );
        // Un administrateur qui a le droit, sur un titulaire.
        assert_eq!(
            plan_revocation(
                &network("ADMIN", ServicesRight::Manage),
                &target,
                ServicesRight::Manage,
                None
            ),
            everything
        );
        // Un administrateur sans le droit, sur un compte sans le droit.
        assert_eq!(
            plan_revocation(
                &network("ADMIN", ServicesRight::None),
                &target,
                ServicesRight::None,
                None
            ),
            everything
        );
    }

    #[test]
    fn closing_ones_own_sessions_keeps_the_current_one() {
        let me = AccountId::new("ME");
        let plan = plan_revocation(
            &network("ME", ServicesRight::None),
            &me,
            ServicesRight::None,
            Some(&SessionId::new("S1")),
        );
        assert_eq!(
            plan.closure,
            SessionClosure::AllExcept(SessionId::new("S1"))
        );
        assert_eq!(plan.trust, TrustAfterRevocation::ForgetWithoutDevice);
    }

    #[test]
    fn a_notice_is_left_only_when_another_account_closed_a_session_over_the_network() {
        let at = now();
        let by = Username::parse("admin").unwrap();
        let target = AccountId::new("T");
        let net = network("ADMIN", ServicesRight::None);
        assert_eq!(
            notice_for(&net, Some(&by), &target, 2, at),
            Some(ClosureNotice { by: by.clone(), at })
        );
        assert_eq!(notice_for(&net, Some(&by), &target, 0, at), None);
        assert_eq!(notice_for(&Authority::Server, None, &target, 2, at), None);
        assert_eq!(
            notice_for(
                &network("T", ServicesRight::None),
                Some(&by),
                &target,
                2,
                at
            ),
            None
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

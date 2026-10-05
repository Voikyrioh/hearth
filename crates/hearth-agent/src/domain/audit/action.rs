//! Catalogue des actions du journal (BR-AUDIT-003) : un code stable (filtre, stockage) et un
//! libellé (ce que l'administrateur lit, ce que la recherche plein texte trouve).

/// Ce qu'une entrée du journal raconte. Le résultat (réussi, refusé, échoué) est à part : une
/// connexion refusée est l'action `Login` avec le résultat « refusé ».
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuditAction {
    Login,
    /// Les échecs ont déclenché une attente (BR-AUDIT-007).
    LoginLocked,
    Logout,
    AccountCreate,
    AccountDelete,
    AccountRole,
    /// Un administrateur change le mot de passe d'un compte.
    AccountPassword,
    /// Le titulaire change son propre mot de passe.
    OwnPassword,
    SessionsRevoke,
    /// Consultation de la liste des comptes : consignée seulement quand elle est refusée.
    AccountsRead,
    /// Lecture du journal : consignée seulement quand elle est refusée (BR-AUDIT-021).
    AuditRead,
    AgentUpdate,
}

impl AuditAction {
    /// Toutes les actions, dans l'ordre du catalogue.
    pub const ALL: [AuditAction; 12] = [
        Self::Login,
        Self::LoginLocked,
        Self::Logout,
        Self::AccountCreate,
        Self::AccountDelete,
        Self::AccountRole,
        Self::AccountPassword,
        Self::OwnPassword,
        Self::SessionsRevoke,
        Self::AccountsRead,
        Self::AuditRead,
        Self::AgentUpdate,
    ];

    /// Code stable : celui du stockage et du filtre `action=`.
    pub fn code(self) -> &'static str {
        match self {
            Self::Login => "login",
            Self::LoginLocked => "login.locked",
            Self::Logout => "logout",
            Self::AccountCreate => "account.create",
            Self::AccountDelete => "account.delete",
            Self::AccountRole => "account.role",
            Self::AccountPassword => "account.password",
            Self::OwnPassword => "account.password.own",
            Self::SessionsRevoke => "sessions.revoke",
            Self::AccountsRead => "accounts.read",
            Self::AuditRead => "audit.read",
            Self::AgentUpdate => "agent.update",
        }
    }

    /// Libellé affiché (la spec fixe celui de la lecture du journal refusée).
    pub fn label(self) -> &'static str {
        match self {
            Self::Login => "Connexion",
            Self::LoginLocked => "Blocage temporaire",
            Self::Logout => "Déconnexion",
            Self::AccountCreate => "Création de compte",
            Self::AccountDelete => "Suppression de compte",
            Self::AccountRole => "Changement de rôle",
            Self::AccountPassword => "Changement du mot de passe d'un compte",
            Self::OwnPassword => "Changement de son mot de passe",
            Self::SessionsRevoke => "Fermeture des sessions",
            Self::AccountsRead => "Consultation des comptes",
            Self::AuditRead => "Tentative de lecture du journal",
            Self::AgentUpdate => "Mise à jour de l'agent",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|action| action.code() == code)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn every_action_has_a_unique_code_and_label_and_round_trips() {
        let codes: HashSet<_> = AuditAction::ALL.iter().map(|a| a.code()).collect();
        let labels: HashSet<_> = AuditAction::ALL.iter().map(|a| a.label()).collect();
        assert_eq!(codes.len(), AuditAction::ALL.len());
        assert_eq!(labels.len(), AuditAction::ALL.len());
        for action in AuditAction::ALL {
            assert_eq!(AuditAction::from_code(action.code()), Some(action));
            assert!(
                !action.label().contains('\u{2014}'),
                "pas de tiret cadratin"
            );
        }
        assert_eq!(AuditAction::from_code("inconnue"), None);
        assert_eq!(AuditAction::from_code(""), None);
    }

    #[test]
    fn the_labels_of_the_spec_are_kept() {
        assert_eq!(
            AuditAction::AuditRead.label(),
            "Tentative de lecture du journal"
        );
        assert_eq!(AuditAction::Login.label(), "Connexion");
        assert_eq!(AuditAction::Logout.label(), "Déconnexion");
        assert_eq!(AuditAction::AgentUpdate.label(), "Mise à jour de l'agent");
    }
}

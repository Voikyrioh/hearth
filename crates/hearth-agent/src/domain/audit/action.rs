//! Catalogue des actions du journal (BR-AUDIT-003) : un code stable (filtre, stockage) et un
//! libellé (ce que l'administrateur lit, ce que la recherche plein texte trouve).

use hearth_proto::api::audit::action;

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
    /// Un poste de confiance est inscrit par une connexion par mot de passe (BR-TRUST-004).
    DeviceEnroll,
    /// Un poste de confiance est retiré par son titulaire (BR-TRUST-022).
    DeviceRemove,
}

impl AuditAction {
    /// Toutes les actions, dans l'ordre du catalogue.
    pub const ALL: [AuditAction; 14] = [
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
        Self::DeviceEnroll,
        Self::DeviceRemove,
    ];

    /// Code stable : celui du stockage et du filtre `action=` (catalogue de `hearth-proto`).
    pub fn code(self) -> &'static str {
        match self {
            Self::Login => action::LOGIN,
            Self::LoginLocked => action::LOGIN_LOCKED,
            Self::Logout => action::LOGOUT,
            Self::AccountCreate => action::ACCOUNT_CREATE,
            Self::AccountDelete => action::ACCOUNT_DELETE,
            Self::AccountRole => action::ACCOUNT_ROLE,
            Self::AccountPassword => action::ACCOUNT_PASSWORD,
            Self::OwnPassword => action::ACCOUNT_PASSWORD_OWN,
            Self::SessionsRevoke => action::SESSIONS_REVOKE,
            Self::AccountsRead => action::ACCOUNTS_READ,
            Self::AuditRead => action::AUDIT_READ,
            Self::AgentUpdate => action::AGENT_UPDATE,
            Self::DeviceEnroll => action::DEVICE_ENROLL,
            Self::DeviceRemove => action::DEVICE_REMOVE,
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
            Self::DeviceEnroll => "Poste de confiance enregistré",
            Self::DeviceRemove => "Poste de confiance retiré",
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
    fn the_catalogue_of_the_protocol_lists_exactly_the_codes_of_the_agent() {
        let ours: Vec<&str> = AuditAction::ALL.iter().map(|a| a.code()).collect();
        assert_eq!(ours, action::ALL.to_vec());
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

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
    /// Une attaque probable vise un identifiant : début ou fin de l'épisode (HRT-24, BR-TRUST-008).
    SecurityAlert,
    /// Un administrateur active le mode attaque (HRT-25, BR-TRUST-030) ; la route consigne aussi ici un
    /// refus ou un échec de l'activation (résultat « refusé » ou « échoué »).
    AttackModeEnable,
    /// Le mode attaque est désactivé, par un administrateur ou par la ligne de commande ; la route consigne
    /// aussi ici un refus ou un échec de la désactivation.
    AttackModeDisable,
    /// Le mode attaque s'est arrêté tout seul : 30 minutes sans tentative refusée.
    AttackModeAutoDisable,
    /// Le mode attaque est suspendu : la machine vient de démarrer (fenêtre de 30 minutes).
    AttackModeSuspend,
    /// La fenêtre de redémarrage est finie : le mode attaque reprend.
    AttackModeResume,
    /// L'essai unique d'un poste qui n'a qu'un critère (BR-TRUST-012, 032).
    AttackModeTrial,
    /// Une session présentée seule, refusée en mode attaque (BR-TRUST-013).
    SessionRefused,
    /// Le réglage de fréquence du mot de passe d'un compte change (HRT-28) ; la route consigne aussi ici
    /// un refus ou un échec.
    ReauthSetting,
    /// Le délai de 5 minutes du mot de passe s'ouvre (HRT-30, BR-TRUST-053) : le compte, le poste de la clé
    /// prouvée, l'origine ; jamais le mot de passe.
    ReauthElevation,
    /// Un compte qui a le droit de gérer les services révèle les valeurs des variables d'un service (HRT-63, BR-SVC-009) : la cible
    /// est le service, jamais un nom ni une valeur de variable. Un refus faute de droits est consigné
    /// sous le même code par la couche d'accès.
    ServiceEnvReveal,
    /// Le droit « gérer les services » est accordé à un compte, depuis le serveur (ligne de commande ou
    /// installation, HRT-77, BR-GRANT-009). Jamais depuis une route (BR-GRANT-005).
    AccountServicesGrant,
    /// Le droit « gérer les services » est retiré à un compte, depuis le serveur (HRT-77, BR-GRANT-009).
    AccountServicesRevoke,
    /// Un conteneur existant est adopté (HRT-80, BR-ADOPT-019) : une entrée par service, cible = son nom
    /// (avec son unité s'il en a une). La route consigne aussi ici un refus ou un échec de l'acte.
    ServiceAdopt,
    /// Un service adopté est rendu à l'état non géré (HRT-80, BR-ADOPT-019).
    ServiceRelease,
    /// Lecture de la liste des conteneurs non gérés : consignée quand elle réussit (sans aucun nom) comme
    /// quand elle est refusée faute du droit (HRT-80, BR-ADOPT-005).
    ServiceExternalRead,
    /// Lecture des journaux d'un service adopté : consignée seulement quand elle est refusée faute du
    /// droit (HRT-80, BR-SLOG-024). Lire n'est jamais consigné (BR-SLOG-022).
    ServiceLogsRead,
    /// Un compte est freiné quand il ouvre des lectures de journaux en rafale (HRT-107, BR-SLOG-027) :
    /// une seule entrée par fenêtre de 10 minutes, avec le nombre de refus ; jamais une par refus.
    ServiceLogsThrottled,
    /// Un service est démarré (HRT-64).
    ServiceStart,
    /// Un service est arrêté (HRT-64).
    ServiceStop,
    /// Un service est redémarré (HRT-64).
    ServiceRestart,
    /// Un service est déployé (HRT-68, HRT-101).
    ServiceDeploy,
    /// Un service est modifié (genre d'opération `update`).
    ServiceUpdate,
    /// Un service est supprimé (HRT-85).
    ServiceDelete,
    /// Les limites d'un service changent (genre `resources`).
    ServiceResources,
    /// Un service est sauvegardé (genre `backup`).
    ServiceBackupCreate,
    /// Un service est restauré (genre `restore`).
    ServiceBackupRestore,
    /// Une sauvegarde est supprimée (HRT-97).
    ServiceBackupDelete,
    /// Un monde est repris d'ailleurs (genre `world_import`).
    ServiceWorldImport,
    /// Les réglages d'un serveur de jeu sont enregistrés (HRT-108).
    ServiceGameSettings,
    /// Un compte qui a le droit de gérer les services révèle le mot de passe d'un serveur de jeu (HRT-103,
    /// BR-JEU-026) : jamais la valeur. Un refus faute de droit est consigné sous le même code.
    ServiceGamePasswordReveal,
    /// Une annulation refusée ou en échec avant d'avoir lu l'opération (HRT-144) ; la réussite s'écrit sous l'action du genre.
    OperationCancel,
    /// Une décision refusée ou en échec avant d'avoir lu l'opération (HRT-144) ; la réussite s'écrit sous l'action du genre.
    OperationDecide,
    /// Une adoption par unité dont l'unité n'est plus relue devient « à confirmer » : elle n'ouvre plus aucun
    /// droit jusqu'à la confirmation d'un compte qui a le droit (HRT-80, BR-ADOPT-027). Auteur : le
    /// système ; cible : le service par sa clé.
    ServiceAdoptionToConfirm,
    /// Une adoption tombe sans qu'aucun compte agisse : délai de confirmation écoulé, unité ou conteneur
    /// disparu (HRT-80, BR-ADOPT-017, BR-ADOPT-027). Auteur : le système ; cible : le service par sa clé.
    ServiceAdoptionLapsed,
    /// Une fiche du catalogue est écartée au chargement (HRT-101, BR-FICHE-007). Auteur : le système.
    CatalogFicheRefused,
}

impl AuditAction {
    /// Toutes les actions, dans l'ordre du catalogue.
    pub const ALL: [AuditAction; 50] = [
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
        Self::SecurityAlert,
        Self::AttackModeEnable,
        Self::AttackModeDisable,
        Self::AttackModeAutoDisable,
        Self::AttackModeSuspend,
        Self::AttackModeResume,
        Self::AttackModeTrial,
        Self::SessionRefused,
        Self::ReauthSetting,
        Self::ReauthElevation,
        Self::ServiceEnvReveal,
        Self::AccountServicesGrant,
        Self::AccountServicesRevoke,
        Self::ServiceAdopt,
        Self::ServiceRelease,
        Self::ServiceExternalRead,
        Self::ServiceLogsRead,
        Self::ServiceLogsThrottled,
        Self::ServiceStart,
        Self::ServiceStop,
        Self::ServiceRestart,
        Self::ServiceDeploy,
        Self::ServiceUpdate,
        Self::ServiceDelete,
        Self::ServiceResources,
        Self::ServiceBackupCreate,
        Self::ServiceBackupRestore,
        Self::ServiceBackupDelete,
        Self::ServiceWorldImport,
        Self::ServiceGameSettings,
        Self::ServiceGamePasswordReveal,
        Self::OperationCancel,
        Self::OperationDecide,
        Self::ServiceAdoptionToConfirm,
        Self::ServiceAdoptionLapsed,
        Self::CatalogFicheRefused,
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
            Self::SecurityAlert => action::SECURITY_ALERT,
            Self::AttackModeEnable => action::ATTACK_MODE_ENABLE,
            Self::AttackModeDisable => action::ATTACK_MODE_DISABLE,
            Self::AttackModeAutoDisable => action::ATTACK_MODE_AUTO_DISABLE,
            Self::AttackModeSuspend => action::ATTACK_MODE_SUSPEND,
            Self::AttackModeResume => action::ATTACK_MODE_RESUME,
            Self::AttackModeTrial => action::ATTACK_MODE_TRIAL,
            Self::SessionRefused => action::SESSION_REFUSED,
            Self::ReauthSetting => action::REAUTH_SETTING,
            Self::ReauthElevation => action::REAUTH_ELEVATION,
            Self::ServiceEnvReveal => action::SERVICE_ENV_REVEAL,
            Self::AccountServicesGrant => action::ACCOUNT_SERVICES_GRANT,
            Self::AccountServicesRevoke => action::ACCOUNT_SERVICES_REVOKE,
            Self::ServiceAdopt => action::SERVICE_ADOPT,
            Self::ServiceRelease => action::SERVICE_RELEASE,
            Self::ServiceExternalRead => action::SERVICE_EXTERNAL_READ,
            Self::ServiceLogsRead => action::SERVICE_LOGS_READ,
            Self::ServiceLogsThrottled => action::SERVICE_LOGS_THROTTLED,
            Self::ServiceStart => action::SERVICE_START,
            Self::ServiceStop => action::SERVICE_STOP,
            Self::ServiceRestart => action::SERVICE_RESTART,
            Self::ServiceDeploy => action::SERVICE_DEPLOY,
            Self::ServiceUpdate => action::SERVICE_UPDATE,
            Self::ServiceDelete => action::SERVICE_DELETE,
            Self::ServiceResources => action::SERVICE_RESOURCES,
            Self::ServiceBackupCreate => action::SERVICE_BACKUP_CREATE,
            Self::ServiceBackupRestore => action::SERVICE_BACKUP_RESTORE,
            Self::ServiceBackupDelete => action::SERVICE_BACKUP_DELETE,
            Self::ServiceWorldImport => action::SERVICE_WORLD_IMPORT,
            Self::ServiceGameSettings => action::SERVICE_GAME_SETTINGS,
            Self::ServiceGamePasswordReveal => action::SERVICE_GAME_PASSWORD_REVEAL,
            Self::OperationCancel => action::OPERATION_CANCEL,
            Self::OperationDecide => action::OPERATION_DECIDE,
            Self::ServiceAdoptionToConfirm => action::SERVICE_ADOPTION_TO_CONFIRM,
            Self::ServiceAdoptionLapsed => action::SERVICE_ADOPTION_LAPSED,
            Self::CatalogFicheRefused => action::CATALOG_FICHE_REFUSED,
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
            Self::SecurityAlert => "Attaque probable signalée",
            Self::AttackModeEnable => "Mode attaque activé",
            Self::AttackModeDisable => "Mode attaque désactivé",
            Self::AttackModeAutoDisable => "Mode attaque arrêté automatiquement",
            Self::AttackModeSuspend => "Mode attaque suspendu (redémarrage de la machine)",
            Self::AttackModeResume => "Mode attaque repris",
            Self::AttackModeTrial => "Essai unique en mode attaque",
            Self::SessionRefused => "Session refusée (mode attaque)",
            Self::ReauthSetting => "Réglage de la fréquence du mot de passe",
            Self::ReauthElevation => "Délai du mot de passe ouvert (5 minutes)",
            Self::ServiceEnvReveal => "Valeurs des variables révélées",
            Self::AccountServicesGrant => "Droit de gérer les services accordé",
            Self::AccountServicesRevoke => "Droit de gérer les services retiré",
            Self::ServiceAdopt => "Service adopté",
            Self::ServiceRelease => "Adoption retirée",
            Self::ServiceExternalRead => "Liste des conteneurs existants",
            Self::ServiceLogsRead => "Journaux lus",
            Self::ServiceLogsThrottled => "Ouvertures de journaux freinées",
            Self::ServiceStart => "Démarrage d'un service",
            Self::ServiceStop => "Arrêt d'un service",
            Self::ServiceRestart => "Redémarrage d'un service",
            Self::ServiceDeploy => "Déploiement d'un service",
            Self::ServiceUpdate => "Modification d'un service",
            Self::ServiceDelete => "Suppression d'un service",
            Self::ServiceResources => "Limites d'un service",
            Self::ServiceBackupCreate => "Sauvegarde d'un service",
            Self::ServiceBackupRestore => "Restauration d'un service",
            Self::ServiceBackupDelete => "Suppression d'une sauvegarde",
            Self::ServiceWorldImport => "Reprise d'un monde",
            Self::ServiceGameSettings => "Réglages d'un jeu",
            Self::ServiceGamePasswordReveal => "Mot de passe de serveur de jeu révélé",
            Self::OperationCancel => "Annulation d'une opération",
            Self::OperationDecide => "Décision sur une opération",
            Self::ServiceAdoptionToConfirm => "Adoption à confirmer",
            Self::ServiceAdoptionLapsed => "Adoption tombée",
            Self::CatalogFicheRefused => "Fiche écartée",
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

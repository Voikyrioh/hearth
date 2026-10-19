//! Genres, statuts, causes et choix d'une opération : des listes fermées (ADR-0045). Les mêmes noms que
//! sur le fil (`hearth_proto::api::service_ops`), mais sans la variante `Unknown` : le domaine ne lit
//! jamais une valeur qu'il ne connaît pas (la base les refuse par `CHECK`).

use hearth_proto::api::service_ops::{
    OperationCause, OperationChoice, OperationKind, OperationStatus,
};

use super::locks::LockMode;
use crate::domain::audit::AuditAction;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OpKind {
    Start,
    Stop,
    Restart,
    Deploy,
    Update,
    Delete,
    Resources,
    Backup,
    Restore,
    WorldImport,
}

impl OpKind {
    pub const ALL: [OpKind; 10] = [
        Self::Start,
        Self::Stop,
        Self::Restart,
        Self::Deploy,
        Self::Update,
        Self::Delete,
        Self::Resources,
        Self::Backup,
        Self::Restore,
        Self::WorldImport,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
            Self::Deploy => "deploy",
            Self::Update => "update",
            Self::Delete => "delete",
            Self::Resources => "resources",
            Self::Backup => "backup",
            Self::Restore => "restore",
            Self::WorldImport => "world_import",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == text)
    }

    pub const fn to_wire(self) -> OperationKind {
        match self {
            Self::Start => OperationKind::Start,
            Self::Stop => OperationKind::Stop,
            Self::Restart => OperationKind::Restart,
            Self::Deploy => OperationKind::Deploy,
            Self::Update => OperationKind::Update,
            Self::Delete => OperationKind::Delete,
            Self::Resources => OperationKind::Resources,
            Self::Backup => OperationKind::Backup,
            Self::Restore => OperationKind::Restore,
            Self::WorldImport => OperationKind::WorldImport,
        }
    }

    /// L'action du journal d'activité de ce genre (HRT-144) : sous elle s'écrivent la réussite d'une
    /// annulation et celle d'une décision, quel que soit le genre. Correspondance exhaustive : un genre de
    /// plus oblige à choisir.
    pub const fn audit_action(self) -> AuditAction {
        match self {
            Self::Start => AuditAction::ServiceStart,
            Self::Stop => AuditAction::ServiceStop,
            Self::Restart => AuditAction::ServiceRestart,
            Self::Deploy => AuditAction::ServiceDeploy,
            Self::Update => AuditAction::ServiceUpdate,
            Self::Delete => AuditAction::ServiceDelete,
            Self::Resources => AuditAction::ServiceResources,
            Self::Backup => AuditAction::ServiceBackupCreate,
            Self::Restore => AuditAction::ServiceBackupRestore,
            Self::WorldImport => AuditAction::ServiceWorldImport,
        }
    }

    /// Démarrer, arrêter, redémarrer : les trois gestes de cycle de vie (HRT-64), qui ne changent que l'état.
    pub const fn is_lifecycle(self) -> bool {
        matches!(self, Self::Start | Self::Stop | Self::Restart)
    }

    /// Le genre réserve le NOM du service tant qu'il n'est pas terminé (BR-DEL-019) : suppression et
    /// déploiement. Sert à rebâtir la réservation d'une opération « à vérifier » au démarrage.
    pub const fn reserves_name(self) -> bool {
        matches!(self, Self::Deploy | Self::Delete)
    }

    /// Le genre prend l'unique place des opérations lourdes du serveur (BR-FILES-024) : sauvegarde,
    /// restauration, reprise de monde.
    pub const fn takes_heavy_io(self) -> bool {
        matches!(self, Self::Backup | Self::Restore | Self::WorldImport)
    }

    /// Le verrou que le genre prend sur son service (BR-LIFE-013) : tous les genres sont EXCLUSIFS.
    /// Les écritures de fichiers (partagé) et les lectures (aucun) ne sont pas des opérations.
    /// Correspondance exhaustive : un genre de plus oblige à choisir.
    pub const fn lock_mode(self) -> LockMode {
        match self {
            Self::Start
            | Self::Stop
            | Self::Restart
            | Self::Deploy
            | Self::Update
            | Self::Delete
            | Self::Resources
            | Self::Backup
            | Self::Restore
            | Self::WorldImport => LockMode::Exclusive,
        }
    }

    /// Ce que le compte peut décider d'une opération `needs_attention` de ce genre. « Revenir » n'est offert
    /// que si un retour en arrière existe ; « j'ai vérifié » l'est toujours (l'agent ne tranche pas à la
    /// place du compte, ADR-0045).
    pub const fn attention_choices(self) -> &'static [Choice] {
        match self {
            Self::Deploy | Self::Update | Self::Restore => {
                &[Choice::Revert, Choice::Keep, Choice::Verified]
            }
            Self::Start
            | Self::Stop
            | Self::Restart
            | Self::Delete
            | Self::Resources
            | Self::Backup
            | Self::WorldImport => &[Choice::Verified],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OpStatus {
    Accepted,
    Running,
    AwaitingDecision,
    Cancelling,
    Cancelled,
    RollingBack,
    RolledBack,
    Succeeded,
    Failed,
    Interrupted,
    NeedsAttention,
}

impl OpStatus {
    pub const ALL: [OpStatus; 11] = [
        Self::Accepted,
        Self::Running,
        Self::AwaitingDecision,
        Self::Cancelling,
        Self::Cancelled,
        Self::RollingBack,
        Self::RolledBack,
        Self::Succeeded,
        Self::Failed,
        Self::Interrupted,
        Self::NeedsAttention,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Running => "running",
            Self::AwaitingDecision => "awaiting_decision",
            Self::Cancelling => "cancelling",
            Self::Cancelled => "cancelled",
            Self::RollingBack => "rolling_back",
            Self::RolledBack => "rolled_back",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
            Self::NeedsAttention => "needs_attention",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|status| status.as_str() == text)
    }

    pub const fn to_wire(self) -> OperationStatus {
        match self {
            Self::Accepted => OperationStatus::Accepted,
            Self::Running => OperationStatus::Running,
            Self::AwaitingDecision => OperationStatus::AwaitingDecision,
            Self::Cancelling => OperationStatus::Cancelling,
            Self::Cancelled => OperationStatus::Cancelled,
            Self::RollingBack => OperationStatus::RollingBack,
            Self::RolledBack => OperationStatus::RolledBack,
            Self::Succeeded => OperationStatus::Succeeded,
            Self::Failed => OperationStatus::Failed,
            Self::Interrupted => OperationStatus::Interrupted,
            Self::NeedsAttention => OperationStatus::NeedsAttention,
        }
    }

    /// Le statut est final : l'opération ne bouge plus, son verrou est rendu.
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Cancelled | Self::RolledBack | Self::Succeeded | Self::Failed | Self::Interrupted
        )
    }

    /// L'opération tient le verrou de son service : tout statut non terminal, `needs_attention` compris.
    pub const fn holds_lock(self) -> bool {
        !self.is_terminal()
    }

    /// Les transitions du diagramme d'états (ADR-0045). `interrupted` et `needs_attention` ne sont posés
    /// qu'au démarrage de l'agent, par [`super::recover::recover`], depuis un statut non terminal.
    pub const fn can_become(self, next: OpStatus) -> bool {
        use OpStatus::{
            Accepted, AwaitingDecision, Cancelled, Cancelling, Failed, Interrupted, NeedsAttention,
            RolledBack, RollingBack, Running, Succeeded,
        };
        matches!(
            (self, next),
            (
                Accepted,
                Running | Failed | Cancelled | Interrupted | NeedsAttention
            ) | (
                Running,
                AwaitingDecision
                    | Cancelling
                    | RollingBack
                    | Succeeded
                    | Failed
                    | Interrupted
                    | NeedsAttention
            ) | (
                AwaitingDecision,
                Running | Cancelled | Interrupted | NeedsAttention
            ) | (Cancelling, Cancelled | Interrupted | NeedsAttention)
                | (RollingBack, RolledBack | Interrupted | NeedsAttention)
                // « Revenir » ne dit pas « revenu » : l'opération passe en `rolling_back`, verrou tenu, et ne
                // devient `rolled_back` que par l'exécuteur du retour arrière (jamais d'un seul geste).
                | (NeedsAttention, Succeeded | RollingBack | Failed)
        )
    }
}

/// Un choix offert au compte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Choice {
    Continue,
    Cancel,
    Revert,
    Keep,
    Verified,
}

impl Choice {
    pub const ALL: [Choice; 5] = [
        Self::Continue,
        Self::Cancel,
        Self::Revert,
        Self::Keep,
        Self::Verified,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Continue => "continue",
            Self::Cancel => "cancel",
            Self::Revert => "revert",
            Self::Keep => "keep",
            Self::Verified => "verified",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|choice| choice.as_str() == text)
    }

    /// Le libellé du choix tel que le journal d'activité l'écrit (texte fixe, jamais libre).
    pub const fn audit_label(self) -> &'static str {
        match self {
            Self::Continue => "poursuivre",
            Self::Cancel => "annuler",
            Self::Revert => "revenir",
            Self::Keep => "garder",
            Self::Verified => "j'ai vérifié",
        }
    }

    pub const fn to_wire(self) -> OperationChoice {
        match self {
            Self::Continue => OperationChoice::Continue,
            Self::Cancel => OperationChoice::Cancel,
            Self::Revert => OperationChoice::Revert,
            Self::Keep => OperationChoice::Keep,
            Self::Verified => OperationChoice::Verified,
        }
    }

    /// Lit un choix reçu du client ; `None` pour un choix que ce domaine ne connaît pas.
    pub const fn from_wire(choice: OperationChoice) -> Option<Self> {
        match choice {
            OperationChoice::Continue => Some(Self::Continue),
            OperationChoice::Cancel => Some(Self::Cancel),
            OperationChoice::Revert => Some(Self::Revert),
            OperationChoice::Keep => Some(Self::Keep),
            OperationChoice::Verified => Some(Self::Verified),
            OperationChoice::Unknown => None,
        }
    }
}

/// Pourquoi une opération est dans son statut (liste fermée de codes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Cause {
    NotExecuted,
    RunsWithPrevious,
    RunsWithNew,
    IntentWithoutDone,
    StepFailed,
    TwoStates,
    StateUnknown,
    DecisionExpired,
    CancelledByAccount,
    DecidedByAccount,
    /// Démarrer, arrêter, redémarrer (HRT-64, BR-LIFE-018) : coupé de force après le délai d'arrêt.
    ForcedStop,
    /// Un port demandé par le service est déjà pris (BR-LIFE-006).
    PortInUse,
    /// Le programme s'est arrêté dans les 10 secondes qui ont suivi son lancement (BR-LIFE-005).
    StoppedRightAfterStart,
    /// Le moteur est inaccessible (BR-LIFE-019).
    EngineUnavailable,
    /// Le moteur ou le système du serveur a refusé l'ordre (BR-LIFE-019).
    EngineRefused,
    /// Reparti tout seul après un arrêt demandé (BR-LIFE-011).
    RestartedByHost,
    /// Pas de réponse dans le délai d'arrêt plus 30 secondes (BR-LIFE-021).
    Timeout,
    /// L'agent s'est arrêté pendant l'action (BR-LIFE-020).
    Interrupted,
    /// Le service n'existait plus au moment de l'action (BR-LIFE-015).
    ServiceNotFound,
    /// Déployer (HRT-68, BR-DEPLOY-017) : l'image n'existe pas.
    ImageNotFound,
    /// L'image existe mais pas cette version.
    TagNotFound,
    /// Le registre dit la même chose d'une image privée et d'une image inconnue.
    ImagePrivateOrMissing,
    /// Le registre est injoignable depuis le serveur.
    RegistryUnreachable,
    /// La limite de téléchargements du registre est atteinte.
    RegistryRateLimited,
    /// L'image n'existe pas pour l'architecture du serveur.
    PlatformMismatch,
    /// Le disque du moteur est plein.
    DiskFull,
    /// Un conteneur a pris le nom entre la vérification et la création (BR-DEPLOY-025).
    NameTaken,
}

impl Cause {
    pub const ALL: [Cause; 27] = [
        Self::NotExecuted,
        Self::RunsWithPrevious,
        Self::RunsWithNew,
        Self::IntentWithoutDone,
        Self::StepFailed,
        Self::TwoStates,
        Self::StateUnknown,
        Self::DecisionExpired,
        Self::CancelledByAccount,
        Self::DecidedByAccount,
        Self::ForcedStop,
        Self::PortInUse,
        Self::StoppedRightAfterStart,
        Self::EngineUnavailable,
        Self::EngineRefused,
        Self::RestartedByHost,
        Self::Timeout,
        Self::Interrupted,
        Self::ServiceNotFound,
        Self::ImageNotFound,
        Self::TagNotFound,
        Self::ImagePrivateOrMissing,
        Self::RegistryUnreachable,
        Self::RegistryRateLimited,
        Self::PlatformMismatch,
        Self::DiskFull,
        Self::NameTaken,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotExecuted => "not_executed",
            Self::RunsWithPrevious => "runs_with_previous",
            Self::RunsWithNew => "runs_with_new",
            Self::IntentWithoutDone => "intent_without_done",
            Self::StepFailed => "step_failed",
            Self::TwoStates => "two_states",
            Self::StateUnknown => "state_unknown",
            Self::DecisionExpired => "decision_expired",
            Self::CancelledByAccount => "cancelled_by_account",
            Self::DecidedByAccount => "decided_by_account",
            Self::ForcedStop => "forced_stop",
            Self::PortInUse => "port_in_use",
            Self::StoppedRightAfterStart => "stopped_right_after_start",
            Self::EngineUnavailable => "engine_unavailable",
            Self::EngineRefused => "engine_refused",
            Self::RestartedByHost => "restarted_by_host",
            Self::Timeout => "timeout",
            Self::Interrupted => "interrupted",
            Self::ServiceNotFound => "service_not_found",
            Self::ImageNotFound => "image_not_found",
            Self::TagNotFound => "tag_not_found",
            Self::ImagePrivateOrMissing => "image_private_or_missing",
            Self::RegistryUnreachable => "registry_unreachable",
            Self::RegistryRateLimited => "registry_rate_limited",
            Self::PlatformMismatch => "platform_mismatch",
            Self::DiskFull => "disk_full",
            Self::NameTaken => "name_taken",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|cause| cause.as_str() == text)
    }

    pub const fn to_wire(self) -> OperationCause {
        match self {
            Self::NotExecuted => OperationCause::NotExecuted,
            Self::RunsWithPrevious => OperationCause::RunsWithPrevious,
            Self::RunsWithNew => OperationCause::RunsWithNew,
            Self::IntentWithoutDone => OperationCause::IntentWithoutDone,
            Self::StepFailed => OperationCause::StepFailed,
            Self::TwoStates => OperationCause::TwoStates,
            Self::StateUnknown => OperationCause::StateUnknown,
            Self::DecisionExpired => OperationCause::DecisionExpired,
            Self::CancelledByAccount => OperationCause::CancelledByAccount,
            Self::DecidedByAccount => OperationCause::DecidedByAccount,
            Self::ForcedStop => OperationCause::ForcedStop,
            Self::PortInUse => OperationCause::PortInUse,
            Self::StoppedRightAfterStart => OperationCause::StoppedRightAfterStart,
            Self::EngineUnavailable => OperationCause::EngineUnavailable,
            Self::EngineRefused => OperationCause::EngineRefused,
            Self::RestartedByHost => OperationCause::RestartedByHost,
            Self::Timeout => OperationCause::Timeout,
            Self::Interrupted => OperationCause::Interrupted,
            Self::ServiceNotFound => OperationCause::ServiceNotFound,
            Self::ImageNotFound => OperationCause::ImageNotFound,
            Self::TagNotFound => OperationCause::TagNotFound,
            Self::ImagePrivateOrMissing => OperationCause::ImagePrivateOrMissing,
            Self::RegistryUnreachable => OperationCause::RegistryUnreachable,
            Self::RegistryRateLimited => OperationCause::RegistryRateLimited,
            Self::PlatformMismatch => OperationCause::PlatformMismatch,
            Self::DiskFull => OperationCause::DiskFull,
            Self::NameTaken => OperationCause::NameTaken,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire<T: serde::Serialize>(value: T) -> String {
        serde_json::to_value(value)
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned()
    }

    #[test]
    fn every_name_is_the_one_of_the_wire_and_parses_back() {
        for kind in OpKind::ALL {
            assert_eq!(kind.as_str(), wire(kind.to_wire()));
            assert_eq!(OpKind::parse(kind.as_str()), Some(kind));
        }
        for status in OpStatus::ALL {
            assert_eq!(status.as_str(), wire(status.to_wire()));
            assert_eq!(OpStatus::parse(status.as_str()), Some(status));
        }
        for cause in Cause::ALL {
            assert_eq!(cause.as_str(), wire(cause.to_wire()));
            assert_eq!(Cause::parse(cause.as_str()), Some(cause));
        }
        for choice in Choice::ALL {
            assert_eq!(choice.as_str(), wire(choice.to_wire()));
            assert_eq!(Choice::parse(choice.as_str()), Some(choice));
            assert_eq!(Choice::from_wire(choice.to_wire()), Some(choice));
        }
        assert_eq!(Choice::from_wire(OperationChoice::Unknown), None);
        assert_eq!(OpKind::parse("teleport"), None);
        assert_eq!(OpStatus::parse("paused"), None);
    }

    #[test]
    fn the_migration_lists_exactly_these_kinds_and_statuses() {
        let sql = include_str!("../../../migrations/0012_service_specs_and_ops.sql");
        for kind in OpKind::ALL {
            assert!(sql.contains(&format!("'{}'", kind.as_str())), "{kind:?}");
        }
        for status in OpStatus::ALL {
            assert!(
                sql.contains(&format!("'{}'", status.as_str())),
                "{status:?}"
            );
        }
    }

    #[test]
    fn the_statuses_that_hold_the_lock_are_the_ones_the_unique_index_covers() {
        let sql = include_str!("../../../migrations/0012_service_specs_and_ops.sql");
        let index = sql
            .split("CREATE UNIQUE INDEX service_ops_open")
            .nth(1)
            .and_then(|rest| rest.split(';').next())
            .unwrap();
        for status in OpStatus::ALL {
            assert_eq!(
                index.contains(&format!("'{}'", status.as_str())),
                status.holds_lock(),
                "{status:?}"
            );
        }
    }

    #[test]
    fn terminal_statuses_never_move_again() {
        for from in OpStatus::ALL.into_iter().filter(|s| s.is_terminal()) {
            for to in OpStatus::ALL {
                assert!(!from.can_become(to), "{from:?} -> {to:?}");
            }
        }
    }

    #[test]
    fn the_transitions_are_those_of_the_state_diagram() {
        use OpStatus::*;
        let allowed = [
            (Accepted, Running),
            (Running, AwaitingDecision),
            (AwaitingDecision, Running),
            (AwaitingDecision, Cancelled),
            (Running, Cancelling),
            (Cancelling, Cancelled),
            (Running, RollingBack),
            (RollingBack, RolledBack),
            (RollingBack, NeedsAttention),
            (Running, Succeeded),
            (Running, Failed),
            (Running, Interrupted),
            (Running, NeedsAttention),
            (NeedsAttention, Succeeded),
            (NeedsAttention, RollingBack),
            (NeedsAttention, Failed),
        ];
        for (from, to) in allowed {
            assert!(from.can_become(to), "{from:?} -> {to:?}");
        }
        // Quelques interdits qui compteraient : sauter l'acceptation, ressusciter, défaire sans avoir fait.
        for (from, to) in [
            (Accepted, Succeeded),
            (Accepted, RollingBack),
            (Succeeded, Running),
            (Cancelled, Running),
            (NeedsAttention, Running),
            (NeedsAttention, RolledBack),
            (NeedsAttention, Cancelled),
            (Cancelling, Succeeded),
            (RollingBack, Succeeded),
            (Interrupted, NeedsAttention),
        ] {
            assert!(!from.can_become(to), "{from:?} -> {to:?}");
        }
    }

    #[test]
    fn every_kind_takes_the_exclusive_lock_and_offers_the_verified_choice_when_in_doubt() {
        for kind in OpKind::ALL {
            assert_eq!(kind.lock_mode(), LockMode::Exclusive);
            assert!(
                kind.attention_choices().contains(&Choice::Verified),
                "{kind:?}"
            );
        }
        // Une suppression ne se défait pas : « revenir » n'y est pas offert.
        assert!(!OpKind::Delete.attention_choices().contains(&Choice::Revert));
        assert!(OpKind::Update.attention_choices().contains(&Choice::Revert));
    }
}

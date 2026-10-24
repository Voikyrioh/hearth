//! Ce que la coquille dit à l'interface d'une opération de service (HRT-84). Une copie fidèle de
//! `OperationView` : aucune valeur de variable, aucune commande, aucun chemin du serveur (`BR-OPS-006`).
//! Chaque énumération porte `Unknown` : une valeur plus récente que ce client se lit « inconnue », jamais
//! comme une erreur.

use hearth_proto::api::service_ops::{
    DecisionView, OperationCause, OperationChoice, OperationKind, OperationStatus, OperationView,
    StepStateView, StepView,
};
use serde::{Deserialize, Serialize};
use specta::Type;

/// Événement d'un changement d'opération (nom côté fenêtre).
pub const EVENT: &str = "link://service-operations";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OperationKindDto {
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
    Unknown,
}

impl From<OperationKind> for OperationKindDto {
    fn from(kind: OperationKind) -> Self {
        match kind {
            OperationKind::Start => Self::Start,
            OperationKind::Stop => Self::Stop,
            OperationKind::Restart => Self::Restart,
            OperationKind::Deploy => Self::Deploy,
            OperationKind::Update => Self::Update,
            OperationKind::Delete => Self::Delete,
            OperationKind::Resources => Self::Resources,
            OperationKind::Backup => Self::Backup,
            OperationKind::Restore => Self::Restore,
            OperationKind::WorldImport => Self::WorldImport,
            OperationKind::Unknown => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OperationStatusDto {
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
    Unknown,
}

impl From<OperationStatus> for OperationStatusDto {
    fn from(status: OperationStatus) -> Self {
        match status {
            OperationStatus::Accepted => Self::Accepted,
            OperationStatus::Running => Self::Running,
            OperationStatus::AwaitingDecision => Self::AwaitingDecision,
            OperationStatus::Cancelling => Self::Cancelling,
            OperationStatus::Cancelled => Self::Cancelled,
            OperationStatus::RollingBack => Self::RollingBack,
            OperationStatus::RolledBack => Self::RolledBack,
            OperationStatus::Succeeded => Self::Succeeded,
            OperationStatus::Failed => Self::Failed,
            OperationStatus::Interrupted => Self::Interrupted,
            OperationStatus::NeedsAttention => Self::NeedsAttention,
            OperationStatus::Unknown => Self::Unknown,
        }
    }
}

/// Un choix de décision. Sert aussi d'ENTRÉE à `decide_service_operation` : `Unknown` n'est jamais envoyé.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OperationChoiceDto {
    Continue,
    Cancel,
    Revert,
    Keep,
    Verified,
    Unknown,
}

impl From<OperationChoice> for OperationChoiceDto {
    fn from(choice: OperationChoice) -> Self {
        match choice {
            OperationChoice::Continue => Self::Continue,
            OperationChoice::Cancel => Self::Cancel,
            OperationChoice::Revert => Self::Revert,
            OperationChoice::Keep => Self::Keep,
            OperationChoice::Verified => Self::Verified,
            OperationChoice::Unknown => Self::Unknown,
        }
    }
}

impl From<OperationChoiceDto> for OperationChoice {
    fn from(choice: OperationChoiceDto) -> Self {
        match choice {
            OperationChoiceDto::Continue => Self::Continue,
            OperationChoiceDto::Cancel => Self::Cancel,
            OperationChoiceDto::Revert => Self::Revert,
            OperationChoiceDto::Keep => Self::Keep,
            OperationChoiceDto::Verified => Self::Verified,
            OperationChoiceDto::Unknown => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OperationCauseDto {
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
    ForcedStop,
    PortInUse,
    StoppedRightAfterStart,
    EngineUnavailable,
    EngineRefused,
    RestartedByHost,
    Timeout,
    Interrupted,
    ServiceNotFound,
    ImageNotFound,
    TagNotFound,
    ImagePrivateOrMissing,
    RegistryUnreachable,
    RegistryRateLimited,
    PlatformMismatch,
    DiskFull,
    NameTaken,
    DataNotErased,
    Unknown,
}

impl From<OperationCause> for OperationCauseDto {
    fn from(cause: OperationCause) -> Self {
        match cause {
            OperationCause::NotExecuted => Self::NotExecuted,
            OperationCause::RunsWithPrevious => Self::RunsWithPrevious,
            OperationCause::RunsWithNew => Self::RunsWithNew,
            OperationCause::IntentWithoutDone => Self::IntentWithoutDone,
            OperationCause::StepFailed => Self::StepFailed,
            OperationCause::TwoStates => Self::TwoStates,
            OperationCause::StateUnknown => Self::StateUnknown,
            OperationCause::DecisionExpired => Self::DecisionExpired,
            OperationCause::CancelledByAccount => Self::CancelledByAccount,
            OperationCause::DecidedByAccount => Self::DecidedByAccount,
            OperationCause::ForcedStop => Self::ForcedStop,
            OperationCause::PortInUse => Self::PortInUse,
            OperationCause::StoppedRightAfterStart => Self::StoppedRightAfterStart,
            OperationCause::EngineUnavailable => Self::EngineUnavailable,
            OperationCause::EngineRefused => Self::EngineRefused,
            OperationCause::RestartedByHost => Self::RestartedByHost,
            OperationCause::Timeout => Self::Timeout,
            OperationCause::Interrupted => Self::Interrupted,
            OperationCause::ServiceNotFound => Self::ServiceNotFound,
            OperationCause::ImageNotFound => Self::ImageNotFound,
            OperationCause::TagNotFound => Self::TagNotFound,
            OperationCause::ImagePrivateOrMissing => Self::ImagePrivateOrMissing,
            OperationCause::RegistryUnreachable => Self::RegistryUnreachable,
            OperationCause::RegistryRateLimited => Self::RegistryRateLimited,
            OperationCause::PlatformMismatch => Self::PlatformMismatch,
            OperationCause::DiskFull => Self::DiskFull,
            OperationCause::NameTaken => Self::NameTaken,
            OperationCause::DataNotErased => Self::DataNotErased,
            OperationCause::Unknown => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum StepStateDto {
    Intent,
    Done,
    Failed,
    Undone,
    Unknown,
}

impl From<StepStateView> for StepStateDto {
    fn from(state: StepStateView) -> Self {
        match state {
            StepStateView::Intent => Self::Intent,
            StepStateView::Done => Self::Done,
            StepStateView::Failed => Self::Failed,
            StepStateView::Undone => Self::Undone,
            StepStateView::Unknown => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
pub struct StepDto {
    pub step: String,
    pub state: StepStateDto,
}

impl From<&StepView> for StepDto {
    fn from(step: &StepView) -> Self {
        Self {
            step: step.step.clone(),
            state: step.state.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DecisionDto {
    pub choices: Vec<OperationChoiceDto>,
    /// Quand la décision expire (ms depuis l'époque, heure de l'agent) ; absent pour une opération à
    /// vérifier, qui attend sans limite.
    pub expires_at: Option<f64>,
}

impl From<&DecisionView> for DecisionDto {
    fn from(decision: &DecisionView) -> Self {
        Self {
            choices: decision.choices.iter().map(|c| (*c).into()).collect(),
            expires_at: decision.expires_at.as_deref().and_then(rfc3339_millis),
        }
    }
}

/// `2026-10-14T10:15:00Z` (avec ou sans fraction) en millisecondes depuis l'époque ; illisible : `None`.
fn rfc3339_millis(text: &str) -> Option<f64> {
    let (date, time) = text.split_once('T')?;
    let time = time.strip_suffix('Z')?;
    let mut date_parts = date.split('-');
    let year: i64 = date_parts.next()?.parse().ok()?;
    let month: i64 = date_parts.next()?.parse().ok()?;
    let day: i64 = date_parts.next()?.parse().ok()?;
    let (clock, fraction) = time.split_once('.').unwrap_or((time, "0"));
    let mut clock_parts = clock.split(':');
    let hour: i64 = clock_parts.next()?.parse().ok()?;
    let minute: i64 = clock_parts.next()?.parse().ok()?;
    let second: i64 = clock_parts.next()?.parse().ok()?;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let millis: i64 = format!("{fraction:0<3}").get(..3)?.parse().ok()?;
    // Jours depuis 1970-01-01 (algorithme de Howard Hinnant).
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let total = ((days * 24 + hour) * 60 + minute) * 60 + second;
    Some((total * 1000 + millis) as f64)
}

/// Une opération telle que l'interface la suit.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OperationDto {
    pub id: String,
    /// La clé du service touché (`h_`, `u_`, `c_`).
    pub key: String,
    pub kind: OperationKindDto,
    pub status: OperationStatusDto,
    pub step: Option<String>,
    pub steps: Vec<StepDto>,
    pub percent: Option<u32>,
    pub items_done: Option<f64>,
    pub bytes_done: Option<f64>,
    pub bytes_total: Option<f64>,
    pub eta_s: Option<f64>,
    pub cancellable: bool,
    pub decision: Option<DecisionDto>,
    pub cause: Option<OperationCauseDto>,
}

impl From<&OperationView> for OperationDto {
    fn from(view: &OperationView) -> Self {
        // Destructuré sans `..` : un champ ajouté au fil force à décider s'il traverse (BR-OPS-006).
        let OperationView {
            id,
            key,
            kind,
            status,
            step,
            steps,
            percent,
            items_done,
            bytes_done,
            bytes_total,
            eta_s,
            cancellable,
            decision,
            cause,
        } = view;
        Self {
            id: id.clone(),
            key: key.clone(),
            kind: (*kind).into(),
            status: (*status).into(),
            step: step.clone(),
            steps: steps.iter().map(StepDto::from).collect(),
            percent: percent.map(u32::from),
            items_done: items_done.map(|n| n as f64),
            bytes_done: bytes_done.map(|n| n as f64),
            bytes_total: bytes_total.map(|n| n as f64),
            eta_s: eta_s.map(|n| n as f64),
            cancellable: *cancellable,
            decision: decision.as_ref().map(DecisionDto::from),
            cause: cause.map(Into::into),
        }
    }
}

/// Ce qui a changé pour une opération.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OperationChangeDto {
    /// L'opération a un nouvel état (en cours ou terminée : l'interface le lit dans `status`).
    Upsert { operation: Box<OperationDto> },
    /// L'agent ne la connaît plus : elle n'est plus suivie, sans issue à annoncer.
    Gone { id: String },
}

/// Un changement, avec le serveur et un numéro qui croît strictement (rejeu écarté par l'interface).
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ServiceOperationEvent {
    pub server_id: String,
    pub seq: u32,
    pub change: OperationChangeDto,
}

/// Pourquoi l'agent a refusé une décision. Sans texte : l'interface choisit le message d'après `kind`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DecisionRefusal {
    /// L'opération est introuvable (purgée, ou service que le compte ne voit pas).
    NotFound,
    /// Rien n'est attendu de ce compte (déjà décidée, ou pas encore).
    NotWaiting,
    /// Ce choix n'est pas parmi ceux offerts.
    NotOffered,
    /// Ce choix ne s'applique pas à l'état de l'opération.
    NotApplicable,
    /// La décision attendait depuis trop longtemps.
    Expired,
    /// L'annulation n'est plus possible : le point de non-retour est passé.
    TooLate,
    /// L'opération a changé pendant la demande : relire.
    Changed,
    /// Le mot de passe de confirmation est incorrect.
    WrongPassword,
    /// L'élévation s'est fermée côté agent : la fenêtre redemande le mot de passe.
    PasswordRequired,
    /// Trop d'essais de mot de passe.
    TooManyAttempts { retry_after_s: u32 },
    /// L'agent est saturé : réessayer dans un instant.
    Busy,
    /// Tout autre refus.
    Other,
}

/// L'issue d'une décision.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DecisionOutcome {
    /// L'agent a pris la décision : l'opération telle qu'elle est maintenant.
    Done {
        operation: Box<OperationDto>,
    },
    Refused {
        refusal: DecisionRefusal,
    },
    /// Le lien est tombé avant la réponse : on ne sait pas. Jamais rejouée ; l'état arrive par le flux.
    Unknown {
        #[serde(rename = "opId")]
        op_id: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_decision_deadline_is_read_in_milliseconds_since_the_epoch() {
        assert_eq!(rfc3339_millis("1970-01-01T00:00:00Z"), Some(0.0));
        assert_eq!(
            rfc3339_millis("2000-03-01T00:00:00Z"),
            Some(951_868_800_000.0)
        );
        assert_eq!(
            rfc3339_millis("2000-03-01T00:00:01.250Z"),
            Some(951_868_801_250.0)
        );
        assert_eq!(rfc3339_millis("hier"), None);
        assert_eq!(rfc3339_millis("2026-13-14T10:15:00Z"), None);
    }

    #[test]
    fn an_operation_never_carries_more_than_the_view_says() {
        let view: OperationView = serde_json::from_value(serde_json::json!({
            "id": "01J", "key": "h_01J", "kind": "teleport", "status": "levitating",
            "cause": "something_new", "percent": 250,
            "decision": { "choices": ["continue", "dance"] }
        }))
        .unwrap();
        let dto = OperationDto::from(&view);
        assert_eq!(dto.kind, OperationKindDto::Unknown);
        assert_eq!(dto.status, OperationStatusDto::Unknown);
        assert_eq!(dto.cause, Some(OperationCauseDto::Unknown));
        // Un pourcentage hors bornes est inconnu, jamais borné (BR-COMPAT-001 point 10).
        assert_eq!(dto.percent, None);
        assert_eq!(
            dto.decision.unwrap().choices,
            [OperationChoiceDto::Continue, OperationChoiceDto::Unknown]
        );
    }
}

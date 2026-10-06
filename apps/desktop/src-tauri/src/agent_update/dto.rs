//! Ce que la coquille dit à l'interface sur la mise à jour de l'agent (types sérialisés, typés par
//! tauri-specta). Aucun texte : l'interface choisit les siens d'après les codes. Aucune adresse,
//! aucune signature, aucune somme : la cible de l'agent ne sort jamais de la coquille ; l'interface
//! ne voit que le numéro de la version disponible.

use hearth_proto::api::update::{
    AgentUpdateStatus, UpdateOutcome, UpdateProgress, UpdateReason, UpdateResult, UpdateStep,
};
use serde::Serialize;
use specta::Type;

/// Nom de l'événement Tauri qui porte chaque progression reçue du flux de l'agent.
pub const PROGRESS_EVENT: &str = "agent-update://progress";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AgentUpdateStepDto {
    Download,
    Verify,
    Install,
    Restart,
    Check,
    Done,
}

impl From<UpdateStep> for AgentUpdateStepDto {
    fn from(step: UpdateStep) -> Self {
        match step {
            UpdateStep::Download => Self::Download,
            UpdateStep::Verify => Self::Verify,
            UpdateStep::Install => Self::Install,
            UpdateStep::Restart => Self::Restart,
            UpdateStep::Check => Self::Check,
            UpdateStep::Done => Self::Done,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AgentUpdateOutcomeDto {
    Succeeded,
    RolledBack,
    Failed,
}

impl From<UpdateOutcome> for AgentUpdateOutcomeDto {
    fn from(outcome: UpdateOutcome) -> Self {
        match outcome {
            UpdateOutcome::Succeeded => Self::Succeeded,
            UpdateOutcome::RolledBack => Self::RolledBack,
            UpdateOutcome::Failed => Self::Failed,
        }
    }
}

/// Pourquoi une mise à jour n'a pas abouti. `Unknown` : une raison qu'un agent plus récent ajoute
/// et que ce client ne connaît pas (jamais une erreur de lecture).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AgentUpdateReasonDto {
    Unreachable,
    DownloadFailed,
    BadChecksum,
    BadSignature,
    BadBinary,
    Staging,
    Swap,
    SupervisorLaunch,
    NoAnswer,
    IdentityChanged,
    Interrupted,
    RollbackFailed,
    Unknown,
}

impl From<UpdateReason> for AgentUpdateReasonDto {
    fn from(reason: UpdateReason) -> Self {
        match reason {
            UpdateReason::Unreachable => Self::Unreachable,
            UpdateReason::DownloadFailed => Self::DownloadFailed,
            UpdateReason::BadChecksum => Self::BadChecksum,
            UpdateReason::BadSignature => Self::BadSignature,
            UpdateReason::BadBinary => Self::BadBinary,
            UpdateReason::Staging => Self::Staging,
            UpdateReason::Swap => Self::Swap,
            UpdateReason::SupervisorLaunch => Self::SupervisorLaunch,
            UpdateReason::NoAnswer => Self::NoAnswer,
            UpdateReason::IdentityChanged => Self::IdentityChanged,
            UpdateReason::Interrupted => Self::Interrupted,
            UpdateReason::RollbackFailed => Self::RollbackFailed,
            UpdateReason::Unknown => Self::Unknown,
        }
    }
}

/// Où en est la mise à jour en cours.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentUpdateProgressDto {
    pub version: String,
    pub step: AgentUpdateStepDto,
    /// 0 à 100, pour l'étape `download` seulement.
    pub percent: Option<u8>,
    /// Pour l'étape `done` seulement.
    pub outcome: Option<AgentUpdateOutcomeDto>,
    pub reason: Option<AgentUpdateReasonDto>,
}

impl From<&UpdateProgress> for AgentUpdateProgressDto {
    fn from(progress: &UpdateProgress) -> Self {
        Self {
            version: progress.version.clone(),
            step: progress.step.into(),
            percent: progress.percent.map(|percent| percent.min(100)),
            outcome: progress.outcome.map(Into::into),
            reason: progress.reason.map(Into::into),
        }
    }
}

/// Le dernier résultat connu (survit au redémarrage de l'agent).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentUpdateResultDto {
    /// La version visée ; `None` quand elle ne se sait pas (trace de travail illisible conclue au
    /// démarrage) : le texte affiché n'a alors pas de numéro.
    pub version: Option<String>,
    pub previous: String,
    pub outcome: AgentUpdateOutcomeDto,
    pub reason: Option<AgentUpdateReasonDto>,
    /// RFC 3339, UTC (celui de l'agent) : l'interface le met en forme.
    pub at: String,
    /// Le résultat date de moins de 24 h : il est annoncé comme un message (BR-UPDATE-017) ; plus
    /// ancien, il n'est qu'une ligne d'historique. Décidé ici : l'interface ne calcule aucune date.
    pub recent: bool,
}

impl AgentUpdateResultDto {
    pub fn new(result: &UpdateResult, recent: bool) -> Self {
        Self {
            version: (!result.version_unknown && !result.version.is_empty())
                .then(|| result.version.clone()),
            previous: result.previous.clone(),
            outcome: result.outcome.into(),
            reason: result.reason.map(Into::into),
            at: result.at.clone(),
            recent,
        }
    }
}

/// Une version plus récente que l'agent est disponible (BR-UPDATE-022). Seulement son numéro.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentAvailableDto {
    pub version: String,
}

/// L'état de la mise à jour de l'agent d'un serveur, relu à la demande (affichage, retour du lien).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentUpdateView {
    /// La version de l'agent qui répond (BR-UPDATE-022).
    pub current: String,
    /// Installation gérée par le système (ou sans systemd) : pas de mise à jour à distance.
    pub managed: bool,
    pub in_progress: bool,
    pub progress: Option<AgentUpdateProgressDto>,
    pub last: Option<AgentUpdateResultDto>,
    /// La version disponible dans le flux de versions, strictement plus récente que `current` ;
    /// `None` si le flux n'en propose pas, ou si l'installation est gérée (aucun bouton à montrer).
    pub available: Option<AgentAvailableDto>,
}

impl AgentUpdateView {
    pub fn new(
        status: &AgentUpdateStatus,
        last: Option<AgentUpdateResultDto>,
        available: Option<AgentAvailableDto>,
    ) -> Self {
        Self {
            current: status.current.clone(),
            managed: status.managed,
            in_progress: status.in_progress,
            progress: status.progress.as_ref().map(Into::into),
            last,
            available: available.filter(|_| !status.managed),
        }
    }
}

/// Événement `agent-update://progress` : une progression reçue du flux d'un serveur.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentUpdateEvent {
    pub server_id: String,
    pub progress: AgentUpdateProgressDto,
}

/// Pourquoi la demande n'a pas été acceptée. Sans texte : l'interface choisit le message d'après
/// `kind`. Le refus de rôle de l'agent est `LinkFailure::Forbidden` (une seule façon de le dire).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentUpdateRefusal {
    /// Installation gérée par le système (ou sans systemd) : aucune mise à jour à distance.
    ManagedInstall,
    /// Une mise à jour est déjà en cours (BR-UPDATE-012).
    InProgress,
    /// La signature de la cible est refusée par l'agent, avant tout téléchargement.
    BadSignature,
    /// Un champ de la cible est refusé par l'agent (`version`, `url`, `sha256`, `signature`).
    InvalidTarget,
    /// Le flux de versions ne propose aucune version à ce client (rien n'a été envoyé).
    NoTarget,
    /// La version que l'utilisateur a vue n'est plus celle que le client retient (rien n'a été
    /// envoyé) : relire l'état et confirmer de nouveau.
    TargetChanged,
    /// La version retenue n'est pas plus récente que l'agent (rien n'a été envoyé : jamais de
    /// rétrogradation).
    NotNewer,
    Other,
}

/// Issue de la demande de mise à jour de l'agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentUpdateOutcome {
    /// L'agent a accepté : la mise à jour s'exécute chez lui, l'avancement arrive par le flux.
    Accepted {
        version: String,
    },
    Refused {
        refusal: AgentUpdateRefusal,
    },
    /// Le lien est tombé avant la réponse : on ne sait pas, la demande n'est JAMAIS rejouée.
    /// L'issue arrive par `link://operation` sous cet identifiant ; l'état se relit au retour du lien.
    Unknown {
        op_id: String,
    },
}

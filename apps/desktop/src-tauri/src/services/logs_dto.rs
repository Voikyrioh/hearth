//! Ce que la coquille dit à l'interface d'une lecture de journaux (HRT-67, ADR-0040).
//!
//! Copie fidèle de `hearth_proto::api::service_logs` pour ce qu'affiche l'onglet Journaux : chaque conversion
//! DESTRUCTURE le type du protocole sans `..`. Le TEXTE d'une ligne est du contenu sensible (secrets possibles) :
//! `LogLineDto` n'a pas de `Debug` qui le montre, et rien de ce module n'est tracé. Toute valeur inconnue d'un agent
//! plus récent devient `Unknown` : l'interface l'affiche de façon neutre, sans en déduire un état.
//!
//! Nombres bornés pour `specta` (pas de `u64` ni de `i64`) : les heures sont des millisecondes depuis l'époque en
//! `f64` (exact sur 53 bits).

use hearth_link::{LogEnd, LogOutput};
use hearth_proto::api::service_logs::{
    GapReason, LogGap, LogItem, LogLines, LogMarker, LogSource, LogState, LogsErrorCode,
    LogsStatus, MarkerKind,
};
use serde::Serialize;
use specta::Type;

/// Nom de l'événement des lectures (une seule voie pour toutes les lectures : la charge porte la session).
pub const EVENT: &str = "link://service-logs";

/// Un message d'une lecture, adressé à sa session.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ServiceLogEvent {
    pub session: String,
    pub body: ServiceLogBody,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum LogStateDto {
    Live,
    Ended,
    Waiting,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum LogSourceDto {
    Out,
    Err,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum GapReasonDto {
    Slow,
    Rotated,
    ResumeLost,
    FirstPage,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum MarkerKindDto {
    Restarted,
    Unknown,
}

/// Le code d'un refus de l'agent. `Unknown` : un code d'un agent plus récent, une fin générique.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum LogRefusalDto {
    LogsLimit,
    LogsAccountLimit,
    LogsRateLimited,
    LogsUnreadable,
    ServiceNotFound,
    EngineUnavailable,
    MissingRight,
    BadRequest,
    Unknown,
}

/// Pourquoi une lecture est finie.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum LogEndDto {
    /// L'agent a refusé ou fermé la lecture ; `retry_after_s` seulement pour le frein.
    Refused {
        refusal: LogRefusalDto,
        retry_after_s: Option<u32>,
    },
    SessionEnded,
    Unsupported,
    Unreachable,
}

/// Une ligne. Le texte est celui de l'agent, déjà nettoyé et coupé à 10 000 caractères ; l'interface le neutralise
/// encore (`safeText`) avant de l'afficher. `Debug` ne le montre pas.
#[derive(Clone, PartialEq, Serialize, Type)]
pub struct LogLineDto {
    /// Heure du moteur, millisecondes depuis l'époque.
    pub t: f64,
    pub s: LogSourceDto,
    pub x: String,
    /// Nombre total de caractères d'une ligne que l'agent a coupée.
    pub cut: Option<u32>,
}

impl std::fmt::Debug for LogLineDto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LogLineDto")
            .field("t", &self.t)
            .field("s", &self.s)
            .field("chars", &self.x.chars().count())
            .field("cut", &self.cut)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum ServiceLogBody {
    Status {
        state: LogStateDto,
        container: Option<String>,
        mixed: bool,
        exit_code: Option<i32>,
        ended_at: Option<f64>,
        not_kept: bool,
    },
    Lines {
        seq: u32,
        items: Vec<LogLineDto>,
    },
    Gap {
        count: Option<u32>,
        reason: GapReasonDto,
    },
    Marker {
        marker: MarkerKindDto,
        at: f64,
    },
    /// Le socket est tombé ou le lien est coupé : la lecture reprend seule.
    Interrupted,
    /// La lecture est finie et ne repart pas seule.
    Ended {
        end: LogEndDto,
    },
}

fn narrow(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn millis(value: i64) -> f64 {
    // Des millisecondes depuis l'époque tiennent dans 53 bits : la conversion est exacte.
    value as f64
}

impl From<LogsStatus> for ServiceLogBody {
    fn from(status: LogsStatus) -> Self {
        let LogsStatus {
            state,
            container,
            mixed,
            exit_code,
            ended_at,
            not_kept,
        } = status;
        Self::Status {
            state: match state {
                LogState::Live => LogStateDto::Live,
                LogState::Ended => LogStateDto::Ended,
                LogState::Waiting => LogStateDto::Waiting,
                LogState::Unknown => LogStateDto::Unknown,
            },
            container,
            mixed,
            exit_code: exit_code.map(|code| i32::try_from(code).unwrap_or(i32::MAX)),
            ended_at: ended_at.map(millis),
            not_kept,
        }
    }
}

impl From<LogItem> for LogLineDto {
    fn from(item: LogItem) -> Self {
        let LogItem { t, s, x, cut } = item;
        Self {
            t: millis(t),
            s: match s {
                LogSource::Out => LogSourceDto::Out,
                LogSource::Err => LogSourceDto::Err,
                LogSource::Unknown => LogSourceDto::Unknown,
            },
            x,
            cut,
        }
    }
}

impl From<LogLines> for ServiceLogBody {
    fn from(lines: LogLines) -> Self {
        // Le curseur de reprise reste dans la liaison : l'interface n'en a pas besoin.
        let LogLines {
            seq,
            items,
            cursor: _,
        } = lines;
        Self::Lines {
            seq: narrow(seq),
            items: items.into_iter().map(LogLineDto::from).collect(),
        }
    }
}

impl From<LogGap> for ServiceLogBody {
    fn from(gap: LogGap) -> Self {
        let LogGap { count, reason } = gap;
        Self::Gap {
            count: count.map(narrow),
            reason: match reason {
                GapReason::Slow => GapReasonDto::Slow,
                GapReason::Rotated => GapReasonDto::Rotated,
                GapReason::ResumeLost => GapReasonDto::ResumeLost,
                GapReason::FirstPage => GapReasonDto::FirstPage,
                GapReason::Unknown => GapReasonDto::Unknown,
            },
        }
    }
}

impl From<LogMarker> for ServiceLogBody {
    fn from(marker: LogMarker) -> Self {
        let LogMarker { kind, at } = marker;
        Self::Marker {
            marker: match kind {
                MarkerKind::Restarted => MarkerKindDto::Restarted,
                MarkerKind::Unknown => MarkerKindDto::Unknown,
            },
            at: millis(at),
        }
    }
}

fn refusal(code: LogsErrorCode) -> LogRefusalDto {
    match code {
        LogsErrorCode::LogsLimit => LogRefusalDto::LogsLimit,
        LogsErrorCode::LogsAccountLimit => LogRefusalDto::LogsAccountLimit,
        LogsErrorCode::LogsRateLimited => LogRefusalDto::LogsRateLimited,
        LogsErrorCode::LogsUnreadable => LogRefusalDto::LogsUnreadable,
        LogsErrorCode::ServiceNotFound => LogRefusalDto::ServiceNotFound,
        LogsErrorCode::EngineUnavailable => LogRefusalDto::EngineUnavailable,
        LogsErrorCode::MissingRight => LogRefusalDto::MissingRight,
        LogsErrorCode::BadRequest => LogRefusalDto::BadRequest,
        // Rendu à part par la liaison (`SessionEnded`) : n'arrive pas ici ; si c'était le cas, une fin générique.
        LogsErrorCode::Unauthenticated => LogRefusalDto::Unknown,
        // Un code d'un agent plus récent : une fin générique et neutre (le socket n'a pas de statut HTTP à lire).
        LogsErrorCode::Unknown => LogRefusalDto::Unknown,
    }
}

impl From<LogEnd> for LogEndDto {
    fn from(end: LogEnd) -> Self {
        match end {
            LogEnd::Refused {
                code,
                retry_after_s,
            } => Self::Refused {
                refusal: refusal(code),
                retry_after_s,
            },
            LogEnd::SessionEnded => Self::SessionEnded,
            LogEnd::Unsupported => Self::Unsupported,
            LogEnd::Unreachable => Self::Unreachable,
        }
    }
}

impl From<LogOutput> for ServiceLogBody {
    fn from(output: LogOutput) -> Self {
        match output {
            LogOutput::Status(status) => status.into(),
            LogOutput::Lines(lines) => lines.into(),
            LogOutput::Gap(gap) => gap.into(),
            LogOutput::Marker(marker) => marker.into(),
            LogOutput::Interrupted => Self::Interrupted,
            LogOutput::Ended(end) => Self::Ended { end: end.into() },
        }
    }
}

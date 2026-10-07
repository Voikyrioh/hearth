//! Journal d'activité côté coquille (HRT-14) : types échangés avec l'interface, conversions depuis
//! `hearth-link` / `hearth-proto`, lecture d'une page, export vers un fichier choisi par
//! l'UTILISATEUR, relais des entrées en direct (`link://audit`).
//!
//! Aucune règle ici : le filtre est validé par `hearth_link::domain::audit_query`, le chemin est
//! construit par la bibliothèque, le CSV est celui de l'agent (ou de `hearth_proto::api::audit_csv`).
//! Rien de ce que dit la page ne choisit une adresse, un chemin de requête ou un fichier : la page
//! donne un FILTRE typé ; l'emplacement du fichier vient de la boîte de dialogue native
//! d'enregistrement (ADR-0019), jamais de la page. Les dates sont des secondes ou des textes UTC,
//! les identifiants des `f64` entiers (un `i64` n'existe pas côté web).

use std::path::PathBuf;

use async_trait::async_trait;
use hearth_link::adapters::file_store::write_atomic;
use hearth_link::domain::audit_query::{
    ActionKind, AuditFilter, FilterError, PAGE_SIZE, RawFilter,
};
use hearth_link::domain::server::ServerId;
use hearth_proto::api::audit::{AuditEventItem, AuditResponse, OriginKindName, OutcomeName};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::link::LinkRuntime;
use crate::link_dto::{InvalidField, LinkFailure};

/// Événement d'une entrée du journal reçue en direct.
pub const EVENT: &str = "link://audit";

/// Événement « le flux a perdu des entrées » (retard de la liaison) : la page relit la tête du journal.
pub const GAP_EVENT: &str = "link://audit-gap";

/// Nom de fichier proposé à l'enregistrement.
pub const SUGGESTED_FILE_NAME: &str = "journal-hearth.csv";

/// Type d'action du filtre (liste fermée de la spec).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AuditKindDto {
    LoginOk,
    LoginDenied,
    Accounts,
    Update,
    Denied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AuditOutcomeDto {
    Ok,
    Denied,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AuditOriginKindDto {
    Client,
    Cli,
    Assistant,
    // L'agent lui-même (HRT-25) : fin d'alerte, sortie automatique du mode attaque.
    System,
}

/// Le filtre demandé par l'interface (BR-AUDIT-014, 015, 016). Dates : secondes depuis l'époque.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AuditFilterDto {
    pub accounts: Vec<String>,
    pub kinds: Vec<AuditKindDto>,
    pub outcomes: Vec<AuditOutcomeDto>,
    pub from_s: Option<f64>,
    pub to_s: Option<f64>,
    pub text: Option<String>,
}

fn seconds(value: Option<f64>) -> Result<Option<i64>, FilterError> {
    match value {
        None => Ok(None),
        Some(v) if v.is_finite() && (0.0..=1.0e12).contains(&v) => {
            // Entier non négatif et borné ci-dessus : la conversion ne perd rien.
            #[allow(clippy::cast_possible_truncation)]
            Ok(Some(v.floor() as i64))
        }
        Some(_) => Err(FilterError::BadDate),
    }
}

impl TryFrom<AuditFilterDto> for AuditFilter {
    type Error = FilterError;

    fn try_from(dto: AuditFilterDto) -> Result<Self, FilterError> {
        AuditFilter::new(RawFilter {
            accounts: dto.accounts,
            kinds: dto
                .kinds
                .into_iter()
                .map(|kind| match kind {
                    AuditKindDto::LoginOk => ActionKind::LoginOk,
                    AuditKindDto::LoginDenied => ActionKind::LoginDenied,
                    AuditKindDto::Accounts => ActionKind::Accounts,
                    AuditKindDto::Update => ActionKind::Update,
                    AuditKindDto::Denied => ActionKind::Denied,
                })
                .collect(),
            outcomes: dto
                .outcomes
                .into_iter()
                .map(|outcome| match outcome {
                    AuditOutcomeDto::Ok => OutcomeName::Ok,
                    AuditOutcomeDto::Denied => OutcomeName::Denied,
                    AuditOutcomeDto::Failed => OutcomeName::Failed,
                })
                .collect(),
            from_s: seconds(dto.from_s)?,
            to_s: seconds(dto.to_s)?,
            text: dto.text,
        })
    }
}

/// D'où vient l'action.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AuditOriginDto {
    pub kind: AuditOriginKindDto,
    pub name: Option<String>,
    pub addr: Option<String>,
    /// Le texte à afficher, fourni par l'agent.
    pub text: String,
}

/// Une entrée du journal. `at` : RFC 3339 en UTC, la source (l'interface l'affiche dans le fuseau du
/// poste, BR-AUDIT-012). Toute valeur est du texte non fiable (saisi par des tiers).
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntryDto {
    pub id: f64,
    pub at: String,
    pub account: Option<String>,
    pub origin: AuditOriginDto,
    pub action: String,
    pub action_label: String,
    pub target: Option<String>,
    pub outcome: AuditOutcomeDto,
    pub reason: Option<String>,
    pub repeat_count: u32,
}

impl From<&AuditEventItem> for AuditEntryDto {
    fn from(item: &AuditEventItem) -> Self {
        Self {
            // Un identifiant d'entrée est un entier positif bien en deçà de 2^53.
            #[allow(clippy::cast_precision_loss)]
            id: item.id as f64,
            at: item.at.clone(),
            account: item.account.clone(),
            origin: AuditOriginDto {
                kind: match item.origin.kind {
                    OriginKindName::Client => AuditOriginKindDto::Client,
                    OriginKindName::Cli => AuditOriginKindDto::Cli,
                    OriginKindName::Assistant => AuditOriginKindDto::Assistant,
                    OriginKindName::System => AuditOriginKindDto::System,
                },
                name: item.origin.name.clone(),
                addr: item.origin.addr.clone(),
                text: item.origin.text.clone(),
            },
            action: item.action.clone(),
            action_label: item.action_label.clone(),
            target: item.target.clone(),
            outcome: match item.outcome {
                OutcomeName::Ok => AuditOutcomeDto::Ok,
                OutcomeName::Denied => AuditOutcomeDto::Denied,
                OutcomeName::Failed => AuditOutcomeDto::Failed,
            },
            reason: item.reason.clone(),
            repeat_count: item.repeat_count,
        }
    }
}

/// Une page du journal filtré, de la plus récente à la plus ancienne.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AuditPageDto {
    pub events: Vec<AuditEntryDto>,
    /// Curseur de la page suivante (absent à la fin).
    pub next_before: Option<f64>,
}

impl From<&AuditResponse> for AuditPageDto {
    fn from(page: &AuditResponse) -> Self {
        Self {
            events: page.events.iter().map(AuditEntryDto::from).collect(),
            #[allow(clippy::cast_precision_loss)]
            next_before: page.next_before.map(|id| id as f64),
        }
    }
}

/// Une entrée reçue en direct (`link://audit`).
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AuditLiveEvent {
    pub server_id: String,
    pub event: AuditEntryDto,
}

pub fn live(server: &ServerId, item: &AuditEventItem) -> AuditLiveEvent {
    AuditLiveEvent {
        server_id: server.to_string(),
        event: AuditEntryDto::from(item),
    }
}

/// Issue d'un export : enregistré, ou abandonné dans la boîte de dialogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AuditExportDto {
    /// `false` : l'utilisateur a fermé la boîte de dialogue, rien n'est écrit.
    pub saved: bool,
    /// Seules les 10 000 entrées les plus récentes du résultat sont dans le fichier.
    pub truncated: bool,
}

/// La boîte de dialogue d'enregistrement : le SEUL moyen de désigner le fichier de l'export.
#[async_trait]
pub trait SaveDialog: Send + Sync {
    /// Le fichier choisi par l'utilisateur, `None` s'il annule. Le système d'exploitation demande
    /// lui-même confirmation avant d'écraser un fichier existant.
    async fn choose(&self, suggested_name: &str) -> Option<PathBuf>;
}

fn invalid_filter(_: FilterError) -> LinkFailure {
    LinkFailure::InvalidInput {
        field: InvalidField::Other,
    }
}

fn server(text: &str) -> Result<ServerId, LinkFailure> {
    ServerId::parse(text).map_err(|_| LinkFailure::UnknownServer)
}

/// Une page du journal (BR-AUDIT-001 : un compte lecture seule reçoit `Forbidden`, et l'agent
/// consigne ce refus, BR-AUDIT-021). `before` : curseur d'une page précédente.
pub async fn read_page(
    runtime: &LinkRuntime,
    server_id: &str,
    filter: AuditFilterDto,
    before: Option<f64>,
) -> Result<AuditPageDto, LinkFailure> {
    let id = server(server_id)?;
    let filter = AuditFilter::try_from(filter).map_err(invalid_filter)?;
    let before = match before {
        None => None,
        // Un curseur est un entier strictement positif, borné : sinon c'est un curseur inventé.
        Some(value) if (1.0..=9.0e15).contains(&value) && value.fract() == 0.0 =>
        {
            #[allow(clippy::cast_possible_truncation)]
            Some(value as i64)
        }
        Some(_) => return Err(invalid_filter(FilterError::BadCursor)),
    };
    let page = runtime
        .manager()
        .audit_page(&id, &filter, before, PAGE_SIZE)
        .await?;
    Ok(AuditPageDto::from(&page))
}

/// Exporte le résultat filtré : le fichier est demandé à l'utilisateur APRÈS la lecture (une erreur
/// de lecture n'ouvre pas de boîte de dialogue) et écrit là où il l'a dit, nulle part ailleurs.
pub async fn export(
    runtime: &LinkRuntime,
    server_id: &str,
    filter: AuditFilterDto,
    dialog: &dyn SaveDialog,
) -> Result<AuditExportDto, LinkFailure> {
    let id = server(server_id)?;
    let filter = AuditFilter::try_from(filter).map_err(invalid_filter)?;
    let file = runtime.manager().audit_export(&id, &filter).await?;
    let Some(destination) = dialog.choose(SUGGESTED_FILE_NAME).await else {
        return Ok(AuditExportDto {
            saved: false,
            truncated: file.truncated,
        });
    };
    // Écriture atomique (fichier temporaire à côté, synchronisation, renommage) : un export
    // interrompu ne laisse jamais un CSV tronqué qui ressemble à un journal complet, et un export
    // précédent que l'utilisateur a accepté d'écraser n'est remplacé qu'une fois le nouveau entier.
    if let Err(error) = write_atomic(&destination, &file.bytes).await {
        tracing::warn!(%error, "export du journal : fichier non écrit");
        return Err(LinkFailure::Storage);
    }
    Ok(AuditExportDto {
        saved: true,
        truncated: file.truncated,
    })
}

/// La boîte de dialogue native d'enregistrement de Windows (`rfd`, déjà embarquée pour la boîte
/// d'erreur de démarrage), rattachée à la fenêtre de l'application.
pub struct NativeSaveDialog(pub tauri::WebviewWindow);

#[async_trait]
impl SaveDialog for NativeSaveDialog {
    async fn choose(&self, suggested_name: &str) -> Option<PathBuf> {
        rfd::AsyncFileDialog::new()
            .set_parent(&self.0)
            .set_title(crate::texts::EXPORT_DIALOG_TITLE)
            .set_file_name(suggested_name)
            .add_filter(crate::texts::EXPORT_FILTER_NAME, &["csv"])
            .save_file()
            .await
            .map(|handle| handle.path().to_owned())
    }
}

//! Ce que la coquille échange avec l'interface pour la console d'un serveur de jeu et les joueurs connectés (HRT-291,
//! ADR-0067, `BR-CONSOLE-001` à `011`).
//!
//! - **Entrée** : une commande (texte tapé) et la classe que l'interface a jugée (`1` libre, `2` sensible). L'agent
//!   rejuge : une autre classe est refusée.
//! - **Sorties** : copies fidèles de `hearth_proto::api::{console, players}`, énumérations en mots du fil (liste
//!   fermée côté interface, toute valeur inconnue se lit « inconnue »). La réponse du jeu et les pseudos sont du texte
//!   hostile : l'interface les montre en texte brut, jamais en HTML ; `Debug` ne montre ni la réponse ni les pseudos.

use hearth_proto::api::console::{ConsoleReply, ConsoleView};
use hearth_proto::api::players::PlayersView;
use serde::Serialize;
use specta::Type;

use super::mods_dto::wire;

/// Un verbe que Hearth n'envoie jamais.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RefusedVerbDto {
    pub verb: String,
    /// `use_hearth_stop`, `use_hearth_restart` ou `breaks_server`.
    pub reason: String,
}

/// Un verbe sensible, avec les mots du jeu pour dire pourquoi le mot de passe est demandé.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SensitiveVerbDto {
    pub verb: String,
    pub title: String,
    pub sentence: String,
    /// `first_arg` ou `none`.
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConsolePolicyDto {
    pub refused: Vec<RefusedVerbDto>,
    pub sensitive: Vec<SensitiveVerbDto>,
}

/// Une commande rapide de la fiche.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct QuickCommandDto {
    pub id: String,
    pub label: String,
    pub text: String,
    /// `send` ou `fill`.
    pub mode: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConsoleLimitsDto {
    pub max_chars: u32,
    pub per_minute: u32,
    pub reply_timeout_s: u32,
}

/// Ce que la console permet maintenant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConsoleViewDto {
    pub available: bool,
    /// `adopted`, `no_console`, `protocol_unsupported` ou `unknown` ; absent si la console est ouverte.
    pub unavailable_reason: Option<String>,
    /// `ready`, `starting`, `stopped`, `busy` ou `unknown`.
    pub state: String,
    pub can_write: bool,
    pub policy: ConsolePolicyDto,
    pub quick: Vec<QuickCommandDto>,
    pub limits: Option<ConsoleLimitsDto>,
}

impl From<&ConsoleView> for ConsoleViewDto {
    fn from(view: &ConsoleView) -> Self {
        Self {
            available: view.available,
            unavailable_reason: view.unavailable_reason.as_ref().map(wire),
            state: wire(&view.state),
            can_write: view.can_write,
            policy: ConsolePolicyDto {
                refused: view
                    .policy
                    .refused
                    .iter()
                    .map(|verb| RefusedVerbDto {
                        verb: verb.verb.clone(),
                        reason: verb.reason.clone(),
                    })
                    .collect(),
                sensitive: view
                    .policy
                    .sensitive
                    .iter()
                    .map(|verb| SensitiveVerbDto {
                        verb: verb.verb.clone(),
                        title: verb.title.clone(),
                        sentence: verb.sentence.clone(),
                        target: verb.target.clone(),
                    })
                    .collect(),
            },
            quick: view
                .quick
                .iter()
                .map(|quick| QuickCommandDto {
                    id: quick.id.clone(),
                    label: quick.label.clone(),
                    text: quick.text.clone(),
                    mode: quick.mode.clone(),
                })
                .collect(),
            limits: view.limits.map(|limits| ConsoleLimitsDto {
                max_chars: limits.max_chars,
                per_minute: limits.per_minute,
                reply_timeout_s: limits.reply_timeout_s,
            }),
        }
    }
}

/// Pourquoi l'agent n'a pas donné une lecture de la console ou des joueurs. Sans texte.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConsoleReadRefusal {
    ServiceNotFound,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConsoleOutcome {
    Ready { view: Box<ConsoleViewDto> },
    Refused { refusal: ConsoleReadRefusal },
}

/// Les joueurs connectés. Les pseudos sont du texte du jeu : `Debug` ne les montre pas.
#[derive(Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlayersViewDto {
    /// `ok`, `stopped`, `starting`, `unavailable`, `no_console` ou `unknown`.
    pub state: String,
    pub online: Option<u32>,
    pub max: Option<u32>,
    pub names: Option<Vec<String>>,
    pub hidden_names: u32,
    pub more_names: u32,
    pub read_at: Option<String>,
}

impl std::fmt::Debug for PlayersViewDto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "PlayersViewDto({}, {:?} en ligne)",
            self.state, self.online
        )
    }
}

impl From<&PlayersView> for PlayersViewDto {
    fn from(view: &PlayersView) -> Self {
        Self {
            state: wire(&view.state),
            online: view.online,
            max: view.max,
            names: view.names.clone(),
            hidden_names: view.hidden_names,
            more_names: view.more_names,
            read_at: view.read_at.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlayersOutcome {
    Ready { view: Box<PlayersViewDto> },
    Refused { refusal: ConsoleReadRefusal },
}

/// La réponse du jeu à une commande : du texte hostile, jamais montré par `Debug`.
#[derive(Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConsoleReplyDto {
    /// `answered`, `empty`, `no_answer` ou `unknown`.
    pub outcome: String,
    pub reply: Option<String>,
    pub truncated: bool,
}

impl std::fmt::Debug for ConsoleReplyDto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ConsoleReplyDto({}, truncated {})",
            self.outcome, self.truncated
        )
    }
}

impl From<&ConsoleReply> for ConsoleReplyDto {
    fn from(reply: &ConsoleReply) -> Self {
        Self {
            outcome: wire(&reply.outcome),
            reply: reply.reply.clone(),
            truncated: reply.truncated,
        }
    }
}

/// Pourquoi l'agent a refusé une commande. Sans texte : l'interface choisit les phrases d'après `kind`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConsoleRefusal {
    WrongPassword,
    PasswordRequired,
    TooManyAttempts {
        retry_after_s: u32,
    },
    Busy,
    ServiceNotFound,
    ServiceAdopted,
    /// La fiche n'ouvre pas la console à Hearth.
    NotAvailable,
    /// Forme refusée : `empty`, `too_long` ou `control_char`.
    Invalid {
        reason: Option<String>,
    },
    /// Verbe refusé : `use_hearth_stop`, `use_hearth_restart` ou `breaks_server`.
    VerbRefused {
        reason: Option<String>,
    },
    /// La classe signée n'est pas celle que l'agent juge : mettre Hearth à jour.
    ClassMismatch,
    NotRunning,
    Starting,
    /// Une opération exclusive tient le service.
    OperationInProgress,
    RateLimited {
        retry_after_s: Option<u32>,
    },
    /// La console du jeu n'a pas pu être jointe.
    Unreachable,
    Other,
}

/// L'issue d'une commande : répondue, refusée, ou résultat inconnu (le lien est tombé avant la réponse : la commande a
/// peut-être été exécutée, elle n'est jamais renvoyée).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConsoleSendOutcome {
    Replied {
        reply: ConsoleReplyDto,
    },
    Refused {
        refusal: ConsoleRefusal,
    },
    Unknown {
        #[serde(rename = "opId")]
        op_id: String,
    },
}

//! La console d'un serveur de jeu (HRT-268, ADR-0067, BR-CONSOLE-001 à 009) : `GET /services/{key}/console` (ce que la
//! console permet), `POST /services/{key}/console` (envoyer UNE commande).
//!
//! La classe d'une commande et la politique des verbes viennent de la fiche du jeu, jamais du client : l'agent rejuge
//! tout (`crate::console::classify`). Le texte d'une commande et la réponse du jeu sont du contenu sensible : ces types
//! n'ont pas de `Debug` qui les montre.
//!
//! Évolution : tout champ ajouté plus tard est facultatif (`serde(default)`) ; une valeur d'énumération inconnue se
//! lit `Unknown` (jamais un repli vers « permis »).

use serde::{Deserialize, Serialize};

use crate::console::{ConsoleLine, normalize};

/// Le nom de la capacité annoncée par `/hello` quand l'agent sert les routes de la console (HRT-268).
pub const CAPABILITY_GAME_CONSOLE: &str = "game-console";

/// La classe signée d'une commande libre.
pub const CLASS_FREE: u8 = 1;
/// La classe signée d'une commande sensible.
pub const CLASS_SENSITIVE: u8 = 2;

/// Pourquoi la console n'est pas ouverte à Hearth sur ce service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnavailableReason {
    /// Service adopté : Hearth ne lui parle jamais.
    Adopted,
    /// Le jeu n'a pas de console, ou la fiche ne l'ouvre pas à Hearth.
    NoConsole,
    /// Le protocole de la console n'est pas parlé par cette version.
    ProtocolUnsupported,
    #[serde(other)]
    Unknown,
}

/// Ce que la console permet maintenant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsoleState {
    /// Le jeu est prêt : une commande part.
    Ready,
    /// Le service démarre, le jeu n'est pas prêt.
    Starting,
    /// Le service est arrêté.
    Stopped,
    /// Une opération exclusive tient le service.
    Busy,
    #[serde(other)]
    Unknown,
}

/// Un verbe que Hearth n'envoie jamais.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefusedVerbView {
    pub verb: String,
    /// `use_hearth_stop`, `use_hearth_restart` ou `breaks_server`.
    pub reason: String,
}

/// Un verbe sensible : de quoi expliquer, avec les mots du jeu, pourquoi le mot de passe est demandé.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SensitiveVerbView {
    pub verb: String,
    pub title: String,
    pub sentence: String,
    /// `first_arg` : le deuxième mot est pris pour `{joueur}` ; `none` sinon.
    pub target: String,
}

/// La politique des verbes de la fiche, pour que l'écran classe avant d'envoyer (l'agent reclasse toujours).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsolePolicyView {
    #[serde(default)]
    pub refused: Vec<RefusedVerbView>,
    #[serde(default)]
    pub sensitive: Vec<SensitiveVerbView>,
}

/// Une commande rapide de l'écran.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuickView {
    pub id: String,
    pub label: String,
    pub text: String,
    /// `send` : envoyée telle quelle ; `fill` : posée dans le champ, à compléter.
    pub mode: String,
}

/// Les bornes de la console.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsoleLimits {
    pub max_chars: u32,
    pub per_minute: u32,
    pub reply_timeout_s: u32,
}

/// La réponse de `GET /services/{key}/console`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsoleView {
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<UnavailableReason>,
    pub state: ConsoleState,
    /// Le compte a le droit de gérer les services : il peut écrire.
    pub can_write: bool,
    #[serde(default)]
    pub policy: ConsolePolicyView,
    #[serde(default)]
    pub quick: Vec<QuickView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limits: Option<ConsoleLimits>,
}

/// Le corps de `POST /services/{key}/console` : la commande telle que tapée, et la classe que le client a jugée (et
/// signée). L'agent reclasse : une classe différente de la sienne est refusée (`CONSOLE_CLASS_MISMATCH`).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsoleRequest {
    pub command: String,
    pub class: u8,
}

impl std::fmt::Debug for ConsoleRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ConsoleRequest({} caractères, classe {})",
            self.command.chars().count(),
            self.class
        )
    }
}

impl ConsoleRequest {
    /// L'empreinte que l'acte `ConsoleCommand` signe : SHA-256 de la ligne NORMALISÉE. Une ligne qui n'a pas la forme
    /// d'une commande ne se signe pas côté client ; l'agent, lui, signe l'empreinte du texte reçu pour pouvoir répondre
    /// `CONSOLE_COMMAND_INVALID` plutôt qu'un refus de preuve.
    pub fn act_digest(&self) -> [u8; 32] {
        match normalize(&self.command) {
            Ok(line) => crate::console::line_digest(&line),
            Err(_) => {
                use sha2::{Digest, Sha256};
                Sha256::digest(self.command.as_bytes()).into()
            }
        }
    }

    /// La ligne normalisée, si elle a la forme d'une commande.
    pub fn line(&self) -> Result<ConsoleLine, crate::console::LineError> {
        normalize(&self.command)
    }
}

/// L'issue d'une commande envoyée.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsoleOutcome {
    /// Le jeu a répondu.
    Answered,
    /// Le jeu n'a rien dit : la commande est partie.
    Empty,
    /// Le délai est dépassé : la commande a peut-être été exécutée, elle n'est jamais renvoyée.
    NoAnswer,
    #[serde(other)]
    Unknown,
}

/// La réponse de `POST /services/{key}/console`.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsoleReply {
    pub outcome: ConsoleOutcome,
    /// Le texte du jeu, jamais interprété ; absent pour `empty` et `no_answer`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply: Option<String>,
    #[serde(default)]
    pub truncated: bool,
}

impl std::fmt::Debug for ConsoleReply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ConsoleReply({:?}, truncated {})",
            self.outcome, self.truncated
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_digest_is_the_one_of_the_normalized_line() {
        let a = ConsoleRequest {
            command: "  /time set day ".into(),
            class: CLASS_FREE,
        };
        let b = ConsoleRequest {
            command: "time set day".into(),
            class: CLASS_FREE,
        };
        assert_eq!(a.act_digest(), b.act_digest());
        let other = ConsoleRequest {
            command: "time set night".into(),
            class: CLASS_FREE,
        };
        assert_ne!(a.act_digest(), other.act_digest());
        // Une ligne hors forme a quand même une empreinte (celle de ses octets), sans panique.
        let bad = ConsoleRequest {
            command: "say a\nstop".into(),
            class: CLASS_FREE,
        };
        assert_ne!(bad.act_digest(), [0; 32]);
        assert!(bad.line().is_err());
    }

    #[test]
    fn the_debug_form_never_shows_the_text() {
        let request = ConsoleRequest {
            command: "op Secret-Pseudo".into(),
            class: CLASS_SENSITIVE,
        };
        assert!(!format!("{request:?}").contains("Secret"));
        let reply = ConsoleReply {
            outcome: ConsoleOutcome::Answered,
            reply: Some("Made Secret-Pseudo a server operator".into()),
            truncated: false,
        };
        assert!(!format!("{reply:?}").contains("Secret"));
    }

    #[test]
    fn an_unknown_word_is_read_unknown_never_as_a_permission() {
        let state: ConsoleState = serde_json::from_str("\"melting\"").unwrap();
        assert_eq!(state, ConsoleState::Unknown);
        let outcome: ConsoleOutcome = serde_json::from_str("\"exploded\"").unwrap();
        assert_eq!(outcome, ConsoleOutcome::Unknown);
        let reason: UnavailableReason = serde_json::from_str("\"later\"").unwrap();
        assert_eq!(reason, UnavailableReason::Unknown);
    }

    #[test]
    fn the_reply_omits_absent_text() {
        let json = serde_json::to_value(ConsoleReply {
            outcome: ConsoleOutcome::NoAnswer,
            reply: None,
            truncated: false,
        })
        .unwrap();
        assert!(json.get("reply").is_none());
        assert_eq!(json["outcome"], "no_answer");
    }
}

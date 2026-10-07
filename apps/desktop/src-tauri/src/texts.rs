//! Textes affichés par la coquille elle-même (menu, notification, boîte
//! d'erreur de démarrage). Français, tutoiement. Les textes de l'interface
//! vivent dans `src/i18n/fr.ts`.

use std::path::Path;

use hearth_link::domain::state::LinkState;

use crate::presence::{AlertKind, SecurityWording};

pub const APP_NAME: &str = "Hearth";
pub const MENU_OPEN_LABEL: &str = "Ouvrir Hearth";
pub const MENU_QUIT_LABEL: &str = "Quitter";
pub const CLOSE_HINT: &str = "Hearth continue de fonctionner. Clique sur l'icône pour rouvrir.";
pub const STARTUP_FAILED_TITLE: &str = "Hearth n'a pas pu démarrer";
/// Boîte d'enregistrement de l'export du journal (HRT-14).
pub const EXPORT_DIALOG_TITLE: &str = "Exporter le journal";
pub const EXPORT_FILTER_NAME: &str = "Tableur (CSV)";
/// Notification système du lien (BR-RESIL-015) : `{Nom} est hors ligne.` / `{Nom} est de nouveau
/// connecté.` ; si des changements ont été absorbés par la limite d'une par minute, le nombre
/// est ajouté (texte hors spec, BR-RESIL-018).
pub fn link_alert_body(name: &str, kind: AlertKind, suppressed: u32) -> String {
    let head = match kind {
        AlertKind::Offline => format!("{name} est hors ligne."),
        AlertKind::Back => format!("{name} est de nouveau connecté."),
        // Les natures de sécurité ont leur texte (`security_alert_body`) ; jamais d'agrégation.
        AlertKind::AttackProbable | AlertKind::AttackModeStopped => {
            return format!("{name} : alerte de sécurité.");
        }
    };
    if suppressed == 0 {
        head
    } else {
        format!("{head} Le lien a changé {suppressed} fois depuis la dernière alerte.")
    }
}

/// Titre d'une notification de sécurité (conception design, écran D) : `Hearth : {nom du serveur}`.
pub fn security_title(name: &str) -> String {
    format!("{APP_NAME} : {name}")
}

/// Corps d'une notification de sécurité (BR-TRUST-009, 019, 033). Jamais le nom d'un autre compte :
/// l'agent n'en donne que le nombre.
pub fn security_alert_body(kind: AlertKind, wording: Option<&SecurityWording>) -> String {
    match kind {
        AlertKind::AttackModeStopped => {
            "L'attaque semble terminée. Le mode attaque s'est arrêté automatiquement.".to_owned()
        }
        _ => match wording {
            Some(SecurityWording::Owner { username }) => format!(
                "Attaque probable sur ton compte {username}. Clique pour activer le mode attaque."
            ),
            Some(SecurityWording::Details { username }) => {
                format!("Attaque probable sur ton compte {username}. Clique pour voir les détails.")
            }
            Some(SecurityWording::Others { count: 1 }) => {
                "Attaque probable sur 1 compte du serveur. Clique pour voir les détails.".to_owned()
            }
            Some(SecurityWording::Others { count }) => format!(
                "Attaque probable sur {count} comptes du serveur. Clique pour voir les détails."
            ),
            None => "Attaque probable sur ton compte. Clique pour voir les détails.".to_owned(),
        },
    }
}

/// Libellé d'un état du lien (les mêmes mots que la pastille de l'interface).
pub fn link_state_label(state: LinkState) -> &'static str {
    match state {
        LinkState::Connected => "Connecté",
        LinkState::Reconnecting => "Reconnexion…",
        LinkState::Offline => "Hors ligne",
        LinkState::SessionExpired => "Session expirée",
        LinkState::AccessRevoked => "Accès révoqué",
    }
}

/// Infobulle de l'icône de la zone de notification : `Hearth : forge, Hors ligne` ; sans serveur,
/// le nom seul.
pub fn tray_tooltip(reflected: Option<(&str, LinkState)>) -> String {
    match reflected {
        Some((name, state)) => format!("{APP_NAME} : {name}, {}", link_state_label(state)),
        None => APP_NAME.to_owned(),
    }
}

pub const PANIC_TITLE: &str = "Hearth a rencontré une erreur grave";

/// Corps de la boîte de message affichée quand une panique ne peut pas être
/// écrite au journal.
pub fn panic_body(panic: &str) -> String {
    format!(
        "Hearth doit se fermer, et le journal n'est pas disponible pour garder la trace de l'erreur.\n\n{panic}"
    )
}

/// Corps de la boîte de message affichée quand le démarrage échoue. Si le
/// journal n'a pas pu s'ouvrir, on le dit au lieu de renvoyer vers un fichier
/// qui n'existe pas.
pub fn startup_failed_body(error: &str, log_dir: &Path, log_problem: Option<&str>) -> String {
    let log = match log_problem {
        None => format!("Le détail est dans le journal : {}", log_dir.display()),
        Some(problem) => format!("Le journal n'a pas pu être écrit ({problem})."),
    };
    format!(
        "Hearth n'a pas pu démarrer.

{error}

{log}"
    )
}

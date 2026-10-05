//! Textes affichés par la coquille elle-même (menu, notification, boîte
//! d'erreur de démarrage). Français, tutoiement. Les textes de l'interface
//! vivent dans `src/i18n/fr.ts`.

use std::path::Path;

use hearth_link::domain::state::LinkState;

use crate::presence::AlertKind;

pub const APP_NAME: &str = "Hearth";
pub const MENU_OPEN_LABEL: &str = "Ouvrir Hearth";
pub const MENU_QUIT_LABEL: &str = "Quitter";
pub const CLOSE_HINT: &str = "Hearth continue de fonctionner. Clique sur l'icône pour rouvrir.";
pub const STARTUP_FAILED_TITLE: &str = "Hearth n'a pas pu démarrer";
/// Notification système du lien (BR-RESIL-015) : `{Nom} est hors ligne.` / `{Nom} est de nouveau
/// connecté.` ; si des changements ont été absorbés par la limite d'une par minute, le nombre
/// est ajouté (texte hors spec, BR-RESIL-018).
pub fn link_alert_body(name: &str, kind: AlertKind, suppressed: u32) -> String {
    let head = match kind {
        AlertKind::Offline => format!("{name} est hors ligne."),
        AlertKind::Back => format!("{name} est de nouveau connecté."),
        AlertKind::Failures(n) => format!("{name} : Reconnexion échouée {n} fois."),
    };
    if suppressed == 0 {
        head
    } else {
        format!("{head} Le lien a changé {suppressed} fois depuis la dernière alerte.")
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

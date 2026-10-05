//! Textes affichés par la coquille elle-même (menu, notification, boîte
//! d'erreur de démarrage). Français, tutoiement. Les textes de l'interface
//! vivent dans `src/i18n/fr.ts`.

use std::path::Path;

pub const APP_NAME: &str = "Hearth";
pub const MENU_OPEN_LABEL: &str = "Ouvrir Hearth";
pub const MENU_QUIT_LABEL: &str = "Quitter";
pub const CLOSE_HINT: &str = "Hearth continue de fonctionner. Clique sur l'icône pour rouvrir.";
pub const STARTUP_FAILED_TITLE: &str = "Hearth n'a pas pu démarrer";
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

//! Textes affichés par la coquille elle-même (menu, notification, boîte
//! d'erreur de démarrage). Français, tutoiement. Les textes de l'interface
//! vivent dans `src/i18n/fr.ts`.

use std::path::Path;

pub const APP_NAME: &str = "Hearth";
pub const MENU_OPEN_LABEL: &str = "Ouvrir Hearth";
pub const MENU_QUIT_LABEL: &str = "Quitter";
pub const CLOSE_HINT: &str = "Hearth continue de fonctionner. Clique sur l'icône pour rouvrir.";
pub const STARTUP_FAILED_TITLE: &str = "Hearth n'a pas pu démarrer";

/// Corps de la boîte de message affichée quand le démarrage échoue.
pub fn startup_failed_body(error: &str, log_dir: &Path) -> String {
    format!(
        "Hearth n'a pas pu démarrer.\n\n{error}\n\nLe détail est dans le journal : {}",
        log_dir.display()
    )
}

//! Règles de la coquille, pures : aucune E/S, aucun type Tauri.

use serde::{Deserialize, Serialize};
use specta::Type;

/// Identifiant de l'application (`tauri.conf.json`) : nom du dossier de données.
pub const IDENTIFIER: &str = "fr.voikyrioh.hearth";

/// Étiquette de la fenêtre principale (`tauri.conf.json`).
pub const MAIN_WINDOW: &str = "main";

/// Argument passé par l'entrée de démarrage de Windows : l'application
/// démarre alors réduite dans la zone de notification (BR-CLIENT-006/007).
pub const MINIMIZED_FLAG: &str = "--minimized";

/// Identifiants du menu de la zone de notification.
pub const MENU_OPEN: &str = "open";
pub const MENU_QUIT: &str = "quit";

/// Réglages locaux, tels que l'interface les voit. Le fait que l'explication de
/// fermeture ait été montrée reste interne à la coquille.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Lancer Hearth au démarrage de Windows (désactivé par défaut, BR-CLIENT-006).
    pub launch_at_startup: bool,
}

/// Lit un booléen du fichier de réglages ; absent ou mal typé = `false`
/// (valeur par défaut de chaque réglage).
pub fn flag_from(value: Option<&serde_json::Value>) -> bool {
    value.and_then(serde_json::Value::as_bool).unwrap_or(false)
}

/// Seule la fenêtre principale se cache dans la zone de notification : toute
/// autre fenêtre se ferme normalement (BR-CLIENT-004).
pub fn hides_on_close(window_label: &str) -> bool {
    window_label == MAIN_WINDOW
}

/// L'explication de la réduction n'est donnée qu'une fois (BR-CLIENT-005).
pub fn should_explain_close(close_hint_seen: bool) -> bool {
    !close_hint_seen
}

/// Entrées du menu de la zone de notification (BR-CLIENT-011).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    Open,
    Quit,
}

/// Traduit l'identifiant d'une entrée de menu ; `None` pour un identifiant inconnu.
pub fn tray_action(menu_id: &str) -> Option<TrayAction> {
    match menu_id {
        MENU_OPEN => Some(TrayAction::Open),
        MENU_QUIT => Some(TrayAction::Quit),
        _ => None,
    }
}

/// Vrai si le lancement vient de l'entrée de démarrage de Windows : la fenêtre
/// reste alors cachée (BR-CLIENT-007).
pub fn is_minimized_launch<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter().any(|arg| arg.as_ref() == MINIMIZED_FLAG)
}

/// Taille maximale, en caractères, d'un message d'erreur de l'interface écrit au
/// journal (au-delà : coupé, avec un point de suspension).
pub const FRONTEND_MESSAGE_MAX_CHARS: usize = 2000;
/// Nombre maximal de lignes d'erreur de l'interface par fenêtre de temps.
pub const FRONTEND_MAX_PER_WINDOW: u32 = 20;
/// Durée de la fenêtre de débit, en secondes.
pub const FRONTEND_WINDOW_SECS: u64 = 60;

/// Coupe `text` à `max` caractères (jamais au milieu d'un caractère).
pub fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut cut: String = text.chars().take(max).collect();
    cut.push('…');
    cut
}

/// Limiteur de débit des erreurs remontées par l'interface : une boucle d'erreurs
/// côté web ne doit pas remplir le journal. Fenêtre fixe ; l'horloge est passée
/// en paramètre (secondes écoulées depuis un instant de référence) pour rester pure.
#[derive(Debug, Default)]
pub struct FrontendErrorLimiter {
    window_start_secs: u64,
    count: u32,
}

impl FrontendErrorLimiter {
    /// Vrai si une erreur survenue à `now_secs` peut être écrite.
    pub fn allow(&mut self, now_secs: u64) -> bool {
        if now_secs.saturating_sub(self.window_start_secs) >= FRONTEND_WINDOW_SECS
            || self.count == 0
        {
            self.window_start_secs = now_secs;
            self.count = 0;
        }
        if self.count >= FRONTEND_MAX_PER_WINDOW {
            return false;
        }
        self.count += 1;
        true
    }
}

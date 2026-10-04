//! Règles de la coquille, pures : aucune E/S, aucun type Tauri.

use serde::{Deserialize, Serialize};
use specta::Type;

/// Argument passé par l'entrée de démarrage de Windows : l'application
/// démarre alors réduite dans la zone de notification (BR-CLIENT-006/007).
pub const MINIMIZED_FLAG: &str = "--minimized";

/// Identifiants du menu de la zone de notification.
pub const MENU_OPEN: &str = "open";
pub const MENU_QUIT: &str = "quit";

/// Réglages locaux, tels que l'interface les voit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Lancer Hearth au démarrage de Windows (désactivé par défaut, BR-CLIENT-006).
    pub launch_at_startup: bool,
    /// L'explication de fermeture a déjà été montrée (BR-CLIENT-005).
    pub close_hint_seen: bool,
}

/// Lit un booléen du fichier de réglages ; absent ou mal typé = `false`
/// (valeur par défaut de chaque réglage).
pub fn flag_from(value: Option<&serde_json::Value>) -> bool {
    value.and_then(serde_json::Value::as_bool).unwrap_or(false)
}

/// Ce que fait la fermeture de la fenêtre (BR-CLIENT-004, BR-CLIENT-005).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseOutcome {
    /// Cacher la fenêtre sans rien dire.
    HideSilently,
    /// Cacher la fenêtre et expliquer, une seule fois.
    HideAndExplain,
}

/// La croix cache toujours la fenêtre ; l'explication n'est donnée qu'à la
/// première fermeture.
pub fn on_close_requested(close_hint_seen: bool) -> CloseOutcome {
    if close_hint_seen {
        CloseOutcome::HideSilently
    } else {
        CloseOutcome::HideAndExplain
    }
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

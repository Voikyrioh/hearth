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

/// Nom de la valeur de démarrage sous `HKCU\...\Run` : le nom du produit
/// (`tauri.conf.json`, que le modèle NSIS de Tauri retire à la désinstallation).
pub const STARTUP_ENTRY_NAME: &str = "Hearth";

/// Ligne de commande de la valeur de démarrage : le chemin de l'exécutable ENTRE
/// GUILLEMETS puis `--minimized`. Forme documentée par Microsoft pour les valeurs
/// `Run`, sûre avec ou sans espace dans le chemin (profil « Jean Dupont »).
// FIX:01M4B118DAFBQZYQX1E5ERY8CA
pub fn startup_command(exe: &str) -> Result<String, StartupCommandError> {
    if exe.trim().is_empty() {
        return Err(StartupCommandError::EmptyPath);
    }
    // Un chemin Windows ne peut pas contenir de guillemet : s'il en contient un, la
    // ligne de commande serait ambiguë (injection d'arguments), on refuse d'écrire.
    if exe.contains('"') {
        return Err(StartupCommandError::QuoteInPath);
    }
    Ok(format!("\"{exe}\" {MINIMIZED_FLAG}"))
}

/// Pourquoi la ligne de démarrage ne peut pas être fabriquée.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum StartupCommandError {
    #[error("chemin de l'application vide")]
    EmptyPath,
    #[error("le chemin de l'application contient un guillemet")]
    QuoteInPath,
}

/// Forme d'une valeur de démarrage déjà écrite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunValueForm {
    /// `"chemin" --minimized` : la forme actuelle.
    Quoted,
    /// `chemin --minimized` sans guillemets : forme écrite avant HRT-29 (greffon).
    Unquoted,
    /// Toute autre forme (posée à la main, autre argument) : jamais réécrite.
    Other,
}

/// Reconnaît les deux formes que Hearth a écrites ; le reste est `Other`.
pub fn run_value_form(value: &str) -> RunValueForm {
    let suffix = format!(" {MINIMIZED_FLAG}");
    let Some(head) = value.strip_suffix(suffix.as_str()) else {
        return RunValueForm::Other;
    };
    let is_exe = |path: &str| !path.is_empty() && path.to_ascii_lowercase().ends_with(".exe");
    if let Some(inner) = head
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
    {
        if is_exe(inner) && !inner.contains('"') {
            return RunValueForm::Quoted;
        }
    } else if is_exe(head) && !head.contains('"') {
        return RunValueForm::Unquoted;
    }
    RunValueForm::Other
}

/// Décision de migration : la valeur à écrire à la place d'une ancienne valeur sans
/// guillemets, `None` si rien à faire (déjà entre guillemets, ou forme inconnue). Le
/// chemin est CELUI de l'ancienne valeur (jamais celui de l'exécutable courant) : la
/// migration ne change que la forme, pas la cible ni le choix de l'utilisateur.
pub fn migrated_run_value(value: &str) -> Option<String> {
    match run_value_form(value) {
        RunValueForm::Unquoted => {
            let head = value.strip_suffix(&format!(" {MINIMIZED_FLAG}"))?;
            startup_command(head).ok()
        }
        RunValueForm::Quoted | RunValueForm::Other => None,
    }
}

/// Le Gestionnaire des tâches autorise-t-il l'entrée ? (`StartupApproved\Run`, comme
/// `auto-launch` 0.6 : valeur absente ou de moins de 8 octets = autorisée, sinon les
/// 8 derniers octets doivent être nuls ; `02 00..` = activée, `03 ..` + date = désactivée.)
pub fn task_manager_allows(approved: Option<&[u8]>) -> bool {
    match approved {
        Some(bytes) if bytes.len() >= 8 => bytes.iter().rev().take(8).all(|b| *b == 0),
        _ => true,
    }
}

/// Valeur « activé » écrite dans `StartupApproved\Run`.
pub const TASK_MANAGER_ENABLED: [u8; 12] = [2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

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

/// Comme [`flag_from`], avec une valeur par défaut quand le réglage est absent ou mal typé
/// (« Notifier quand un serveur devient hors ligne ou revient » : activé par défaut).
pub fn flag_or(value: Option<&serde_json::Value>, default: bool) -> bool {
    value
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(default)
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

/// Une entrée de journal = une ligne : retours à la ligne et caractères de contrôle d'un
/// texte venu de l'interface sont neutralisés (`\n` littéral, `?` pour les autres), sinon
/// un message pourrait fabriquer de fausses lignes de journal.
pub fn single_line(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push('?'),
            c => out.push(c),
        }
    }
    out
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

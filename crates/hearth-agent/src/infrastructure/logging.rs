//! Journaux `tracing` : texte lisible en terminal, JSON structuré sinon (journald, fichier).

use std::io::IsTerminal;

use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::writer::BoxMakeWriter;

pub const ENV_LOG_FORMAT: &str = "HEARTH_LOG_FORMAT";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    Text,
    Json,
}

/// Choisit le format : valeur explicite (`json` ou `text`) sinon texte en terminal, JSON ailleurs.
/// Le booléen est `false` si la valeur fournie est inconnue (on retombe alors sur le défaut).
pub fn choose_format(value: Option<&str>, is_terminal: bool) -> (LogFormat, bool) {
    let default = if is_terminal {
        LogFormat::Text
    } else {
        LogFormat::Json
    };
    match value.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
        None | Some("") => (default, true),
        Some("json") => (LogFormat::Json, true),
        Some("text") => (LogFormat::Text, true),
        Some(_) => (default, false),
    }
}

/// Initialise les journaux. Sur la sortie standard (reprise par journald), sauf `to_stderr`
/// (commande `fingerprint` : la sortie standard ne contient que l'empreinte).
pub fn init(to_stderr: bool) {
    init_with(to_stderr, "info");
}

/// Comme `init`, avec le niveau `warn` par défaut : les commandes interactives (`install`,
/// `uninstall`) parlent par leurs propres messages, pas par des journaux d'information.
/// `RUST_LOG` reste prioritaire.
pub fn init_quiet(to_stderr: bool) {
    init_with(to_stderr, "warn");
}

fn init_with(to_stderr: bool, default_level: &str) {
    let is_terminal = if to_stderr {
        std::io::stderr().is_terminal()
    } else {
        std::io::stdout().is_terminal()
    };
    let requested = std::env::var(ENV_LOG_FORMAT).ok();
    let (format, valid) = choose_format(requested.as_deref(), is_terminal);

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level));
    let writer = if to_stderr {
        BoxMakeWriter::new(std::io::stderr)
    } else {
        BoxMakeWriter::new(std::io::stdout)
    };
    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(writer);
    match format {
        LogFormat::Json => builder.json().init(),
        LogFormat::Text => builder.with_ansi(is_terminal).init(),
    }

    if !valid {
        tracing::warn!(
            value = requested.as_deref().unwrap_or_default(),
            "{ENV_LOG_FORMAT} inconnu (json ou text attendu) : format par défaut"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_depends_on_the_terminal() {
        assert_eq!(choose_format(None, true), (LogFormat::Text, true));
        assert_eq!(choose_format(None, false), (LogFormat::Json, true));
        assert_eq!(choose_format(Some(""), false), (LogFormat::Json, true));
    }

    #[test]
    fn explicit_value_wins_whatever_the_terminal() {
        assert_eq!(choose_format(Some("json"), true), (LogFormat::Json, true));
        assert_eq!(
            choose_format(Some(" TEXT "), false),
            (LogFormat::Text, true)
        );
    }

    #[test]
    fn unknown_value_falls_back_and_is_flagged() {
        assert_eq!(choose_format(Some("xml"), true), (LogFormat::Text, false));
    }
}

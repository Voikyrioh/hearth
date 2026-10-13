//! Journaux `tracing` : texte lisible en terminal, JSON structuré sinon (journald, fichier).

use std::io::IsTerminal;

use tracing::{Event, Metadata};
use tracing_log::NormalizeEvent;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::FilterExt;
use tracing_subscriber::fmt::writer::BoxMakeWriter;
use tracing_subscriber::layer::{Context, Filter, Layer, SubscriberExt};
use tracing_subscriber::util::SubscriberInitExt;

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
    // Une seconde initialisation est sans effet : l'abonné global reste le premier.
    let _ = try_init_with(writer, format, is_terminal, filter);

    if !valid {
        tracing::warn!(
            value = requested.as_deref().unwrap_or_default(),
            "{ENV_LOG_FORMAT} inconnu (json ou text attendu) : format par défaut"
        );
    }
}

/// Le chemin d'initialisation unique : la production et le test de fuite passent par ici, pont
/// `log` vers `tracing` compris. `filter` est le filtre demandé (`RUST_LOG`) ; le [`TransportGuard`]
/// s'y ajoute toujours et ne se contourne pas.
pub fn try_init_with(
    writer: BoxMakeWriter,
    format: LogFormat,
    ansi: bool,
    filter: EnvFilter,
) -> Result<(), tracing_subscriber::util::TryInitError> {
    let layer = match format {
        LogFormat::Json => tracing_subscriber::fmt::layer()
            .json()
            .with_writer(writer)
            .boxed(),
        LogFormat::Text => tracing_subscriber::fmt::layer()
            .with_ansi(ansi)
            .with_writer(writer)
            .boxed(),
    };
    tracing_subscriber::registry()
        .with(layer.with_filter(filter.and(TransportGuard)))
        .try_init()
}

/// Les bibliothèques de transport vers le moteur de conteneurs. `bollard` écrit le corps de chaque
/// réponse décodée au niveau `debug` (`Decoded into string`), donc, pour une inspection, les
/// variables d'environnement et la commande de chaque conteneur ; `hyper` et ses voisines écrivent
/// des échanges bruts. Rien de ce qu'elles disent ne sort sous le niveau `warn` (HRT-61, ADR-0038).
/// Les autres dépendances de l'agent n'ont pas été auditées ici une à une : celles qui parlent à
/// `tracing` le font au niveau de leurs propres cibles (voir `SECURITY.md`).
const GUARDED_TARGETS: &[&str] = &[
    "bollard",
    "bollard_stubs",
    "hyper",
    "hyper_util",
    "hyperlocal",
    "hyper_named_pipe",
    "h2",
];

/// Cette trace peut-elle sortir ? Oui sauf si elle vient d'une cible gardée à un niveau plus
/// bavard que `warn`.
pub fn is_allowed(level: tracing::Level, target: &str) -> bool {
    // Dans `tracing`, plus bavard veut dire plus grand : `TRACE > DEBUG > INFO > WARN > ERROR`.
    level <= tracing::Level::WARN
        || !GUARDED_TARGETS.iter().any(|guarded| {
            target == *guarded
                || target
                    .strip_prefix(guarded)
                    .is_some_and(|rest| rest.starts_with("::"))
        })
}

/// Filtre non contournable : il s'applique en plus de `RUST_LOG`, quelle que soit sa précision (une
/// directive `bollard::docker=trace` ne le dépasse pas). Il lit la cible RÉELLE des traces qui
/// arrivent par le pont `log` (leur cible `tracing` est « log »).
struct TransportGuard;

impl<S> Filter<S> for TransportGuard {
    fn enabled(&self, metadata: &Metadata<'_>, _: &Context<'_, S>) -> bool {
        // Une trace du pont `log` n'a pas encore sa vraie cible ici : `event_enabled` tranche.
        metadata.target() == "log" || is_allowed(*metadata.level(), metadata.target())
    }

    fn event_enabled(&self, event: &Event<'_>, _: &Context<'_, S>) -> bool {
        let normalized = event.normalized_metadata();
        let metadata = normalized.as_ref().unwrap_or_else(|| event.metadata());
        is_allowed(*metadata.level(), metadata.target())
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

    #[test]
    fn transport_libraries_never_speak_below_warn_whatever_the_filter() {
        use tracing::Level;
        for target in [
            "bollard",
            "bollard::docker",
            "hyper_util::client::legacy::connect::http",
            "hyperlocal",
            "h2::codec",
        ] {
            assert!(!is_allowed(Level::DEBUG, target), "{target}");
            assert!(!is_allowed(Level::TRACE, target), "{target}");
            assert!(!is_allowed(Level::INFO, target), "{target}");
            assert!(is_allowed(Level::WARN, target), "{target}");
            assert!(is_allowed(Level::ERROR, target), "{target}");
        }
        // Le préfixe ne suffit pas : une cible voisine n'est pas gardée.
        assert!(is_allowed(Level::DEBUG, "bollardesque"));
        assert!(is_allowed(
            Level::DEBUG,
            "hearth_agent::application::services"
        ));
    }
}

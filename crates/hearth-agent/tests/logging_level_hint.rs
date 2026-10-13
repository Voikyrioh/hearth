//! Le filtre de sécurité des traces ne doit pas relever le niveau global (revue HRT-61, I-B) : au
//! niveau `info`, les `debug!` des dépendances ne doivent même pas franchir le pont `log`.
//! Un binaire de test à part : l'abonné et le niveau de `log` sont globaux au processus.

use hearth_agent::infrastructure::logging::{self, LogFormat};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::writer::BoxMakeWriter;

#[test]
fn the_safety_filter_keeps_the_ceiling_of_the_environment_filter() {
    logging::try_init_with(
        BoxMakeWriter::new(std::io::sink),
        LogFormat::Json,
        false,
        EnvFilter::new("info"),
    )
    .expect("première installation du journal de ce binaire de test");
    assert_eq!(log::max_level(), log::LevelFilter::Info);
    assert_eq!(
        tracing::level_filters::LevelFilter::current(),
        tracing::level_filters::LevelFilter::INFO
    );
}

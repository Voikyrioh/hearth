//! Agent Hearth : service installé sur le serveur piloté.

use std::io::IsTerminal;

use clap::Parser;
use hearth_agent::entrypoint::cli::{Cli, Command, run};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    init_tracing(matches!(cli.command(), Command::Fingerprint));
    run(cli).await?;
    Ok(())
}

/// Journaux sur la sortie standard (repris par journald). Pour `fingerprint`, ils passent par
/// la sortie d'erreur afin que la sortie standard ne contienne que l'empreinte.
fn init_tracing(to_stderr: bool) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    // Pas de séquences de couleur quand la sortie est un fichier ou journald.
    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    if to_stderr {
        builder
            .with_ansi(std::io::stderr().is_terminal())
            .with_writer(std::io::stderr)
            .init();
    } else {
        builder.with_ansi(std::io::stdout().is_terminal()).init();
    }
}

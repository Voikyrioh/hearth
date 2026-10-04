//! Agent Hearth : service installé sur le serveur piloté.

use std::process::ExitCode;

use clap::Parser;
use hearth_agent::app;
use hearth_agent::entrypoint::cli::{Cli, Command};
use hearth_agent::infrastructure::logging;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    logging::init(cli.command() == Command::Fingerprint);

    match app::run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // Une erreur fatale passe par les journaux, comme tout le reste. Chaque message
            // porte déjà sa cause : on n'affiche pas la chaîne des sources en plus.
            tracing::error!(%error, "arrêt sur erreur fatale");
            ExitCode::FAILURE
        }
    }
}

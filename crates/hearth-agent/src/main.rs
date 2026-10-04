//! Agent Hearth : service installé sur le serveur piloté.

use std::process::ExitCode;

use clap::Parser;
use hearth_agent::app;
use hearth_agent::entrypoint::cli::{Cli, Command};
use hearth_agent::infrastructure::logging;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    // `fingerprint` et `account` écrivent leur résultat sur la sortie standard : journaux à part.
    let command = cli.command();
    let is_account = matches!(command, Command::Account { .. });
    logging::init(command == Command::Fingerprint || is_account);

    match app::run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) if is_account => {
            // Commande interactive : le message est destiné à la personne qui l'a tapée.
            eprintln!("{error}");
            ExitCode::FAILURE
        }
        Err(error) => {
            // Une erreur fatale passe par les journaux, comme tout le reste. Chaque message
            // porte déjà sa cause : on n'affiche pas la chaîne des sources en plus.
            tracing::error!(%error, "arrêt sur erreur fatale");
            ExitCode::FAILURE
        }
    }
}

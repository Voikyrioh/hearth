//! Ligne de commande : définition des options et sous-commandes (`serve` par défaut, `fingerprint`).
//! L'exécution est assemblée par `app::run`.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "hearth-agent", version, about = "Agent Hearth")]
pub struct Cli {
    /// Dossier de données (certificat, clé, base). Prioritaire sur `HEARTH_DATA_DIR`.
    #[arg(long, global = true, value_name = "DIR")]
    pub data_dir: Option<PathBuf>,

    /// Fichier de configuration `agent.toml`. Prioritaire sur `HEARTH_CONFIG`.
    #[arg(long, global = true, value_name = "FILE")]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Subcommand)]
pub enum Command {
    /// Démarre l'agent (commande par défaut).
    Serve,
    /// Affiche l'empreinte du certificat, en créant l'identité si elle n'existe pas encore.
    Fingerprint,
}

impl Cli {
    pub fn command(&self) -> Command {
        self.command.unwrap_or(Command::Serve)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serve_is_the_default_command() {
        let cli = Cli::parse_from(["hearth-agent"]);
        assert_eq!(cli.command(), Command::Serve);
    }

    #[test]
    fn options_are_accepted_before_and_after_the_subcommand() {
        let cli = Cli::parse_from(["hearth-agent", "--data-dir", "/d", "fingerprint"]);
        assert_eq!(cli.command(), Command::Fingerprint);
        assert_eq!(cli.data_dir, Some(PathBuf::from("/d")));
        let cli = Cli::parse_from(["hearth-agent", "fingerprint", "--config", "/c.toml"]);
        assert_eq!(cli.config, Some(PathBuf::from("/c.toml")));
    }
}

//! Ligne de commande : `serve` (par défaut) et `fingerprint`.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::app::{self, AppError};
use crate::infrastructure::config::{self, CliOverrides};

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

#[derive(Debug, Clone, Copy, Subcommand)]
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

    fn overrides(&self) -> CliOverrides {
        CliOverrides {
            config_path: self.config.clone(),
            data_dir: self.data_dir.clone(),
        }
    }
}

pub async fn run(cli: Cli) -> Result<(), AppError> {
    let env = |name: &str| std::env::var(name).ok();
    let config = config::load(&cli.overrides(), &env)?;
    let identity = app::load_identity(&config)?;

    match cli.command() {
        Command::Fingerprint => {
            println!("{}", identity.fingerprint);
            Ok(())
        }
        Command::Serve => {
            let server = app::start(&config, &identity)?;
            tracing::info!(
                addr = %server.local_addr(),
                fingerprint = %identity.fingerprint,
                install_id = %identity.install_id,
                "agent démarré"
            );
            server.run_until(shutdown_signal()).await?;
            tracing::info!("agent arrêté");
            Ok(())
        }
    }
}

/// Se termine sur Ctrl+C, ou sur SIGTERM sous Unix.
async fn shutdown_signal() {
    let ctrl_c = async {
        if tokio::signal::ctrl_c().await.is_err() {
            // Pas de gestionnaire de signal : on ne s'arrête jamais de ce côté.
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
    tracing::info!("signal d'arrêt reçu");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serve_is_the_default_command() {
        let cli = Cli::parse_from(["hearth-agent"]);
        assert!(matches!(cli.command(), Command::Serve));
    }

    #[test]
    fn options_are_accepted_before_and_after_the_subcommand() {
        let cli = Cli::parse_from(["hearth-agent", "--data-dir", "/d", "fingerprint"]);
        assert!(matches!(cli.command(), Command::Fingerprint));
        assert_eq!(cli.data_dir, Some(PathBuf::from("/d")));
        let cli = Cli::parse_from(["hearth-agent", "fingerprint", "--config", "/c.toml"]);
        assert_eq!(cli.config, Some(PathBuf::from("/c.toml")));
    }
}

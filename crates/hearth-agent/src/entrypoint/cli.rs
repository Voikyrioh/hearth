//! Ligne de commande : définition des options et sous-commandes (`serve` par défaut,
//! `fingerprint`, `account …`). L'exécution est assemblée par `app::run`.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::domain::accounts::Role;

#[derive(Debug, Parser)]
#[command(name = "hearth-agent", version = crate::build_info::VERSION, about = "Agent Hearth")]
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

#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum Command {
    /// Démarre l'agent (commande par défaut).
    Serve,
    /// Affiche l'empreinte du certificat, en créant l'identité si elle n'existe pas encore.
    Fingerprint,
    /// Installe l'agent sur ce serveur : binaire, configuration, identité, premier compte,
    /// service au démarrage. Droits d'administration requis. Sans terminal, tout se règle par les
    /// variables HEARTH_ADMIN_USER, HEARTH_ADMIN_PASSWORD (ou HEARTH_ADMIN_PASSWORD_HASH,
    /// haché Argon2id au format PHC) et HEARTH_PORT.
    Install(InstallArgs),
    /// Désinstalle l'agent : arrête et retire le service et le binaire ; les comptes, le journal
    /// et la configuration sont conservés ou supprimés selon le choix.
    Uninstall(UninstallArgs),
    /// Le superviseur d'une mise à jour de l'agent (lancé par l'agent lui-même, détaché ; pas une
    /// commande d'administration).
    #[command(hide = true)]
    UpdateSupervise {
        #[arg(long, value_name = "FICHIER")]
        job: PathBuf,
    },
    /// Fabrique le haché Argon2id (format PHC) d'un mot de passe, pour `HEARTH_ADMIN_PASSWORD_HASH` :
    /// le mot de passe est saisi sans écho avec confirmation, le haché est écrit sur la sortie
    /// standard. Le mot de passe n'apparaît jamais sur une ligne de commande.
    /// Écrit la clé de mise à jour embarquée et ce qui distingue cette construction d'une
    /// publication (contrôle de `cargo xtask agent`).
    #[command(hide = true)]
    BuildInfo,
    HashPassword {
        /// Le compte auquel le mot de passe est destiné (le mot de passe ne doit pas le contenir).
        #[arg(long, value_name = "NOM")]
        user: String,
    },
    /// Gère les comptes directement sur le serveur, sans réseau (mêmes règles que l'interface).
    Account {
        #[command(subcommand)]
        action: AccountAction,
    },
    /// Le mode attaque, directement sur le serveur, sans réseau : voir son état, ou le désactiver (la
    /// voie de secours d'un administrateur qui n'a pas de clé inscrite, BR-TRUST-027).
    AttackMode {
        #[command(subcommand)]
        action: AttackModeAction,
    },
}

/// Opérations sur le mode attaque. Il ne s'active que depuis un client, avec la clé d'un poste.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Subcommand)]
pub enum AttackModeAction {
    /// Affiche l'état : éteint, actif ou suspendu (avec le temps restant), et l'identifiant de
    /// l'activation.
    Status,
    /// Désactive le mode attaque (consigné au journal, comme depuis un client).
    Off,
}

#[derive(Debug, Clone, PartialEq, Eq, Args)]
pub struct InstallArgs {
    /// Port d'écoute (7341 par défaut). Prioritaire sur HEARTH_PORT.
    #[arg(long, value_name = "PORT")]
    pub port: Option<String>,
    /// Installation gérée par le système (NixOS par exemple) : aucune unité n'est écrite, le
    /// binaire n'est pas copié.
    #[arg(long)]
    pub managed: bool,
    /// Ne pose aucune question : tout vient des variables et des options.
    #[arg(long, short = 'y')]
    pub yes: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Args)]
pub struct UninstallArgs {
    /// Conserve les comptes, le journal, l'identité et la configuration.
    #[arg(long, conflicts_with = "purge")]
    pub keep_data: bool,
    /// Supprime aussi les comptes, le journal, l'identité et la configuration.
    #[arg(long)]
    pub purge: bool,
    /// Ne pose aucune question (sans --purge, les données sont conservées).
    #[arg(long, short = 'y')]
    pub yes: bool,
    /// Installation gérée par le système : le binaire n'est pas retiré.
    #[arg(long)]
    pub managed: bool,
}

/// Opérations sur les comptes. Le mot de passe est demandé sans écho avec confirmation, ou lu
/// dans la variable d'environnement `HEARTH_ACCOUNT_PASSWORD` (automatisation). Jamais en argument.
#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum AccountAction {
    /// Crée un compte.
    Add {
        /// Identifiant : 3 à 32 caractères, minuscules, chiffres, tiret, underscore.
        username: String,
        /// Rôle du compte : `admin` ou `readonly`.
        #[arg(long, value_parser = parse_role)]
        role: Role,
    },
    /// Liste les comptes avec leur dernière connexion et leurs sessions ouvertes.
    List,
    /// Définit un nouveau mot de passe et ferme les sessions du compte.
    Passwd { username: String },
    /// Change le rôle d'un compte (`admin` ou `readonly`).
    Role {
        username: String,
        #[arg(value_parser = parse_role)]
        role: Role,
    },
    /// Supprime un compte et ferme ses sessions.
    Remove { username: String },
    /// Ferme toutes les sessions d'un compte sans changer son mot de passe.
    Revoke { username: String },
}

fn parse_role(value: &str) -> Result<Role, String> {
    value.parse::<Role>().map_err(|error| error.to_string())
}

impl Cli {
    pub fn command(&self) -> Command {
        self.command.clone().unwrap_or(Command::Serve)
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

    #[test]
    fn account_subcommands_are_parsed() {
        let action = |args: &[&str]| match Cli::parse_from(args).command() {
            Command::Account { action } => action,
            other => panic!("attendu : account, reçu {other:?}"),
        };
        assert_eq!(
            action(&["hearth-agent", "account", "add", "marie", "--role", "admin"]),
            AccountAction::Add {
                username: "marie".into(),
                role: Role::Admin
            }
        );
        assert_eq!(
            action(&["hearth-agent", "account", "list"]),
            AccountAction::List
        );
        assert_eq!(
            action(&["hearth-agent", "account", "passwd", "marie"]),
            AccountAction::Passwd {
                username: "marie".into()
            }
        );
        assert_eq!(
            action(&["hearth-agent", "account", "role", "marie", "readonly"]),
            AccountAction::Role {
                username: "marie".into(),
                role: Role::ReadOnly
            }
        );
        assert_eq!(
            action(&["hearth-agent", "account", "remove", "marie"]),
            AccountAction::Remove {
                username: "marie".into()
            }
        );
        assert_eq!(
            action(&["hearth-agent", "account", "revoke", "marie"]),
            AccountAction::Revoke {
                username: "marie".into()
            }
        );
    }

    #[test]
    fn install_options_are_parsed() {
        let install = |args: &[&str]| match Cli::parse_from(args).command() {
            Command::Install(args) => args,
            other => panic!("attendu : install, reçu {other:?}"),
        };
        assert_eq!(
            install(&["hearth-agent", "install"]),
            InstallArgs {
                port: None,
                managed: false,
                yes: false
            }
        );
        assert_eq!(
            install(&[
                "hearth-agent",
                "install",
                "--port",
                "9000",
                "--managed",
                "--yes"
            ]),
            InstallArgs {
                port: Some("9000".into()),
                managed: true,
                yes: true
            }
        );
    }

    #[test]
    fn uninstall_options_are_parsed_and_the_two_choices_exclude_each_other() {
        let uninstall = |args: &[&str]| match Cli::parse_from(args).command() {
            Command::Uninstall(args) => args,
            other => panic!("attendu : uninstall, reçu {other:?}"),
        };
        assert!(uninstall(&["hearth-agent", "uninstall", "--purge", "-y"]).purge);
        assert!(uninstall(&["hearth-agent", "uninstall", "--keep-data"]).keep_data);
        assert!(
            Cli::try_parse_from(["hearth-agent", "uninstall", "--keep-data", "--purge"]).is_err()
        );
    }

    #[test]
    fn hash_password_needs_the_account_name_and_takes_no_password() {
        assert_eq!(
            Cli::parse_from(["hearth-agent", "hash-password", "--user", "marie"]).command(),
            Command::HashPassword {
                user: "marie".into()
            }
        );
        assert!(Cli::try_parse_from(["hearth-agent", "hash-password"]).is_err());
        assert!(
            Cli::try_parse_from(["hearth-agent", "hash-password", "--user", "marie", "x"]).is_err()
        );
    }

    #[test]
    fn the_admin_password_is_never_an_argument_of_install() {
        assert!(Cli::try_parse_from(["hearth-agent", "install", "--admin-password", "x"]).is_err());
        assert!(Cli::try_parse_from(["hearth-agent", "install", "--password", "x"]).is_err());
    }

    #[test]
    fn a_password_is_never_accepted_as_an_argument() {
        assert!(
            Cli::try_parse_from([
                "hearth-agent",
                "account",
                "add",
                "marie",
                "--role",
                "admin",
                "--password",
                "x"
            ])
            .is_err()
        );
        assert!(
            Cli::try_parse_from(["hearth-agent", "account", "passwd", "marie", "Secret-12345"])
                .is_err()
        );
    }

    #[test]
    fn an_unknown_role_is_refused() {
        assert!(
            Cli::try_parse_from(["hearth-agent", "account", "add", "marie", "--role", "root"])
                .is_err()
        );
        assert!(Cli::try_parse_from(["hearth-agent", "account", "add", "marie"]).is_err());
    }
}

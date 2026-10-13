//! Composition des sous-commandes `install` et `uninstall` : assemble les adaptateurs (machine,
//! service, identité, comptes) et lance le flux de `entrypoint::install`. Ce module est le seul
//! à connaître le type concret des comptes de l'installation.

use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;

use super::{AppError, load_config};
use crate::application::install::Installer;
use crate::application::ports::{
    AdminAccounts, AdminAccountsError, AdminCredential, InstallPaths, PasswordHasher,
    ServiceManager, Store,
};
use crate::domain::accounts::{Role, ServicesRight, Username};
use crate::domain::audit::Actor;
use crate::domain::install::Version;
use crate::domain::secret::Secret;
use crate::entrypoint::cli::{Cli, InstallArgs, UninstallArgs};
use crate::entrypoint::install::{self, Context};
use crate::entrypoint::signal::interruption_signal;
use crate::entrypoint::terminal::TerminalPrompter;
use crate::infrastructure::argon2::Argon2Hasher;
use crate::infrastructure::engine::{InstallMarkers, engine_detected};
use crate::infrastructure::install::SystemHost;
use crate::infrastructure::service::{Systemd, Unmanaged};
use crate::infrastructure::sqlite::{Database, SqliteStore};
use crate::infrastructure::tls::FileIdentityStore;

/// Dossier de données par défaut sous Linux : un autre dossier est écrit dans la configuration.
const DEFAULT_DATA_DIR: &str = "/var/lib/hearth";

/// Les comptes de l'installation : la base de l'agent, ouverte seulement quand il le faut.
struct LocalAdmins {
    data_dir: PathBuf,
    hasher: Argon2Hasher,
}

/// Les comptes de l'installation tels que `install` les assemble, sur ce dossier de données : le vrai
/// chemin de création du premier compte et de l'octroi du droit de gérer les services, sans service
/// système ni installation de fichiers. Pour les tests d'intégration (`tests/install_real_accounts.rs`).
pub fn local_admins(data_dir: PathBuf) -> Result<impl AdminAccounts, AppError> {
    Ok(LocalAdmins {
        data_dir,
        hasher: Argon2Hasher::new()?,
    })
}

fn failed(error: impl std::fmt::Display) -> AdminAccountsError {
    AdminAccountsError::Failed(error.to_string())
}

#[async_trait]
impl AdminAccounts for LocalAdmins {
    fn check_hash(&self, hash: &Secret) -> Result<(), AdminAccountsError> {
        self.hasher.validate_hash(hash).map_err(failed)
    }

    async fn admin_count(&self) -> Result<u64, AdminAccountsError> {
        // En lecture seule, sans migration : observer ne modifie jamais la base d'un service
        // qui tourne. Les migrations d'une base existante sont jouées par le service au
        // démarrage, jamais par l'installateur (sauf pour y créer le premier compte).
        crate::infrastructure::sqlite::count_admins_read_only(&self.data_dir)
            .await
            .map_err(failed)
    }

    async fn create_admin(
        &self,
        name: &Username,
        credential: AdminCredential,
        grant_services: bool,
    ) -> Result<(), AdminAccountsError> {
        let database = Database::open(&self.data_dir).await.map_err(failed)?;
        let accounts = super::account_service(&database).map_err(failed)?;
        let by = Actor::command_line();
        let result = match credential {
            AdminCredential::Password(password) => {
                accounts
                    .create(name.as_str(), password, Role::Admin, &by)
                    .await
            }
            AdminCredential::Hash(hash) => {
                accounts
                    .create_with_hash(name.as_str(), hash, Role::Admin, &by)
                    .await
            }
        };
        // Le droit de gérer les services (BR-GRANT-006, 007), consigné au journal avec l'origine
        // « ligne de commande du serveur » comme la commande `account services` (BR-GRANT-009). Si
        // l'octroi échoue, le compte qui vient d'être créé n'est pas laissé derrière.
        let result = match result {
            Ok(_) if grant_services => {
                let granted = async {
                    let adapters = super::Adapters::production().map_err(failed)?;
                    super::services_right_service(&database, &adapters)
                        .set(name.as_str(), ServicesRight::Manage, &by)
                        .await
                        .map(|_| ())
                        .map_err(failed)
                }
                .await;
                if granted.is_err() {
                    let _ = delete_account(&database, name).await;
                }
                granted
            }
            other => other.map(|_| ()).map_err(failed),
        };
        // La base est fermée avant que le service ne l'ouvre : le journal WAL est reporté.
        database.pool().close().await;
        result
    }

    async fn remove_created(&self, name: &Username) -> Result<(), AdminAccountsError> {
        let database = Database::open(&self.data_dir).await.map_err(failed)?;
        let result = delete_account(&database, name).await;
        database.pool().close().await;
        result
    }
}

/// Supprime un compte sans la garde du dernier administrateur : retour en arrière de l'installation.
async fn delete_account(database: &Database, name: &Username) -> Result<(), AdminAccountsError> {
    let accounts = super::account_service(database).map_err(failed)?;
    let account = accounts.find(name.as_str()).await.map_err(failed)?;
    let store = SqliteStore::new(database.pool().clone());
    let mut tx = store.begin().await.map_err(failed)?;
    tx.accounts().delete(&account.id).await.map_err(failed)?;
    tx.commit().await.map_err(failed)
}

/// L'installation est-elle gérée par le système (option ou `HEARTH_MANAGED`) ?
fn managed(requested: bool) -> bool {
    install::is_managed(requested, &|name| std::env::var(name).ok())
}

/// Lève le drapeau quand l'utilisateur interrompt (Ctrl+C, SIGTERM).
fn watch_interruptions() -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(false));
    let raised = flag.clone();
    tokio::spawn(async move {
        interruption_signal().await;
        raised.store(true, Ordering::SeqCst);
    });
    flag
}

struct Assembled {
    host: SystemHost,
    service: Box<dyn ServiceManager>,
    identity: Box<dyn crate::application::ports::IdentityStore>,
    admins: LocalAdmins,
    paths: InstallPaths,
    context: ContextData,
}

struct ContextData {
    source_binary: PathBuf,
    listen_addr: std::net::IpAddr,
    data_dir_override: Option<PathBuf>,
    command_line: String,
    systemd_missing: bool,
    /// Un moteur de conteneurs est détecté (repère d'installation ou socket) et le pilotage permis.
    engine_detected: bool,
}

fn assemble(cli: &Cli, managed: bool) -> Result<Assembled, AppError> {
    let config = load_config(cli)?;
    let config_path = cli
        .config
        .clone()
        .or_else(|| std::env::var("HEARTH_CONFIG").ok().map(PathBuf::from));
    let data_dir_override = (config.data_dir.as_path() != std::path::Path::new(DEFAULT_DATA_DIR))
        .then(|| config.data_dir.clone());
    let service: Box<dyn ServiceManager> = if managed {
        Box::new(Unmanaged)
    } else {
        Box::new(Systemd::system())
    };
    Ok(Assembled {
        host: SystemHost,
        service,
        identity: Box::new(FileIdentityStore::new(&config.data_dir)),
        admins: LocalAdmins {
            data_dir: config.data_dir.clone(),
            hasher: Argon2Hasher::new()?,
        },
        paths: InstallPaths::system(config_path, config.data_dir.clone()),
        context: ContextData {
            source_binary: std::env::current_exe().map_err(AppError::CurrentExe)?,
            listen_addr: config.listen_addr,
            data_dir_override,
            command_line: std::env::args().collect::<Vec<_>>().join(" "),
            systemd_missing: !managed && !Systemd::is_running_here(),
            engine_detected: engine_detected(
                config.engine.enabled,
                &InstallMarkers::linux(),
                &config.engine.socket,
            ),
        },
    })
}

fn context<'a>(
    data: &'a ContextData,
    env: &'a dyn Fn(&str) -> Option<String>,
) -> Result<Context<'a>, AppError> {
    Ok(Context {
        env,
        interactive: std::io::stdin().is_terminal() && std::io::stdout().is_terminal(),
        source_binary: data.source_binary.clone(),
        listen_addr: data.listen_addr,
        data_dir_override: data.data_dir_override.clone(),
        target: Version::parse(crate::build_info::VERSION).map_err(AppError::Version)?,
        command_line: data.command_line.clone(),
        systemd_missing: data.systemd_missing,
        engine_detected: data.engine_detected,
    })
}

pub async fn run_install(cli: &Cli, args: &InstallArgs) -> Result<(), AppError> {
    let managed = managed(args.managed);
    let assembled = assemble(cli, managed)?;
    let interrupted = watch_interruptions();
    let installer = Installer {
        host: &assembled.host,
        service: assembled.service.as_ref(),
        identity: assembled.identity.as_ref(),
        admins: &assembled.admins,
        paths: assembled.paths.clone(),
        interrupted: &interrupted,
    };
    let env = |name: &str| std::env::var(name).ok();
    let context = context(&assembled.context, &env)?;
    install::install(
        args,
        &context,
        &installer,
        &mut TerminalPrompter,
        &mut std::io::stdout(),
    )
    .await?;
    Ok(())
}

pub async fn run_uninstall(cli: &Cli, args: &UninstallArgs) -> Result<(), AppError> {
    let managed = managed(args.managed);
    let assembled = assemble(cli, managed)?;
    let interrupted = watch_interruptions();
    let installer = Installer {
        host: &assembled.host,
        service: assembled.service.as_ref(),
        identity: assembled.identity.as_ref(),
        admins: &assembled.admins,
        paths: assembled.paths.clone(),
        interrupted: &interrupted,
    };
    let env = |name: &str| std::env::var(name).ok();
    let context = context(&assembled.context, &env)?;
    install::uninstall(
        args,
        &context,
        &installer,
        &mut TerminalPrompter,
        &mut std::io::stdout(),
    )
    .await?;
    Ok(())
}

/// `hearth-agent hash-password --user NOM`.
pub async fn run_hash_password(user: &str) -> Result<(), AppError> {
    let hasher = Argon2Hasher::new()?;
    let passwords =
        crate::entrypoint::terminal::TerminalPasswords::from_env(&|name| std::env::var(name).ok());
    crate::entrypoint::account::hash_password(user, &passwords, &hasher, &mut std::io::stdout())
        .await?;
    Ok(())
}

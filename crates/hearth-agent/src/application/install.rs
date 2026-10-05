//! Cas d'usage de l'installation et de la désinstallation de l'agent (BR-INSTALL-*).
//!
//! Les règles sont celles de `domain::install` (plan, prérequis, retour en arrière, désinstallation) ;
//! ce module observe la machine par les ports, enchaîne les étapes et **défait ce qu'il a fait**
//! en cas d'erreur ou d'interruption (BR-INSTALL-008). Il ne parle ni à l'utilisateur ni au
//! terminal : l'entrypoint écrit les messages, d'après les `Event` qu'il reçoit.

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use hearth_proto::fingerprint::Fingerprint;
use hearth_proto::product::DEFAULT_PORT;
use thiserror::Error;

use super::ports::{
    AdminAccounts, AdminAccountsError, AdminCredential, BinaryInstalled, ConfigSpec, HostError,
    HostFacts, IdentityError, IdentityStore, InstallHost, InstallPaths, ServiceError, ServiceKind,
    ServiceManager, ServiceSpec,
};
use crate::domain::accounts::Username;
use crate::domain::install::{
    Asset, BinaryState, Blocker, Done, InstallKind, InstallPlan, Observed, PlanError,
    Prerequisites, ServiceAction, Undo, UninstallPlan, UnitState, Version, check_prerequisites,
    plan_install, undo_plan,
};

/// Combien de temps on attend que l'agent réponde après son démarrage.
pub const START_TIMEOUT: Duration = Duration::from_secs(30);

/// Fichiers de l'identité dans le dossier de données (BR-INSTALL-004).
const IDENTITY_FILES: [&str; 4] = ["cert.pem", "key.pem", "install_id", "identity.lock"];
/// Fichiers de la base : `hearth.db` et ceux que SQLite y ajoute.
const DATABASE_FILES: [&str; 3] = ["hearth.db", "hearth.db-wal", "hearth.db-shm"];

/// Ce que l'entrypoint raconte à l'utilisateur, étape après étape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// Les écritures commencent (avant : rien n'a été modifié).
    Installing,
    /// L'unité est écrite, le service démarre.
    StartingService,
    /// Installation gérée : aucune unité écrite, le service n'est pas lancé.
    ServiceNotWritten,
}

/// Ce qu'une installation a besoin de savoir, déjà validé.
pub struct InstallInputs {
    pub port: u16,
    pub listen_addr: IpAddr,
    /// Le premier compte, s'il est à créer.
    pub admin: Option<(Username, AdminCredential)>,
    pub target: Version,
    /// Le binaire à installer (celui qui s'exécute).
    pub source_binary: PathBuf,
    /// Installation gérée par le système : ni unité, ni binaire copié.
    pub managed: bool,
    /// Dossier de données différent du défaut : écrit dans la configuration.
    pub data_dir_override: Option<PathBuf>,
}

/// Le résultat d'une installation réussie.
#[derive(Debug)]
pub struct Installed {
    pub kind: InstallKind,
    pub fingerprint: Fingerprint,
    pub port: u16,
    pub service: ServiceKind,
    /// Le service tournait déjà avec ce binaire et n'a pas été touché (BR-INSTALL-007).
    pub service_untouched: bool,
    /// Le certificat servi par l'agent est celui qui est affiché ; `None` si aucun agent n'a
    /// été lancé (installation gérée).
    pub verified_served: bool,
}

#[derive(Debug, Error)]
pub enum InstallError {
    #[error(transparent)]
    Blocked(#[from] Blocker),
    #[error(transparent)]
    Plan(#[from] PlanError),
    #[error(transparent)]
    Host(#[from] HostError),
    #[error(transparent)]
    Service(#[from] ServiceError),
    #[error(transparent)]
    Identity(#[from] IdentityError),
    #[error(transparent)]
    Accounts(#[from] AdminAccountsError),
    #[error("le premier compte administrateur est requis")]
    AdminRequired,
    #[error("l'agent n'a pas répondu en {} s", START_TIMEOUT.as_secs())]
    NoAnswer,
    #[error("le certificat servi par l'agent n'est pas celui de l'identité installée")]
    WrongCertificate,
    #[error("interrompu")]
    Interrupted,
}

/// Une installation qui a échoué, et ce qui n'a pas pu être défait.
#[derive(Debug)]
pub struct Failure {
    pub cause: InstallError,
    /// Ce que le retour en arrière n'a pas pu retirer ; vide quand la machine est revenue à son
    /// état d'avant.
    pub left_behind: Vec<String>,
}

pub struct Installer<'a> {
    pub host: &'a dyn InstallHost,
    pub service: &'a dyn ServiceManager,
    pub identity: &'a dyn IdentityStore,
    pub admins: &'a dyn AdminAccounts,
    pub paths: InstallPaths,
    /// Levé par l'entrypoint quand l'utilisateur interrompt (Ctrl+C, SIGTERM).
    pub interrupted: &'a AtomicBool,
}

impl Installer<'_> {
    fn interrupted(&self) -> bool {
        self.interrupted.load(Ordering::SeqCst)
    }

    fn check_interrupted(&self) -> Result<(), InstallError> {
        if self.interrupted() {
            Err(InstallError::Interrupted)
        } else {
            Ok(())
        }
    }

    /// Ce que l'on trouve sur la machine : binaire, unité, service, données, configuration,
    /// administrateurs. Ne modifie rien (la base n'est pas créée).
    pub async fn observe(
        &self,
        source: &std::path::Path,
        managed: bool,
    ) -> Result<(Observed, HostFacts), InstallError> {
        let facts = self.host.inspect(&self.paths, source)?;
        let (binary, unit) = if managed {
            (BinaryState::ProvidedBySystem, UnitState::NotWritten)
        } else {
            let unit = if self.service.is_installed()? {
                UnitState::Present
            } else {
                UnitState::Absent
            };
            (facts.binary, unit)
        };
        let service_active = self.service.is_active()?;
        let admin_accounts = if facts.data.database {
            self.admins.admin_count().await?
        } else {
            0
        };
        let observed = Observed {
            binary,
            unit,
            service_active,
            data: facts.data,
            config_exists: facts.config_exists,
            admin_accounts,
        };
        Ok((observed, facts))
    }

    /// Les prérequis, dans l'ordre de la spécification (BR-INSTALL-001, 006, 012). Le port est
    /// dit occupé s'il l'est par un autre processus que l'agent installé qui tourne.
    pub fn check_prerequisites(
        &self,
        observed: &Observed,
        facts: &HostFacts,
        port: u16,
        listen_addr: IpAddr,
    ) -> Result<(), Blocker> {
        let own = observed.service_active && facts.configured_port.unwrap_or(DEFAULT_PORT) == port;
        let port_taken = !own && self.host.port_taken(listen_addr, port);
        let free_bytes = self
            .host
            .free_bytes(&self.paths.data_dir)
            .unwrap_or(u64::MAX);
        check_prerequisites(&Prerequisites {
            privileged: self.host.is_privileged(),
            os: self.host.os(),
            arch: self.host.arch(),
            port,
            port_taken,
            free_bytes,
        })
    }

    /// Le plan d'après l'état observé.
    pub fn plan(&self, observed: &Observed, target: Version) -> Result<InstallPlan, PlanError> {
        plan_install(observed, target)
    }

    /// Exécute le plan. En cas d'erreur ou d'interruption, défait ce qui a été fait.
    pub async fn apply(
        &self,
        plan: &InstallPlan,
        observed: &Observed,
        inputs: InstallInputs,
        say: &mut dyn FnMut(Event),
    ) -> Result<Installed, Failure> {
        let mut done: Vec<Done> = Vec::new();
        let mut binary: Option<BinaryInstalled> = None;
        let admin_name = inputs.admin.as_ref().map(|(name, _)| name.clone());
        match self
            .run(plan, observed, inputs, say, &mut done, &mut binary)
            .await
        {
            Ok(installed) => {
                if let Some(binary) = &binary {
                    self.host.discard_backup(binary);
                }
                Ok(installed)
            }
            Err(cause) => {
                let left_behind = self
                    .rollback(&done, binary.as_ref(), admin_name.as_ref())
                    .await;
                Err(Failure { cause, left_behind })
            }
        }
    }

    async fn run(
        &self,
        plan: &InstallPlan,
        observed: &Observed,
        inputs: InstallInputs,
        say: &mut dyn FnMut(Event),
        done: &mut Vec<Done>,
        binary: &mut Option<BinaryInstalled>,
    ) -> Result<Installed, InstallError> {
        let paths = &self.paths;
        self.check_interrupted()?;
        say(Event::Installing);

        // 1. Le binaire (écriture atomique ; l'ancien est gardé de côté).
        if plan.replace_binary && !inputs.managed {
            let installed =
                self.host
                    .install_binary(&inputs.source_binary, &paths.binary, &paths.backup())?;
            done.push(if installed.backup.is_some() {
                Done::Replaced(Asset::Binary)
            } else {
                Done::Created(Asset::Binary)
            });
            *binary = Some(installed);
            self.check_interrupted()?;
        }

        // 2. Le dossier de données.
        if self.host.ensure_data_dir(&paths.data_dir)? {
            done.push(Done::Created(Asset::DataDir));
        }
        self.check_interrupted()?;

        // 3. La configuration : créée si elle manque, jamais écrasée (BR-INSTALL-003).
        if plan.write_config {
            let spec = ConfigSpec {
                port: inputs.port,
                managed: inputs.managed,
                data_dir: inputs.data_dir_override.clone(),
            };
            if self.host.write_config_new(&paths.config, &spec)? {
                done.push(Done::Created(Asset::Config));
            }
            self.check_interrupted()?;
        }

        // 4. L'identité : générée une fois, jamais modifiée (BR-INSTALL-004).
        if !observed.data.identity {
            // Notée avant l'appel : une création interrompue laisse des restes à retirer.
            done.push(Done::Created(Asset::Identity));
        }
        let identity = self.identity.load_or_create()?;
        self.check_interrupted()?;

        // 5. Le premier compte administrateur (BR-INSTALL-002).
        if plan.needs_first_admin {
            let (name, credential) = inputs.admin.ok_or(InstallError::AdminRequired)?;
            if !observed.data.database {
                done.push(Done::Created(Asset::Database));
            }
            self.admins.create_admin(&name, credential).await?;
            done.push(Done::Created(Asset::FirstAccount));
            self.check_interrupted()?;
        }

        // 6. Le service (BR-INSTALL-005, BR-INSTALL-007).
        let service_kind = self.service.kind();
        let mut service_untouched = false;
        let mut verified_served = false;
        match (plan.service, inputs.managed) {
            (_, true) => say(Event::ServiceNotWritten),
            (ServiceAction::Leave, false) => service_untouched = true,
            (action, false) => {
                say(Event::StartingService);
                let spec = ServiceSpec {
                    binary: paths.binary.clone(),
                    config: paths.config.clone(),
                    data_dir: paths.data_dir.clone(),
                };
                // Noté avant l'appel : une unité à moitié installée est défaite aussi.
                done.push(if observed.unit == UnitState::Present {
                    Done::Replaced(Asset::Service)
                } else {
                    Done::Created(Asset::Service)
                });
                self.service.install(&spec)?;
                if action == ServiceAction::Restart {
                    self.service.restart()?;
                }
                self.check_interrupted()?;
            }
        }

        // 7. L'agent répond, avec le certificat de l'identité installée.
        if !inputs.managed {
            let addr = SocketAddr::new(probe_ip(inputs.listen_addr), inputs.port);
            let answered = self
                .host
                .wait_for_hello(addr, START_TIMEOUT, &|| self.interrupted())
                .map_err(|error| match error {
                    HostError::Other(_) if self.interrupted() => InstallError::Interrupted,
                    HostError::Other(_) => InstallError::NoAnswer,
                    other => InstallError::Host(other),
                })?;
            if answered.served != identity.fingerprint {
                return Err(InstallError::WrongCertificate);
            }
            verified_served = true;
        }
        Ok(Installed {
            kind: plan.kind,
            fingerprint: identity.fingerprint,
            port: inputs.port,
            service: service_kind,
            service_untouched,
            verified_served,
        })
    }

    /// Défait ce qui a été fait, dans l'ordre inverse (`domain::install::undo_plan`). Rend ce
    /// qui n'a pas pu l'être.
    async fn rollback(
        &self,
        done: &[Done],
        binary: Option<&BinaryInstalled>,
        created_admin: Option<&Username>,
    ) -> Vec<String> {
        let mut left = Vec::new();
        let paths = &self.paths;
        let mut restart_old_service = false;
        for undo in undo_plan(done) {
            let result: Result<(), String> = match undo {
                Undo::Remove(Asset::Service) => self.drop_service().map_err(|e| e.to_string()),
                Undo::Remove(Asset::DataDir) => self
                    .host
                    .remove_dir(&paths.data_dir)
                    .map_err(|e| e.to_string()),
                Undo::Remove(Asset::Config) => self.remove_config().map_err(|e| e.to_string()),
                Undo::Remove(Asset::Binary) => binary.map_or(Ok(()), |installed| {
                    self.host
                        .restore_binary(&paths.binary, installed)
                        .map_err(|e| e.to_string())
                }),
                Undo::Remove(Asset::Identity) => self.remove_files(&IDENTITY_FILES),
                Undo::Remove(Asset::Database) => self.remove_files(&DATABASE_FILES),
                Undo::Remove(Asset::FirstAccount) => match created_admin {
                    Some(name) => self
                        .admins
                        .remove_created(name)
                        .await
                        .map_err(|e| e.to_string()),
                    None => Ok(()),
                },
                Undo::Restore(Asset::Binary) => binary.map_or(Ok(()), |installed| {
                    self.host
                        .restore_binary(&paths.binary, installed)
                        .map_err(|e| e.to_string())
                }),
                // Le service tournait avec l'ancien binaire : il repart dessus une fois celui-ci
                // rétabli (en dernier).
                Undo::Restore(Asset::Service) => {
                    restart_old_service = true;
                    Ok(())
                }
                Undo::Restore(_) => Ok(()),
            };
            if let Err(detail) = result {
                left.push(detail);
            }
        }
        if restart_old_service && let Err(error) = self.service.restart() {
            left.push(error.to_string());
        }
        left
    }

    fn remove_files(&self, names: &[&str]) -> Result<(), String> {
        for name in names {
            self.host
                .remove_file(&self.paths.data_dir.join(name))
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn remove_config(&self) -> Result<(), HostError> {
        self.host.remove_file(&self.paths.config)?;
        if let Some(parent) = self.paths.config.parent() {
            self.host.remove_dir_if_empty(parent)?;
        }
        Ok(())
    }

    fn drop_service(&self) -> Result<(), ServiceError> {
        self.service.stop()?;
        self.service.disable()?;
        self.service.remove()
    }

    /// Désinstalle selon le plan (BR-INSTALL-011). Chaque étape est tentée même si une autre a
    /// échoué ; ce qui n'a pas pu être retiré est rendu, pour un nettoyage manuel.
    pub fn uninstall(&self, plan: &UninstallPlan) -> Vec<String> {
        let paths = &self.paths;
        let mut failed = Vec::new();
        let mut attempt = |result: Result<(), String>| {
            if let Err(detail) = result {
                failed.push(detail);
            }
        };
        if plan.remove_service {
            attempt(self.drop_service().map_err(|e| e.to_string()));
        }
        if plan.remove_binary {
            attempt(
                self.host
                    .remove_file(&paths.binary)
                    .and_then(|()| self.host.remove_file(&paths.backup()))
                    .map_err(|e| e.to_string()),
            );
        }
        if plan.remove_data {
            attempt(
                self.host
                    .remove_dir(&paths.data_dir)
                    .map_err(|e| e.to_string()),
            );
        }
        if plan.remove_config {
            attempt(self.remove_config().map_err(|e| e.to_string()));
        }
        failed
    }
}

/// L'adresse jointe pour la vérification : `0.0.0.0` et `::` s'atteignent par le bouclage.
fn probe_ip(listen: IpAddr) -> IpAddr {
    match listen {
        IpAddr::V4(ip) if ip.is_unspecified() => IpAddr::from([127, 0, 0, 1]),
        IpAddr::V6(ip) if ip.is_unspecified() => IpAddr::from([0, 0, 0, 0, 0, 0, 0, 1]),
        other => other,
    }
}

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
use crate::domain::install::{
    DATA_FILES, DATABASE_FILES, IDENTITY_FILES, UPDATE_DIR, UPDATE_FILES, is_binary_temporary,
    is_database_temporary, is_fingerprint_secret_temporary, is_identity_temporary,
    is_update_temporary,
};

/// Combien de temps on attend que l'agent réponde après son démarrage.
pub const START_TIMEOUT: Duration = Duration::from_secs(30);

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

/// Ce que la désinstallation n'a pas pu faire, et ce qu'elle a laissé volontairement.
#[derive(Debug, Default)]
pub struct Uninstalled {
    /// Étapes qui ont échoué : à nettoyer à la main.
    pub failed: Vec<String>,
    /// Fichiers qui ne sont pas à Hearth, restés dans le dossier de données (non touchés).
    pub foreign: Vec<String>,
}

/// L'état d'avant, de quoi y revenir.
struct Before<'a> {
    binary: Option<&'a BinaryInstalled>,
    created_admin: Option<&'a Username>,
    /// Le texte de l'unité d'avant.
    unit: Option<&'a str>,
    /// Le service tournait.
    was_active: bool,
    /// Le service démarrait avec le système.
    was_enabled: bool,
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
            data_dir: self.paths.data_dir.to_string_lossy().into_owned(),
            config: self.paths.config.to_string_lossy().into_owned(),
            data_dir_state: self.host.data_dir_state(&self.paths.data_dir),
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
        // L'unité telle qu'elle est avant : rétablie à l'identique en cas d'échec.
        let previous_unit = self.service.unit_text().unwrap_or(None);
        let was_active = observed.service_active;
        // FIX:01M460G9AE6AFPSTRF4ZD872QB : une erreur de lecture n'est pas « pas activé » ; sans
        // cette réponse, le retour en arrière pourrait désactiver un service qui l'était. Rien n'a
        // encore été modifié : l'installation s'arrête.
        let was_enabled = match self.service.is_enabled() {
            Ok(enabled) => enabled,
            Err(error) => {
                return Err(Failure {
                    cause: InstallError::Service(error),
                    left_behind: Vec::new(),
                });
            }
        };
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
                    .rollback(
                        &done,
                        &Before {
                            binary: binary.as_ref(),
                            created_admin: admin_name.as_ref(),
                            unit: previous_unit.as_deref(),
                            was_active,
                            was_enabled,
                        },
                    )
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
        if !observed.data.identity && !observed.data.identity_partial {
            // Rien n'existait : noté avant l'appel (une création interrompue laisse des restes à
            // retirer). Une identité qui existait déjà, même à moitié, n'est jamais notée : on
            // ne la supprime pas (le plan a d'ailleurs refusé le cas de l'identité partielle).
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
    async fn rollback(&self, done: &[Done], before: &Before<'_>) -> Vec<String> {
        let Before {
            binary,
            created_admin,
            ..
        } = *before;
        let mut left = Vec::new();
        let paths = &self.paths;
        let mut restore_service = false;
        for undo in undo_plan(done) {
            let result: Result<(), String> = match undo {
                Undo::Remove(Asset::Service) => self.drop_service().map_err(|e| e.to_string()),
                Undo::Remove(Asset::DataDir) => self
                    .remove_known_data()
                    .map(|_| ())
                    .map_err(|e| e.to_string()),
                Undo::Remove(Asset::Config) => self.remove_config().map_err(|e| e.to_string()),
                Undo::Remove(Asset::Binary) => binary.map_or(Ok(()), |installed| {
                    self.host
                        .restore_binary(&paths.binary, installed)
                        .map_err(|e| e.to_string())
                }),
                Undo::Remove(Asset::Identity) => self.remove_identity(),
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
                    restore_service = true;
                    Ok(())
                }
                Undo::Restore(_) => Ok(()),
            };
            if let Err(detail) = result {
                left.push(detail);
            }
        }
        if restore_service {
            // L'unité d'avant, telle quelle ; le service reprend l'état d'avant : relancé s'il
            // tournait, arrêté sinon.
            let restored = match before.unit {
                Some(text) => self.service.restore_unit(text),
                None => Ok(()),
            }
            .and_then(|()| {
                // FIX:01M460G9AE6AFPSTRF4ZD872QB : l'activation au démarrage d'avant est remise,
                // réactivée si elle l'était, retirée sinon.
                if before.was_enabled {
                    self.service.enable()
                } else {
                    self.service.disable()
                }
            })
            .and_then(|()| {
                if before.was_active {
                    self.service.restart()
                } else {
                    self.service.stop()
                }
            });
            if let Err(error) = restored {
                left.push(error.to_string());
            }
        }
        left
    }

    /// Retire les fichiers que Hearth connaît dans le dossier de données, puis le dossier s'il est
    /// vide. Rend les noms de ce qui reste (fichiers qui ne sont pas à Hearth : intacts).
    // FIX:01M460G9WVEJW4GPTAZ6MVVC0V : les temporaires d'écriture et le dossier `update/` partent aussi.
    fn remove_known_data(&self) -> Result<Vec<String>, HostError> {
        let data = &self.paths.data_dir;
        for name in DATA_FILES {
            self.host.remove_file(&data.join(name))?;
        }
        // Les temporaires d'écriture de l'identité (peuvent contenir une clé privée) et le dossier
        // de la mise à jour, avec ce que Hearth y écrit.
        for name in self.host.list_dir(data)? {
            if is_identity_temporary(&name)
                || is_database_temporary(&name)
                || is_fingerprint_secret_temporary(&name)
            {
                self.host.remove_file(&data.join(&name))?;
            }
        }
        let update = data.join(UPDATE_DIR);
        for name in UPDATE_FILES {
            self.host.remove_file(&update.join(name))?;
        }
        for name in self.host.list_dir(&update)? {
            if is_update_temporary(&name) {
                self.host.remove_file(&update.join(&name))?;
            }
        }
        self.host.remove_dir_if_empty(&update)?;
        self.host.remove_dir_if_empty(data)?;
        self.host.list_dir(data)
    }

    /// Retire les fichiers voisins du binaire restés après une copie interrompue
    /// (`.hearth-agent.new-<pid>`).
    fn remove_binary_temporaries(&self) -> Result<(), HostError> {
        let Some(dir) = self.paths.binary.parent() else {
            return Ok(());
        };
        for name in self.host.list_dir(dir)? {
            if is_binary_temporary(&name) {
                self.host.remove_file(&dir.join(&name))?;
            }
        }
        Ok(())
    }

    /// Les fichiers de l'identité, et ses temporaires d'écriture.
    fn remove_identity(&self) -> Result<(), String> {
        self.remove_files(&IDENTITY_FILES)?;
        let data = &self.paths.data_dir;
        let names = self.host.list_dir(data).map_err(|e| e.to_string())?;
        for name in names.iter().filter(|name| is_identity_temporary(name)) {
            self.host
                .remove_file(&data.join(name))
                .map_err(|e| e.to_string())?;
        }
        Ok(())
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
    pub fn uninstall(&self, plan: &UninstallPlan) -> Uninstalled {
        let paths = &self.paths;
        let mut failed = Vec::new();
        let mut foreign = Vec::new();
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
                    .and_then(|()| self.remove_binary_temporaries())
                    .map_err(|e| e.to_string()),
            );
        }
        if plan.remove_data {
            match self.remove_known_data() {
                Ok(left) => foreign = left,
                Err(error) => attempt(Err(error.to_string())),
            }
        }
        if plan.remove_config {
            attempt(self.remove_config().map_err(|e| e.to_string()));
        }
        Uninstalled { failed, foreign }
    }
}

/// L'adresse jointe pour la vérification : `0.0.0.0` et `::` s'atteignent par le bouclage.
pub(crate) fn probe_ip(listen: IpAddr) -> IpAddr {
    match listen {
        IpAddr::V4(ip) if ip.is_unspecified() => IpAddr::from([127, 0, 0, 1]),
        IpAddr::V6(ip) if ip.is_unspecified() => IpAddr::from([0, 0, 0, 0, 0, 0, 0, 1]),
        other => other,
    }
}

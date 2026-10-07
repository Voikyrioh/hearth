//! Entrée de démarrage de Windows (BR-CLIENT-006/007, HRT-29) : lecture, écriture et
//! migration derrière le port `RunRegistry`, pour la tester sans toucher au vrai
//! registre. Remplace le greffon `tauri-plugin-autostart`, qui écrit le chemin SANS
//! guillemets (voir ADR-0028).

use crate::domain::{
    TASK_MANAGER_ENABLED, migrated_run_value, startup_command, task_manager_allows,
};
use crate::error::AppError;
use crate::settings::Autostart;

/// Ruche de la valeur : l'utilisateur (écrite par Hearth) ou la machine (lue seulement,
/// comme le greffon : une valeur posée là par un administrateur compte comme « activé »).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hive {
    User,
    Machine,
}

/// Accès au registre de Windows, réduit à ce que fait le démarrage. Toutes les
/// méthodes agissent sur la valeur nommée `STARTUP_ENTRY_NAME`.
pub trait RunRegistry {
    /// Valeur sous `...\Run` ; `None` si absente.
    fn run_value(&self, hive: Hive) -> Result<Option<String>, AppError>;
    /// Écrit la valeur sous `...\Run` de l'utilisateur.
    fn set_run_value(&self, value: &str) -> Result<(), AppError>;
    /// Retire la valeur ; absente = rien à faire ; accès refusé sur la machine = ignoré.
    fn remove_run_value(&self, hive: Hive) -> Result<(), AppError>;
    /// Octets sous `...\StartupApproved\Run` (Gestionnaire des tâches) ; `None` si absents.
    fn approved(&self, hive: Hive) -> Result<Option<Vec<u8>>, AppError>;
    /// Écrit « activé » dans `StartupApproved\Run` de l'utilisateur (sans effet si la clé n'existe pas).
    fn set_approved_enabled(&self, bytes: &[u8]) -> Result<(), AppError>;
}

/// L'entrée de démarrage de l'application : le registre et le chemin de l'exécutable.
pub struct StartupEntry<R> {
    registry: R,
    exe: String,
}

impl<R: RunRegistry> StartupEntry<R> {
    pub fn new(registry: R, exe: impl Into<String>) -> Self {
        Self {
            registry,
            exe: exe.into(),
        }
    }

    /// Migre une ancienne valeur sans guillemets vers la forme entre guillemets (HRT-29).
    /// Ne change JAMAIS le choix de l'utilisateur : valeur absente = rien ; activée reste
    /// activée ; `StartupApproved` (désactivée dans le Gestionnaire des tâches) n'est
    /// pas touché ; le chemin reste celui de l'ancienne valeur. `true` si réécrite.
    pub fn migrate(&self) -> Result<bool, AppError> {
        let Some(old) = self.registry.run_value(Hive::User)? else {
            return Ok(false);
        };
        let Some(new) = migrated_run_value(&old) else {
            return Ok(false);
        };
        self.registry.set_run_value(&new)?;
        Ok(true)
    }
}

impl<R: RunRegistry> Autostart for StartupEntry<R> {
    /// Comme le greffon : entrée présente (quelle que soit sa forme) dans l'une des ruches
    /// ET non désactivée dans le Gestionnaire des tâches de chacune.
    fn is_enabled(&self) -> Result<bool, AppError> {
        let registered = self.registry.run_value(Hive::User)?.is_some()
            || self.registry.run_value(Hive::Machine)?.is_some();
        if !registered {
            return Ok(false);
        }
        for hive in [Hive::Machine, Hive::User] {
            if !task_manager_allows(self.registry.approved(hive)?.as_deref()) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn set_enabled(&self, enabled: bool) -> Result<(), AppError> {
        if enabled {
            let command = startup_command(&self.exe)
                .map_err(|error| AppError::Autostart(error.to_string()))?;
            self.registry.set_run_value(&command)?;
            self.registry.set_approved_enabled(&TASK_MANAGER_ENABLED)
        } else {
            self.registry.remove_run_value(Hive::User)?;
            self.registry.remove_run_value(Hive::Machine)
        }
    }
}

#[cfg(windows)]
pub use self::windows::WindowsRegistry;

#[cfg(windows)]
mod windows {
    use std::io::ErrorKind;

    use windows_registry::{CURRENT_USER, Key, LOCAL_MACHINE, Type};

    use super::{Hive, RunRegistry};
    use crate::domain::STARTUP_ENTRY_NAME;
    use crate::error::AppError;

    const RUN_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run";
    const APPROVED_KEY: &str =
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

    /// Le vrai registre de Windows (jamais utilisé par les tests locaux).
    pub struct WindowsRegistry;

    fn root(hive: Hive) -> &'static Key {
        match hive {
            Hive::User => CURRENT_USER,
            Hive::Machine => LOCAL_MACHINE,
        }
    }

    fn io(error: impl Into<std::io::Error>) -> std::io::Error {
        error.into()
    }

    fn failed(error: std::io::Error) -> AppError {
        AppError::Autostart(error.to_string())
    }

    /// Absent (valeur ou clé) = `None`.
    fn optional<T, E: Into<std::io::Error>>(result: Result<T, E>) -> Result<Option<T>, AppError> {
        match result {
            Ok(value) => Ok(Some(value)),
            Err(error) => {
                let error = io(error);
                if error.kind() == ErrorKind::NotFound {
                    Ok(None)
                } else {
                    Err(failed(error))
                }
            }
        }
    }

    impl RunRegistry for WindowsRegistry {
        fn run_value(&self, hive: Hive) -> Result<Option<String>, AppError> {
            match optional(root(hive).open(RUN_KEY))? {
                None => Ok(None),
                Some(key) => optional(key.get_string(STARTUP_ENTRY_NAME)),
            }
        }

        fn set_run_value(&self, value: &str) -> Result<(), AppError> {
            CURRENT_USER
                .create(RUN_KEY)
                .and_then(|key| key.set_string(STARTUP_ENTRY_NAME, value))
                .map_err(|error| failed(io(error)))
        }

        fn remove_run_value(&self, hive: Hive) -> Result<(), AppError> {
            let machine = hive == Hive::Machine;
            let key = match optional(root(hive).options().write().open(RUN_KEY)) {
                Ok(Some(key)) => key,
                Ok(None) => return Ok(()),
                // Sans droits d'administration, rien à retirer de la machine pour nous.
                Err(_) if machine => return Ok(()),
                Err(error) => return Err(error),
            };
            match optional(key.remove_value(STARTUP_ENTRY_NAME)) {
                Ok(_) => Ok(()),
                Err(_) if machine => Ok(()),
                Err(error) => Err(error),
            }
        }

        fn approved(&self, hive: Hive) -> Result<Option<Vec<u8>>, AppError> {
            let Some(key) = optional(root(hive).open(APPROVED_KEY))? else {
                return Ok(None);
            };
            Ok(optional(key.get_value(STARTUP_ENTRY_NAME))?.map(|value| value.to_vec()))
        }

        fn set_approved_enabled(&self, bytes: &[u8]) -> Result<(), AppError> {
            let Some(key) = optional(CURRENT_USER.options().write().open(APPROVED_KEY))? else {
                return Ok(());
            };
            key.set_bytes(STARTUP_ENTRY_NAME, Type::Bytes, bytes)
                .map_err(|error| failed(io(error)))
        }
    }
}

/// Hors Windows (le client n'y existe pas) : le démarrage est simplement inaccessible.
#[cfg(not(windows))]
pub struct WindowsRegistry;

#[cfg(not(windows))]
impl RunRegistry for WindowsRegistry {
    fn run_value(&self, _: Hive) -> Result<Option<String>, AppError> {
        Err(unsupported())
    }
    fn set_run_value(&self, _: &str) -> Result<(), AppError> {
        Err(unsupported())
    }
    fn remove_run_value(&self, _: Hive) -> Result<(), AppError> {
        Err(unsupported())
    }
    fn approved(&self, _: Hive) -> Result<Option<Vec<u8>>, AppError> {
        Err(unsupported())
    }
    fn set_approved_enabled(&self, _: &[u8]) -> Result<(), AppError> {
        Err(unsupported())
    }
}

#[cfg(not(windows))]
fn unsupported() -> AppError {
    AppError::Autostart("démarrage avec Windows : système non pris en charge".into())
}

/// L'entrée de démarrage de l'application en cours (chemin de l'exécutable courant).
pub fn current() -> Result<StartupEntry<WindowsRegistry>, AppError> {
    let exe = std::env::current_exe()
        .map_err(|error| AppError::Autostart(error.to_string()))?
        .into_os_string()
        .into_string()
        .map_err(|_| AppError::Autostart("chemin de l'application non UTF-8".into()))?;
    Ok(StartupEntry::new(WindowsRegistry, exe))
}

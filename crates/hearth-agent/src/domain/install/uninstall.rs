//! La désinstallation (BR-INSTALL-011) : l'utilisateur choisit de conserver ou de supprimer les
//! comptes, le journal et la configuration.

use thiserror::Error;

use super::observed::{BinaryState, Observed, UnitState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataChoice {
    /// Les comptes, le journal, l'identité et la configuration restent sur la machine.
    Keep,
    /// Tout est supprimé : « aucune trace de l'agent ne reste sur ta machine ».
    Purge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("Réponds 'conserver' ou 'supprimer'.")]
pub struct ChoiceError;

/// Lit la réponse à « Supprimer aussi les comptes, le journal et la configuration ? ».
pub fn parse_choice(answer: &str) -> Result<DataChoice, ChoiceError> {
    match answer.trim().to_lowercase().as_str() {
        "conserver" => Ok(DataChoice::Keep),
        "supprimer" => Ok(DataChoice::Purge),
        _ => Err(ChoiceError),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UninstallPlan {
    /// Rien n'est installé et rien ne traîne : « Rien à désinstaller ».
    pub nothing_to_do: bool,
    /// Arrêter le service, le retirer du démarrage et supprimer l'unité.
    pub remove_service: bool,
    pub remove_binary: bool,
    /// Supprimer le dossier de données (base, journal, identité).
    pub remove_data: bool,
    pub remove_config: bool,
}

/// L'agent est-il installé (binaire ou unité, ou service qui tourne) ?
pub fn is_installed(observed: &Observed) -> bool {
    matches!(observed.binary, BinaryState::Present { .. })
        || observed.unit == UnitState::Present
        || observed.service_active
}

/// Ce que la désinstallation retire. Le binaire et l'unité partent toujours (sauf installation
/// gérée : le système les fournit) ; les données et la configuration seulement si l'utilisateur
/// l'a choisi.
pub fn uninstall_plan(observed: &Observed, choice: DataChoice) -> UninstallPlan {
    let installed = is_installed(observed);
    let leftovers = observed.data.dir_exists || observed.data.any() || observed.config_exists;
    let purge = choice == DataChoice::Purge;
    UninstallPlan {
        nothing_to_do: !installed && !leftovers,
        remove_service: observed.unit == UnitState::Present || observed.service_active,
        remove_binary: matches!(observed.binary, BinaryState::Present { .. }),
        remove_data: purge && (observed.data.dir_exists || observed.data.any()),
        remove_config: purge && observed.config_exists,
    }
}

#[cfg(test)]
mod tests {
    use super::super::observed::{DataState, blank};
    use super::*;

    fn installed() -> Observed {
        Observed {
            binary: BinaryState::Present {
                version: None,
                identical: false,
            },
            unit: UnitState::Present,
            service_active: true,
            data: DataState {
                dir_exists: true,
                identity: true,
                database: true,
            },
            config_exists: true,
            admin_accounts: 1,
        }
    }

    #[test]
    fn only_conserver_and_supprimer_are_answers() {
        assert_eq!(parse_choice("conserver"), Ok(DataChoice::Keep));
        assert_eq!(parse_choice("  Supprimer \n"), Ok(DataChoice::Purge));
        for bad in ["", "oui", "non", "keep", "conserve"] {
            assert_eq!(parse_choice(bad), Err(ChoiceError), "{bad}");
        }
        assert_eq!(
            ChoiceError.to_string(),
            "Réponds 'conserver' ou 'supprimer'."
        );
    }

    #[test]
    fn keeping_removes_the_service_and_the_binary_and_nothing_else() {
        let plan = uninstall_plan(&installed(), DataChoice::Keep);
        assert!(!plan.nothing_to_do);
        assert!(plan.remove_service);
        assert!(plan.remove_binary);
        assert!(
            !plan.remove_data,
            "BR-INSTALL-011 : comptes et journal conservés"
        );
        assert!(!plan.remove_config);
    }

    #[test]
    fn purging_removes_the_data_and_the_configuration_too() {
        let plan = uninstall_plan(&installed(), DataChoice::Purge);
        assert!(plan.remove_service && plan.remove_binary);
        assert!(plan.remove_data && plan.remove_config);
    }

    #[test]
    fn nothing_installed_and_nothing_left_means_nothing_to_uninstall() {
        let plan = uninstall_plan(&blank(), DataChoice::Purge);
        assert!(plan.nothing_to_do);
        assert!(!plan.remove_service && !plan.remove_binary && !plan.remove_data);
    }

    #[test]
    fn data_kept_by_a_previous_uninstall_can_still_be_purged() {
        let mut leftovers = blank();
        leftovers.data = DataState {
            dir_exists: true,
            identity: true,
            database: true,
        };
        leftovers.config_exists = true;
        let plan = uninstall_plan(&leftovers, DataChoice::Purge);
        assert!(!plan.nothing_to_do);
        assert!(plan.remove_data && plan.remove_config);
        assert!(!plan.remove_service && !plan.remove_binary);
        assert!(!is_installed(&leftovers));
    }

    #[test]
    fn a_managed_installation_keeps_the_binary_the_system_provides() {
        let mut managed = installed();
        managed.binary = BinaryState::ProvidedBySystem;
        managed.unit = UnitState::NotWritten;
        managed.service_active = false;
        let plan = uninstall_plan(&managed, DataChoice::Purge);
        assert!(!plan.remove_binary);
        assert!(!plan.remove_service);
        assert!(plan.remove_data && plan.remove_config);
        assert!(!plan.nothing_to_do);
    }

    #[test]
    fn a_running_service_without_unit_or_binary_is_still_stopped() {
        let mut odd = blank();
        odd.service_active = true;
        let plan = uninstall_plan(&odd, DataChoice::Keep);
        assert!(plan.remove_service);
        assert!(is_installed(&odd));
    }
}

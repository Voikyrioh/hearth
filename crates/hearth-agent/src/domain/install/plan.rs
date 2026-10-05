//! Le plan d'une installation, d'après l'état observé de la machine : première installation,
//! réinstallation de la même version, mise à niveau, ou installation abîmée à réparer
//! (BR-INSTALL-002, 003, 004, 007).

use thiserror::Error;

use super::observed::{BinaryState, Observed, UnitState};
use super::version::Version;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallKind {
    /// Rien n'existe : tout est créé, dont le premier compte (BR-INSTALL-002).
    Fresh,
    /// Installation complète, même version.
    Reinstall,
    /// Installation complète, version plus ancienne.
    Upgrade { from: Version },
    /// Des traces d'une installation qui n'est pas complète (binaire, unité, identité ou base
    /// manquants, version illisible) : ce qui manque est refait, les données sont conservées.
    Repair,
}

/// Ce que le service devient (BR-INSTALL-007).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceAction {
    /// Le service n'est pas touché : il n'est pas interrompu inutilement, ou le système le gère.
    Leave,
    /// L'unité est (ré)écrite et le service démarré, il ne tournait pas.
    Start,
    /// Le service tournait avec l'ancien binaire : il redémarre sur le nouveau.
    Restart,
}

/// Ce qu'une réinstallation conserve (BR-INSTALL-003).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kept {
    Accounts,
    Journal,
    /// Le certificat et sa clé : donc l'empreinte (BR-INSTALL-004).
    Identity,
    Config,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallPlan {
    pub kind: InstallKind,
    /// Aucun administrateur n'existe : le premier compte est à créer (BR-INSTALL-002).
    pub needs_first_admin: bool,
    /// Le binaire installé est à remplacer par celui qu'on installe.
    pub replace_binary: bool,
    /// La configuration est à créer (elle n'est jamais écrasée).
    pub write_config: bool,
    pub service: ServiceAction,
    pub keeps: Vec<Kept>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum PlanError {
    /// Une partie de l'identité manque : on ne la régénère jamais en silence (l'empreinte ne doit
    /// pas changer) et on ne supprime rien. Rien n'est écrit.
    #[error(
        "L'identité du serveur est incomplète dans le dossier de données : le certificat, la clé et l'identifiant d'installation doivent exister ensemble. Rien n'a été modifié. Restaure-les depuis une sauvegarde ; si tu acceptes une nouvelle empreinte (les clients devront la réapprouver), supprime ces trois fichiers toi-même puis relance l'installation."
    )]
    IdentityIncomplete,
    /// On ne rétrograde pas : le binaire installé est plus récent que celui qu'on installe.
    #[error(
        "Une version plus récente de l'agent ({installed}) est déjà installée : la version {target} ne la remplace pas. Aucune modification n'a été apportée à ta machine."
    )]
    NewerInstalled { installed: Version, target: Version },
}

/// Décide ce qu'il faut faire d'après ce qui est observé, pour installer la version `target`.
pub fn plan_install(observed: &Observed, target: Version) -> Result<InstallPlan, PlanError> {
    if observed.data.identity_partial {
        return Err(PlanError::IdentityIncomplete);
    }
    if let BinaryState::Present {
        version: Some(installed),
        ..
    } = observed.binary
        && installed > target
    {
        return Err(PlanError::NewerInstalled { installed, target });
    }

    let kind = kind_of(observed, target);
    let replace_binary = match observed.binary {
        BinaryState::ProvidedBySystem => false,
        BinaryState::Present {
            identical: true, ..
        } => false,
        BinaryState::Absent | BinaryState::Present { .. } => true,
    };
    Ok(InstallPlan {
        kind,
        needs_first_admin: observed.admin_accounts == 0,
        replace_binary,
        write_config: !observed.config_exists,
        service: service_action(observed, kind, replace_binary),
        keeps: kept(observed, kind),
    })
}

fn kind_of(observed: &Observed, target: Version) -> InstallKind {
    if observed.is_blank() {
        return InstallKind::Fresh;
    }
    let binary_ok = matches!(
        observed.binary,
        BinaryState::Present {
            version: Some(_),
            ..
        } | BinaryState::ProvidedBySystem
    );
    let unit_ok = matches!(observed.unit, UnitState::Present | UnitState::NotWritten);
    if !(binary_ok && unit_ok && observed.data.identity && observed.data.database) {
        return InstallKind::Repair;
    }
    match observed.binary {
        BinaryState::Present {
            version: Some(installed),
            ..
        } if installed < target => InstallKind::Upgrade { from: installed },
        _ => InstallKind::Reinstall,
    }
}

fn service_action(observed: &Observed, kind: InstallKind, replace_binary: bool) -> ServiceAction {
    if observed.unit == UnitState::NotWritten {
        return ServiceAction::Leave;
    }
    // Un service qui tourne déjà avec exactement ce binaire, une unité et des données en ordre :
    // rien à relancer (BR-INSTALL-007).
    if kind == InstallKind::Reinstall && observed.service_active && !replace_binary {
        return ServiceAction::Leave;
    }
    if observed.service_active {
        ServiceAction::Restart
    } else {
        ServiceAction::Start
    }
}

fn kept(observed: &Observed, kind: InstallKind) -> Vec<Kept> {
    if kind == InstallKind::Fresh {
        return Vec::new();
    }
    let mut kept = Vec::new();
    if observed.admin_accounts > 0 || observed.data.database {
        kept.push(Kept::Accounts);
    }
    if observed.data.database {
        kept.push(Kept::Journal);
    }
    if observed.data.identity {
        kept.push(Kept::Identity);
    }
    if observed.config_exists {
        kept.push(Kept::Config);
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::super::observed::{DataState, blank};
    use super::*;

    const TARGET: Version = Version::new(0, 2, 0);

    fn complete(version: Version, identical: bool, active: bool) -> Observed {
        Observed {
            binary: BinaryState::Present {
                version: Some(version),
                identical,
            },
            unit: UnitState::Present,
            service_active: active,
            data: DataState {
                dir_exists: true,
                identity: true,
                database: true,
                ..DataState::default()
            },
            config_exists: true,
            admin_accounts: 2,
        }
    }

    #[test]
    fn a_blank_machine_is_a_first_installation_that_creates_everything_and_the_first_account() {
        let plan = plan_install(&blank(), TARGET).unwrap();
        assert_eq!(plan.kind, InstallKind::Fresh);
        assert!(plan.needs_first_admin, "BR-INSTALL-002");
        assert!(plan.replace_binary);
        assert!(plan.write_config);
        assert_eq!(plan.service, ServiceAction::Start);
        assert!(plan.keeps.is_empty());
    }

    #[test]
    fn a_reinstallation_keeps_accounts_journal_identity_and_configuration() {
        let plan = plan_install(&complete(TARGET, false, true), TARGET).unwrap();
        assert_eq!(plan.kind, InstallKind::Reinstall);
        assert!(
            !plan.needs_first_admin,
            "BR-INSTALL-003 : pas de nouveau compte"
        );
        assert!(!plan.write_config, "la configuration n'est jamais écrasée");
        assert_eq!(
            plan.keeps,
            [Kept::Accounts, Kept::Journal, Kept::Identity, Kept::Config]
        );
    }

    #[test]
    fn the_same_binary_on_a_running_service_does_not_interrupt_it() {
        let plan = plan_install(&complete(TARGET, true, true), TARGET).unwrap();
        assert_eq!(plan.kind, InstallKind::Reinstall);
        assert!(!plan.replace_binary);
        assert_eq!(plan.service, ServiceAction::Leave, "BR-INSTALL-007");
    }

    #[test]
    fn the_same_version_with_other_bytes_restarts_the_running_service() {
        let plan = plan_install(&complete(TARGET, false, true), TARGET).unwrap();
        assert!(plan.replace_binary);
        assert_eq!(plan.service, ServiceAction::Restart);
    }

    #[test]
    fn a_stopped_service_is_started_even_when_nothing_else_changes() {
        let plan = plan_install(&complete(TARGET, true, false), TARGET).unwrap();
        assert_eq!(plan.service, ServiceAction::Start);
    }

    #[test]
    fn an_older_version_is_an_upgrade_that_restarts_the_service() {
        let from = Version::new(0, 1, 0);
        let plan = plan_install(&complete(from, false, true), TARGET).unwrap();
        assert_eq!(plan.kind, InstallKind::Upgrade { from });
        assert!(plan.replace_binary);
        assert_eq!(plan.service, ServiceAction::Restart);
        assert!(!plan.needs_first_admin);
    }

    #[test]
    fn a_newer_installed_version_is_never_replaced() {
        let error =
            plan_install(&complete(Version::new(0, 3, 0), false, true), TARGET).unwrap_err();
        assert_eq!(
            error,
            PlanError::NewerInstalled {
                installed: Version::new(0, 3, 0),
                target: TARGET
            }
        );
        assert!(error.to_string().contains("0.3.0"));
    }

    #[test]
    fn a_partial_identity_is_refused_before_anything_and_never_regenerated() {
        let mut observed = blank();
        observed.data.dir_exists = true;
        observed.data.identity_partial = true;
        assert_eq!(
            plan_install(&observed, TARGET),
            Err(PlanError::IdentityIncomplete)
        );
        // Même au milieu d'une installation par ailleurs complète.
        let mut full = complete(TARGET, true, true);
        full.data.identity = false;
        full.data.identity_partial = true;
        assert_eq!(
            plan_install(&full, TARGET),
            Err(PlanError::IdentityIncomplete)
        );
        assert!(
            PlanError::IdentityIncomplete
                .to_string()
                .contains("Rien n'a été modifié")
        );
    }

    #[test]
    fn a_damaged_installation_is_repaired_and_keeps_what_is_there() {
        // Binaire absent, données là.
        let mut observed = complete(TARGET, false, false);
        observed.binary = BinaryState::Absent;
        observed.unit = UnitState::Absent;
        let plan = plan_install(&observed, TARGET).unwrap();
        assert_eq!(plan.kind, InstallKind::Repair);
        assert!(plan.replace_binary);
        assert_eq!(plan.service, ServiceAction::Start);
        assert_eq!(
            plan.keeps,
            [Kept::Accounts, Kept::Journal, Kept::Identity, Kept::Config]
        );
        assert!(!plan.needs_first_admin);
    }

    #[test]
    fn each_missing_piece_makes_an_installation_damaged() {
        let mut no_unit = complete(TARGET, true, false);
        no_unit.unit = UnitState::Absent;
        let mut no_identity = complete(TARGET, true, true);
        no_identity.data.identity = false;
        let mut no_database = complete(TARGET, true, true);
        no_database.data.database = false;
        let mut unreadable = complete(TARGET, true, true);
        unreadable.binary = BinaryState::Present {
            version: None,
            identical: false,
        };
        for (name, observed) in [
            ("unité", no_unit),
            ("identité", no_identity),
            ("base", no_database),
            ("version illisible", unreadable),
        ] {
            let plan = plan_install(&observed, TARGET).unwrap();
            assert_eq!(plan.kind, InstallKind::Repair, "{name}");
        }
    }

    #[test]
    fn leftovers_alone_make_a_damaged_installation_not_a_fresh_one() {
        // Après une désinstallation qui conserve les données : le dossier et la base restent.
        let mut observed = blank();
        observed.data = DataState {
            dir_exists: true,
            identity: true,
            database: true,
            ..DataState::default()
        };
        observed.admin_accounts = 1;
        observed.config_exists = true;
        let plan = plan_install(&observed, TARGET).unwrap();
        assert_eq!(plan.kind, InstallKind::Repair);
        assert!(!plan.needs_first_admin, "les comptes sont retrouvés");
        assert!(plan.keeps.contains(&Kept::Identity), "BR-INSTALL-004");
    }

    #[test]
    fn a_database_without_administrator_asks_for_the_first_one_again() {
        // Une installation interrompue après la base mais avant le compte.
        let mut observed = blank();
        observed.data = DataState {
            dir_exists: true,
            identity: true,
            database: true,
            ..DataState::default()
        };
        let plan = plan_install(&observed, TARGET).unwrap();
        assert_eq!(plan.kind, InstallKind::Repair);
        assert!(plan.needs_first_admin);
    }

    #[test]
    fn a_managed_installation_never_touches_binary_or_service() {
        let mut observed = complete(TARGET, false, true);
        observed.binary = BinaryState::ProvidedBySystem;
        observed.unit = UnitState::NotWritten;
        let plan = plan_install(&observed, TARGET).unwrap();
        assert_eq!(plan.kind, InstallKind::Reinstall);
        assert!(!plan.replace_binary);
        assert_eq!(plan.service, ServiceAction::Leave);

        let mut fresh = blank();
        fresh.binary = BinaryState::ProvidedBySystem;
        fresh.unit = UnitState::NotWritten;
        let plan = plan_install(&fresh, TARGET).unwrap();
        assert_eq!(plan.kind, InstallKind::Fresh);
        assert!(!plan.replace_binary);
        assert_eq!(plan.service, ServiceAction::Leave);
    }
}

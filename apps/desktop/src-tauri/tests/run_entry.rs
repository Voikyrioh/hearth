//! Entrée de démarrage de Windows (HRT-29) : valeur exacte, reconnaissance des deux formes,
//! migration sans changer le choix de l'utilisateur. Registre SIMULÉ : aucun test n'écrit
//! dans le vrai registre.
#![allow(clippy::unwrap_used, clippy::expect_used)] // tests : les helpers peuvent paniquer

use std::cell::RefCell;
use std::collections::HashMap;

use hearth_desktop_lib::domain::{
    RunValueForm, StartupCommandError, migrated_run_value, run_value_form, startup_command,
    task_manager_allows,
};
use hearth_desktop_lib::error::AppError;
use hearth_desktop_lib::settings::Autostart;
use hearth_desktop_lib::startup::{
    Hive, RegistryFailure, RunRegistry, StartupEntry, classify_registry_error,
};

const SPACED: &str = r"C:\Users\Jean Dupont\AppData\Local\Hearth\hearth-desktop.exe";
const PLAIN: &str = r"C:\Users\jean\AppData\Local\Hearth\hearth-desktop.exe";
const DISABLED_BY_TASK_MANAGER: [u8; 12] = [3, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8];

/// Registre simulé : (ruche) -> valeur Run ; (ruche) -> octets StartupApproved.
#[derive(Default)]
struct FakeRegistry {
    run: RefCell<HashMap<&'static str, String>>,
    approved: RefCell<HashMap<&'static str, Vec<u8>>>,
    approved_key_exists: bool,
    writes: RefCell<Vec<String>>,
}

fn name(hive: Hive) -> &'static str {
    match hive {
        Hive::User => "user",
        Hive::Machine => "machine",
    }
}

impl FakeRegistry {
    fn with_run(value: &str) -> Self {
        let fake = Self {
            approved_key_exists: true,
            ..Self::default()
        };
        fake.run.borrow_mut().insert("user", value.to_owned());
        fake
    }
}

impl RunRegistry for &FakeRegistry {
    fn run_value(&self, hive: Hive) -> Result<Option<String>, AppError> {
        Ok(self.run.borrow().get(name(hive)).cloned())
    }
    fn set_run_value(&self, value: &str) -> Result<(), AppError> {
        self.writes.borrow_mut().push(format!("run={value}"));
        self.run.borrow_mut().insert("user", value.to_owned());
        Ok(())
    }
    fn remove_run_value(&self, hive: Hive) -> Result<(), AppError> {
        self.writes
            .borrow_mut()
            .push(format!("remove={}", name(hive)));
        self.run.borrow_mut().remove(name(hive));
        Ok(())
    }
    fn approved(&self, hive: Hive) -> Result<Option<Vec<u8>>, AppError> {
        Ok(self.approved.borrow().get(name(hive)).cloned())
    }
    fn set_approved_enabled(&self, bytes: &[u8]) -> Result<(), AppError> {
        self.writes.borrow_mut().push("approved".into());
        if self.approved_key_exists {
            self.approved.borrow_mut().insert("user", bytes.to_vec());
        }
        Ok(())
    }
}

#[test]
fn the_command_line_is_the_quoted_path_then_the_minimized_flag() {
    assert_eq!(
        startup_command(SPACED).unwrap(),
        r#""C:\Users\Jean Dupont\AppData\Local\Hearth\hearth-desktop.exe" --minimized"#
    );
    assert_eq!(
        startup_command(PLAIN).unwrap(),
        r#""C:\Users\jean\AppData\Local\Hearth\hearth-desktop.exe" --minimized"#
    );
}

#[test]
fn a_path_with_a_quote_or_nothing_is_refused() {
    assert_eq!(
        startup_command(r#"C:\a"b\hearth-desktop.exe"#),
        Err(StartupCommandError::QuoteInPath)
    );
    assert_eq!(startup_command(""), Err(StartupCommandError::EmptyPath));
    assert_eq!(startup_command("  "), Err(StartupCommandError::EmptyPath));
    // Refusé aussi côté réglage : rien n'est écrit, l'erreur est typée.
    let registry = FakeRegistry::default();
    let entry = StartupEntry::new(&registry, r#"C:\a"b.exe"#);
    assert!(matches!(
        entry.set_enabled(true),
        Err(AppError::Autostart(_))
    ));
    assert!(registry.writes.borrow().is_empty());
}

#[test]
fn both_forms_written_by_hearth_are_recognised() {
    assert_eq!(
        run_value_form(&format!("{SPACED} --minimized")),
        RunValueForm::Unquoted
    );
    assert_eq!(
        run_value_form(&format!("\"{SPACED}\" --minimized")),
        RunValueForm::Quoted
    );
    assert_eq!(
        run_value_form(&format!("\"{PLAIN}\" --minimized")),
        RunValueForm::Quoted
    );
    // Posées à la main ou inconnues : jamais réécrites.
    for other in [
        "",
        "--minimized",
        r"C:\autre\appli.exe",
        r"C:\Hearth\hearth-desktop.exe --autre",
        r#""C:\Hearth\hearth-desktop.exe --minimized"#,
        r#""C:\Hearth\hearth-desktop.exe" --autre"#,
        r#"C:\He"arth\h.exe --minimized"#,
        r"C:\Hearth\script.cmd --minimized",
    ] {
        assert_eq!(run_value_form(other), RunValueForm::Other, "{other}");
    }
}

#[test]
fn only_an_unquoted_value_is_migrated_and_keeps_its_own_path() {
    assert_eq!(
        migrated_run_value(&format!("{SPACED} --minimized")).unwrap(),
        format!("\"{SPACED}\" --minimized")
    );
    // Déjà entre guillemets, ou forme inconnue : aucune décision de migration.
    assert_eq!(
        migrated_run_value(&format!("\"{SPACED}\" --minimized")),
        None
    );
    assert_eq!(migrated_run_value(r"C:\autre\appli.exe"), None);
    assert_eq!(migrated_run_value(""), None);
}

#[test]
fn the_task_manager_state_is_read_like_the_plugin_did() {
    assert!(task_manager_allows(None));
    assert!(task_manager_allows(Some(&[2, 0, 0, 0])));
    assert!(task_manager_allows(Some(&[2; 7])));
    assert!(task_manager_allows(Some(&[
        2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
    ])));
    assert!(!task_manager_allows(Some(&DISABLED_BY_TASK_MANAGER)));
    assert!(!task_manager_allows(Some(&[0, 0, 0, 0, 0, 0, 0, 1])));
}

#[test]
fn enabling_writes_one_quoted_value_and_it_reads_back_as_enabled() {
    let registry = FakeRegistry {
        approved_key_exists: true,
        ..FakeRegistry::default()
    };
    let entry = StartupEntry::new(&registry, SPACED);
    assert!(!entry.is_enabled().unwrap());
    entry.set_enabled(true).unwrap();
    assert!(entry.is_enabled().unwrap());
    let run = registry.run.borrow();
    assert_eq!(run.len(), 1, "une seule valeur, jamais de seconde entrée");
    assert_eq!(
        run["user"],
        format!("\"{SPACED}\" --minimized"),
        "valeur exacte entre guillemets"
    );
    // Activer une seconde fois remplace la valeur, n'en ajoute pas.
    drop(run);
    entry.set_enabled(true).unwrap();
    assert_eq!(registry.run.borrow().len(), 1);
    entry.set_enabled(false).unwrap();
    assert!(!entry.is_enabled().unwrap());
    assert!(registry.run.borrow().is_empty());
}

#[test]
fn a_value_in_either_form_reads_as_enabled() {
    for value in [
        format!("{SPACED} --minimized"),
        format!("\"{SPACED}\" --minimized"),
    ] {
        let registry = FakeRegistry::with_run(&value);
        assert!(StartupEntry::new(&registry, SPACED).is_enabled().unwrap());
    }
}

#[test]
fn a_machine_wide_value_counts_as_enabled_like_the_plugin() {
    let registry = FakeRegistry::default();
    registry.run.borrow_mut().insert("machine", "x".into());
    assert!(StartupEntry::new(&registry, SPACED).is_enabled().unwrap());
}

#[test]
fn migration_rewrites_an_old_value_and_the_user_choice_stays_enabled() {
    let registry = FakeRegistry::with_run(&format!("{SPACED} --minimized"));
    let entry = StartupEntry::new(&registry, SPACED);
    assert!(entry.is_enabled().unwrap());
    assert!(entry.migrate().unwrap());
    assert!(entry.is_enabled().unwrap(), "activé reste activé");
    assert_eq!(
        registry.run.borrow()["user"],
        format!("\"{SPACED}\" --minimized")
    );
    // Idempotente : la seconde fois, rien n'est réécrit.
    registry.writes.borrow_mut().clear();
    assert!(!entry.migrate().unwrap());
    assert!(registry.writes.borrow().is_empty());
}

#[test]
fn migration_keeps_the_path_of_the_old_value_not_the_current_executable() {
    let registry =
        FakeRegistry::with_run(r"D:\ancien dossier\Hearth\hearth-desktop.exe --minimized");
    let entry = StartupEntry::new(&registry, SPACED);
    entry.migrate().unwrap();
    assert_eq!(
        registry.run.borrow()["user"],
        r#""D:\ancien dossier\Hearth\hearth-desktop.exe" --minimized"#
    );
}

#[test]
fn migration_of_an_absent_value_creates_nothing() {
    let registry = FakeRegistry::default();
    let entry = StartupEntry::new(&registry, SPACED);
    assert!(!entry.migrate().unwrap());
    assert!(registry.run.borrow().is_empty());
    assert!(registry.writes.borrow().is_empty());
    assert!(!entry.is_enabled().unwrap(), "absent reste absent");
}

#[test]
fn migration_never_touches_an_entry_disabled_in_the_task_manager() {
    let registry = FakeRegistry::with_run(&format!("{SPACED} --minimized"));
    registry
        .approved
        .borrow_mut()
        .insert("user", DISABLED_BY_TASK_MANAGER.to_vec());
    let entry = StartupEntry::new(&registry, SPACED);
    assert!(!entry.is_enabled().unwrap());
    assert!(entry.migrate().unwrap());
    assert!(!entry.is_enabled().unwrap(), "désactivé reste désactivé");
    assert_eq!(
        registry.approved.borrow()["user"],
        DISABLED_BY_TASK_MANAGER,
        "StartupApproved n'est pas touché"
    );
    assert!(
        registry
            .writes
            .borrow()
            .iter()
            .all(|w| w.starts_with("run=")),
        "seule la valeur Run est réécrite : {:?}",
        registry.writes.borrow()
    );
}

#[test]
fn migration_leaves_an_unknown_value_alone() {
    let registry = FakeRegistry::with_run(r"C:\autre\appli.exe /silencieux");
    let entry = StartupEntry::new(&registry, SPACED);
    assert!(!entry.migrate().unwrap());
    assert_eq!(
        registry.run.borrow()["user"],
        r"C:\autre\appli.exe /silencieux"
    );
}

#[test]
fn the_entry_name_is_the_product_name_the_installer_and_uninstaller_use() {
    let config: serde_json::Value =
        serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
    assert_eq!(
        hearth_desktop_lib::domain::STARTUP_ENTRY_NAME,
        config["productName"].as_str().unwrap()
    );
}

/// Les codes que `windows-registry` rend sont des HRESULT (`HRESULT::from_win32`), pas des
/// codes Win32 : `0x80070002`, jamais `2` (FIX:01M4B118DAFBQZYQX1E5ERY8CA).
#[test]
fn registry_error_codes_are_classified_on_the_hresult() {
    let hresult = |win32: u32| i32::from_ne_bytes((0x8007_0000 | win32).to_ne_bytes());
    assert_eq!(classify_registry_error(hresult(2)), RegistryFailure::Absent);
    assert_eq!(classify_registry_error(hresult(3)), RegistryFailure::Absent);
    assert_eq!(
        classify_registry_error(hresult(5)),
        RegistryFailure::AccessDenied
    );
    // Type inattendu (ERROR_INVALID_DATA), disque, etc. : une vraie erreur.
    assert_eq!(classify_registry_error(hresult(13)), RegistryFailure::Other);
    // Le code Win32 nu n'est PAS ce que la crate rend : il ne doit rien classer.
    assert_eq!(classify_registry_error(2), RegistryFailure::Other);
    assert_eq!(classify_registry_error(5), RegistryFailure::Other);
    assert_eq!(classify_registry_error(0), RegistryFailure::Other);
}

#[test]
fn a_command_line_longer_than_the_run_key_allows_is_refused() {
    let long = format!(r"C:\{}\hearth-desktop.exe", "d".repeat(260));
    assert_eq!(startup_command(&long), Err(StartupCommandError::TooLong));
    // Pile à la limite : acceptée (260 caractères, guillemets et argument compris).
    let overhead = "\"\" --minimized".len();
    let exact = format!("{}.exe", "e".repeat(260 - overhead - 4));
    assert_eq!(startup_command(&exact).unwrap().chars().count(), 260);
}

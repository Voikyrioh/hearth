//! Le VRAI registre de Windows, sur une sous-clé jetable (FIX:01M4B118DAFBQZYQX1E5ERY8CA).
//! `#[ignore]` : ces tests écrivent dans le registre de l'utilisateur qui les lance. Ils ne
//! tournent QUE sur le runner Windows jetable de la CI (`cargo test -p hearth-desktop --test
//! windows_registry -- --ignored`), jamais sur un poste de travail, et jamais sur la vraie
//! clé `Run` : chaque test crée sa propre sous-clé sous `HKCU\Software` et la retire à la fin.
#![cfg(windows)]
#![allow(clippy::unwrap_used, clippy::expect_used)] // tests : les helpers peuvent paniquer

use hearth_desktop_lib::error::AppError;
use hearth_desktop_lib::settings::Autostart;
use hearth_desktop_lib::startup::{Hive, RunRegistry, StartupEntry, WindowsRegistry};
use windows_registry::{CURRENT_USER, Type};

const EXE: &str = r"C:\Users\Jean Dupont\AppData\Local\Hearth\hearth-desktop.exe";

/// Sous-clé jetable, retirée à la fin du test (même en cas d'échec).
struct Scratch {
    base: String,
}

impl Scratch {
    fn new(test: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = format!(r"Software\HearthTest-{test}-{}-{nanos}", std::process::id());
        let _ = CURRENT_USER.remove_tree(&base);
        Self { base }
    }

    fn run_key(&self) -> String {
        format!(r"{}\Run", self.base)
    }

    fn approved_key(&self) -> String {
        format!(r"{}\StartupApproved\Run", self.base)
    }

    fn registry(&self) -> WindowsRegistry {
        WindowsRegistry::with_keys(self.run_key(), self.approved_key())
    }

    fn entry(&self) -> StartupEntry<WindowsRegistry> {
        StartupEntry::new(self.registry(), EXE)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = CURRENT_USER.remove_tree(&self.base);
    }
}

fn quoted() -> String {
    format!("\"{EXE}\" --minimized")
}

#[test]
#[ignore = "écrit dans le registre : runner Windows jetable de la CI seulement"]
fn an_absent_key_and_an_absent_value_read_as_disabled_without_error() {
    let scratch = Scratch::new("absent");
    let registry = scratch.registry();
    let entry = scratch.entry();
    // Clé absente.
    assert_eq!(registry.run_value(Hive::User).unwrap(), None);
    assert_eq!(registry.run_value(Hive::Machine).unwrap(), None);
    assert_eq!(registry.approved(Hive::User).unwrap(), None);
    assert_eq!(registry.approved(Hive::Machine).unwrap(), None);
    assert!(!entry.is_enabled().unwrap());
    // Clé présente, valeur absente.
    CURRENT_USER.create(scratch.run_key()).unwrap();
    CURRENT_USER.create(scratch.approved_key()).unwrap();
    assert_eq!(registry.run_value(Hive::User).unwrap(), None);
    assert_eq!(registry.approved(Hive::User).unwrap(), None);
    assert!(!entry.is_enabled().unwrap());
    // Retirer ce qui n'existe pas n'est pas une erreur.
    entry.set_enabled(false).unwrap();
}

#[test]
#[ignore = "écrit dans le registre : runner Windows jetable de la CI seulement"]
fn enabling_writes_the_exact_quoted_value_and_disabling_removes_it() {
    let scratch = Scratch::new("roundtrip");
    // Le Gestionnaire des tâches a sa clé : l'activation y écrit « activé ».
    CURRENT_USER.create(scratch.approved_key()).unwrap();
    let registry = scratch.registry();
    let entry = scratch.entry();
    entry.set_enabled(true).unwrap();
    assert_eq!(
        registry.run_value(Hive::User).unwrap().as_deref(),
        Some(quoted().as_str())
    );
    assert_eq!(
        registry.approved(Hive::User).unwrap(),
        Some(vec![2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])
    );
    assert!(entry.is_enabled().unwrap());
    // Une seule valeur sous la clé Run.
    let key = CURRENT_USER.open(scratch.run_key()).unwrap();
    assert_eq!(key.values().unwrap().count(), 1);
    entry.set_enabled(false).unwrap();
    assert_eq!(registry.run_value(Hive::User).unwrap(), None);
    assert!(!entry.is_enabled().unwrap());
}

#[test]
#[ignore = "écrit dans le registre : runner Windows jetable de la CI seulement"]
fn an_old_unquoted_value_is_migrated_and_a_task_manager_choice_stays() {
    let scratch = Scratch::new("migrate");
    let registry = scratch.registry();
    let approved = CURRENT_USER.create(scratch.approved_key()).unwrap();
    let disabled = [3_u8, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8];
    approved
        .set_bytes("Hearth", Type::Bytes, &disabled)
        .unwrap();
    registry
        .set_run_value(&format!("{EXE} --minimized"))
        .unwrap();
    let entry = scratch.entry();
    assert!(
        !entry.is_enabled().unwrap(),
        "désactivé dans le Gestionnaire des tâches"
    );
    assert!(entry.migrate().unwrap());
    assert_eq!(
        registry.run_value(Hive::User).unwrap().as_deref(),
        Some(quoted().as_str())
    );
    assert_eq!(
        registry.approved(Hive::User).unwrap(),
        Some(disabled.to_vec())
    );
    assert!(!entry.is_enabled().unwrap(), "toujours désactivé");
}

#[test]
#[ignore = "écrit dans le registre : runner Windows jetable de la CI seulement"]
fn a_value_of_an_unexpected_type_is_a_typed_error_and_is_never_overwritten() {
    let scratch = Scratch::new("type");
    let registry = scratch.registry();
    let key = CURRENT_USER.create(scratch.run_key()).unwrap();
    key.set_u32("Hearth", 7).unwrap();
    assert!(matches!(
        registry.run_value(Hive::User),
        Err(AppError::Autostart(_))
    ));
    let entry = scratch.entry();
    assert!(matches!(entry.is_enabled(), Err(AppError::Autostart(_))));
    // La migration lit la valeur : elle échoue sans rien écraser.
    assert!(matches!(entry.migrate(), Err(AppError::Autostart(_))));
    assert_eq!(key.get_u32("Hearth").unwrap(), 7);
}

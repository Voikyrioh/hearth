//! Le coffre : clés `Hearth/{id}`, jamais de secret en clair, effacement. Le test du vrai
//! Gestionnaire d'identification de Windows est marqué `ignore` (il écrit dans la session de
//! l'utilisateur) : `cargo test -p hearth-desktop --test vault -- --ignored`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::sync::Mutex;

use hearth_desktop_lib::vault::{CredentialBackend, CredentialVault, credential_target};
use hearth_link::domain::secret::Secret;
use hearth_link::domain::server::ServerId;
use hearth_link::ports::vault::{SecretKind, Vault as _};

#[derive(Default)]
struct Memory(Mutex<HashMap<String, Vec<u8>>>);

impl CredentialBackend for Memory {
    fn read(&self, target: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(self.0.lock().unwrap().get(target).cloned())
    }
    fn write(&self, target: &str, secret: &[u8]) -> Result<(), String> {
        self.0.lock().unwrap().insert(target.into(), secret.into());
        Ok(())
    }
    fn remove(&self, target: &str) -> Result<(), String> {
        self.0.lock().unwrap().remove(target);
        Ok(())
    }
}

#[test]
fn the_password_lives_under_hearth_slash_id_and_the_token_beside_it() {
    let id = ServerId::parse("01J9ZY0G3Q").unwrap();
    assert_eq!(
        credential_target(&id, SecretKind::Password),
        "Hearth/01J9ZY0G3Q"
    );
    assert_eq!(
        credential_target(&id, SecretKind::Token),
        "Hearth/01J9ZY0G3Q/token"
    );
}

#[test]
fn secrets_round_trip_per_server_and_kind_and_delete_cleanly() {
    let vault = CredentialVault::new(Memory::default());
    let a = ServerId::parse("a").unwrap();
    let b = ServerId::parse("b").unwrap();
    assert!(vault.get(&a, SecretKind::Password).unwrap().is_none());
    vault
        .put(&a, SecretKind::Password, &Secret::from("Mot-de-passe-é-12"))
        .unwrap();
    vault
        .put(&a, SecretKind::Token, &Secret::from("tok-a"))
        .unwrap();
    vault
        .put(&b, SecretKind::Token, &Secret::from("tok-b"))
        .unwrap();
    assert_eq!(
        vault
            .get(&a, SecretKind::Password)
            .unwrap()
            .unwrap()
            .expose(),
        "Mot-de-passe-é-12"
    );
    assert_eq!(
        vault.get(&b, SecretKind::Token).unwrap().unwrap().expose(),
        "tok-b"
    );
    vault.delete(&a, SecretKind::Password).unwrap();
    vault.delete(&a, SecretKind::Password).unwrap();
    assert!(vault.get(&a, SecretKind::Password).unwrap().is_none());
    assert!(vault.get(&a, SecretKind::Token).unwrap().is_some());
}

#[test]
fn a_secret_that_is_not_text_is_refused_without_showing_it() {
    let backend = Memory::default();
    backend.write("Hearth/a", &[0xff, 0xfe]).unwrap();
    let vault = CredentialVault::new(backend);
    let error = vault
        .get(&ServerId::parse("a").unwrap(), SecretKind::Password)
        .unwrap_err();
    assert!(!error.to_string().contains("255"));
}

#[cfg(windows)]
#[test]
#[ignore = "écrit dans le Gestionnaire d'identification de la session Windows"]
fn the_windows_credential_manager_stores_reads_and_erases_a_secret() {
    use hearth_desktop_lib::vault::WindowsCredentials;
    let vault = CredentialVault::new(WindowsCredentials::new().unwrap());
    let id = ServerId::parse(&format!("test-{}", std::process::id())).unwrap();
    assert!(vault.get(&id, SecretKind::Password).unwrap().is_none());
    vault
        .put(&id, SecretKind::Password, &Secret::from("Secret-é-1234"))
        .unwrap();
    assert_eq!(
        vault
            .get(&id, SecretKind::Password)
            .unwrap()
            .unwrap()
            .expose(),
        "Secret-é-1234"
    );
    // La clé exacte dans le Gestionnaire d'identification : `Hearth/{id}`.
    let listed = std::process::Command::new("cmdkey")
        .arg(format!("/list:Hearth/{id}"))
        .output()
        .unwrap();
    let listed = String::from_utf8_lossy(&listed.stdout).to_lowercase();
    assert!(
        listed.contains(&format!("hearth/{id}").to_lowercase()),
        "{listed}"
    );
    vault
        .put(&id, SecretKind::Password, &Secret::from("Remplacé"))
        .unwrap();
    assert_eq!(
        vault
            .get(&id, SecretKind::Password)
            .unwrap()
            .unwrap()
            .expose(),
        "Remplacé"
    );
    vault.delete(&id, SecretKind::Password).unwrap();
    vault.delete(&id, SecretKind::Password).unwrap();
    assert!(vault.get(&id, SecretKind::Password).unwrap().is_none());
}

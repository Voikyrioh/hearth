//! Mémoire durable de la mise à jour (fichier `update.json`) et garde-fous de configuration : la
//! clé embarquée est une clé minisign lisible, l'interface n'a aucune permission du greffon de mise
//! à jour, la configuration n'ouvre aucun contournement (HTTP, certificats, adresse).
#![allow(clippy::unwrap_used, clippy::expect_used)] // tests : les helpers peuvent paniquer

use hearth_desktop_lib::update::domain::{Release, UpdateRecord};
use hearth_desktop_lib::update::feed::{
    DEV_KEY_MARK, EMBEDDED_PUBLIC_KEY, embedded_key_is_development, is_development_key,
    public_key_text,
};
use hearth_desktop_lib::update::ports::{Clock, UpdateStore};
use hearth_desktop_lib::update::store::{FILE_NAME, FileUpdateStore, SystemClock};

fn record() -> UpdateRecord {
    UpdateRecord {
        last_attempt_at: Some(1_800_000_000_000),
        last_request_at: Some(1_800_000_000_000),
        automatic_attempts: vec![1_799_999_000_000, 1_800_000_000_000],
        last_success_at: Some(1_799_999_999_000),
        postponed_until: Some(1_800_086_400_000),
        available: Some(Release {
            version: "1.1.0".to_owned(),
            notes: "Notes.".to_owned(),
        }),
        // La cible de l'agent (HRT-17) survit aussi au redémarrage du client.
        agent: Some(hearth_desktop_lib::agent_update::domain::AgentTargetRecord {
            version: "0.2.0".to_owned(),
            url: "https://github.com/Voikyrioh/hearth/releases/download/v0.2.0/hearth-agent-linux-x86_64"
                .to_owned(),
            signature: "untrusted comment: s".to_owned(),
            sha256: "ab".repeat(32),
        }),
        agent_results_seen: [("s1".to_owned(), "2026-10-06T10:00:00Z".to_owned())].into(),
    }
}

#[test]
fn a_missing_file_is_the_default_state() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(
        FileUpdateStore::new(dir.path()).load(),
        UpdateRecord::default()
    );
}

#[test]
fn the_state_is_written_and_read_back() {
    let dir = tempfile::tempdir().unwrap();
    let store = FileUpdateStore::new(dir.path());
    store.save(&record()).unwrap();
    assert_eq!(FileUpdateStore::new(dir.path()).load(), record());
    // Pas de fichier temporaire oublié.
    let names: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, [FILE_NAME]);
}

#[test]
fn a_corrupt_file_never_blocks_the_client() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(FILE_NAME), b"{ pas du json").unwrap();
    assert_eq!(
        FileUpdateStore::new(dir.path()).load(),
        UpdateRecord::default()
    );
}

#[test]
fn saving_creates_the_data_directory_and_replaces_the_previous_state() {
    let dir = tempfile::tempdir().unwrap();
    let nested = dir.path().join("a").join("b");
    let store = FileUpdateStore::new(&nested);
    store.save(&UpdateRecord::default()).unwrap();
    store.save(&record()).unwrap();
    assert_eq!(store.load(), record());
}

#[test]
fn the_system_clock_gives_a_plausible_instant() {
    let now = SystemClock.now_ms();
    assert!(now > 1_700_000_000_000, "{now}");
}

#[test]
fn the_embedded_key_is_a_readable_minisign_public_key() {
    let text = public_key_text(EMBEDDED_PUBLIC_KEY).unwrap();
    let key = minisign::PublicKeyBox::from_string(&text)
        .and_then(minisign::PublicKeyBox::into_public_key);
    assert!(key.is_ok(), "{key:?}");
}

#[test]
fn the_development_key_is_recognised_by_its_comment_only() {
    // Garde de publication : le flux de publication refuse un fichier qui porte cette marque
    // (runbook `publier-une-version-du-client`).
    assert!(is_development_key(
        "untrusted comment: Hearth client DEV public key. No secret key exists.
RWQ..."
    ));
    assert!(!is_development_key(
        "untrusted comment: minisign public key: 6FAC17371631EAA5
RWQ..."
    ));
    assert!(!is_development_key(
        "untrusted comment: x
RWQ... DEV public key"
    ));
    // Cohérence : ce que le test de la clé du dépôt lit est ce que le programme lit.
    assert_eq!(
        embedded_key_is_development(),
        EMBEDDED_PUBLIC_KEY
            .lines()
            .next()
            .unwrap()
            .contains(DEV_KEY_MARK)
    );
}

#[test]
fn both_key_file_formats_are_accepted() {
    use base64::Engine as _;
    let raw = "untrusted comment: minisign public key: 6FAC17371631EAA5
RWSl6jEWNxesbzhRAEgmczf4XH0dRzssIkZ8Y9NgodPNBzcmTQpzChiE
";
    // Celle de `tauri signer generate` : le même texte en base64, sur une ligne.
    let tauri = base64::engine::general_purpose::STANDARD.encode(raw);
    assert_eq!(public_key_text(raw).unwrap(), raw.trim());
    assert_eq!(
        public_key_text(&format!(
            "{tauri}
"
        ))
        .unwrap(),
        raw.trim()
    );
    assert_eq!(public_key_text("pas une clé"), None);
    assert_eq!(
        public_key_text(&base64::engine::general_purpose::STANDARD.encode("autre chose")),
        None
    );
    // La marque de développement se lit dans les deux formes.
    let dev = "untrusted comment: Hearth client DEV public key. x
RWQabc
";
    assert!(is_development_key(dev));
    assert!(is_development_key(
        &base64::engine::general_purpose::STANDARD.encode(dev)
    ));
}

fn read(path: &str) -> String {
    std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(path)).unwrap()
}

#[test]
fn the_web_view_has_no_permission_of_the_update_plugin() {
    let capabilities = read("capabilities/default.json");
    assert!(!capabilities.contains("updater"), "{capabilities}");
    // Seules nos commandes typées, qui ne prennent ni adresse, ni chemin, ni clé.
    for command in [
        "allow-get-update-state",
        "allow-check-for-updates",
        "allow-postpone-update",
        "allow-install-update",
    ] {
        assert!(capabilities.contains(command), "{command}");
    }
}

#[test]
fn the_configuration_opens_no_bypass_and_fixes_no_address() {
    let config: serde_json::Value = serde_json::from_str(&read("tauri.conf.json")).unwrap();
    let updater = &config["plugins"]["updater"];
    assert!(
        updater["endpoints"].is_null(),
        "l'adresse du flux est dans le code"
    );
    for flag in [
        "dangerousInsecureTransportProtocol",
        "dangerousAcceptInvalidCerts",
        "dangerousAcceptInvalidHostnames",
        "allowDowngrades",
    ] {
        assert!(
            updater[flag].is_null() || updater[flag] == false,
            "{flag} ne doit pas être activé"
        );
    }
    // La version annoncée doit être celle du commentaire signé (pas de rejeu d'un ancien installateur).
    assert_eq!(updater["requireSignedVersion"], true);
    // La clé vient du fichier embarqué (`update-key.pub`), pas de la configuration.
    assert_eq!(updater["pubkey"], "");
    // Le web ne peut joindre que la coquille : aucune origine distante.
    let csp = config["app"]["security"]["csp"].as_str().unwrap();
    assert!(
        csp.contains("connect-src ipc: http://ipc.localhost;"),
        "{csp}"
    );
}

#[test]
fn no_createupdaterartifacts_so_that_local_builds_need_no_secret_key() {
    let config: serde_json::Value = serde_json::from_str(&read("tauri.conf.json")).unwrap();
    assert!(config["bundle"]["createUpdaterArtifacts"].is_null());
}

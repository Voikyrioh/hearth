//! Sauvegarder les données d'un service, de bout en bout (HRT-97, ADR-0047, ADR-0045, BR-BACKUP-*) : vrai agent en
//! HTTPS, vraie base, faux moteur, faux dossiers de données, fausse racine des sauvegardes. Les routes répondent
//! `202`, la copie court à l'arrière-plan, l'issue se lit dans l'opération et dans le journal d'activité.
//!
//! Règles : BR-BACKUP-001 à BR-BACKUP-015, BR-BACKUP-025, BR-BACKUP-028, BR-BACKUP-029, BR-BACKUP-032, BR-BACKUP-033.
//! La preuve de clé et le mot de passe sont dans la matrice de `admin_reauth.rs` ; ici : ce que font les routes et
//! le cas d'usage, jusqu'au journal d'activité et à la base. **La règle absolue** (BR-BACKUP-004) : une racine
//! semée de fichiers étrangers, dont un nommé comme une archive de Hearth, reste identique octet pour octet et
//! inode pour inode après chaque geste. Le parcours réel du disque est dans `infrastructure/backup_files.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use hearth_agent::application::ports::{DiskSpace, EngineInfo, HostPaths, Resolved};
use hearth_agent::domain::accounts::Role;
use hearth_agent::domain::services::{RunState, ServiceKey};
use hearth_proto::api::backups::{
    BackupAccepted, BackupDeleted, BackupFlag, BackupIntegrity, BackupMode, BackupPreview,
    BackupsView, VerifyResult,
};
use hearth_proto::api::deploy::DeployAccepted;
use hearth_proto::api::service_ops::{OperationCause, OperationStatus, OperationView};
use serde_json::{Value, json};
use support::device::Actor;
use support::engine::{fast_timings, parts_supervised};
use support::fake_backup_files::FakeBackupFiles;
use support::fake_engine::{Call, FakeEngine, PullScript};
use support::fake_host::{FakeFreeSpace, FakeHostPorts};
use support::fake_safe_fs::FakeSafeFs;
use support::fake_supervisor::FakeSupervisor;
use support::https::{self, Agent, Wire};
use support::{Env, env};

const GIB: u64 = 1024 * 1024 * 1024;
/// Le dossier géré monté dans le service : son nom ne doit apparaître NULLE PART hors du disque.
const FOLDER: &str = "sentinelle-dossier-91bc";
/// Une note de compte : elle ne doit apparaître ni au manifeste, ni au journal, ni en clair en base.
const NOTE: &str = "Avant mise a jour sentinelle-note-4d2e";

struct Bench {
    env: Env,
    engine: Arc<FakeEngine>,
    safe_fs: Arc<FakeSafeFs>,
    files: Arc<FakeBackupFiles>,
    agent: Agent,
    /// Administrateur avec le droit de gérer les services.
    ada: Actor,
    /// Administrateur SANS le droit.
    bob: Actor,
    /// Lecture seule SANS le droit.
    lucas: Actor,
    /// Lecture seule AVEC le droit.
    dan: Actor,
}

async fn bench() -> Bench {
    bench_with(|_| {}).await
}

async fn bench_with(change: impl FnOnce(&mut hearth_agent::app::EngineParts)) -> Bench {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    engine.set_info(Some(EngineInfo {
        cpus: 8,
        memory_bytes: 16 * GIB,
        storage_dir: Some("/var/lib/docker".into()),
    }));
    let safe_fs = Arc::new(FakeSafeFs::new());
    let files = Arc::new(FakeBackupFiles::new());
    let mut parts = parts_supervised(
        engine.clone(),
        Arc::new(FakeSupervisor::new()),
        fast_timings(),
    );
    parts.host_ports = Arc::new(FakeHostPorts::new());
    parts.free_space = Arc::new(FakeFreeSpace::new());
    parts.safe_fs = Some(safe_fs.clone());
    parts.backups.files = Some(files.clone());
    change(&mut parts);
    let agent = https::start_engined(&env, parts).await;
    let ada = agent.actor(&env, "ada", Role::Admin).await;
    let bob = agent.actor(&env, "bob", Role::Admin).await;
    let lucas = agent.actor(&env, "lucas", Role::ReadOnly).await;
    let dan = agent.actor(&env, "dan", Role::ReadOnly).await;
    env.grant_services("ada").await;
    env.grant_services("dan").await;
    engine.publish_image("nginx:latest", PullScript::two_layers());
    Bench {
        env,
        engine,
        safe_fs,
        files,
        agent,
        ada,
        bob,
        lucas,
        dan,
    }
}

fn spec(name: &str) -> Value {
    json!({
        "name": name,
        "image": "nginx",
        "mounts": [{ "target": format!("/{FOLDER}") }],
        "resources": { "cpu_millis": 1000, "memory_bytes": 268_435_456_u64 }
    })
}

struct Deployed {
    key: ServiceKey,
    container: String,
}

impl Bench {
    async fn operation(&self, id: &str) -> OperationView {
        let reply = self
            .agent
            .request("GET", &format!("/operations/services/{id}"))
            .token(&self.dan.token)
            .send()
            .await;
        assert_eq!(reply.status, 200, "{}", reply.text);
        serde_json::from_value(reply.body).expect("OperationView")
    }

    async fn until(&self, id: &str, done: impl Fn(&OperationView) -> bool) -> OperationView {
        let started = Instant::now();
        loop {
            let operation = self.operation(id).await;
            if done(&operation) {
                return operation;
            }
            assert!(
                started.elapsed() < Duration::from_secs(15),
                "l'état attendu n'arrive pas : {operation:?}"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    async fn finished(&self, id: &str) -> OperationView {
        self.until(id, |operation| {
            !matches!(
                operation.status,
                OperationStatus::Accepted | OperationStatus::Running | OperationStatus::Cancelling
            )
        })
        .await
    }

    /// Déploie `name` (un dossier géré) et sème des données dedans.
    async fn deploy(&self, name: &str) -> Deployed {
        let reply = self
            .agent
            .confirmed(&self.ada, "POST", "/deployments", spec(name))
            .await
            .send()
            .await;
        assert_eq!(reply.status, 202, "{}", reply.text);
        let accepted: DeployAccepted = serde_json::from_value(reply.body).expect("accepté");
        let operation = self.finished(&accepted.operation_id).await;
        assert_eq!(
            operation.status,
            OperationStatus::Succeeded,
            "{operation:?}"
        );
        let key = ServiceKey::parse(&accepted.service_key).expect("clé");
        self.safe_fs
            .seed_file(&key, &format!("{FOLDER}/world.wld"), b"le monde");
        self.safe_fs
            .seed_file(&key, &format!("{FOLDER}/saves/a.bak"), b"sauvegarde");
        let container = self
            .engine
            .container_ids()
            .into_iter()
            .last()
            .expect("conteneur");
        Deployed { key, container }
    }

    async fn resync(&self) {
        self.agent.services().resync().await.expect("liste");
    }

    /// Arrête le conteneur dans le faux moteur et relit la liste.
    async fn stop_container(&self, service: &Deployed) {
        self.engine
            .update(&service.container, |o| o.run_state = RunState::Exited);
        self.resync().await;
    }

    async fn create_as(&self, who: &Actor, key: &str, body: Value) -> Wire {
        self.agent
            .confirmed(who, "POST", &format!("/services/{key}/backups"), body)
            .await
            .send()
            .await
    }

    async fn create(&self, key: &str, body: Value) -> Wire {
        self.create_as(&self.ada, key, body).await
    }

    async fn accepted(&self, key: &str, body: Value) -> BackupAccepted {
        let reply = self.create(key, body).await;
        assert_eq!(reply.status, 202, "{}", reply.text);
        serde_json::from_value(reply.body).expect("BackupAccepted")
    }

    /// Crée une sauvegarde et attend sa fin réussie.
    async fn made(&self, key: &str, body: Value) -> (BackupAccepted, OperationView) {
        let accepted = self.accepted(key, body).await;
        let operation = self.finished(&accepted.operation_id).await;
        assert_eq!(
            operation.status,
            OperationStatus::Succeeded,
            "{operation:?}"
        );
        (accepted, operation)
    }

    async fn list_as(&self, who: &Actor, key: &str) -> Wire {
        self.agent
            .request("GET", &format!("/services/{key}/backups"))
            .token(&who.token)
            .send()
            .await
    }

    async fn list(&self, key: &str) -> BackupsView {
        let reply = self.list_as(&self.lucas, key).await;
        assert_eq!(reply.status, 200, "{}", reply.text);
        serde_json::from_value(reply.body).expect("BackupsView")
    }

    async fn preview(&self, key: &str) -> Wire {
        self.agent
            .request("POST", &format!("/services/{key}/backups/preview"))
            .token(&self.ada.token)
            .json(&json!({}))
            .send()
            .await
    }

    async fn verify(&self, id: &str) -> Wire {
        self.agent
            .request("POST", &format!("/backups/{id}/verify"))
            .token(&self.ada.token)
            .json(&json!({}))
            .send()
            .await
    }

    async fn delete(&self, id: &str) -> Wire {
        self.agent
            .confirmed(&self.ada, "DELETE", &format!("/backups/{id}"), json!({}))
            .await
            .send()
            .await
    }

    async fn settings(&self, key: &str, body: Value) -> Wire {
        self.agent
            .confirmed(
                &self.ada,
                "PUT",
                &format!("/services/{key}/backup-settings"),
                body,
            )
            .await
            .send()
            .await
    }

    /// Les entrées du journal d'activité d'une action : résultat, cible, raison, compte.
    async fn journal(
        &self,
        action: &str,
    ) -> Vec<(String, Option<String>, Option<String>, Option<String>)> {
        self.agent.flush_journal().await;
        sqlx::query_as(
            "SELECT outcome, target, reason, account FROM audit_events WHERE action = ? ORDER BY id",
        )
        .bind(action)
        .fetch_all(self.env.db.pool())
        .await
        .unwrap()
    }

    /// Le nom du sous-dossier de la racine des sauvegardes d'un service.
    fn dir(key: &ServiceKey) -> String {
        key.to_string()
    }
}

fn code(wire: &Wire) -> String {
    wire.body["error"]["code"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

fn steps(operation: &OperationView) -> Vec<&str> {
    operation.steps.iter().map(|s| s.step.as_str()).collect()
}

fn root() -> String {
    format!("/{FOLDER}")
}

// ---------------------------------------------------------------------------------------------
// Créer : à chaud par défaut, rien n'est arrêté (BR-BACKUP-006, BR-BACKUP-007)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_hot_backup_of_a_running_service_stops_nothing_and_ends_verified() {
    let b = bench().await;
    let service = b.deploy("web").await;
    let calls_before = b.engine.calls().len();
    let (accepted, operation) = b.made(service.key.as_str(), json!({ "mode": "hot" })).await;
    assert_eq!(steps(&operation), ["copy", "verify"], "{operation:?}");
    assert_eq!(operation.cause, None);
    // Rien n'a été demandé au moteur : ni arrêt, ni démarrage (BR-BACKUP-006).
    assert_eq!(b.engine.calls().len(), calls_before);
    let view = b.list(service.key.as_str()).await;
    assert_eq!(view.backups.len(), 1, "{view:?}");
    let backup = &view.backups[0];
    assert_eq!(backup.id, accepted.backup_id);
    assert_eq!(backup.mode, BackupMode::Hot);
    assert_eq!(backup.flags, [BackupFlag::Hot]);
    assert_eq!(backup.integrity, BackupIntegrity::Verified);
    assert!(backup.verified_at.is_some());
    assert_eq!(backup.account, "ada");
    // Un fichier : `world.wld` ; `saves/a.bak` est ignoré par défaut (nom en « .bak »).
    assert_eq!(backup.files, 1);
    assert_eq!(backup.data_bytes, "le monde".len() as u64);
    assert!(backup.archive_bytes > 0);
    assert_eq!(view.total_bytes, backup.archive_bytes);
    assert!(!view.nothing_to_save);
    // L'archive et son manifeste voisin sont là, sous le sous-dossier du service ; aucun fichier partiel.
    let mut names = b.files.names(&Bench::dir(&service.key));
    names.sort();
    assert_eq!(
        names,
        [
            format!("{}.manifest.json", accepted.backup_id),
            format!("{}.tar.gz", accepted.backup_id)
        ]
    );
    // Le fichier ignoré n'a jamais été ouvert, et le dossier du service listé dossier par dossier.
    assert!(
        b.safe_fs
            .opened_paths()
            .iter()
            .all(|path| !path.ends_with("a.bak")),
        "{:?}",
        b.safe_fs.opened_paths()
    );
    // Le journal d'activité : des nombres, jamais un chemin ni le nom du dossier (BR-BACKUP-032).
    let journal = b.journal("service.backup.create").await;
    assert_eq!(journal.len(), 1, "{journal:?}");
    assert_eq!(journal[0].0, "ok");
    assert_eq!(journal[0].3.as_deref(), Some("ada"));
    let target = journal[0].1.clone().unwrap();
    assert!(
        target.starts_with("service web, sauvegarde créée (à chaud, 1 fichier, "),
        "{target}"
    );
    assert!(target.contains("aucun changement détecté"), "{target}");
    assert!(
        !target.contains(FOLDER) && !target.contains('/'),
        "{target}"
    );
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_service_that_is_not_running_is_copied_directly_whatever_mode_was_asked() {
    let b = bench().await;
    let service = b.deploy("web").await;
    b.stop_container(&service).await;
    let calls_before = b.engine.calls().len();
    let (_, operation) = b
        .made(service.key.as_str(), json!({ "mode": "stop_copy_start" }))
        .await;
    // Pas d'arrêt ni de relance : le service n'était pas en marche (BR-BACKUP-006).
    assert_eq!(steps(&operation), ["copy", "verify"], "{operation:?}");
    assert_eq!(b.engine.calls().len(), calls_before);
    let view = b.list(service.key.as_str()).await;
    assert_eq!(view.backups[0].mode, BackupMode::Stopped);
    assert!(view.backups[0].flags.is_empty());
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_running_service_needs_an_explicit_mode_and_is_never_stopped_without_it() {
    let b = bench().await;
    let service = b.deploy("web").await;
    let reply = b
        .create(service.key.as_str(), json!({ "mode": "stopped" }))
        .await;
    assert_eq!(reply.status, 422, "{}", reply.text);
    assert_eq!(code(&reply), "VALIDATION_ERROR");
    assert_eq!(reply.body["error"]["details"]["field"], "mode");
    assert!(
        !b.engine
            .calls()
            .iter()
            .any(|call| matches!(call, Call::Stop(..))),
        "{:?}",
        b.engine.calls()
    );
    assert!(b.files.all().is_empty());
    b.agent.shutdown().await;
}

#[tokio::test]
async fn stopping_to_copy_stops_with_the_service_delay_copies_then_restarts_what_ran() {
    let b = bench().await;
    let service = b.deploy("web").await;
    let calls_before = b.engine.calls().len();
    let (_, operation) = b
        .made(service.key.as_str(), json!({ "mode": "stop_copy_start" }))
        .await;
    assert_eq!(
        steps(&operation),
        ["stop", "copy", "verify", "start"],
        "{operation:?}"
    );
    let calls: Vec<Call> = b.engine.calls().into_iter().skip(calls_before).collect();
    assert!(
        matches!(calls.first(), Some(Call::Stop(id, _)) if *id == service.container),
        "{calls:?}"
    );
    assert!(
        matches!(calls.last(), Some(Call::Start(id)) if *id == service.container),
        "{calls:?}"
    );
    // Le service est reparti : l'arrêt voulu est oublié (BR-LIFE-010).
    let intents: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM service_intents")
        .fetch_one(b.env.db.pool())
        .await
        .unwrap();
    assert_eq!(intents, 0);
    let view = b.list(service.key.as_str()).await;
    assert_eq!(view.backups[0].mode, BackupMode::StopCopyStart);
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// La règle absolue : les fichiers qui ne sont pas à Hearth ne sont jamais touchés (BR-BACKUP-004)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn files_that_are_not_in_the_registry_are_never_listed_read_renamed_or_removed() {
    let b = bench().await;
    let service = b.deploy("web").await;
    let dir = Bench::dir(&service.key);
    // Des sentinelles : un fichier d'un autre outil, des archives de la forme de celles de forge, et un fichier
    // NOMMÉ COMME UNE ARCHIVE DE HEARTH (un identifiant valable) qui n'est dans aucune ligne du registre.
    let ghost = "01J9ZY0G3Q8M2K6W4T7V5N1B9D";
    for (name, bytes) in [
        ("notes.txt", b"mes notes".as_slice()),
        ("Forge.wld.2026-09-02-120000", b"un monde".as_slice()),
        ("backup-2026.tar.gz", b"autre outil".as_slice()),
        (&format!("{ghost}.tar.gz"), b"imitation".as_slice()),
        (&format!("{ghost}.manifest.json"), b"{}".as_slice()),
    ] {
        b.files.plant(&dir, name, bytes);
    }
    b.files
        .plant("terraria-backups", "Forge.wld.bak", b"autre dossier");
    let before = b.files.all();
    // Tous les gestes de Hearth : créer, lister, estimer, vérifier, supprimer, régler.
    let (accepted, _) = b.made(service.key.as_str(), json!({ "mode": "hot" })).await;
    let view = b.list(service.key.as_str()).await;
    assert_eq!(
        view.backups.len(),
        1,
        "seule SA sauvegarde est listée : {view:?}"
    );
    assert_eq!(view.backups[0].id, accepted.backup_id);
    assert_eq!(b.preview(service.key.as_str()).await.status, 200);
    assert_eq!(b.verify(&accepted.backup_id).await.status, 200);
    assert_eq!(
        b.settings(service.key.as_str(), json!({ "ignored": [] }))
            .await
            .status,
        200
    );
    // Hearth ne connaît pas l'imitation : vérifier ou supprimer son identifiant est « sauvegarde inconnue ».
    for reply in [b.verify(ghost).await, b.delete(ghost).await] {
        assert_eq!(reply.status, 404, "{}", reply.text);
        assert_eq!(code(&reply), "BACKUP_NOT_FOUND");
    }
    let deleted = b.delete(&accepted.backup_id).await;
    assert_eq!(deleted.status, 200, "{}", deleted.text);
    // Après tout cela, chaque sentinelle est identique : contenu ET inode.
    let after = b.files.all();
    for (d, name, ino, bytes) in &before {
        let found = after
            .iter()
            .find(|(d2, n2, _, _)| d2 == d && n2 == name)
            .unwrap_or_else(|| panic!("{d}/{name} a disparu"));
        assert_eq!(
            (found.2, &found.3),
            (*ino, bytes),
            "{d}/{name} a été touché"
        );
    }
    assert_eq!(after.len(), before.len(), "rien n'a été laissé : {after:?}");
    // Aucune écriture n'a visé une sentinelle.
    assert!(
        b.files
            .writes()
            .iter()
            .all(|write| !write.contains("notes.txt") && !write.contains("Forge")),
        "{:?}",
        b.files.writes()
    );
    b.agent.shutdown().await;
}

#[tokio::test]
async fn an_archive_replaced_by_hand_is_corrupt_and_deleting_it_only_drops_the_registry_line() {
    let b = bench().await;
    let service = b.deploy("web").await;
    let (accepted, _) = b.made(service.key.as_str(), json!({ "mode": "hot" })).await;
    let dir = Bench::dir(&service.key);
    let archive = format!("{}.tar.gz", accepted.backup_id);
    let manifest = format!("{}.manifest.json", accepted.backup_id);
    let manifest_before = b.files.content(&dir, &manifest).unwrap();
    // Quelqu'un remplace l'archive par un fichier à lui (autre inode).
    b.files
        .replace_by_hand(&dir, &archive, b"remplace a la main");
    let view = b.list(service.key.as_str()).await;
    assert_eq!(view.backups[0].integrity, BackupIntegrity::Corrupt);
    let verified: VerifyResult =
        serde_json::from_value(b.verify(&accepted.backup_id).await.body).unwrap();
    assert_eq!(verified.integrity, BackupIntegrity::Corrupt);
    // Supprimer : seule la ligne du registre part, le fichier d'autrui reste (BR-BACKUP-013).
    let reply = b.delete(&accepted.backup_id).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let deleted: BackupDeleted = serde_json::from_value(reply.body).unwrap();
    assert!(!deleted.removed_files);
    assert_eq!(
        b.files.content(&dir, &archive),
        Some(b"remplace a la main".to_vec())
    );
    assert_eq!(b.files.content(&dir, &manifest), Some(manifest_before));
    assert!(b.list(service.key.as_str()).await.backups.is_empty());
    let journal = b.journal("service.backup.delete").await;
    assert_eq!(journal.len(), 1, "{journal:?}");
    assert!(
        journal[0]
            .1
            .as_deref()
            .unwrap()
            .contains("ligne du registre seulement"),
        "{journal:?}"
    );
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_backup_of_hearth_is_deleted_with_its_manifest_and_nothing_else() {
    let b = bench().await;
    let service = b.deploy("web").await;
    let dir = Bench::dir(&service.key);
    b.files.plant(&dir, "voisin.txt", b"voisin");
    let (accepted, _) = b.made(service.key.as_str(), json!({ "mode": "hot" })).await;
    let reply = b.delete(&accepted.backup_id).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let deleted: BackupDeleted = serde_json::from_value(reply.body).unwrap();
    assert!(deleted.removed_files);
    assert_eq!(b.files.names(&dir), ["voisin.txt"]);
    assert!(b.list(service.key.as_str()).await.backups.is_empty());
    // Une deuxième suppression : plus de ligne, « sauvegarde inconnue ».
    let again = b.delete(&accepted.backup_id).await;
    assert_eq!(again.status, 404);
    assert_eq!(code(&again), "BACKUP_NOT_FOUND");
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_vanished_archive_is_listed_as_missing_and_its_line_can_be_removed() {
    let b = bench().await;
    let service = b.deploy("web").await;
    let (accepted, _) = b.made(service.key.as_str(), json!({ "mode": "hot" })).await;
    let dir = Bench::dir(&service.key);
    b.files
        .remove_by_hand(&dir, &format!("{}.tar.gz", accepted.backup_id));
    let view = b.list(service.key.as_str()).await;
    assert_eq!(view.backups[0].integrity, BackupIntegrity::Missing);
    let verified: VerifyResult =
        serde_json::from_value(b.verify(&accepted.backup_id).await.body).unwrap();
    assert_eq!(verified.integrity, BackupIntegrity::Missing);
    let reply = b.delete(&accepted.backup_id).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert!(b.list(service.key.as_str()).await.backups.is_empty());
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// L'intégrité (BR-BACKUP-013) et le format (BR-BACKUP-012, BR-BACKUP-014)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_changed_byte_in_hearths_own_archive_is_found_by_the_check_on_demand() {
    let b = bench().await;
    let service = b.deploy("web").await;
    let (accepted, _) = b.made(service.key.as_str(), json!({ "mode": "hot" })).await;
    let dir = Bench::dir(&service.key);
    let archive = format!("{}.tar.gz", accepted.backup_id);
    let ok: VerifyResult =
        serde_json::from_value(b.verify(&accepted.backup_id).await.body).unwrap();
    assert_eq!(ok.integrity, BackupIntegrity::Verified);
    assert_eq!(ok.files_checked, 1);
    b.files.flip_a_byte(&dir, &archive);
    let bad: VerifyResult =
        serde_json::from_value(b.verify(&accepted.backup_id).await.body).unwrap();
    assert_eq!(bad.integrity, BackupIntegrity::Corrupt);
    assert_eq!(
        b.list(service.key.as_str()).await.backups[0].integrity,
        BackupIntegrity::Corrupt
    );
    // Une sauvegarde corrompue se supprime : c'est le fichier de Hearth.
    let deleted: BackupDeleted =
        serde_json::from_value(b.delete(&accepted.backup_id).await.body).unwrap();
    assert!(deleted.removed_files);
    assert!(b.files.names(&dir).is_empty());
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_backup_whose_check_fails_at_creation_is_removed_by_hearth_and_the_failure_is_said() {
    let b = bench().await;
    let service = b.deploy("web").await;
    b.files.corrupt_on_publish();
    let accepted = b
        .accepted(service.key.as_str(), json!({ "mode": "hot" }))
        .await;
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(operation.status, OperationStatus::Failed, "{operation:?}");
    assert_eq!(operation.cause, Some(OperationCause::BackupCorrupt));
    // L'archive et son manifeste sont partis (le fichier est à Hearth), la ligne aussi.
    assert!(b.files.names(&Bench::dir(&service.key)).is_empty());
    assert!(b.list(service.key.as_str()).await.backups.is_empty());
    let journal = b.journal("service.backup.create").await;
    assert_eq!(journal[0].0, "failed");
    assert_eq!(journal[0].2.as_deref(), Some("sauvegarde corrompue"));
    b.agent.shutdown().await;
}

#[tokio::test]
async fn the_archive_is_a_standard_tar_gz_with_a_manifest_beside_it_and_inside_it() {
    let b = bench().await;
    let service = b.deploy("web").await;
    b.safe_fs
        .seed_symlink(&service.key, &format!("{FOLDER}/lien"), "world.wld");
    b.safe_fs
        .seed_special(&service.key, &format!("{FOLDER}/tube"));
    b.safe_fs.seed_dir(&service.key, &format!("{FOLDER}/vide"));
    let (accepted, _) = b
        .made(service.key.as_str(), json!({ "mode": "hot", "note": NOTE }))
        .await;
    let dir = Bench::dir(&service.key);
    let side = b
        .files
        .content(&dir, &format!("{}.manifest.json", accepted.backup_id))
        .unwrap();
    let manifest: Value = serde_json::from_slice(&side).unwrap();
    assert_eq!(manifest["format"], 1);
    assert_eq!(manifest["backup_id"], accepted.backup_id);
    assert_eq!(manifest["service_name"], "web");
    assert_eq!(manifest["mode"], "hot");
    assert_eq!(
        manifest["archive"]["name"],
        format!("{}.tar.gz", accepted.backup_id)
    );
    let paths: Vec<(String, String)> = manifest["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["path"].as_str().unwrap().to_owned(),
                e["type"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    // Le dossier, un fichier, un lien (jamais suivi), un dossier vide, et le dossier `saves` sans son `.bak` ; le
    // tube n'est ni lu ni archivé.
    assert!(
        paths.contains(&("world.wld".into(), "file".into())),
        "{paths:?}"
    );
    assert!(
        paths.contains(&("lien".into(), "symlink".into())),
        "{paths:?}"
    );
    assert!(paths.contains(&("vide".into(), "dir".into())), "{paths:?}");
    assert!(
        !paths
            .iter()
            .any(|(p, _)| p == "tube" || p.ends_with(".bak")),
        "{paths:?}"
    );
    // Le lien n'a jamais été ouvert comme un fichier.
    assert!(
        b.safe_fs
            .opened_paths()
            .iter()
            .all(|p| !p.ends_with("lien") && !p.ends_with("tube")),
        "{:?}",
        b.safe_fs.opened_paths()
    );
    // La note n'est NULLE PART dans le manifeste (il voyage avec l'archive téléchargée).
    assert!(!String::from_utf8_lossy(&side).contains("sentinelle-note"));
    // L'archive se lit avec un outil courant : tar + gzip, le manifeste en dernière entrée.
    let archive = b
        .files
        .content(&dir, &format!("{}.tar.gz", accepted.backup_id))
        .unwrap();
    let gz = flate2::read::GzDecoder::new(std::io::Cursor::new(archive));
    let mut tar = tar::Archive::new(gz);
    let names: Vec<String> = tar
        .entries()
        .unwrap()
        .map(|e| e.unwrap().path().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names.last().map(String::as_str),
        Some(".hearth-manifest.json")
    );
    assert!(names.iter().any(|n| n.ends_with("world.wld")), "{names:?}");
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// La note (BR-BACKUP-012) : chiffrée au registre, jamais au manifeste ni au journal
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn the_note_is_sealed_in_the_registry_and_never_in_the_journal_the_manifest_or_the_clear() {
    let b = bench().await;
    let service = b.deploy("web").await;
    let (accepted, _) = b
        .made(service.key.as_str(), json!({ "mode": "hot", "note": NOTE }))
        .await;
    // La liste la rend à qui lit.
    assert_eq!(
        b.list(service.key.as_str()).await.backups[0]
            .note
            .as_deref(),
        Some(NOTE)
    );
    // En base : un scellé, jamais le texte.
    let (nonce, sealed): (Vec<u8>, Vec<u8>) =
        sqlx::query_as("SELECT note_nonce, note_sealed FROM backups WHERE id = ?")
            .bind(&accepted.backup_id)
            .fetch_one(b.env.db.pool())
            .await
            .unwrap();
    assert_eq!(nonce.len(), 12);
    assert!(
        !sealed
            .windows("sentinelle-note".len())
            .any(|w| w == b"sentinelle-note")
    );
    // Au journal d'activité : rien.
    for action in ["service.backup.create", "service.backup.delete"] {
        for (_, target, reason, _) in b.journal(action).await {
            assert!(!target.unwrap_or_default().contains("sentinelle-note"));
            assert!(!reason.unwrap_or_default().contains("sentinelle-note"));
        }
    }
    // Une note trop longue, ou avec un caractère de contrôle, est refusée avant tout.
    for bad in [format!("{}x", "a".repeat(80)), "a\nb".to_owned()] {
        let reply = b
            .create(service.key.as_str(), json!({ "mode": "hot", "note": bad }))
            .await;
        assert_eq!(reply.status, 422, "{}", reply.text);
        assert_eq!(reply.body["error"]["details"]["field"], "note");
    }
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Les éléments ignorés (BR-BACKUP-003) et les gros fichiers (BR-BACKUP-033)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn the_preview_names_what_is_ignored_and_the_name_rule_is_only_a_proposal() {
    let b = bench().await;
    let service = b.deploy("web").await;
    // Un nom qui se rate : `saves_old` n'est PAS ignoré par la règle de noms (la protection ne repose pas sur elle).
    b.safe_fs
        .seed_file(&service.key, &format!("{FOLDER}/saves_old/x.dat"), b"x");
    let reply = b.preview(service.key.as_str()).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let preview: BackupPreview = serde_json::from_value(reply.body).unwrap();
    assert!(preview.running);
    assert_eq!(preview.roots, [root()]);
    assert_eq!(preview.files, 2, "{preview:?}");
    assert!(preview.enough_space);
    assert_eq!(preview.ignored.len(), 1, "{preview:?}");
    assert_eq!(preview.ignored[0].item.path, "saves/a.bak");
    assert!(!preview.nothing_to_save);
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_very_large_file_is_named_in_the_preview_so_the_account_can_skip_it_for_this_backup() {
    let b = bench().await;
    let service = b.deploy("web").await;
    b.safe_fs.seed_file(
        &service.key,
        &format!("{FOLDER}/logs/enorme.log"),
        b"plus de sept octets",
    );
    let preview: BackupPreview =
        serde_json::from_value(b.preview(service.key.as_str()).await.body).unwrap();
    let names: Vec<(&str, u64)> = preview
        .big_files
        .iter()
        .map(|file| (file.item.path.as_str(), file.bytes))
        .collect();
    // Les plus gros d'abord ; le chemin est celui du service, jamais celui de la machine.
    assert_eq!(names, [("logs/enorme.log", 19), ("world.wld", 8)]);
    assert!(
        preview
            .big_files
            .iter()
            .all(|file| file.item.root == root())
    );
    // Elle ne bloque rien : la sauvegarde l'inclut, sauf si le compte la saute pour cette fois.
    let (_, _) = b
        .made(
            service.key.as_str(),
            json!({ "mode": "hot", "ignore_once": [{ "root": root(), "path": "logs/enorme.log" }] }),
        )
        .await;
    assert_eq!(b.list(service.key.as_str()).await.backups[0].files, 1);
    b.agent.shutdown().await;
}

#[tokio::test]
async fn ignored_items_are_neither_read_nor_archived_and_the_change_is_journaled_by_count() {
    let b = bench().await;
    let service = b.deploy("web").await;
    b.safe_fs.seed_file(
        &service.key,
        &format!("{FOLDER}/logs/big.log"),
        b"gros journal",
    );
    // Le compte ajoute `logs` aux éléments ignorés.
    let reply = b
        .settings(
            service.key.as_str(),
            json!({ "ignored": [{ "root": root(), "path": "logs" }] }),
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let view = b.list(service.key.as_str()).await;
    assert_eq!(view.settings.ignored.len(), 1);
    let journal = b.journal("service.backup.settings").await;
    assert_eq!(journal.len(), 1, "{journal:?}");
    let target = journal[0].1.clone().unwrap();
    assert!(target.contains("1 ignoré"), "{target}");
    assert!(
        !target.contains("logs") && !target.contains('/'),
        "{target}"
    );
    let (_, _) = b.made(service.key.as_str(), json!({ "mode": "hot" })).await;
    let opened = b.safe_fs.opened_paths();
    assert!(
        opened
            .iter()
            .all(|p| !p.contains("logs") && !p.ends_with(".bak")),
        "{opened:?}"
    );
    assert!(
        opened.iter().any(|p| p.ends_with("world.wld")),
        "{opened:?}"
    );
    // Pour cette sauvegarde seulement : un fichier de plus ignoré, la liste du service ne change pas.
    let (_, _) = b
        .made(
            service.key.as_str(),
            json!({ "mode": "hot", "ignore_once": [{ "root": root(), "path": "world.wld" }] }),
        )
        .await;
    assert_eq!(b.list(service.key.as_str()).await.settings.ignored.len(), 1);
    let view = b.list(service.key.as_str()).await;
    let files: Vec<u64> = view.backups.iter().map(|backup| backup.files).collect();
    assert_eq!(files, [0, 1], "{view:?}");
    b.agent.shutdown().await;
}

#[tokio::test]
async fn taking_a_default_ignored_item_back_brings_it_into_the_backups() {
    let b = bench().await;
    let service = b.deploy("web").await;
    let reply = b
        .settings(
            service.key.as_str(),
            json!({ "included": [{ "root": root(), "path": "saves/a.bak" }] }),
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let (_, _) = b.made(service.key.as_str(), json!({ "mode": "hot" })).await;
    assert_eq!(b.list(service.key.as_str()).await.backups[0].files, 2);
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_hot_copy_says_which_files_changed_while_it_was_read() {
    let b = bench().await;
    let service = b.deploy("web").await;
    b.safe_fs.change_while_read(&format!("{FOLDER}/world.wld"));
    let (_, _) = b.made(service.key.as_str(), json!({ "mode": "hot" })).await;
    let view = b.list(service.key.as_str()).await;
    let backup = &view.backups[0];
    assert_eq!(backup.changed_files, 1);
    assert_eq!(
        backup.flags,
        [BackupFlag::Hot, BackupFlag::MaybeInconsistent]
    );
    let journal = b.journal("service.backup.create").await;
    let target = journal[0].1.clone().unwrap();
    assert!(
        target.contains("1 fichier changé pendant la copie : peut être incohérente"),
        "{target}"
    );
    assert_eq!(
        journal[0].0, "ok",
        "une sauvegarde incohérente reste utilisable"
    );
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Une opération lourde à la fois (BR-BACKUP-010, BR-BACKUP-029), l'annulation (BR-BACKUP-011)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn one_heavy_operation_at_a_time_and_the_service_is_busy_for_its_own_actions() {
    let b = bench().await;
    let web = b.deploy("web").await;
    let api = b.deploy("api").await;
    let hold = b.safe_fs.hold_reads();
    let accepted = b.accepted(web.key.as_str(), json!({ "mode": "hot" })).await;
    b.until(&accepted.operation_id, |op| {
        op.step.as_deref() == Some("copy")
    })
    .await;
    // Une seconde sauvegarde sur un AUTRE service : la place des opérations lourdes est prise (BR-BACKUP-010).
    let other = b.create(api.key.as_str(), json!({ "mode": "hot" })).await;
    assert_eq!(other.status, 409, "{}", other.text);
    assert_eq!(code(&other), "HEAVY_IO_BUSY");
    // Une seconde sauvegarde du MÊME service : une action est déjà en cours, avec le compte (BR-BACKUP-029).
    let same = b.create(web.key.as_str(), json!({ "mode": "hot" })).await;
    assert_eq!(same.status, 409, "{}", same.text);
    assert_eq!(code(&same), "SERVICE_BUSY");
    assert_eq!(same.body["error"]["details"]["by"], "ada");
    assert_eq!(same.body["error"]["details"]["kind"], "backup");
    // L'avancement est dit dans l'opération (plan à deux étapes, annulable : rien n'est détruit).
    let during = b.operation(&accepted.operation_id).await;
    assert!(during.cancellable, "{during:?}");
    drop(hold);
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Succeeded,
        "{operation:?}"
    );
    // La place est rendue : la seconde sauvegarde passe.
    let (_, _) = b.made(api.key.as_str(), json!({ "mode": "hot" })).await;
    b.agent.shutdown().await;
}

#[tokio::test]
async fn cancelling_a_backup_removes_the_part_written_and_restarts_a_service_it_stopped() {
    let b = bench().await;
    let service = b.deploy("web").await;
    let hold = b.safe_fs.hold_reads();
    let accepted = b
        .accepted(service.key.as_str(), json!({ "mode": "stop_copy_start" }))
        .await;
    b.until(&accepted.operation_id, |op| {
        op.step.as_deref() == Some("copy")
    })
    .await;
    // Le service est arrêté, l'archive partielle existe (inscrite au registre en cours d'écriture).
    assert!(
        b.engine
            .calls()
            .iter()
            .any(|call| matches!(call, Call::Stop(..)))
    );
    assert_eq!(
        b.files.names(&Bench::dir(&service.key)),
        [format!("{}.tar.gz.part", accepted.backup_id)]
    );
    let cancel = b
        .agent
        .confirmed(
            &b.ada,
            "DELETE",
            &format!("/operations/services/{}", accepted.operation_id),
            json!({}),
        )
        .await
        .send()
        .await;
    assert!(cancel.status < 300, "{}", cancel.text);
    drop(hold);
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Cancelled,
        "{operation:?}"
    );
    assert_eq!(operation.cause, Some(OperationCause::CancelledByAccount));
    // La partie écrite est supprimée, rien d'autre ; le service est relancé (retour à l'état d'avant).
    assert!(b.files.names(&Bench::dir(&service.key)).is_empty());
    assert!(b.list(service.key.as_str()).await.backups.is_empty());
    assert!(
        matches!(b.engine.calls().last(), Some(Call::Start(id)) if *id == service.container),
        "{:?}",
        b.engine.calls()
    );
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// L'espace (BR-BACKUP-009)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_backup_that_would_eat_the_reserve_is_refused_with_the_figures_and_writes_nothing() {
    let b = bench().await;
    let service = b.deploy("web").await;
    b.files.set_space(DiskSpace {
        free: 2 * GIB,
        total: 100 * GIB,
    });
    let reply = b
        .create(service.key.as_str(), json!({ "mode": "hot" }))
        .await;
    assert_eq!(reply.status, 507, "{}", reply.text);
    assert_eq!(code(&reply), "NO_SPACE");
    assert_eq!(reply.body["error"]["details"]["free_bytes"], 2 * GIB);
    assert_eq!(reply.body["error"]["details"]["reserve_bytes"], 5 * GIB);
    // Rien n'a été écrit, aucune opération n'a été acceptée.
    assert!(b.files.writes().is_empty());
    assert!(b.files.all().is_empty());
    let ops: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM service_ops WHERE kind = 'backup'")
        .fetch_one(b.env.db.pool())
        .await
        .unwrap();
    assert_eq!(ops, 0);
    // La fenêtre le dit aussi : aucune case ne permet de passer outre.
    let preview: BackupPreview =
        serde_json::from_value(b.preview(service.key.as_str()).await.body).unwrap();
    assert!(!preview.enough_space);
    // Une mesure d'espace qui échoue n'est pas un feu vert pour écrire.
    b.files.space_unreadable();
    let unknown = b
        .create(service.key.as_str(), json!({ "mode": "hot" }))
        .await;
    assert!(unknown.status >= 400, "{}", unknown.text);
    assert!(b.files.writes().is_empty());
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_disk_that_fills_during_the_copy_stops_it_and_removes_the_partial_archive() {
    let b = bench().await;
    let service = b.deploy("web").await;
    // Le premier relevé (avant) est large ; ceux de la copie tombent sous la moitié de la réserve.
    b.files.fill_after(
        1,
        DiskSpace {
            free: GIB,
            total: 100 * GIB,
        },
    );
    let accepted = b
        .accepted(service.key.as_str(), json!({ "mode": "hot" }))
        .await;
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(operation.status, OperationStatus::Failed, "{operation:?}");
    assert_eq!(operation.cause, Some(OperationCause::DiskAlmostFull));
    assert!(b.files.names(&Bench::dir(&service.key)).is_empty());
    assert!(b.list(service.key.as_str()).await.backups.is_empty());
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Le droit (BR-BACKUP-001), la racine (BR-BACKUP-004 (b)), les services visés
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn only_an_account_with_the_right_acts_whatever_its_role_and_everyone_reads_the_list() {
    let b = bench().await;
    let service = b.deploy("web").await;
    let (accepted, _) = b.made(service.key.as_str(), json!({ "mode": "hot" })).await;
    // Un administrateur sans le droit, un lecteur sans le droit : refusés, journalisés.
    for who in [&b.bob, &b.lucas] {
        let reply = b
            .create_as(who, service.key.as_str(), json!({ "mode": "hot" }))
            .await;
        assert_eq!(reply.status, 403, "{}", reply.text);
        let preview = b
            .agent
            .request(
                "POST",
                &format!("/services/{}/backups/preview", service.key.as_str()),
            )
            .token(&who.token)
            .json(&json!({}))
            .send()
            .await;
        assert_eq!(preview.status, 403, "{}", preview.text);
    }
    let denied = b
        .agent
        .confirmed(
            &b.bob,
            "DELETE",
            &format!("/backups/{}", accepted.backup_id),
            json!({}),
        )
        .await
        .send()
        .await;
    assert_eq!(denied.status, 403, "{}", denied.text);
    assert_eq!(b.list(service.key.as_str()).await.backups.len(), 1);
    // La liste est visible de tous, droit ou non, rôle ou non.
    for who in [&b.bob, &b.lucas, &b.dan, &b.ada] {
        assert_eq!(b.list_as(who, service.key.as_str()).await.status, 200);
    }
    // Un lecteur AVEC le droit agit : le droit est indépendant du rôle.
    let by_dan = b
        .create_as(&b.dan, service.key.as_str(), json!({ "mode": "hot" }))
        .await;
    assert_eq!(by_dan.status, 202, "{}", by_dan.text);
    let journal = b.journal("service.backup.create").await;
    assert!(
        journal.iter().any(|row| row.0 == "denied"),
        "le refus faute de droit est au journal : {journal:?}"
    );
    b.agent.shutdown().await;
}

/// Une topologie où la racine des sauvegardes est DANS le dossier des services.
struct Overlapping;

impl HostPaths for Overlapping {
    fn resolve(&self, path: &str) -> Resolved {
        if path.ends_with("hearth-backups") {
            return Resolved::Found {
                canonical: "/data/services/sauvegardes".into(),
                lineage: vec![(1, 90), (1, 40), (1, 10), (1, 2)],
            };
        }
        if path.ends_with("services") {
            return Resolved::Found {
                canonical: "/data/services".into(),
                lineage: vec![(1, 40), (1, 10), (1, 2)],
            };
        }
        Resolved::Absent
    }
}

#[tokio::test]
async fn a_backups_root_inside_a_data_folder_refuses_every_backup_function() {
    let b = bench_with(|parts| parts.host_paths = Some(Arc::new(Overlapping))).await;
    let service = b.deploy("web").await;
    let reply = b
        .create(service.key.as_str(), json!({ "mode": "hot" }))
        .await;
    assert_eq!(reply.status, 422, "{}", reply.text);
    assert_eq!(code(&reply), "PATH_REFUSED");
    assert_eq!(
        reply.body["error"]["message"],
        "La racine des sauvegardes chevauche un dossier de données."
    );
    assert_eq!(
        reply.body["error"]["details"]["reason"],
        "backups_root_overlap"
    );
    for reply in [
        b.list_as(&b.ada, service.key.as_str()).await,
        b.preview(service.key.as_str()).await,
    ] {
        assert_eq!(code(&reply), "PATH_REFUSED", "{}", reply.text);
    }
    // Rien n'a été écrit dans la racine.
    assert!(b.files.writes().is_empty());
    b.agent.shutdown().await;
}

#[tokio::test]
async fn an_unsafe_root_or_an_unknown_service_is_refused_without_a_trace_of_writing() {
    let b = bench().await;
    let service = b.deploy("web").await;
    b.files.refuse_root();
    let reply = b
        .create(service.key.as_str(), json!({ "mode": "hot" }))
        .await;
    assert_eq!(reply.status, 422, "{}", reply.text);
    assert_eq!(
        reply.body["error"]["details"]["reason"],
        "backups_root_unavailable"
    );
    assert!(b.files.writes().is_empty());
    let unknown = b
        .create("h_01J9ZY0G3Q8M2K6W4T7V5N1B9D", json!({ "mode": "hot" }))
        .await;
    assert_eq!(unknown.status, 404, "{}", unknown.text);
    assert_eq!(code(&unknown), "SERVICE_NOT_FOUND");
    let list = b.list_as(&b.ada, "h_01J9ZY0G3Q8M2K6W4T7V5N1B9D").await;
    assert_eq!(list.status, 404);
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_service_without_any_data_folder_has_nothing_to_save() {
    let b = bench().await;
    let reply = b
        .agent
        .confirmed(
            &b.ada,
            "POST",
            "/deployments",
            json!({
                "name": "nu",
                "image": "nginx",
                "resources": { "cpu_millis": 1000, "memory_bytes": 268_435_456_u64 }
            }),
        )
        .await
        .send()
        .await;
    assert_eq!(reply.status, 202, "{}", reply.text);
    let accepted: DeployAccepted = serde_json::from_value(reply.body).unwrap();
    assert_eq!(
        b.finished(&accepted.operation_id).await.status,
        OperationStatus::Succeeded
    );
    let key = accepted.service_key;
    let view = b.list(&key).await;
    assert!(view.nothing_to_save);
    let refused = b.create(&key, json!({ "mode": "hot" })).await;
    assert_eq!(refused.status, 422, "{}", refused.text);
    assert_eq!(
        refused.body["error"]["details"]["reason"],
        "nothing_to_save"
    );
    assert!(b.files.all().is_empty());
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Le registre : une ligne « writing » ne survit pas à un redémarrage (BR-BACKUP-025)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn the_registry_drops_writing_lines_at_startup_and_keeps_published_ones() {
    use hearth_agent::application::ports::{BackupRepo, NewBackup, Published};
    use hearth_agent::domain::backups::BackupId;
    let env = env().await;
    let repo = hearth_agent::infrastructure::sqlite::SqliteBackupRepo::new(env.db.pool().clone());
    let key = ServiceKey::parse("h_01J9ZY0G3Q8M2K6W4T7V5N1B9D").unwrap();
    let new = |id: &str| NewBackup {
        id: BackupId::parse(id).unwrap(),
        service_key: key.clone(),
        service_name: "web".into(),
        created_at: time::OffsetDateTime::UNIX_EPOCH,
        created_by: "ada".into(),
        dev: u64::MAX,
        ino: 7,
        mode: BackupMode::Hot,
        image: "x:1".into(),
        note: None,
    };
    let done = BackupId::parse("01J9ZY0G3Q8M2K6W4T7V5N1B9E").unwrap();
    let partial = BackupId::parse("01J9ZY0G3Q8M2K6W4T7V5N1B9F").unwrap();
    repo.insert_writing(&new(done.as_str())).await.unwrap();
    repo.insert_writing(&new(partial.as_str())).await.unwrap();
    assert!(
        repo.mark_ready(
            &done,
            &Published {
                archive_bytes: 10,
                data_bytes: 20,
                files: 1,
                flags: vec![BackupFlag::Hot],
                changed_files: 0,
                manifest_sha256: "ab".repeat(32),
                archive_sha256: "cd".repeat(32),
            }
        )
        .await
        .unwrap()
    );
    // Une ligne en cours d'écriture n'est jamais listée.
    assert_eq!(repo.list(&key).await.unwrap().len(), 1);
    assert_eq!(repo.drop_writing().await.unwrap(), 1);
    assert!(repo.get(&partial).await.unwrap().is_none());
    let kept = repo.get(&done).await.unwrap().expect("ligne publiée");
    // Le périphérique et l'inode font l'aller-retour, même au-delà de i64.
    assert_eq!((kept.dev, kept.ino), (u64::MAX, 7));
    assert_eq!(kept.flags, [BackupFlag::Hot]);
}

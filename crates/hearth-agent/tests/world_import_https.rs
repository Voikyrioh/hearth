//! Reprendre un monde existant, de bout en bout (HRT-105, ADR-0059) : vrai agent en HTTPS, vraie base, faux moteur,
//! faux dossiers de données, FAUX `WorldFs` en mémoire (la source, le dossier d'import et la mise en place ;
//! l'adaptateur Unix a ses propres tests). Règles : BR-MONDE-002 à 004, 006, 009, 010, 012 à 016, 023, 024, 026.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use hearth_agent::application::catalog::CatalogService;
use hearth_agent::application::ports::EngineInfo;
use hearth_agent::domain::accounts::Role;
use hearth_agent::domain::catalog::loading::model::Fiche;
use hearth_agent::domain::catalog::{Origin, load};
use hearth_agent::infrastructure::config::{AllowedRoot, RootAccess};
use hearth_proto::api::deploy::DeployAccepted;
use hearth_proto::api::service_ops::{OperationStatus, OperationView};
use serde_json::{Value, json};
use support::device::Actor;
use support::engine::{fast_timings, parts_supervised};
use support::fake_engine::{FakeEngine, PullScript};
use support::fake_host::{FakeFreeSpace, FakeHostPorts};
use support::fake_safe_fs::FakeSafeFs;
use support::fake_supervisor::FakeSupervisor;
use support::fake_world_fs::{FakeWorldFs, Handle};
use support::https::{self, Agent, Wire};
use support::{Env, env};

const GIB: u64 = 1024 * 1024 * 1024;
const TERRARIA_FICHE: &str = include_str!("../catalog/terraria/fiche.toml");
const TERRARIA_SETTINGS: &str = include_str!("../catalog/terraria/settings.toml");
const IMAGE_DIGEST: &str =
    "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const ROOT: &str = "/srv/jeux";
type JournalRow = (String, Option<String>);

fn terraria() -> Fiche {
    let text = TERRARIA_FICHE.replacen(
        "digest = \"sha256:3ece8ceb88c11e47bc490d4e17849e89f5343722142fca506fbde9a7ab04a677\"",
        &format!("digest = \"{IMAGE_DIGEST}\""),
        1,
    );
    load(
        Origin::Embedded,
        text.as_bytes(),
        TERRARIA_SETTINGS.as_bytes(),
    )
    .expect("Terraria épinglée valide")
}

struct Bench {
    env: Env,
    engine: Arc<FakeEngine>,
    free_space: Arc<FakeFreeSpace>,
    worlds: Arc<FakeWorldFs>,
    agent: Agent,
    fiche: Fiche,
    ada: Actor,
    bob: Actor,
}

async fn bench(with_roots: bool) -> Bench {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    engine.set_info(Some(EngineInfo {
        cpus: 8,
        memory_bytes: 16 * GIB,
        storage_dir: Some("/var/lib/docker".into()),
    }));
    engine.publish_image(
        &format!("ryshe/terraria:vanilla-1.4.5.8@{IMAGE_DIGEST}"),
        PullScript::two_layers(),
    );
    let free_space = Arc::new(FakeFreeSpace::new());
    let worlds = FakeWorldFs::new();
    worlds.put(ROOT, "Worlds/Alpha.wld", &vec![7u8; 300_000]);
    worlds.put(ROOT, "Worlds/Alpha.wld.bak", b"old");
    worlds.put(ROOT, "Worlds/Beta.wld", b"beta");
    worlds.put(ROOT, "Worlds/notes.txt", b"not a world");
    worlds.put(ROOT, "Backups/Gamma.wld", b"in a backup folder");
    let fiche = terraria();
    let mut parts = parts_supervised(
        engine.clone(),
        Arc::new(FakeSupervisor::new()),
        fast_timings(),
    );
    parts.host_ports = Arc::new(FakeHostPorts::new());
    parts.free_space = free_space.clone();
    parts.safe_fs = Some(Arc::new(FakeSafeFs::new()));
    parts.catalog.include_preview = true;
    parts.catalog.world_fs = Some(Arc::new(Handle(worlds.clone())));
    parts.catalog.service = Some(Arc::new(CatalogService::from_parts(
        vec![fiche.clone()],
        Vec::new(),
        true,
    )));
    let roots = if with_roots {
        vec![AllowedRoot {
            path: PathBuf::from(ROOT),
            access: RootAccess::Ro,
        }]
    } else {
        Vec::new()
    };
    let agent = https::start_with_roots(&env, parts, roots).await;
    let ada = agent.actor(&env, "ada", Role::Admin).await;
    let bob = agent.actor(&env, "bob", Role::Admin).await;
    env.grant_services("ada").await;
    Bench {
        env,
        engine,
        free_space,
        worlds,
        agent,
        fiche,
        ada,
        bob,
    }
}

impl Bench {
    fn request(&self, name: &str, path: &str) -> Value {
        json!({
            "name": name,
            "resources": { "cpu_millis": 1000, "memory_bytes": 2 * GIB },
            "source": {
                "type": "fiche",
                "id": self.fiche.id(),
                "version": self.fiche.file.version,
                "digest": self.fiche.digest_hex(),
                "world_imported": true,
                "world": {
                    "folder": ROOT,
                    "path": path,
                    "version_understood": true,
                    "keep_if_changed": false
                }
            }
        })
    }

    async fn install(&self, body: Value) -> Wire {
        self.agent
            .confirmed(&self.ada, "POST", "/deployments", body)
            .await
            .send()
            .await
    }

    async fn accepted(&self, body: Value) -> DeployAccepted {
        let reply = self.install(body).await;
        assert_eq!(reply.status, 202, "{}", reply.text);
        serde_json::from_value(reply.body).expect("DeployAccepted")
    }

    async fn finished(&self, id: &str) -> OperationView {
        let started = Instant::now();
        loop {
            let reply = self
                .agent
                .request("GET", &format!("/operations/services/{id}"))
                .token(&self.ada.token)
                .send()
                .await;
            assert_eq!(reply.status, 200, "{}", reply.text);
            let operation: OperationView = serde_json::from_value(reply.body).unwrap();
            if !matches!(
                operation.status,
                OperationStatus::Accepted | OperationStatus::Running | OperationStatus::Cancelling
            ) {
                return operation;
            }
            assert!(
                started.elapsed() < Duration::from_secs(20),
                "la reprise ne finit pas : {operation:?}"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    async fn until(&self, what: &str, ready: impl Fn() -> bool) {
        let started = Instant::now();
        while !ready() {
            assert!(
                started.elapsed() < Duration::from_secs(15),
                "attendu : {what}"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    async fn journal(&self) -> Vec<JournalRow> {
        self.agent.flush_journal().await;
        sqlx::query_as(
            "SELECT outcome, target FROM audit_events WHERE action = 'service.deploy' ORDER BY id",
        )
        .fetch_all(self.env.db.pool())
        .await
        .unwrap()
    }

    async fn specs_count(&self) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM service_specs")
            .fetch_one(self.env.db.pool())
            .await
            .unwrap()
    }

    async fn get(&self, who: &Actor, path: &str) -> Wire {
        self.agent
            .request("GET", path)
            .token(&who.token)
            .send()
            .await
    }

    async fn inspect(&self, who: &Actor, path: &str) -> Wire {
        self.agent
            .request("POST", "/catalog/terraria/worlds/inspect")
            .token(&who.token)
            .json(&json!({ "folder": ROOT, "path": path }))
            .send()
            .await
    }

    /// La source n'a pas bougé d'un octet.
    fn assert_source_untouched(&self, before: &std::collections::BTreeMap<String, Vec<u8>>) {
        assert_eq!(
            &self.worlds.root_snapshot(ROOT),
            before,
            "la source a changé"
        );
    }
}

fn code(wire: &Wire) -> String {
    wire.body["error"]["code"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

fn reason(wire: &Wire) -> String {
    wire.body["error"]["details"]["reason"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

// ---------------------------------------------------------------------------------------------
// Lister et inspecter (BR-MONDE-002, 003, 006, 024)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn the_list_shows_the_worlds_of_the_open_folders_and_never_a_backup_or_a_foreign_file() {
    let b = bench(true).await;
    let reply = b.get(&b.ada, "/catalog/terraria/worlds").await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let roots = reply.body["roots"].as_array().unwrap();
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0]["folder"], ROOT);
    assert_eq!(roots[0]["readable"], true);
    let names: Vec<&str> = roots[0]["worlds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["Alpha", "Beta"], "{}", reply.text);
    assert_eq!(roots[0]["worlds"][0]["path"], "Worlds/Alpha.wld");
    assert_eq!(roots[0]["worlds"][0]["bytes"], 300_000);
    assert_eq!(reply.body["max_depth"], 3);
    assert_eq!(reply.body["max_entries"], 5000);
    // La consultation n'est pas journalisée (BR-MONDE-003).
    assert!(b.journal().await.is_empty());
    b.agent.shutdown().await;
}

#[tokio::test]
async fn without_an_open_folder_the_list_is_empty() {
    let b = bench(false).await;
    let reply = b.get(&b.ada, "/catalog/terraria/worlds").await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["roots"].as_array().unwrap().len(), 0);
    b.agent.shutdown().await;
}

#[tokio::test]
async fn an_account_without_the_right_sees_neither_the_list_nor_the_inspection() {
    let b = bench(true).await;
    for reply in [
        b.get(&b.bob, "/catalog/terraria/worlds").await,
        b.inspect(&b.bob, "Worlds/Alpha.wld").await,
    ] {
        assert_eq!(reply.status, 403, "{}", reply.text);
    }
    b.agent.shutdown().await;
}

#[tokio::test]
async fn the_inspection_gives_the_size_to_copy_and_what_stays_where_it_is() {
    let b = bench(true).await;
    let reply = b.inspect(&b.ada, "Worlds/Alpha.wld").await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["files"], 1);
    assert_eq!(reply.body["bytes"], 300_000);
    assert_eq!(reply.body["not_copied"], 0);
    assert!(reply.body["free_bytes"].as_u64().unwrap() > 0);
    for path in ["Worlds/notes.txt", "Backups/Gamma.wld", "Worlds/Absent.wld"] {
        let reply = b.inspect(&b.ada, path).await;
        assert_eq!(reply.status, 422, "{path} : {}", reply.text);
    }
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Une reprise qui réussit (BR-MONDE-004, 010, 014, 015, 023)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_world_is_copied_verified_placed_and_loaded_by_name_and_the_source_is_untouched() {
    let b = bench(true).await;
    let before = b.worlds.root_snapshot(ROOT);
    let accepted = b
        .accepted(b.request("mon-terraria", "Worlds/Alpha.wld"))
        .await;
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Succeeded,
        "{operation:?}"
    );
    // La copie est mise en place dans le dossier géré du monde, avec le propriétaire de la fiche.
    let placed = b.worlds.placed();
    assert_eq!(placed.len(), 1, "{placed:?}");
    assert_eq!(placed[0].key, accepted.service_key);
    assert_eq!(
        placed[0].files.keys().collect::<Vec<_>>(),
        vec!["Alpha.wld"]
    );
    assert_eq!(placed[0].files["Alpha.wld"], vec![7u8; 300_000]);
    assert_eq!(placed[0].owner, (0, 0));
    assert!(b.worlds.leftover_imports().is_empty());
    // La fiche charge le monde repris par son nom de fichier (BR-MONDE-015).
    let plan = &b.engine.created()[0];
    assert!(
        plan.env
            .iter()
            .any(|(name, value)| name == "WORLD_FILENAME" && value == "Alpha.wld"),
        "{:?}",
        plan.env.iter().map(|(n, _)| n).collect::<Vec<_>>()
    );
    // La source n'a été lue que pour le monde choisi, jamais ses voisins (BR-MONDE-005).
    assert_eq!(b.worlds.opened(), vec!["Worlds/Alpha.wld".to_owned()]);
    b.assert_source_untouched(&before);
    // Le journal : le dossier ouvert tel qu'écrit, des nombres, jamais le sous-chemin ni le nom (BR-MONDE-023).
    let journal = b.journal().await;
    assert_eq!(journal.len(), 1, "{journal:?}");
    assert_eq!(journal[0].0, "ok");
    let target = journal[0].1.clone().unwrap();
    assert!(
        target.contains("monde existant repris depuis le dossier ouvert /srv/jeux (1 fichier(s), 300000 octet(s), 0 élément(s) non copié(s), aucun fichier modifié pendant la copie)"),
        "{target}"
    );
    assert!(
        !target.contains("Alpha") && !target.contains("Worlds"),
        "{target}"
    );
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Ce qui est refusé avant de rien créer (BR-MONDE-002, 012, 014, 016, 024)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_request_that_the_agent_cannot_honour_creates_nothing() {
    let b = bench(true).await;
    // « Je comprends » est exigé par l'agent, même d'un client modifié (BR-MONDE-016).
    let mut body = b.request("t1", "Worlds/Alpha.wld");
    body["source"]["world"]["version_understood"] = json!(false);
    let reply = b.install(body).await;
    assert_eq!(
        (reply.status, code(&reply)),
        (422, "VALIDATION_ERROR".into())
    );
    assert_eq!(reason(&reply), "version_not_understood");
    // Un dossier qui n'est pas ouvert aux services (BR-MONDE-002).
    let mut body = b.request("t2", "Worlds/Alpha.wld");
    body["source"]["world"]["folder"] = json!("/etc");
    let reply = b.install(body).await;
    assert_eq!(reason(&reply), "unknown_folder", "{}", reply.text);
    // Un chemin qui n'est pas un monde, ou qui sort du dossier.
    for (path, expected) in [
        ("Worlds/notes.txt", "not_a_world"),
        ("Backups/Gamma.wld", "not_a_world"),
        ("../etc/passwd", "bad_path"),
        ("/etc/passwd", "bad_path"),
        ("Worlds/Absent.wld", "not_found"),
    ] {
        let reply = b.install(b.request("t3", path)).await;
        assert_eq!(reply.status, 422, "{path} : {}", reply.text);
        assert_eq!(reason(&reply), expected, "{path}");
    }
    // Le nom du monde vient du monde, jamais du client (BR-MONDE-014).
    let mut body = b.request("t4", "Worlds/Alpha.wld");
    body["source"]["answers"] = json!([{"id": "world_name", "value": "Autre"}]);
    let reply = b.install(body).await;
    assert_eq!(reply.status, 422, "{}", reply.text);
    // Le drapeau sans le monde.
    let mut body = b.request("t5", "Worlds/Alpha.wld");
    body["source"].as_object_mut().unwrap().remove("world");
    let reply = b.install(body).await;
    assert_eq!(reply.status, 422, "{}", reply.text);
    assert_eq!(b.specs_count().await, 0);
    assert!(b.engine.created().is_empty() && b.engine.pulls().is_empty());
    assert!(b.worlds.placed().is_empty() && b.worlds.leftover_imports().is_empty());
    b.agent.shutdown().await;
}

#[tokio::test]
async fn the_room_for_the_copy_is_asked_on_top_of_the_room_the_game_needs() {
    let b = bench(true).await;
    // De quoi installer le jeu (mesure de la fiche : 53 014 364 octets), pas de quoi y copier le monde de 300 ko.
    b.free_space.set(53_014_364 + 100_000);
    let reply = b.install(b.request("trop-juste", "Worlds/Alpha.wld")).await;
    assert_eq!(reply.status, 507, "{}", reply.text);
    assert_eq!(code(&reply), "NO_SPACE");
    let details = &reply.body["error"]["details"];
    assert!(
        details["needed_bytes"].as_u64().unwrap() >= 53_014_364 + 300_000,
        "{details}"
    );
    assert!(details["missing_bytes"].as_u64().unwrap() > 0, "{details}");
    assert!(b.worlds.leftover_imports().is_empty());
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Une copie qui ne tient pas (BR-MONDE-009, 010)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_copy_that_differs_from_the_source_creates_nothing_and_removes_only_its_import() {
    let b = bench(true).await;
    let before = b.worlds.root_snapshot(ROOT);
    b.worlds.corrupt_writes(true);
    let accepted = b
        .accepted(b.request("mon-terraria", "Worlds/Alpha.wld"))
        .await;
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(operation.status, OperationStatus::Failed, "{operation:?}");
    assert!(b.engine.created().is_empty());
    assert_eq!(b.specs_count().await, 0);
    assert!(b.worlds.placed().is_empty());
    assert_eq!(b.worlds.removed().len(), 1, "seul le dossier d'import part");
    assert!(b.worlds.leftover_imports().is_empty());
    b.assert_source_untouched(&before);
    let journal = b.journal().await;
    assert_eq!(journal[0].0, "failed", "{journal:?}");
    assert!(
        journal[0]
            .1
            .as_deref()
            .unwrap()
            .contains("monde repris incomplet"),
        "{journal:?}"
    );
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_file_changed_during_the_copy_is_discarded_unless_the_copy_was_kept_in_advance() {
    let b = bench(true).await;
    b.worlds.make_unstable("Worlds/Alpha.wld");
    // Par défaut : « Recommencer », la copie est retirée, rien n'est créé.
    let accepted = b
        .accepted(b.request("mon-terraria", "Worlds/Alpha.wld"))
        .await;
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(operation.status, OperationStatus::Failed, "{operation:?}");
    assert!(b.worlds.placed().is_empty() && b.worlds.leftover_imports().is_empty());
    assert!(b.engine.created().is_empty());
    let journal = b.journal().await;
    assert!(
        journal[0]
            .1
            .as_deref()
            .unwrap()
            .contains("fichiers modifiés pendant la copie, copie retirée"),
        "{journal:?}"
    );
    // « Garder cette copie » d'avance : le service est créé, la copie est dite possiblement incohérente.
    let mut body = b.request("garde", "Worlds/Alpha.wld");
    body["source"]["world"]["keep_if_changed"] = json!(true);
    let accepted = b.accepted(body).await;
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Succeeded,
        "{operation:?}"
    );
    assert_eq!(b.worlds.placed().len(), 1);
    let journal = b.journal().await;
    assert!(
        journal[1]
            .1
            .as_deref()
            .unwrap()
            .contains("fichiers modifiés pendant la copie, copie gardée"),
        "{journal:?}"
    );
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Une place pour les opérations lourdes, annuler (BR-MONDE-013)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn one_heavy_operation_at_a_time_and_the_second_world_is_refused_not_queued() {
    let b = bench(true).await;
    b.worlds.hold_copy(true);
    let first = b.accepted(b.request("premier", "Worlds/Alpha.wld")).await;
    b.until("la copie du premier a commencé", || {
        b.worlds.copy_started()
    })
    .await;
    let mut second = b.request("second", "Worlds/Beta.wld");
    second["source"]["ports"] = json!([{"id": "game", "host_port": 7778}]);
    let reply = b.install(second).await;
    assert_eq!(reply.status, 409, "{}", reply.text);
    assert_eq!(code(&reply), "HEAVY_IO_BUSY");
    b.worlds.hold_copy(false);
    let operation = b.finished(&first.operation_id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Succeeded,
        "{operation:?}"
    );
    b.agent.shutdown().await;
}

#[tokio::test]
async fn cancelling_during_the_copy_removes_the_import_and_creates_nothing() {
    let b = bench(true).await;
    let before = b.worlds.root_snapshot(ROOT);
    b.worlds.hold_copy(true);
    let accepted = b
        .accepted(b.request("mon-terraria", "Worlds/Alpha.wld"))
        .await;
    b.until("la copie a commencé", || b.worlds.copy_started())
        .await;
    let reply = b
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
    assert_eq!(reply.status, 200, "{}", reply.text);
    b.worlds.hold_copy(false);
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Cancelled,
        "{operation:?}"
    );
    assert!(b.engine.created().is_empty());
    assert_eq!(b.specs_count().await, 0);
    assert!(b.worlds.placed().is_empty());
    assert_eq!(b.worlds.removed().len(), 1);
    assert!(b.worlds.leftover_imports().is_empty());
    b.assert_source_untouched(&before);
    b.agent.shutdown().await;
}

#[tokio::test]
async fn the_agent_announces_the_capability() {
    let b = bench(true).await;
    let reply = b.agent.request("GET", "/hello").send().await;
    assert!(
        reply.body["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c == "world-import"),
        "{}",
        reply.text
    );
    b.agent.shutdown().await;
}

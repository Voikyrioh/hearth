//! Supprimer un service, de bout en bout (HRT-85, ADR-0045, ADR-0046, BR-DEL-*) : vrai agent en HTTPS, vraie base,
//! faux moteur, faux dossiers de données. Les routes répondent `202`, la suppression court à l'arrière-plan,
//! l'issue se lit dans l'opération et dans le journal d'activité.
//!
//! Règles : BR-DEL-001 à BR-DEL-006, BR-DEL-008, BR-DEL-011 à BR-DEL-018, BR-DEL-021, BR-DEL-029, BR-DEL-030.
//! La preuve de clé et le mot de passe sont dans la matrice de `admin_reauth.rs` ; ici : ce que fait la route et le
//! cas d'usage, jusqu'au journal d'activité et à la base. Le parcours réel des fichiers (liens, courses, montages) est
//! dans `infrastructure/fs/tests_unix.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use hearth_agent::application::catalog::CatalogService;
use hearth_agent::application::game_phase::GamePhases;
use hearth_agent::application::ports::{EngineError, EngineInfo, HostPaths, Resolved};
use hearth_agent::domain::accounts::Role;
use hearth_agent::domain::catalog::loading::model::Fiche;
use hearth_agent::domain::catalog::{Origin, load};
use hearth_agent::domain::services::{MountInfo, MountType, RunState, ServiceKey};
use hearth_agent::infrastructure::logging::{self, LogFormat};
use hearth_proto::api::deploy::DeployAccepted;
use hearth_proto::api::service_delete::{DeleteAccepted, DeletePreview, FolderFate};
use hearth_proto::api::service_ops::{OperationCause, OperationStatus, OperationView};
use hearth_proto::api::services::ServicesState;
use serde_json::{Value, json};
use support::device::Actor;
use support::engine::{fast_timings, parts_supervised};
use support::fake_engine::{Call, FakeEngine, PullScript, container};
use support::fake_host::{FakeFreeSpace, FakeHostPorts};
use support::fake_safe_fs::FakeSafeFs;
use support::fake_supervisor::FakeSupervisor;
use support::https::{self, Agent, Wire};
use support::{Env, env};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::writer::BoxMakeWriter;

const GIB: u64 = 1024 * 1024 * 1024;
/// Un nom de dossier qui ne doit apparaître NULLE PART hors du disque : il se retrouverait dans un chemin.
const FOLDER: &str = "sentinelle-dossier-91bc";
const TERRARIA_FICHE: &str = include_str!("../catalog/terraria/fiche.toml");
const TERRARIA_SETTINGS: &str = include_str!("../catalog/terraria/settings.toml");
const IMAGE_DIGEST: &str =
    "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

struct Bench {
    env: Env,
    engine: Arc<FakeEngine>,
    safe_fs: Arc<FakeSafeFs>,
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
    let mut parts = parts_supervised(
        engine.clone(),
        Arc::new(FakeSupervisor::new()),
        fast_timings(),
    );
    parts.host_ports = Arc::new(FakeHostPorts::new());
    parts.free_space = Arc::new(FakeFreeSpace::new());
    parts.safe_fs = Some(safe_fs.clone());
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
        agent,
        ada,
        bob,
        lucas,
        dan,
    }
}

/// Un service qui monte un dossier géré (`/sentinelle-dossier-91bc` dans le service).
fn spec(name: &str) -> Value {
    json!({
        "name": name,
        "image": "nginx",
        "mounts": [{ "target": format!("/{FOLDER}") }],
        "resources": { "cpu_millis": 1000, "memory_bytes": 268_435_456_u64 }
    })
}

/// Un service déployé, avec des données.
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

    /// Attend la fin de l'opération.
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

    /// Relit la liste des conteneurs du moteur : ce que le faux moteur vient de changer est publié.
    async fn resync(&self) {
        self.agent.services().resync().await.expect("liste");
    }

    async fn delete_as(&self, who: &Actor, key: &str, body: Value) -> Wire {
        self.agent
            .confirmed(who, "DELETE", &format!("/services/{key}"), body)
            .await
            .send()
            .await
    }

    async fn delete(&self, key: &str, body: Value) -> Wire {
        self.delete_as(&self.ada, key, body).await
    }

    async fn accepted(&self, key: &str, body: Value) -> DeleteAccepted {
        let reply = self.delete(key, body).await;
        assert_eq!(reply.status, 202, "{}", reply.text);
        serde_json::from_value(reply.body).expect("DeleteAccepted")
    }

    async fn preview(&self, key: &str) -> Wire {
        self.agent
            .request("GET", &format!("/services/{key}/delete-preview"))
            .token(&self.dan.token)
            .send()
            .await
    }

    async fn services(&self) -> ServicesState {
        let reply = self
            .agent
            .request("GET", "/services")
            .token(&self.lucas.token)
            .send()
            .await;
        serde_json::from_value(reply.body).expect("ServicesState")
    }

    /// Les entrées du journal d'activité de suppression : résultat, cible, raison, compte.
    async fn journal(&self) -> Vec<(String, Option<String>, Option<String>, Option<String>)> {
        self.agent.flush_journal().await;
        sqlx::query_as(
            "SELECT outcome, target, reason, account FROM audit_events
             WHERE action = 'service.delete' ORDER BY id",
        )
        .fetch_all(self.env.db.pool())
        .await
        .unwrap()
    }

    async fn count(&self, table: &str) -> i64 {
        let sql: &'static str = match table {
            "service_specs" => "SELECT COUNT(*) FROM service_specs",
            "service_spec_secrets" => "SELECT COUNT(*) FROM service_spec_secrets",
            "service_containers" => "SELECT COUNT(*) FROM service_containers",
            "service_intents" => "SELECT COUNT(*) FROM service_intents",
            "service_ops" => "SELECT COUNT(*) FROM service_ops",
            "game_settings" => "SELECT COUNT(*) FROM game_settings",
            "game_setting_secrets" => "SELECT COUNT(*) FROM game_setting_secrets",
            other => panic!("table inconnue du test : {other}"),
        };
        sqlx::query_scalar(sql)
            .fetch_one(self.env.db.pool())
            .await
            .unwrap()
    }

    /// Le chemin réel du dossier géré d'un service, tel que l'agent le calcule.
    fn folder_path(&self, key: &ServiceKey) -> String {
        format!(
            "{}/{}/{FOLDER}",
            self.env.dir.path().join("services").to_string_lossy(),
            key.as_str()
        )
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

// ---------------------------------------------------------------------------------------------
// Sans case : le service part, tout le reste est gardé (BR-DEL-004, BR-DEL-005)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_deletion_without_a_box_removes_the_service_and_keeps_every_byte_of_data() {
    let b = bench().await;
    let service = b.deploy("web").await;
    assert_eq!(b.count("service_specs").await, 1);
    assert_eq!(b.count("service_containers").await, 1);
    let accepted = b.accepted(service.key.as_str(), json!({})).await;
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Succeeded,
        "{operation:?}"
    );
    assert_eq!(operation.cause, None);
    // Pas d'étape d'effacement dans le plan : rien ne peut effacer sans l'avoir annoncé.
    assert_eq!(
        steps(&operation),
        ["stop", "remove_container", "forget"],
        "{operation:?}"
    );
    // Le conteneur est parti SANS toucher à ses volumes (le geste de nettoyage d'un déploiement échoué n'a pas
    // servi) ; la description, ses valeurs scellées et les conteneurs consignés sont oubliés.
    assert!(b.engine.container_ids().is_empty());
    assert_eq!(
        b.engine.removed_keeping_volumes(),
        std::slice::from_ref(&service.container)
    );
    assert!(
        b.engine.removed().is_empty(),
        "jamais le geste qui emporte les volumes"
    );
    assert_eq!(b.count("service_specs").await, 0);
    assert_eq!(b.count("service_containers").await, 0);
    assert_eq!(b.count("service_intents").await, 0);
    // Les dossiers de données restent, octet pour octet ; rien n'a été demandé à SafeFs.
    assert!(b.safe_fs.erase_calls().is_empty());
    assert_eq!(
        b.safe_fs
            .content(&service.key, &format!("{FOLDER}/world.wld")),
        Some(b"le monde".to_vec())
    );
    assert_eq!(
        b.safe_fs
            .content(&service.key, &format!("{FOLDER}/saves/a.bak")),
        Some(b"sauvegarde".to_vec())
    );
    assert!(b.safe_fs.has_root(&service.key));
    // Le service a disparu de la liste pour tous les comptes (BR-DEL-028).
    assert!(
        b.services()
            .await
            .services
            .iter()
            .all(|item| item.key != service.key.as_str())
    );
    // Le journal d'activité : réussite, le compte, le service, aucun chemin (BR-DEL-029).
    let journal = b.journal().await;
    assert_eq!(journal.len(), 1, "{journal:?}");
    assert_eq!(journal[0].0, "ok");
    assert_eq!(journal[0].3.as_deref(), Some("ada"));
    assert_eq!(journal[0].1.as_deref(), Some("service web"));
    b.agent.shutdown().await;
}

#[tokio::test]
async fn deleting_a_service_forgets_what_the_description_dragged_along() {
    let b = bench().await;
    let service = b.deploy("web").await;
    // Les réglages d'un jeu partent avec la description : le déclencheur de la base (HRT-108).
    sqlx::query(
        "INSERT INTO game_settings (service_key, settings_rev, answers_json, updated_by, updated_at)
         VALUES (?, 1, '[]', 'ada', '2026-10-24T10:00:00.000Z')",
    )
    .bind(service.key.as_str())
    .execute(b.env.db.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO game_setting_secrets (service_key, setting_id, nonce, sealed)
         VALUES (?, 'password', zeroblob(12), zeroblob(32))",
    )
    .bind(service.key.as_str())
    .execute(b.env.db.pool())
    .await
    .unwrap();
    let secrets_before = b.count("service_spec_secrets").await;
    let accepted = b.accepted(service.key.as_str(), json!({})).await;
    assert_eq!(
        b.finished(&accepted.operation_id).await.status,
        OperationStatus::Succeeded
    );
    assert_eq!(b.count("service_specs").await, 0);
    assert_eq!(
        b.count("service_spec_secrets").await,
        0,
        "{secrets_before} avant"
    );
    assert_eq!(b.count("game_settings").await, 0);
    assert_eq!(b.count("game_setting_secrets").await, 0);
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Avec la case « données » (BR-DEL-006, BR-DEL-011, BR-DEL-012, BR-DEL-014, BR-DEL-019, BR-DEL-021)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn the_data_box_needs_the_exact_name_and_nothing_is_touched_before_it_is_right() {
    let b = bench().await;
    let service = b.deploy("web").await;
    for typed in [None, Some("Web"), Some("web "), Some("")] {
        let mut body = json!({ "erase_data": true });
        if let Some(typed) = typed {
            body["confirm_name"] = json!(typed);
        }
        let reply = b.delete(service.key.as_str(), body).await;
        assert_eq!(reply.status, 422, "{typed:?} : {}", reply.text);
        assert_eq!(code(&reply), "VALIDATION_ERROR");
        assert_eq!(reply.body["error"]["details"]["field"], "confirm_name");
        assert_eq!(
            reply.body["error"]["message"],
            "Le nom saisi ne correspond pas."
        );
    }
    // Rien n'a bougé : le conteneur, la description, les données.
    assert_eq!(b.engine.container_ids().len(), 1);
    assert!(b.engine.removed_keeping_volumes().is_empty());
    assert_eq!(b.count("service_specs").await, 1);
    assert!(b.safe_fs.erase_calls().is_empty());
    assert_eq!(
        b.count("service_ops").await,
        1,
        "seul le déploiement est au journal"
    );
    b.agent.shutdown().await;
}

#[tokio::test]
async fn the_data_goes_only_after_the_container_while_the_name_is_reserved_and_the_service_busy() {
    let b = bench().await;
    let service = b.deploy("web").await;
    let _hold = b.safe_fs.hold_erasure();
    let accepted = b
        .accepted(
            service.key.as_str(),
            json!({ "erase_data": true, "confirm_name": "web" }),
        )
        .await;
    // L'effacement est en cours (suspendu par le test) : l'étape est « erase », et le conteneur est DÉJÀ parti.
    let during = b
        .until(&accepted.operation_id, |op| {
            op.step.as_deref() == Some("erase")
        })
        .await;
    assert_eq!(during.status, OperationStatus::Running);
    assert!(!during.cancellable, "passé le point de non-retour");
    assert!(
        b.engine.container_ids().is_empty(),
        "le conteneur part AVANT les données"
    );
    assert_eq!(b.count("service_specs").await, 0, "la description aussi");
    // Les données sont là tant que l'effacement n'a pas eu lieu.
    assert!(
        b.safe_fs
            .content(&service.key, &format!("{FOLDER}/world.wld"))
            .is_some()
    );
    // Un déploiement du même nom est refusé avec le texte de la spec (BR-DEL-019).
    let deploy = b
        .agent
        .confirmed(&b.ada, "POST", "/deployments", spec("web"))
        .await
        .send()
        .await;
    assert_eq!(deploy.status, 409, "{}", deploy.text);
    assert_eq!(code(&deploy), "NAME_BEING_DELETED");
    // Une seconde suppression du même service : une action est déjà en cours, avec le compte (BR-DEL-021).
    let second = b.delete(service.key.as_str(), json!({})).await;
    assert_eq!(second.status, 409, "{}", second.text);
    assert_eq!(code(&second), "SERVICE_BUSY");
    assert_eq!(second.body["error"]["details"]["by"], "ada");
    assert_eq!(second.body["error"]["details"]["kind"], "delete");
    // L'effacement est visible dans la bande, avec son étape ; on le laisse finir.
    b.safe_fs.release_erasure();
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Succeeded,
        "{operation:?}"
    );
    assert_eq!(
        steps(&operation),
        ["stop", "remove_container", "forget", "erase"]
    );
    // Les dossiers gérés sont partis, la racine du service avec (elle était vide) ; le nom est libre.
    assert!(!b.safe_fs.has_root(&service.key));
    assert_eq!(b.safe_fs.empty_root_removals(), 1);
    assert_eq!(b.safe_fs.erase_calls(), [FOLDER.to_owned()]);
    let again = b
        .agent
        .confirmed(&b.ada, "POST", "/deployments", spec("web"))
        .await
        .send()
        .await;
    assert_eq!(again.status, 202, "{}", again.text);
    // Le journal d'activité : « données effacées » avec des nombres, JAMAIS un chemin (BR-DEL-029).
    let journal = b.journal().await;
    // Le refus de la seconde demande (action en cours) est au journal avec sa cause, puis la réussite.
    assert_eq!(journal.len(), 2, "{journal:?}");
    assert_eq!(journal[0].0, "failed");
    assert_eq!(journal[0].2.as_deref(), Some("action déjà en cours"));
    assert_eq!(journal[1].0, "ok");
    let target = journal[1].1.clone().unwrap();
    assert!(
        target.starts_with("service web, données effacées (1 dossier, "),
        "{target}"
    );
    assert!(
        !target.contains(FOLDER) && !target.contains('/'),
        "{target}"
    );
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_folder_another_container_mounts_is_kept_even_when_the_data_box_is_ticked() {
    let b = bench().await;
    let service = b.deploy("web").await;
    // Un conteneur arrêté, créé hors de Hearth, monte ce dossier : le supprimer le lui retirerait (BR-DEL-008).
    let mut other = container("sauvegarde", RunState::Exited);
    other.mounts = vec![MountInfo {
        kind: MountType::Folder,
        source: b.folder_path(&service.key),
        target: "/in".into(),
        read_only: true,
    }];
    b.engine.put_silently(other);
    b.resync().await;
    let reply = b
        .delete(
            service.key.as_str(),
            json!({ "erase_data": true, "confirm_name": "web" }),
        )
        .await;
    assert_eq!(reply.status, 202, "{}", reply.text);
    let accepted: DeleteAccepted = serde_json::from_value(reply.body).unwrap();
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Succeeded,
        "{operation:?}"
    );
    // Le dossier est monté ailleurs : conservé, rien n'a été effacé, la racine reste (elle n'est pas vide).
    assert!(b.safe_fs.erase_calls().is_empty());
    assert!(
        b.safe_fs
            .content(&service.key, &format!("{FOLDER}/world.wld"))
            .is_some()
    );
    assert!(b.safe_fs.has_root(&service.key));
    assert_eq!(b.safe_fs.empty_root_removals(), 0);
    // Le service, lui, est bien supprimé.
    assert_eq!(b.count("service_specs").await, 0);
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Le même dossier sous un autre nom, un parent ou un enfant monté (revue de la PR 125, round 2)
// ---------------------------------------------------------------------------------------------

/// Une topologie de chemins donnée à l'agent : un montage « bind » ne se fabrique pas sans droits, la lignée
/// (identité du dossier, puis de ses parents) se dicte ici. Notre dossier géré : `(1,100)`, son parent `(1,50)`,
/// le dossier de données `(1,10)`, la racine `(1,2)`.
struct Topology {
    others: Vec<(&'static str, Resolved)>,
    /// Combien de temps met chaque lecture à répondre (un partage réseau mort).
    delay: Duration,
}

impl HostPaths for Topology {
    fn resolve(&self, path: &str) -> Resolved {
        std::thread::sleep(self.delay);
        if path.ends_with(FOLDER) {
            return found(path, &[(1, 100), (1, 50), (1, 10), (1, 2)]);
        }
        self.others
            .iter()
            .find(|(source, _)| *source == path)
            .map_or(Resolved::Absent, |(_, resolved)| resolved.clone())
    }
}

fn found(path: &str, lineage: &[(u64, u64)]) -> Resolved {
    Resolved::Found {
        canonical: path.to_owned(),
        lineage: lineage.to_vec(),
    }
}

/// Un agent dont les chemins du serveur sont `others` ; `delay` ralentit chaque lecture, `budget` borne leur somme.
async fn bench_over(
    others: Vec<(&'static str, Resolved)>,
    delay: Duration,
    budget: Option<Duration>,
) -> Bench {
    bench_with(move |parts| {
        parts.host_paths = Some(Arc::new(Topology { others, delay }));
        if let Some(budget) = budget {
            parts.delete.resolve_budget = budget;
        }
    })
    .await
}

/// Un conteneur arrêté, créé hors de Hearth, qui monte `source`.
fn mounting(source: &str) -> hearth_agent::domain::services::Observation {
    let mut other = container("sauvegarde", RunState::Exited);
    other.mounts = vec![MountInfo {
        kind: MountType::Folder,
        source: source.into(),
        target: "/in".into(),
        read_only: true,
    }];
    other
}

/// Déploie `web`, fait monter `source` par un autre conteneur, demande l'effacement : rend le sort annoncé par la
/// fenêtre et le nombre d'effacements réellement lancés.
async fn fate_with(b: &Bench, source: &str) -> (FolderFate, usize) {
    let service = b.deploy("web").await;
    b.engine.put_silently(mounting(source));
    b.resync().await;
    let preview = b.preview(service.key.as_str()).await;
    assert_eq!(preview.status, 200, "{}", preview.text);
    let preview: DeletePreview = serde_json::from_value(preview.body).unwrap();
    let accepted = b
        .accepted(
            service.key.as_str(),
            json!({ "erase_data": true, "confirm_name": "web" }),
        )
        .await;
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Succeeded,
        "{operation:?}"
    );
    (preview.folders[0].fate, b.safe_fs.erase_calls().len())
}

#[tokio::test]
async fn a_parent_of_the_folder_mounted_under_another_name_keeps_it() {
    // `/persist/hearth` est le dossier de données (`(1,10)`) vu par un autre chemin : aucun nom ne se recouvre,
    // l'identité d'un PARENT de notre dossier est pourtant celle du montage de l'autre conteneur.
    let b = bench_over(
        vec![(
            "/persist/hearth",
            found("/persist/hearth", &[(1, 10), (1, 5), (1, 2)]),
        )],
        Duration::ZERO,
        None,
    )
    .await;
    let (fate, erased) = fate_with(&b, "/persist/hearth").await;
    assert_eq!(fate, FolderFate::KeptShared);
    assert_eq!(erased, 0);
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_child_of_the_folder_mounted_under_another_name_keeps_it() {
    let b = bench_over(
        vec![(
            "/elsewhere/saves",
            found(
                "/elsewhere/saves",
                &[(1, 200), (1, 100), (1, 50), (1, 10), (1, 2)],
            ),
        )],
        Duration::ZERO,
        None,
    )
    .await;
    let (fate, erased) = fate_with(&b, "/elsewhere/saves").await;
    assert_eq!(fate, FolderFate::KeptShared);
    assert_eq!(erased, 0);
    b.agent.shutdown().await;
}

#[tokio::test]
async fn parents_in_common_are_not_a_sharing() {
    // Le dossier de l'autre conteneur n'a que la racine en commun avec le nôtre : l'effacement a lieu.
    let b = bench_over(
        vec![(
            "/srv/media",
            found("/srv/media", &[(1, 300), (1, 3), (1, 2)]),
        )],
        Duration::ZERO,
        None,
    )
    .await;
    let (fate, erased) = fate_with(&b, "/srv/media").await;
    assert_eq!(fate, FolderFate::Erasable);
    assert_eq!(erased, 1);
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_mount_that_cannot_be_read_keeps_the_folder_as_unverifiable() {
    let b = bench_over(
        vec![("/mnt/nfs-mort", Resolved::Unreadable)],
        Duration::ZERO,
        None,
    )
    .await;
    let (fate, erased) = fate_with(&b, "/mnt/nfs-mort").await;
    assert_eq!(fate, FolderFate::KeptUnverifiable);
    assert_eq!(erased, 0);
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_dead_network_share_never_hangs_the_request_the_folder_is_kept() {
    // Chaque lecture met 3 s ; le budget de lecture est de 300 ms : la fenêtre répond vite, dossier conservé.
    let b = bench_over(
        Vec::new(),
        Duration::from_secs(3),
        Some(Duration::from_millis(300)),
    )
    .await;
    let service = b.deploy("web").await;
    let started = Instant::now();
    let preview = b.preview(service.key.as_str()).await;
    assert_eq!(preview.status, 200, "{}", preview.text);
    assert!(
        started.elapsed() < Duration::from_millis(2500),
        "la fenêtre a attendu le partage : {:?}",
        started.elapsed()
    );
    let preview: DeletePreview = serde_json::from_value(preview.body).unwrap();
    assert_eq!(preview.folders[0].fate, FolderFate::KeptUnverifiable);
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Une liste de conteneurs pas fiable ne dit jamais « personne ne monte ce dossier »
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_container_list_that_cannot_be_trusted_keeps_the_folder() {
    let b = bench().await;
    let service = b.deploy("web").await;
    // Un conteneur ne s'inspecte plus : la liste n'est plus fiable, personne ne peut dire qu'il ne monte rien.
    let other = container("gros", RunState::Running);
    let id = other.id.clone();
    b.engine.put_silently(other);
    b.resync().await;
    b.engine.fail_inspect(&id, Some(EngineError::Timeout));
    b.resync().await;
    let preview = b.preview(service.key.as_str()).await;
    assert_eq!(preview.status, 200, "{}", preview.text);
    let preview: DeletePreview = serde_json::from_value(preview.body).unwrap();
    assert_eq!(preview.folders[0].fate, FolderFate::KeptUnverifiable);
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Une panique de l'exécution après l'ordre de suppression du conteneur
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_panic_after_the_container_removal_order_leaves_the_operation_to_check_and_the_name_reserved()
 {
    let b = bench().await;
    let service = b.deploy("web").await;
    b.engine.panic_on_remove(true);
    let accepted = b.accepted(service.key.as_str(), json!({})).await;
    let operation = b
        .until(&accepted.operation_id, |op| {
            op.status == OperationStatus::NeedsAttention
        })
        .await;
    // Comme après un redémarrage : cause et choix identiques, rien n'est rejoué.
    assert_eq!(operation.cause, Some(OperationCause::IntentWithoutDone));
    assert_eq!(
        operation.decision.expect("décision").choices,
        [hearth_proto::api::service_ops::OperationChoice::Verified]
    );
    // Le nom reste réservé : on ne remonte pas un service du même nom tant que personne n'a vérifié.
    b.engine.panic_on_remove(false);
    let deploy = b
        .agent
        .confirmed(&b.ada, "POST", "/deployments", spec("web"))
        .await
        .send()
        .await;
    assert_eq!(deploy.status, 409, "{}", deploy.text);
    assert_eq!(code(&deploy), "NAME_BEING_DELETED");
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// La fenêtre : dossiers, tailles, sort de chacun (BR-DEL-006 à BR-DEL-009)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn the_preview_lists_the_folders_their_size_and_what_will_happen_to_each() {
    let b = bench().await;
    let service = b.deploy("web").await;
    let reply = b.preview(service.key.as_str()).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let preview: DeletePreview = serde_json::from_value(reply.body).expect("aperçu");
    assert_eq!(preview.name, "web");
    assert_eq!(preview.image, "nginx:latest");
    assert!(preview.running && !preview.container_missing);
    assert_eq!(preview.folders.len(), 1, "{preview:?}");
    let folder = &preview.folders[0];
    assert_eq!(folder.path, b.folder_path(&service.key));
    assert_eq!(folder.fate, FolderFate::Erasable);
    assert_eq!(
        folder.items, 3,
        "world.wld, saves et saves/a.bak : {folder:?}"
    );
    assert!(folder.bytes > 0 && !folder.partial && !folder.unmeasured);
    assert!(preview.server_folders.is_empty());
    assert!(preview.measured_in_time);
    assert_eq!(preview.backups, 0);
    // Une consultation : rien n'a été écrit, aucun journal.
    assert_eq!(b.count("service_ops").await, 1);
    assert!(b.journal().await.is_empty());
    b.agent.shutdown().await;
}

#[tokio::test]
async fn the_preview_says_who_else_uses_a_folder_and_a_container_made_outside_hearth_without_a_name()
 {
    let b = bench().await;
    let service = b.deploy("web").await;
    let mut hearth_like = container("autre-service", RunState::Running);
    hearth_like.mounts = vec![MountInfo {
        kind: MountType::Folder,
        source: format!("{}/", b.folder_path(&service.key)),
        target: "/in".into(),
        read_only: true,
    }];
    b.engine.put_silently(hearth_like);
    b.resync().await;
    let preview: DeletePreview =
        serde_json::from_value(b.preview(service.key.as_str()).await.body).expect("aperçu");
    let folder = &preview.folders[0];
    assert_eq!(folder.fate, FolderFate::KeptShared);
    assert!(
        folder.shared_with.is_empty(),
        "un conteneur non adopté n'a pas de nom ici"
    );
    assert!(folder.shared_with_unmanaged);
    assert!(
        !serde_json::to_string(&preview)
            .unwrap()
            .contains("autre-service")
    );
    b.agent.shutdown().await;
}

#[tokio::test]
async fn volumes_are_counted_and_kept_the_removal_never_asks_the_engine_to_take_them() {
    let b = bench().await;
    let service = b.deploy("web").await;
    b.engine.update(&service.container, |observation| {
        observation.mounts.push(MountInfo {
            kind: MountType::Volume,
            source: "volume-anonyme".into(),
            target: "/var/lib/x".into(),
            read_only: false,
        });
    });
    b.resync().await;
    let preview: DeletePreview =
        serde_json::from_value(b.preview(service.key.as_str()).await.body).expect("aperçu");
    assert_eq!(preview.volumes, 1);
    let accepted = b.accepted(service.key.as_str(), json!({})).await;
    b.finished(&accepted.operation_id).await;
    assert!(b.engine.removed().is_empty());
    assert_eq!(b.engine.removed_keeping_volumes().len(), 1);
    b.agent.shutdown().await;
}

#[tokio::test]
async fn only_an_account_with_the_right_may_preview_or_delete() {
    let b = bench().await;
    let service = b.deploy("web").await;
    for token in [&b.bob.token, &b.lucas.token] {
        let reply = b
            .agent
            .request(
                "GET",
                &format!("/services/{}/delete-preview", service.key.as_str()),
            )
            .token(token)
            .send()
            .await;
        assert_eq!(reply.status, 403, "{}", reply.text);
    }
    let refused = b.delete_as(&b.bob, service.key.as_str(), json!({})).await;
    assert_eq!(refused.status, 403, "{}", refused.text);
    // Un compte en lecture seule qui a le droit peut supprimer : le droit est indépendant du rôle (BR-GRANT-001).
    let allowed = b.delete_as(&b.dan, service.key.as_str(), json!({})).await;
    assert_eq!(allowed.status, 202, "{}", allowed.text);
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Ce qu'on ne supprime pas, ce qui n'existe plus (BR-DEL-002, BR-DEL-018)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn an_adopted_service_is_never_deleted_and_the_refusal_is_journaled() {
    let b = bench().await;
    let adopted = container("7dtd", RunState::Running);
    let key = format!("c_{}", adopted.id);
    b.engine.put_silently(adopted.clone());
    b.resync().await;
    b.env.adopt(&key, "7dtd", "ada").await;
    b.agent.services().reload_adopted().await;
    let before = b.engine.calls().len();
    // Ouvrir la fenêtre n'écrit aucun refus `service.delete` : seul l'acte le fait.
    let preview = b.preview(&key).await;
    assert_eq!(code(&preview), "SERVICE_ADOPTED");
    assert_eq!(b.journal().await.len(), 0, "la fenêtre n'écrit rien");
    for body in [
        json!({}),
        json!({ "erase_data": true, "confirm_name": "7dtd" }),
    ] {
        let reply = b.delete(&key, body).await;
        assert_eq!(reply.status, 409, "{}", reply.text);
        assert_eq!(code(&reply), "SERVICE_ADOPTED");
        assert_eq!(
            reply.body["error"]["message"],
            "Hearth ne supprime pas les services créés hors de Hearth. Tu peux seulement ne plus le gérer."
        );
    }
    // Rien n'est touché, et le refus est au journal avec sa cause (BR-DEL-002).
    assert!(b.engine.container_ids().contains(&adopted.id));
    assert!(b.engine.removed_keeping_volumes().is_empty());
    assert_eq!(b.engine.calls().len(), before);
    // Deux demandes refusées : au plus deux entrées (les répétitions se regroupent), chacune échouée, avec la
    // cause et le service. On compte les entrées ; on ne lit pas le libellé d'un regroupement.
    let journal = b.journal().await;
    assert!(
        (1..=2).contains(&journal.len()),
        "une entrée par refus au plus : {journal:?}"
    );
    for entry in &journal {
        assert_eq!(entry.0, "failed");
        assert!(
            entry
                .2
                .as_deref()
                .is_some_and(|reason| reason.starts_with("service adopté")),
            "{entry:?}"
        );
        assert_eq!(entry.1.as_deref(), Some("service 7dtd"));
    }
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_service_whose_container_has_vanished_is_deleted_from_its_description() {
    let b = bench().await;
    let service = b.deploy("web").await;
    // Purge du moteur : le conteneur n'existe plus, la description reste (BR-DEL-018).
    b.engine.remove(&service.container);
    b.resync().await;
    let preview: DeletePreview =
        serde_json::from_value(b.preview(service.key.as_str()).await.body).expect("aperçu");
    assert!(preview.container_missing && !preview.running, "{preview:?}");
    assert_eq!(
        preview.folders.len(),
        1,
        "ses dossiers sont connus par la description"
    );
    let accepted = b
        .accepted(
            service.key.as_str(),
            json!({ "erase_data": true, "confirm_name": "web" }),
        )
        .await;
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Succeeded,
        "{operation:?}"
    );
    assert_eq!(b.count("service_specs").await, 0);
    assert_eq!(b.count("service_containers").await, 0);
    assert!(
        b.safe_fs
            .content(&service.key, &format!("{FOLDER}/world.wld"))
            .is_none()
    );
    assert!(!b.safe_fs.has_root(&service.key));
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_service_hearth_does_not_know_is_a_service_that_is_gone() {
    let b = bench().await;
    for key in [
        "h_01J9ZY0G3Q8M2K6W4T7V5N1B9D",
        "c_inconnu",
        "pas-une-cle",
        "u_x.service",
    ] {
        let reply = b.delete(key, json!({})).await;
        assert_eq!(reply.status, 404, "{key} : {}", reply.text);
        assert_eq!(code(&reply), "SERVICE_NOT_FOUND");
        assert_eq!(reply.body["error"]["message"], "Ce service n'existe plus.");
        assert_eq!(code(&b.preview(key).await), "SERVICE_NOT_FOUND");
    }
    assert!(b.safe_fs.erase_calls().is_empty());
    b.agent.shutdown().await;
}

#[tokio::test]
async fn deleting_twice_in_a_row_the_second_finds_nothing() {
    let b = bench().await;
    let service = b.deploy("web").await;
    let accepted = b.accepted(service.key.as_str(), json!({})).await;
    b.finished(&accepted.operation_id).await;
    let second = b.delete(service.key.as_str(), json!({})).await;
    assert_eq!(second.status, 404, "{}", second.text);
    assert_eq!(code(&second), "SERVICE_NOT_FOUND");
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Le moteur refuse, l'arrêt se fait avant, rien n'est jamais coupé en silence (BR-DEL-013, BR-DEL-015)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_running_service_is_stopped_with_its_grace_before_the_container_goes() {
    let b = bench().await;
    let service = b.deploy("web").await;
    b.engine.update(&service.container, |observation| {
        observation.stop_timeout_s = Some(7);
    });
    b.resync().await;
    let accepted = b.accepted(service.key.as_str(), json!({})).await;
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Succeeded,
        "{operation:?}"
    );
    // L'arrêt propre a été demandé avec le délai du service ; la suppression sans force n'aurait pas passé sinon.
    let stops: Vec<_> = b
        .engine
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            Call::Stop(id, grace) => Some((id, grace)),
            _ => None,
        })
        .collect();
    assert_eq!(stops, [(service.container.clone(), Duration::from_secs(7))]);
    // L'arrêt voulu a été oublié avec le service.
    assert_eq!(b.count("service_intents").await, 0);
    b.agent.shutdown().await;
}

#[tokio::test]
async fn an_engine_that_refuses_the_removal_leaves_the_service_whole_and_a_retry_succeeds() {
    let b = bench().await;
    let service = b.deploy("web").await;
    b.engine.fail_remove(Some(EngineError::Refused));
    let accepted = b
        .accepted(
            service.key.as_str(),
            json!({ "erase_data": true, "confirm_name": "web" }),
        )
        .await;
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(operation.status, OperationStatus::Failed, "{operation:?}");
    assert_eq!(operation.cause, Some(OperationCause::EngineRefused));
    // BR-DEL-015 : la description, les données, la liste sont intactes ; aucun fichier n'a été touché.
    assert_eq!(b.count("service_specs").await, 1);
    assert_eq!(b.count("service_containers").await, 1);
    assert!(b.safe_fs.erase_calls().is_empty());
    assert!(
        b.safe_fs
            .content(&service.key, &format!("{FOLDER}/world.wld"))
            .is_some()
    );
    assert!(
        b.services()
            .await
            .services
            .iter()
            .any(|item| item.key == service.key.as_str())
    );
    let journal = b.journal().await;
    assert_eq!(journal.len(), 1, "{journal:?}");
    assert_eq!(journal[0].0, "failed");
    assert_eq!(journal[0].2.as_deref(), Some("refusé par le moteur"));
    // Le verrou est rendu : on peut recommencer.
    b.engine.fail_remove(None);
    let again = b
        .accepted(
            service.key.as_str(),
            json!({ "erase_data": true, "confirm_name": "web" }),
        )
        .await;
    assert_eq!(
        b.finished(&again.operation_id).await.status,
        OperationStatus::Succeeded
    );
    b.agent.shutdown().await;
}

#[tokio::test]
async fn an_engine_that_does_not_answer_makes_the_result_unknown_and_touches_nothing_else() {
    let b = bench().await;
    let service = b.deploy("web").await;
    b.engine.fail_remove(Some(EngineError::Unreachable));
    let accepted = b
        .accepted(
            service.key.as_str(),
            json!({ "erase_data": true, "confirm_name": "web" }),
        )
        .await;
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Interrupted,
        "{operation:?}"
    );
    assert_eq!(operation.cause, Some(OperationCause::EngineUnavailable));
    assert_eq!(b.count("service_specs").await, 1);
    assert!(b.safe_fs.erase_calls().is_empty());
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_paused_service_is_refused_with_its_reason_and_nothing_is_touched() {
    let b = bench().await;
    let service = b.deploy("web").await;
    // Une pause : Hearth ne pilote pas ce service dans cet état (BR-DEL-003).
    b.engine.update(&service.container, |observation| {
        observation.run_state = RunState::Paused;
    });
    b.resync().await;
    let paused = b.delete(service.key.as_str(), json!({})).await;
    assert_eq!(paused.status, 409, "{}", paused.text);
    assert_eq!(code(&paused), "SERVICE_NOT_DRIVABLE");
    assert_eq!(b.engine.removed_keeping_volumes().len(), 0);
    b.agent.shutdown().await;
}

#[tokio::test]
async fn without_a_ready_engine_nothing_is_deleted_and_the_window_does_not_open() {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let parts = parts_supervised(engine, Arc::new(FakeSupervisor::new()), fast_timings());
    let agent = https::start_engine_disabled(&env, parts).await;
    let ada = agent.actor(&env, "ada", Role::Admin).await;
    env.grant_services("ada").await;
    let key = "h_01J9ZY0G3Q8M2K6W4T7V5N1B9D";
    let reply = agent
        .confirmed(&ada, "DELETE", &format!("/services/{key}"), json!({}))
        .await
        .send()
        .await;
    assert_eq!(reply.status, 503, "{}", reply.text);
    assert_eq!(code(&reply), "ENGINE_UNAVAILABLE");
    let preview = agent
        .request("GET", &format!("/services/{key}/delete-preview"))
        .token(&ada.token)
        .send()
        .await;
    assert_eq!(code(&preview), "ENGINE_UNAVAILABLE");
    agent.shutdown().await;
}

#[tokio::test]
async fn the_backups_box_is_refused_not_ignored_until_the_agent_makes_backups() {
    let b = bench().await;
    let service = b.deploy("web").await;
    let reply = b
        .delete(
            service.key.as_str(),
            json!({ "erase_backups": true, "confirm_name": "web" }),
        )
        .await;
    assert_eq!(reply.status, 422, "{}", reply.text);
    assert_eq!(reply.body["error"]["details"]["field"], "erase_backups");
    assert_eq!(b.count("service_specs").await, 1);
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Ce qui reste (BR-DEL-016) et ce qu'on ne dit jamais (BR-DEL-029)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn data_that_refuses_to_go_is_counted_and_the_deletion_still_succeeds() {
    let b = bench().await;
    let service = b.deploy("web").await;
    b.safe_fs.refuse_to_erase(&format!("{FOLDER}/saves"));
    let accepted = b
        .accepted(
            service.key.as_str(),
            json!({ "erase_data": true, "confirm_name": "web" }),
        )
        .await;
    let operation = b.finished(&accepted.operation_id).await;
    // Le service est supprimé ; la suite le dit par une cause, jamais un chemin.
    assert_eq!(
        operation.status,
        OperationStatus::Succeeded,
        "{operation:?}"
    );
    assert_eq!(operation.cause, Some(OperationCause::DataNotErased));
    assert_eq!(b.count("service_specs").await, 0);
    assert!(
        b.safe_fs
            .content(&service.key, &format!("{FOLDER}/world.wld"))
            .is_none()
    );
    assert!(
        b.safe_fs
            .content(&service.key, &format!("{FOLDER}/saves/a.bak"))
            .is_some()
    );
    // La racine n'est pas retirée : il reste quelque chose dedans.
    assert!(b.safe_fs.has_root(&service.key));
    let journal = b.journal().await;
    assert_eq!(journal[0].0, "ok", "le résultat reste « Réussi »");
    let target = journal[0].1.clone().unwrap();
    assert!(target.ends_with(", données non effacées (1)"), "{target}");
    b.agent.shutdown().await;
}

fn captured_traces() -> Arc<Mutex<Vec<u8>>> {
    #[derive(Clone)]
    struct Sink(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for Sink {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Sink {
        type Writer = Sink;
        fn make_writer(&'a self) -> Sink {
            self.clone()
        }
    }
    static TRACES: OnceLock<Arc<Mutex<Vec<u8>>>> = OnceLock::new();
    TRACES
        .get_or_init(|| {
            let buffer = Arc::new(Mutex::new(Vec::new()));
            logging::try_init_with(
                BoxMakeWriter::new(Sink(buffer.clone())),
                LogFormat::Json,
                false,
                EnvFilter::new("trace,bollard=trace,hyper_util=trace"),
            )
            .expect("première installation du journal de ce binaire de test");
            buffer
        })
        .clone()
}

/// Un nom de dossier, une valeur de variable, un nom de variable et la commande ne se retrouvent ni au journal
/// d'activité, ni dans les traces, ni dans l'opération, ni dans ce que les routes rendent (hors la fenêtre, qui
/// dit les chemins aux comptes qui ont le droit) ; et la description scellée disparaît de la base.
#[tokio::test]
async fn no_path_folder_name_or_secret_reaches_the_journal_the_traces_or_the_operation() {
    const VALUE: &str = "SENTINEL-VALUE-5c1d90e7";
    const NAME: &str = "SENTINEL_NAME_TOKEN_77aa";
    const COMMAND: &str = "--password=SENTINEL-CMD-3f2b11";
    let traces = captured_traces();
    log::debug!(target: "hearth_witness", "witness-delete-line");
    let b = bench().await;
    let mut body = spec("web");
    body["env"] = json!([{ "name": NAME, "value": VALUE }]);
    body["command"] = json!(format!("serve {COMMAND}"));
    let reply = b
        .agent
        .confirmed(&b.ada, "POST", "/deployments", body)
        .await
        .send()
        .await;
    let accepted: DeployAccepted = serde_json::from_value(reply.body).expect("accepté");
    b.finished(&accepted.operation_id).await;
    let key = ServiceKey::parse(&accepted.service_key).unwrap();
    b.safe_fs
        .seed_file(&key, &format!("{FOLDER}/world.wld"), b"x");
    let mut shown = String::new();
    // Les refus comme la réussite.
    shown.push_str(
        &b.delete(
            key.as_str(),
            json!({ "erase_data": true, "confirm_name": "faux" }),
        )
        .await
        .text,
    );
    let removal = b
        .accepted(
            key.as_str(),
            json!({ "erase_data": true, "confirm_name": "web" }),
        )
        .await;
    shown.push_str(&serde_json::to_string(&b.finished(&removal.operation_id).await).unwrap());
    let journal = format!("{:?}", b.journal().await);
    let audit = b
        .agent
        .request("GET", "/audit")
        .token(&b.ada.token)
        .send()
        .await
        .text;
    let sealed_left = b.count("service_spec_secrets").await;
    let data_dir = b.env.dir.path().to_owned();
    b.agent.shutdown().await;
    let captured = String::from_utf8_lossy(&traces.lock().unwrap()).into_owned();
    assert!(
        captured.contains("witness-delete-line"),
        "le pont `log` doit être actif"
    );
    assert!(
        captured.contains("suppression terminée"),
        "la trace de fin existe : le test n'est pas aveugle"
    );
    assert_eq!(
        sealed_left, 0,
        "les valeurs scellées partent avec le service"
    );
    let files = all_bytes(&data_dir);
    for forbidden in [VALUE, NAME, COMMAND, "SENTINEL-CMD", FOLDER] {
        assert!(!captured.contains(forbidden), "{forbidden} dans les traces");
        assert!(
            !journal.contains(forbidden),
            "{forbidden} au journal d'activité"
        );
        assert!(
            !audit.contains(forbidden),
            "{forbidden} dans le journal rendu"
        );
        assert!(
            !shown.contains(forbidden),
            "{forbidden} dans ce que les routes rendent"
        );
    }
    for forbidden in [VALUE, NAME, COMMAND] {
        assert!(
            !contains(&files, forbidden),
            "{forbidden} dans les fichiers de l'agent"
        );
    }
}

fn all_bytes(dir: &std::path::Path) -> Vec<u8> {
    let mut bytes = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            bytes.extend(all_bytes(&path));
        } else if let Ok(content) = std::fs::read(&path) {
            bytes.extend(content);
        }
    }
    bytes
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

// ---------------------------------------------------------------------------------------------
// Un serveur de jeu : ses phases sont oubliées (HRT-197)
// ---------------------------------------------------------------------------------------------

fn terraria() -> Fiche {
    let text = TERRARIA_FICHE.replacen(
        "game_updates = \"never\"",
        &format!("digest = \"{IMAGE_DIGEST}\"\ngame_updates = \"never\""),
        1,
    );
    load(
        Origin::Embedded,
        text.as_bytes(),
        TERRARIA_SETTINGS.as_bytes(),
    )
    .expect("Terraria valide")
}

#[tokio::test]
async fn deleting_a_game_server_makes_the_agent_forget_its_phases() {
    let fiche = terraria();
    let env_for_clock = env().await;
    let engine_for_phases = Arc::new(FakeEngine::new());
    let phases = GamePhases::new(engine_for_phases.clone(), env_for_clock.clock.clone());
    drop(env_for_clock);
    let shared = phases.clone();
    let fiche_for_catalog = fiche.clone();
    let b = bench_with(move |parts| {
        parts.catalog.include_preview = true;
        parts.catalog.service = Some(Arc::new(CatalogService::from_parts(
            vec![fiche_for_catalog],
            Vec::new(),
            true,
        )));
        parts.catalog.phases = Some(shared);
    })
    .await;
    let pinned = format!("ryshe/terraria:vanilla-1.4.5.8@{IMAGE_DIGEST}");
    b.engine.publish_image(&pinned, PullScript::two_layers());
    let agreements: Vec<String> = fiche.file.agreement.iter().map(|a| a.id.clone()).collect();
    let body = json!({
        "name": "mon-terraria",
        "resources": { "cpu_millis": 1000, "memory_bytes": 2 * GIB },
        "source": {
            "type": "fiche", "id": fiche.id(), "version": fiche.file.version,
            "digest": fiche.digest_hex(), "agreements": agreements,
            "answers": [{"id": "password", "value": "x"}, {"id": "motd", "value": "Salut"}]
        }
    });
    let reply = b
        .agent
        .confirmed(&b.ada, "POST", "/deployments", body)
        .await
        .send()
        .await;
    assert_eq!(reply.status, 202, "{}", reply.text);
    let accepted: DeployAccepted = serde_json::from_value(reply.body).unwrap();
    assert_eq!(
        b.finished(&accepted.operation_id).await.status,
        OperationStatus::Succeeded
    );
    let key = ServiceKey::parse(&accepted.service_key).unwrap();
    assert!(
        phases.phase_of(&key).is_some(),
        "l'installation a commencé le suivi"
    );
    let removal = b.accepted(key.as_str(), json!({})).await;
    assert_eq!(
        b.finished(&removal.operation_id).await.status,
        OperationStatus::Succeeded
    );
    assert!(
        phases.phase_of(&key).is_none(),
        "la suppression oublie le suivi"
    );
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Un redémarrage de l'agent au milieu de l'effacement (BR-DEL-025)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn an_agent_restarting_in_the_middle_of_an_erasure_never_resumes_it_and_asks_the_account() {
    let b = bench().await;
    let service = b.deploy("web").await;
    let _hold = b.safe_fs.hold_erasure();
    let accepted = b
        .accepted(
            service.key.as_str(),
            json!({ "erase_data": true, "confirm_name": "web" }),
        )
        .await;
    b.until(&accepted.operation_id, |op| {
        op.step.as_deref() == Some("erase")
    })
    .await;
    // L'effacement est bien commencé (le faux est entré dans `erase_tree` et attend).
    let started = Instant::now();
    while b.safe_fs.erase_calls().is_empty() {
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "l'effacement ne commence pas"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(b.safe_fs.erase_calls().len(), 1);
    // L'agent s'arrête là (coupure) : un autre démarre sur la même base et les mêmes dossiers.
    let mut parts = parts_supervised(
        b.engine.clone(),
        Arc::new(FakeSupervisor::new()),
        fast_timings(),
    );
    parts.host_ports = Arc::new(FakeHostPorts::new());
    parts.free_space = Arc::new(FakeFreeSpace::new());
    parts.safe_fs = Some(b.safe_fs.clone());
    let second = https::start_engined(&b.env, parts).await;
    let read = |id: String| {
        let second = &second;
        let token = b.dan.token.clone();
        async move {
            let reply = second
                .request("GET", &format!("/operations/services/{id}"))
                .token(&token)
                .send()
                .await;
            assert_eq!(reply.status, 200, "{}", reply.text);
            serde_json::from_value::<OperationView>(reply.body).expect("OperationView")
        }
    };
    // Signalée « à vérifier » : l'effacement a commencé sans qu'on sache où il en est, la seule décision est
    // « j'ai vérifié ». Rien n'est repris, rien n'est effacé de plus.
    let operation = read(accepted.operation_id.clone()).await;
    assert_eq!(
        operation.status,
        OperationStatus::NeedsAttention,
        "{operation:?}"
    );
    assert_eq!(operation.cause, Some(OperationCause::IntentWithoutDone));
    let choices = operation.decision.expect("décision").choices;
    assert_eq!(
        choices,
        [hearth_proto::api::service_ops::OperationChoice::Verified]
    );
    assert_eq!(b.safe_fs.erase_calls().len(), 1, "jamais rejoué");
    // Le nom reste réservé tant que personne n'a vérifié : le dossier a pu être réutilisé, on n'y remonte pas.
    let deploy = second
        .confirmed(&b.ada, "POST", "/deployments", spec("web"))
        .await
        .send()
        .await;
    assert_eq!(deploy.status, 409, "{}", deploy.text);
    assert_eq!(code(&deploy), "NAME_BEING_DELETED");
    // Le compte vérifie : le nom est libre.
    let decision = second
        .confirmed(
            &b.ada,
            "POST",
            &format!("/operations/services/{}/decision", accepted.operation_id),
            json!({ "choice": "verified" }),
        )
        .await
        .send()
        .await;
    assert_eq!(decision.status, 200, "{}", decision.text);
    let again = second
        .confirmed(&b.ada, "POST", "/deployments", spec("web"))
        .await
        .send()
        .await;
    assert_eq!(again.status, 202, "{}", again.text);
    assert_eq!(b.safe_fs.erase_calls().len(), 1, "toujours jamais rejoué");
    // On laisse finir l'ancien fil bloqué du faux, qui n'existerait pas dans un vrai arrêt.
    b.safe_fs.release_erasure();
    second.shutdown().await;
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// L'arrêt de l'agent pendant un effacement (revue de la PR 125)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn stopping_the_agent_stops_the_erasure_and_leaves_the_operation_to_check() {
    let b = bench().await;
    let service = b.deploy("web").await;
    let hold = b.safe_fs.hold_erasure();
    let accepted = b
        .accepted(
            service.key.as_str(),
            json!({ "erase_data": true, "confirm_name": "web" }),
        )
        .await;
    b.until(&accepted.operation_id, |op| {
        op.step.as_deref() == Some("erase")
    })
    .await;
    let started = Instant::now();
    while b.safe_fs.erase_calls().is_empty() {
        assert!(started.elapsed() < Duration::from_secs(15));
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    // L'agent s'arrête pendant l'effacement : le suivi demande l'arrêt, l'opération n'est PAS close.
    b.agent.shutdown().await;
    drop(hold);
    tokio::time::sleep(Duration::from_millis(300)).await;
    let status: String = sqlx::query_scalar("SELECT status FROM service_ops WHERE id = ?")
        .bind(&accepted.operation_id)
        .fetch_one(b.env.db.pool())
        .await
        .unwrap();
    assert_eq!(
        status, "running",
        "ni réussie ni échouée : à vérifier au prochain démarrage"
    );
    let done: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM service_op_steps WHERE op_id = ? AND step = 'erase' AND state = 'done'",
    )
    .bind(&accepted.operation_id)
    .fetch_one(b.env.db.pool())
    .await
    .unwrap();
    assert_eq!(done, 0, "l'étape d'effacement n'est pas dite finie");
}

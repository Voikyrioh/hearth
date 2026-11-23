//! Appliquer un lot de mods à un serveur Minecraft installé, de bout en bout (HRT-258, HRT-259, ADR-0062, BR-MOD-024 à
//! 027, 031 à 033) : vrai agent en HTTPS, vraie base, faux moteur, faux dossiers de données, fausse zone d'attente.
//! Les jars sont fabriqués octet par octet (zip sans compression), jamais un binaire versionné.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use hearth_agent::application::catalog::CatalogService;
use hearth_agent::application::ports::EngineInfo;
use hearth_agent::domain::accounts::Role;
use hearth_agent::domain::catalog::loading::model::Fiche;
use hearth_agent::domain::catalog::{Origin, load};
use hearth_agent::domain::services::{RunState, ServiceKey};
use hearth_proto::api::deploy::DeployAccepted;
use hearth_proto::api::service_ops::{OperationStatus, OperationView};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use support::device::Actor;
use support::engine::{fast_timings, parts_supervised};
use support::fake_backup_files::FakeBackupFiles;
use support::fake_engine::{Call, FakeEngine, PullScript};
use support::fake_host::{FakeFreeSpace, FakeHostPorts};
use support::fake_pack_deposits::FakePackDeposits;
use support::fake_safe_fs::FakeSafeFs;
use support::fake_supervisor::FakeSupervisor;
use support::https::{self, Agent, Wire};
use support::{Env, env};

const GIB: u64 = 1024 * 1024 * 1024;
const FICHE: &str = include_str!("../catalog/minecraft/fiche.toml");
const SETTINGS: &str = include_str!("../catalog/minecraft/settings.toml");
const READY: &str = "Done (3.1s)! For help, type \"help\"";

fn minecraft() -> Fiche {
    load(Origin::Embedded, FICHE.as_bytes(), SETTINGS.as_bytes()).expect("fiche minecraft")
}

/// Un zip sans compression : un jar minimal mais valable pour le lecteur d'archives hostiles.
fn jar(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in entries {
        let mut crc = flate2::Crc::new();
        crc.update(data);
        let offset = u32::try_from(out.len()).unwrap();
        let size = u32::try_from(data.len()).unwrap();
        let name_len = u16::try_from(name.len()).unwrap();
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0x21, 0]);
        out.extend_from_slice(&crc.sum().to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&name_len.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0x21, 0]);
        central.extend_from_slice(&crc.sum().to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&name_len.to_le_bytes());
        central.extend_from_slice(&[0; 12]);
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }
    let central_offset = u32::try_from(out.len()).unwrap();
    let central_size = u32::try_from(central.len()).unwrap();
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    let count = u16::try_from(entries.len()).unwrap();
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&central_size.to_le_bytes());
    out.extend_from_slice(&central_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

fn fabric(id: &str, version: &str) -> Vec<u8> {
    let json = format!(
        r#"{{"schemaVersion":1,"id":"{id}","version":"{version}","name":"{id}","depends":{{"minecraft":"~1.20.1"}}}}"#
    );
    jar(&[("fabric.mod.json", json.as_bytes())])
}

fn sha_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

struct Bench {
    env: Env,
    engine: Arc<FakeEngine>,
    safe_fs: Arc<FakeSafeFs>,
    deposits: Arc<FakePackDeposits>,
    agent: Agent,
    fiche: Fiche,
    ada: Actor,
    lucas: Actor,
    port: std::sync::atomic::AtomicU16,
}

async fn bench() -> Bench {
    let mut fiche = minecraft();
    // Le délai de la fiche, raccourci : un jeu qui ne dit jamais « prêt » se juge en secondes.
    fiche.file.first_start.ready_timeout_s = 3;
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    engine.set_info(Some(EngineInfo {
        cpus: 16,
        memory_bytes: 32 * GIB,
        storage_dir: Some("/var/lib/docker".into()),
    }));
    let safe_fs = Arc::new(FakeSafeFs::new());
    let deposits = Arc::new(FakePackDeposits::new(safe_fs.clone()));
    let mut parts = parts_supervised(
        engine.clone(),
        Arc::new(FakeSupervisor::new()),
        fast_timings(),
    );
    parts.host_ports = Arc::new(FakeHostPorts::new());
    parts.free_space = Arc::new(FakeFreeSpace::new());
    parts.safe_fs = Some(safe_fs.clone());
    parts.backups.files = Some(Arc::new(FakeBackupFiles::new()));
    parts.catalog.include_preview = true;
    parts.catalog.pack_deposits = Some(deposits.clone());
    parts.catalog.service = Some(Arc::new(CatalogService::from_parts(
        vec![fiche.clone()],
        Vec::new(),
        true,
    )));
    let agent = https::start_engined(&env, parts).await;
    let ada = agent.actor(&env, "ada", Role::Admin).await;
    let lucas = agent.actor(&env, "lucas", Role::ReadOnly).await;
    env.grant_services("ada").await;
    Bench {
        env,
        engine,
        safe_fs,
        deposits,
        agent,
        fiche,
        ada,
        lucas,
        port: std::sync::atomic::AtomicU16::new(25570),
    }
}

struct Server {
    key: ServiceKey,
    container: String,
    name: String,
}

impl Bench {
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
                OperationStatus::Accepted
                    | OperationStatus::Running
                    | OperationStatus::Cancelling
                    | OperationStatus::RollingBack
            ) {
                return operation;
            }
            assert!(
                started.elapsed() < Duration::from_secs(20),
                "l'opération ne finit pas : {operation:?}"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Installe un serveur Minecraft Fabric 1.20.1 : arrivé en marche, prêt dans ses journaux à chaque démarrage.
    async fn install(&self, name: &str, variant: &str) -> Server {
        let image = {
            let v = self
                .fiche
                .file
                .variant
                .iter()
                .find(|v| v.id == variant)
                .expect("variante");
            let tag = v
                .image_tag
                .clone()
                .unwrap_or(self.fiche.file.image.tag.clone());
            let digest = v
                .image_digest
                .clone()
                .or(self.fiche.file.image.digest.clone())
                .expect("empreinte");
            format!("{}:{tag}@{digest}", self.fiche.file.image.repository)
        };
        self.engine.publish_image(&image, PullScript::two_layers());
        // Sans le signe « prêt » : les lignes d'un démarrage ne comptent pas pour le suivant (les horloges du banc diffèrent).
        self.engine.log_on_start(name, "Loading...");
        let body = json!({
            "name": name,
            "resources": { "cpu_millis": 2000, "memory_bytes": 4 * GIB },
            "source": {
                "type": "fiche", "id": self.fiche.id(), "version": self.fiche.file.version,
                "digest": self.fiche.digest_hex(), "variant": variant,
                "agreements": ["eula"],
                "answers": [{ "id": "version", "value": "1.20.1" }],
                "ports": [{ "id": "game", "host_port": self.port.fetch_add(1, std::sync::atomic::Ordering::SeqCst) }],
            },
        });
        let reply = self
            .agent
            .confirmed(&self.ada, "POST", "/deployments", body)
            .await
            .send()
            .await;
        assert_eq!(reply.status, 202, "{}", reply.text);
        let accepted: DeployAccepted = serde_json::from_value(reply.body).unwrap();
        let operation = self.finished(&accepted.operation_id).await;
        assert_eq!(
            operation.status,
            OperationStatus::Succeeded,
            "{operation:?}"
        );
        let container = self
            .engine
            .container_ids()
            .into_iter()
            .last()
            .expect("conteneur");
        Server {
            key: ServiceKey::parse(&accepted.service_key).unwrap(),
            container,
            name: name.to_owned(),
        }
    }

    fn put(&self, server: &Server, rel: &str, bytes: &[u8]) {
        self.safe_fs
            .seed_file(&server.key, &format!("data/{rel}"), bytes);
    }

    fn content(&self, server: &Server, rel: &str) -> Option<Vec<u8>> {
        self.safe_fs.content(&server.key, &format!("data/{rel}"))
    }

    /// Dépose un jar dans la zone d'attente de `ada` : l'identifiant et l'empreinte à envoyer.
    fn deposit(&self, id: &str, bytes: &[u8]) -> Value {
        self.deposits.put("ada", id, bytes.to_vec());
        json!({ "deposit": id, "sha256": sha_hex(bytes) })
    }

    async fn apply_as(&self, who: &Actor, server: &Server, body: Value) -> Wire {
        self.agent
            .confirmed(
                who,
                "POST",
                &format!("/services/{}/mods/apply", server.key),
                body,
            )
            .await
            .send()
            .await
    }

    async fn apply(&self, server: &Server, body: Value) -> Wire {
        self.apply_as(&self.ada, server, body).await
    }

    /// Applique et attend la fin.
    async fn applied(&self, server: &Server, body: Value) -> (Value, OperationView) {
        let reply = self.apply(server, body).await;
        assert_eq!(reply.status, 202, "{}", reply.text);
        let id = reply.body["operation_id"].as_str().unwrap().to_owned();
        let operation = self.finished(&id).await;
        (reply.body, operation)
    }

    async fn stop_container(&self, server: &Server) {
        self.engine
            .update(&server.container, |o| o.run_state = RunState::Exited);
        self.agent.services().resync().await.expect("liste");
    }

    fn calls_len(&self) -> usize {
        self.engine.calls().len()
    }

    async fn journal(&self, action: &str) -> Vec<(String, Option<String>)> {
        self.agent.flush_journal().await;
        sqlx::query_as("SELECT outcome, target FROM audit_events WHERE action = ? ORDER BY id")
            .bind(action)
            .fetch_all(self.env.db.pool())
            .await
            .unwrap()
    }

    async fn mods_view(&self, server: &Server) -> Value {
        let reply = self
            .agent
            .request("GET", &format!("/services/{}/mods", server.key))
            .token(&self.lucas.token)
            .send()
            .await;
        assert_eq!(reply.status, 200, "{}", reply.text);
        reply.body
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

fn started(calls: &[Call]) -> usize {
    calls.iter().filter(|c| matches!(c, Call::Start(_))).count()
}

fn stopped(calls: &[Call]) -> usize {
    calls.iter().filter(|c| matches!(c, Call::Stop(..))).count()
}

// ---------------------------------------------------------------------------------------------
// Le chemin nominal (BR-MOD-024 à 026, 032, 033)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_running_server_is_stopped_saved_changed_restarted_and_watched_and_nothing_is_deleted() {
    let b = bench().await;
    let server = b.install("mc", "fabric").await;
    let lithium_old = fabric("lithium", "0.11.1");
    let iris = fabric("iris", "1.6.4");
    let carpet = fabric("carpet", "1.4.112");
    let off = fabric("farmer", "3.0.0");
    b.put(&server, "mods/lithium-0.11.1.jar", &lithium_old);
    b.put(&server, "mods/iris-1.6.4.jar", &iris);
    b.put(&server, "mods/carpet-1.4.112.jar", &carpet);
    b.put(&server, "mods/farmer-3.0.0.jar.disabled", &off);
    let sodium = fabric("sodium", "0.5.3");
    let lithium_new = fabric("lithium", "0.11.2");
    let mut replace = b.deposit("D-LITHIUM", &lithium_new);
    replace["replace"] = json!("lithium-0.11.1.jar");
    let calls_before = b.engine.calls();
    b.engine.log_on_start(&server.name, READY);

    let (accepted, operation) = b
        .applied(
            &server,
            json!({
                "add": [b.deposit("D-SODIUM", &sodium), replace],
                "retire": ["iris-1.6.4.jar"],
                "disable": ["carpet-1.4.112.jar"],
                "enable": ["farmer-3.0.0.jar.disabled"],
            }),
        )
        .await;
    assert_eq!(
        operation.status,
        OperationStatus::Succeeded,
        "{operation:?}"
    );
    assert_eq!(operation.cause, None);
    assert_eq!(
        steps(&operation),
        ["stop", "copy", "verify", "place", "start", "watch"]
    );
    // Le service a été arrêté puis relancé, une fois chacun.
    let calls = b.engine.calls();
    assert_eq!(stopped(&calls) - stopped(&calls_before), 1);
    assert_eq!(started(&calls) - started(&calls_before), 1);
    // Posés en 0644 au propriétaire de la fiche, sous le nom choisi par Hearth.
    assert_eq!(b.content(&server, "mods/sodium-0.5.3.jar"), Some(sodium));
    assert_eq!(
        b.content(&server, "mods/lithium-0.11.2.jar"),
        Some(lithium_new)
    );
    let owner = b.fiche.file.owner;
    assert_eq!(
        b.safe_fs
            .perms_of(&server.key, "data/mods/sodium-0.5.3.jar"),
        Some((owner.uid, owner.gid, 0o644))
    );
    // Rien n'est supprimé : l'ancien et le retiré sont dans « Mods retirés », intacts.
    assert_eq!(
        b.content(&server, "mods-retires/lithium-0.11.1.jar"),
        Some(lithium_old)
    );
    assert_eq!(
        b.content(&server, "mods-retires/iris-1.6.4.jar"),
        Some(iris)
    );
    assert_eq!(b.content(&server, "mods/lithium-0.11.1.jar"), None);
    assert_eq!(b.content(&server, "mods/iris-1.6.4.jar"), None);
    // Désactivé, réactivé : renommés dans `mods`, rien déplacé hors du dossier.
    assert_eq!(
        b.content(&server, "mods/carpet-1.4.112.jar.disabled"),
        Some(carpet)
    );
    assert_eq!(b.content(&server, "mods/farmer-3.0.0.jar"), Some(off));
    // Les dépôts sont consommés.
    assert!(!b.deposits.has("ada", "D-SODIUM"));
    assert!(!b.deposits.has("ada", "D-LITHIUM"));
    // La sauvegarde « Avant mods » est prête, citée par la réponse.
    let backups = b
        .agent
        .request("GET", &format!("/services/{}/backups", server.key))
        .token(&b.lucas.token)
        .send()
        .await;
    assert_eq!(backups.status, 200, "{}", backups.text);
    let id = accepted["backup_id"].as_str().unwrap();
    assert!(
        backups.body["backups"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["id"] == id),
        "{}",
        backups.text
    );
    // Le journal d'activité : des nombres et des noms sûrs, jamais un chemin.
    let journal = b.journal("service.mods").await;
    assert_eq!(journal.len(), 1, "{journal:?}");
    assert_eq!(journal[0].0, "ok");
    let target = journal[0].1.clone().unwrap_or_default();
    assert!(target.contains("1 ajouté"), "{target}");
    assert!(target.contains("1 remplacé"), "{target}");
    assert!(target.contains("1 retiré"), "{target}");
    assert!(target.contains("1 désactivé"), "{target}");
    assert!(target.contains("1 réactivé"), "{target}");
    assert!(!target.contains("data/"), "{target}");
    // L'origine : ce que Hearth a posé est « Ajouté avec Hearth ».
    let view = b.mods_view(&server).await;
    let origin_of = |file: &str| {
        view["mods"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["file"] == file)
            .unwrap_or_else(|| panic!("{file} absent de {view}"))["origin"]
            .clone()
    };
    assert_eq!(origin_of("sodium-0.5.3.jar"), "hearth");
    assert_eq!(origin_of("lithium-0.11.2.jar"), "hearth");
    assert_eq!(origin_of("carpet-1.4.112.jar.disabled"), "manual");
    assert_eq!(server.name, "mc");
}

#[tokio::test]
async fn a_stopped_server_copies_verifies_places_and_stays_stopped() {
    let b = bench().await;
    let server = b.install("mc", "fabric").await;
    b.put(&server, "mods/old.jar", &fabric("old", "1.0.0"));
    b.stop_container(&server).await;
    let calls_before = b.engine.calls();
    let (_, operation) = b.applied(&server, json!({ "retire": ["old.jar"] })).await;
    assert_eq!(
        operation.status,
        OperationStatus::Succeeded,
        "{operation:?}"
    );
    assert_eq!(steps(&operation), ["copy", "verify", "place"]);
    let calls = b.engine.calls();
    assert_eq!(started(&calls), started(&calls_before), "il reste arrêté");
    assert_eq!(stopped(&calls), stopped(&calls_before));
    assert!(b.content(&server, "mods-retires/old.jar").is_some());
    assert!(b.content(&server, "mods/old.jar").is_none());
}

// ---------------------------------------------------------------------------------------------
// Les refus synchrones : rien n'est arrêté ni écrit
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn every_synchronous_refusal_touches_nothing() {
    let b = bench().await;
    let server = b.install("mc", "fabric").await;
    b.put(&server, "mods/a.jar", &fabric("a", "1.0.0"));
    b.put(&server, "mods/a.jar.disabled", &fabric("a", "1.0.0"));
    b.put(&server, "mods/b.jar", &fabric("b", "1.0.0"));
    b.put(&server, "mods/b-off.jar.disabled", &fabric("b", "1.0.0"));
    let paths_before = b.safe_fs.paths(&server.key);
    let calls_before = b.calls_len();
    let junk = b.deposit("D-JUNK", b"ceci n'est pas une archive");
    let good = b.deposit("D-GOOD", &fabric("good", "1.0.0"));
    let mut wrong_replace = good.clone();
    wrong_replace["replace"] = json!("b.jar");
    let cases: Vec<(Value, &str)> = vec![
        (json!({}), "MODS_BATCH_INVALID"),
        (
            json!({ "retire": ["x.jar", "x.jar"] }),
            "MODS_BATCH_INVALID",
        ),
        (json!({ "retire": ["../x.jar"] }), "MODS_BATCH_INVALID"),
        (json!({ "retire": ["absent.jar"] }), "MOD_FILE_MISSING"),
        (json!({ "restore": ["absent.jar"] }), "MOD_FILE_MISSING"),
        (
            json!({ "enable": ["absent.jar.disabled"] }),
            "MOD_FILE_MISSING",
        ),
        (json!({ "add": [junk] }), "MOD_REFUSED"),
        (json!({ "add": [wrong_replace] }), "MOD_REFUSED"),
        // Le nom d'arrivée d'une désactivation est pris.
        (json!({ "disable": ["a.jar"] }), "MOD_NAME_TAKEN"),
        // Un autre fichier actif porte le même identifiant.
        (
            json!({ "enable": ["b-off.jar.disabled"] }),
            "MOD_ALREADY_ACTIVE",
        ),
    ];
    for (body, expected) in cases {
        let reply = b.apply(&server, body.clone()).await;
        assert_eq!(code(&reply), expected, "{body} : {}", reply.text);
        assert!(reply.status == 422, "{body} : {}", reply.status);
        assert_eq!(b.safe_fs.paths(&server.key), paths_before, "{body}");
        assert_eq!(
            b.calls_len(),
            calls_before,
            "{body} : le moteur n'est pas touché"
        );
    }
    // Un dépôt inconnu arrête tout.
    let reply = b
        .apply(
            &server,
            json!({ "add": [{ "deposit": "D-NONE", "sha256": sha_hex(b"x") }] }),
        )
        .await;
    assert_eq!(code(&reply), "DEPOSIT_UNKNOWN", "{}", reply.text);
    // Une empreinte qui n'est pas celle du dépôt : refusé, jamais posé.
    b.deposits.put("ada", "D-SWAP", fabric("swap", "1.0.0"));
    let reply = b
        .apply(
            &server,
            json!({ "add": [{ "deposit": "D-SWAP", "sha256": sha_hex(b"autre") }] }),
        )
        .await;
    assert_eq!(code(&reply), "MOD_REFUSED", "{}", reply.text);
    assert_eq!(b.safe_fs.paths(&server.key), paths_before);
    assert_eq!(b.calls_len(), calls_before);
}

#[tokio::test]
async fn an_unsigned_act_a_reader_and_a_server_without_mods_are_refused() {
    let b = bench().await;
    let server = b.install("mc", "fabric").await;
    b.put(&server, "mods/a.jar", &fabric("a", "1.0.0"));
    let paths_before = b.safe_fs.paths(&server.key);
    // Sans confirmation : refusé, rien ne bouge.
    let reply = b
        .agent
        .request("POST", &format!("/services/{}/mods/apply", server.key))
        .token(&b.ada.token)
        .json(&json!({ "retire": ["a.jar"] }))
        .send()
        .await;
    assert!(
        matches!(reply.status, 401 | 403 | 426),
        "{} {}",
        reply.status,
        reply.text
    );
    // Un compte sans le droit de gérer les services.
    let reply = b
        .apply_as(&b.lucas, &server, json!({ "retire": ["a.jar"] }))
        .await;
    assert!(reply.status == 403, "{}", reply.text);
    assert_eq!(b.safe_fs.paths(&server.key), paths_before);
    // Une variante sans mods.
    let vanilla = b.install("vanilla", "vanilla").await;
    let reply = b.apply(&vanilla, json!({ "retire": ["a.jar"] })).await;
    assert_eq!(code(&reply), "MODS_NOT_APPLICABLE", "{}", reply.text);
}

#[tokio::test]
async fn a_second_batch_on_the_same_server_or_another_heavy_operation_is_refused() {
    let b = bench().await;
    let first = b.install("mc", "fabric").await;
    let second = b.install("mc2", "fabric").await;
    b.put(&first, "mods/a.jar", &fabric("a", "1.0.0"));
    b.put(&second, "mods/z.jar", &fabric("z", "1.0.0"));
    // Un arrêt lent tient le serveur et la place des opérations lourdes.
    b.engine.log_on_start(&first.name, READY);
    b.engine
        .set_stop_delay(&first.container, Duration::from_millis(800));
    let reply = b.apply(&first, json!({ "retire": ["a.jar"] })).await;
    assert_eq!(reply.status, 202, "{}", reply.text);
    let id = reply.body["operation_id"].as_str().unwrap().to_owned();
    let again = b.apply(&first, json!({ "retire": ["a.jar"] })).await;
    assert_eq!(code(&again), "OPERATION_IN_PROGRESS", "{}", again.text);
    let other = b.apply(&second, json!({ "retire": ["z.jar"] })).await;
    assert_eq!(code(&other), "HEAVY_IO_BUSY", "{}", other.text);
    assert!(b.content(&second, "mods/z.jar").is_some());
    let operation = b.finished(&id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Succeeded,
        "{operation:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// L'annulation (avant la mise en place) et l'erreur au milieu de la mise en place
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn cancelling_before_the_placement_leaves_the_mods_and_restarts_the_server() {
    let b = bench().await;
    let server = b.install("mc", "fabric").await;
    b.put(&server, "mods/a.jar", &fabric("a", "1.0.0"));
    b.engine
        .set_stop_delay(&server.container, Duration::from_millis(600));
    b.engine.log_on_start(&server.name, READY);
    let calls_before = b.engine.calls();
    let reply = b.apply(&server, json!({ "retire": ["a.jar"] })).await;
    assert_eq!(reply.status, 202, "{}", reply.text);
    let id = reply.body["operation_id"].as_str().unwrap().to_owned();
    let cancel = b
        .agent
        .confirmed(
            &b.ada,
            "DELETE",
            &format!("/operations/services/{id}"),
            json!({}),
        )
        .await
        .send()
        .await;
    assert!(cancel.status < 300, "{}", cancel.text);
    let operation = b.finished(&id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Cancelled,
        "{operation:?}"
    );
    // Rien n'a bougé, et le service qui tournait est relancé.
    assert!(b.content(&server, "mods/a.jar").is_some());
    assert!(b.content(&server, "mods-retires/a.jar").is_none());
    let calls = b.engine.calls();
    assert_eq!(started(&calls) - started(&calls_before), 1);
}

#[tokio::test]
async fn an_error_in_the_middle_of_the_placement_puts_back_what_was_moved_and_restarts_the_server()
{
    let b = bench().await;
    let server = b.install("mc", "fabric").await;
    let a = fabric("a", "1.0.0");
    b.put(&server, "mods/a.jar", &a);
    b.put(&server, "mods/b.jar", &fabric("b", "1.0.0"));
    // Le premier mouvement (retirer `a`) réussit, le second (désactiver `b`) échoue.
    b.safe_fs.fail_renames_to(".jar.disabled");
    let calls_before = b.engine.calls();
    let (_, operation) = b
        .applied(
            &server,
            json!({ "retire": ["a.jar"], "disable": ["b.jar"] }),
        )
        .await;
    assert_eq!(operation.status, OperationStatus::Failed, "{operation:?}");
    assert_eq!(
        operation.cause,
        Some(hearth_proto::api::service_ops::OperationCause::StepFailed)
    );
    // Revenu : `a` est à sa place, `b` n'a pas bougé, rien n'est supprimé.
    assert_eq!(b.content(&server, "mods/a.jar"), Some(a));
    assert!(b.content(&server, "mods/b.jar").is_some());
    assert!(b.content(&server, "mods/b.jar.disabled").is_none());
    // Le service qui tournait est relancé.
    let calls = b.engine.calls();
    assert_eq!(started(&calls) - started(&calls_before), 1);
}

// ---------------------------------------------------------------------------------------------
// Un serveur qui ne démarre pas avec ses nouveaux mods revient seul à l'état d'avant (HRT-259)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_server_that_exits_before_ready_is_rolled_back_to_the_mods_of_before() {
    let b = bench().await;
    let server = b.install("mc", "fabric").await;
    let old = fabric("old", "1.0.0");
    b.put(&server, "mods/old.jar", &old);
    let new = fabric("new", "1.0.0");
    // Au prochain démarrage, le jeu s'arrête aussitôt, une seule fois : le retour le relance sain (il dit « prêt »).
    b.engine.log_once_on_start(&server.name, "Loading...");
    b.engine.log_on_start(&server.name, READY);
    b.engine.exit_once_on_start(&server.name, 1);
    let (_, operation) = b
        .applied(
            &server,
            json!({ "add": [b.deposit("D-NEW", &new)], "retire": ["old.jar"] }),
        )
        .await;
    assert_eq!(
        operation.status,
        OperationStatus::RolledBack,
        "{operation:?}"
    );
    // Les mods d'avant sont en place ; l'ajouté est dans « Mods retirés » ; rien n'est supprimé.
    assert_eq!(b.content(&server, "mods/old.jar"), Some(old));
    assert!(b.content(&server, "mods/new-1.0.0.jar").is_none());
    assert_eq!(b.content(&server, "mods-retires/new-1.0.0.jar"), Some(new));
    assert!(b.content(&server, "mods-retires/old.jar").is_none());
    // Une opération revenue est un échec dans le journal d'activité.
    let journal = b.journal("service.mods").await;
    assert_eq!(journal.len(), 1, "{journal:?}");
    assert_ne!(journal[0].0, "succeeded");
}

#[tokio::test]
async fn a_server_that_never_says_ready_before_the_deadline_is_rolled_back_too() {
    let b = bench().await;
    let server = b.install("mc", "fabric").await;
    let old = fabric("old", "1.0.0");
    b.put(&server, "mods/old.jar", &old);
    // Plus de signe « prêt » au premier démarrage : le délai de la fiche est passé sans lui ; le retour, lui, le dit.
    b.engine.log_once_on_start(&server.name, "Loading...");
    b.engine.log_on_start(&server.name, READY);
    let (_, operation) = b.applied(&server, json!({ "retire": ["old.jar"] })).await;
    assert_eq!(
        operation.status,
        OperationStatus::RolledBack,
        "{operation:?}"
    );
    assert_eq!(b.content(&server, "mods/old.jar"), Some(old));
}

async fn tail_of(b: &Bench, operation: &str) -> Wire {
    b.agent
        .request("GET", &format!("/operations/services/{operation}/tail"))
        .token(&b.ada.token)
        .send()
        .await
}

#[tokio::test]
async fn the_last_lines_of_the_game_and_the_plain_cause_are_kept_after_a_rollback() {
    let b = bench().await;
    let server = b.install("mc", "fabric").await;
    b.put(&server, "mods/old.jar", &fabric("old", "1.0.0"));
    let report =
        "Mod 'Sodium Extra' (sodium-extra) requires any version of mod sodium, which is missing!";
    b.engine.log_once_on_start(&server.name, report);
    b.engine.exit_once_on_start(&server.name, 1);
    b.engine.log_on_start(&server.name, READY);
    let (_, operation) = b.applied(&server, json!({ "retire": ["old.jar"] })).await;
    assert_eq!(
        operation.status,
        OperationStatus::RolledBack,
        "{operation:?}"
    );
    let tail = tail_of(&b, &operation.id).await;
    assert_eq!(tail.status, 200, "{}", tail.text);
    assert!(
        tail.body["lines"]
            .as_array()
            .unwrap()
            .iter()
            .any(|line| line.as_str().unwrap().contains("sodium-extra")),
        "{}",
        tail.text
    );
    assert!(tail.body["lines"].as_array().unwrap().len() <= 20);
    assert_eq!(tail.body["reason"], "missing_dependency");
    assert!(
        tail.body["sentence"]
            .as_str()
            .unwrap()
            .contains("autre mod qui est absent")
    );
    // Une opération inconnue n'a pas de queue.
    assert_eq!(tail_of(&b, "01JZ0000000000000000000099").await.status, 404);
    // La phrase n'est jamais dans le journal d'activité, les lignes non plus.
    let journal = format!("{:?}", b.journal("service.mods").await);
    assert!(!journal.contains("sodium-extra"), "{journal}");
}

#[tokio::test]
async fn an_unrecognised_failure_keeps_the_lines_without_a_cause() {
    let b = bench().await;
    let server = b.install("mc", "fabric").await;
    b.put(&server, "mods/old.jar", &fabric("old", "1.0.0"));
    b.engine
        .log_once_on_start(&server.name, "quelque chose d'inattendu");
    b.engine.exit_once_on_start(&server.name, 1);
    b.engine.log_on_start(&server.name, READY);
    let (_, operation) = b.applied(&server, json!({ "retire": ["old.jar"] })).await;
    assert_eq!(
        operation.status,
        OperationStatus::RolledBack,
        "{operation:?}"
    );
    let tail = tail_of(&b, &operation.id).await;
    assert_eq!(tail.status, 200, "{}", tail.text);
    assert!(tail.body.get("reason").is_none());
    assert!(tail.body.get("sentence").is_none());
}

#[tokio::test]
async fn a_return_that_does_not_come_up_either_is_interrupted_stops_the_game_and_relaunches_nothing()
 {
    let b = bench().await;
    let server = b.install("mc", "fabric").await;
    let old = fabric("old", "1.0.0");
    b.put(&server, "mods/old.jar", &old);
    // Le jeu ne dit jamais « prêt », ni avec les nouveaux mods ni avec les anciens.
    b.engine.log_on_start(&server.name, "Loading...");
    let (_, operation) = b.applied(&server, json!({ "retire": ["old.jar"] })).await;
    assert_eq!(
        operation.status,
        OperationStatus::Interrupted,
        "{operation:?}"
    );
    assert_eq!(
        operation.cause,
        Some(hearth_proto::api::service_ops::OperationCause::StepFailed),
        "{operation:?}"
    );
    // Les mods d'avant sont en place malgré tout, et le jeu est arrêté : rien n'est relancé seul.
    assert_eq!(b.content(&server, "mods/old.jar"), Some(old));
    b.agent.services().resync().await.expect("liste");
    let state = hearth_agent::application::ports::ContainerEngine::inspect(
        b.engine.as_ref(),
        &server.container,
    )
    .await
    .expect("moteur")
    .expect("conteneur")
    .run_state;
    assert_ne!(state, RunState::Running, "le jeu est arrêté");
    // L'arrêt du retour est un arrêt VOULU, inscrit (jamais « En erreur »).
    let intents: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM service_intents WHERE service_key = ?")
            .bind(server.key.as_str())
            .fetch_one(b.env.db.pool())
            .await
            .unwrap();
    assert_eq!(intents, 1, "arrêt voulu inscrit");
}

// ---------------------------------------------------------------------------------------------
// « Revenir à l'état d'avant » d'un lot coupé par le redémarrage de l'agent (HRT-259, revue Stephen)
// ---------------------------------------------------------------------------------------------

impl Bench {
    async fn wait_step(&self, id: &str, wanted: &str) {
        let started = Instant::now();
        loop {
            let reply = self
                .agent
                .request("GET", &format!("/operations/services/{id}"))
                .token(&self.ada.token)
                .send()
                .await;
            let operation: OperationView = serde_json::from_value(reply.body).unwrap();
            if operation.step.as_deref() == Some(wanted) {
                return;
            }
            assert!(started.elapsed() < Duration::from_secs(15), "{operation:?}");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

/// Un second agent sur la même base, le même moteur et les mêmes dossiers : le redémarrage de l'agent. Prend les pièces
/// du banc une à une pour que le test ait pu rendre le premier agent (`Agent::shutdown`) avant d'en lancer un second.
async fn restarted(
    env: &Env,
    engine: &Arc<FakeEngine>,
    safe_fs: &Arc<FakeSafeFs>,
    deposits: &Arc<FakePackDeposits>,
    fiche: &Fiche,
) -> Agent {
    let mut parts = parts_supervised(
        engine.clone(),
        Arc::new(FakeSupervisor::new()),
        fast_timings(),
    );
    parts.host_ports = Arc::new(FakeHostPorts::new());
    parts.free_space = Arc::new(FakeFreeSpace::new());
    parts.safe_fs = Some(safe_fs.clone());
    parts.backups.files = Some(Arc::new(FakeBackupFiles::new()));
    parts.catalog.include_preview = true;
    parts.catalog.pack_deposits = Some(deposits.clone());
    parts.catalog.service = Some(Arc::new(CatalogService::from_parts(
        vec![fiche.clone()],
        Vec::new(),
        true,
    )));
    https::start_engined(env, parts).await
}

#[tokio::test]
async fn reverting_a_batch_whose_undo_was_cut_in_the_middle_finishes_the_return_and_closes_the_operation()
 {
    use hearth_agent::application::ports::SafeFs;
    use hearth_agent::domain::files::RelPath;
    let b = bench().await;
    let server = b.install("mc", "fabric").await;
    let old = fabric("old", "1.0.0");
    let new = fabric("new", "1.0.0");
    b.put(&server, "mods/old.jar", &old);
    // Le jeu ne dit jamais « prêt » : le lot reste dans la surveillance, l'agent redémarre à ce moment.
    b.engine.log_on_start(&server.name, "Loading...");
    let reply = b
        .apply(
            &server,
            json!({ "add": [b.deposit("D-NEW", &new)], "retire": ["old.jar"] }),
        )
        .await;
    assert_eq!(reply.status, 202, "{}", reply.text);
    let id = reply.body["operation_id"].as_str().unwrap().to_owned();
    b.wait_step(&id, "watch").await;
    // L'agent d'avant s'arrête AVANT que le second ne démarre : jamais deux agents sur la même base.
    b.agent.shutdown().await;
    let second = restarted(&b.env, &b.engine, &b.safe_fs, &b.deposits, &b.fiche).await;
    // Avec les anciens mods, le jeu redit « prêt ».
    b.engine.log_on_start(&server.name, READY);
    // Le retour de l'agent d'avant avait déjà rendu le premier mouvement (l'ancien jar) avant d'être coupé.
    let root = b.safe_fs.open_service_root(&server.key).expect("racine");
    root.rename_noreplace(
        &RelPath::parse("data/mods-retires/old.jar").unwrap(),
        &RelPath::parse("data/mods/old.jar").unwrap(),
    )
    .expect("premier mouvement déjà revenu");
    let reply = second
        .confirmed(
            &b.ada,
            "POST",
            &format!("/operations/services/{id}/decision"),
            json!({ "choice": "revert" }),
        )
        .await
        .send()
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let started = Instant::now();
    loop {
        let reply = second
            .request("GET", &format!("/operations/services/{id}"))
            .token(&b.ada.token)
            .send()
            .await;
        let operation: OperationView = serde_json::from_value(reply.body).unwrap();
        if operation.status == OperationStatus::RolledBack {
            break;
        }
        assert!(
            !matches!(operation.status, OperationStatus::Interrupted),
            "le retour doit finir : {operation:?}"
        );
        assert!(started.elapsed() < Duration::from_secs(15), "{operation:?}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        b.safe_fs.content(&server.key, "data/mods/old.jar"),
        Some(old)
    );
    let paths = b.safe_fs.paths(&server.key);
    assert!(
        !paths
            .iter()
            .any(|p| p.starts_with("data/mods/") && p.contains("new")),
        "{paths:?}"
    );
    assert!(
        paths
            .iter()
            .any(|p| p.starts_with("data/mods-retires/") && p.contains("new")),
        "le fichier posé est rangé dans Mods retirés : {paths:?}"
    );
}

#[tokio::test]
async fn a_return_whose_stop_fails_moves_nothing_and_leaves_the_operation_interrupted() {
    let b = bench().await;
    let server = b.install("mc", "fabric").await;
    let old = fabric("old", "1.0.0");
    let new = fabric("new", "1.0.0");
    b.put(&server, "mods/old.jar", &old);
    b.engine.log_on_start(&server.name, "Loading...");
    let reply = b
        .apply(
            &server,
            json!({ "add": [b.deposit("D-NEW", &new)], "retire": ["old.jar"] }),
        )
        .await;
    assert_eq!(reply.status, 202, "{}", reply.text);
    let id = reply.body["operation_id"].as_str().unwrap().to_owned();
    b.wait_step(&id, "watch").await;
    b.agent.shutdown().await;
    let second = restarted(&b.env, &b.engine, &b.safe_fs, &b.deposits, &b.fiche).await;
    let before = b.safe_fs.paths(&server.key);
    let renames_before = b.safe_fs.renames().len();
    // Le jeu tourne : l'arrêt voulu avant le retour est refusé par le moteur.
    b.engine.fail_stop(
        &server.container,
        Some(hearth_agent::application::ports::EngineError::Refused),
    );
    let reply = second
        .confirmed(
            &b.ada,
            "POST",
            &format!("/operations/services/{id}/decision"),
            json!({ "choice": "revert" }),
        )
        .await
        .send()
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let started = Instant::now();
    let operation = loop {
        let reply = second
            .request("GET", &format!("/operations/services/{id}"))
            .token(&b.ada.token)
            .send()
            .await;
        let operation: OperationView = serde_json::from_value(reply.body).unwrap();
        if matches!(
            operation.status,
            OperationStatus::Interrupted | OperationStatus::RolledBack
        ) {
            break operation;
        }
        assert!(started.elapsed() < Duration::from_secs(15), "{operation:?}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert_eq!(
        operation.status,
        OperationStatus::Interrupted,
        "{operation:?}"
    );
    assert_eq!(b.safe_fs.paths(&server.key), before, "data/mods inchangé");
    assert_eq!(
        b.safe_fs.renames().len(),
        renames_before,
        "aucun renommage après l'arrêt raté"
    );
}

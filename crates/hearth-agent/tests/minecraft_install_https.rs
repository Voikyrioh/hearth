//! Installer Minecraft Java depuis SA fiche embarquée, de bout en bout (HRT-116, ADR-0050, ADR-0054) : vrai agent en
//! HTTPS, vraie base, faux moteur, faux dossiers de données, fausse zone d'attente. Le scénario qui compte est celui
//! de Prominence II : un pack serveur Fabric 1.20.1 déposé, une grosse mémoire, le contrat accepté par un geste.
//! Rien ici ne lance un vrai serveur : la fiche a été essayée sur ses vraies images par `tests/fiche_live_minecraft.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use hearth_agent::application::catalog::CatalogService;
use hearth_agent::application::ports::EngineInfo;
use hearth_agent::domain::accounts::Role;
use hearth_agent::domain::catalog::loading::model::Fiche;
use hearth_agent::domain::catalog::{Origin, load};
use hearth_agent::domain::services::ServiceKey;
use hearth_proto::api::deploy::DeployAccepted;
use hearth_proto::api::service_ops::{OperationStatus, OperationView};
use serde_json::{Value, json};
use support::device::Actor;
use support::engine::{fast_timings, parts_supervised};
use support::fake_engine::{FakeEngine, PullScript};
use support::fake_host::{FakeFreeSpace, FakeHostPorts};
use support::fake_pack_deposits::{FakePackDeposits, zip_of};
use support::fake_safe_fs::FakeSafeFs;
use support::fake_supervisor::FakeSupervisor;
use support::https::{self, Agent, Wire};
use support::{Env, env};

const GIB: u64 = 1024 * 1024 * 1024;
const FICHE: &str = include_str!("../catalog/minecraft/fiche.toml");
const SETTINGS: &str = include_str!("../catalog/minecraft/settings.toml");

fn minecraft() -> Fiche {
    load(Origin::Embedded, FICHE.as_bytes(), SETTINGS.as_bytes()).expect("fiche minecraft")
}

/// La référence de l'image telle que le moteur la reçoit (version et empreinte épinglée) pour une variante.
fn reference(fiche: &Fiche, variant: &str) -> String {
    let v = fiche
        .file
        .variant
        .iter()
        .find(|v| v.id == variant)
        .expect("variante");
    let tag = v.image_tag.clone().unwrap_or(fiche.file.image.tag.clone());
    let digest = v
        .image_digest
        .clone()
        .or(fiche.file.image.digest.clone())
        .expect("empreinte");
    format!("{}:{tag}@{digest}", fiche.file.image.repository)
}

struct Bench {
    env: Env,
    engine: Arc<FakeEngine>,
    safe_fs: Arc<FakeSafeFs>,
    deposits: Arc<FakePackDeposits>,
    agent: Agent,
    fiche: Fiche,
    ada: Actor,
}

async fn bench() -> Bench {
    let fiche = minecraft();
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
    parts.catalog.include_preview = true;
    parts.catalog.pack_deposits = Some(deposits.clone());
    parts.catalog.service = Some(Arc::new(CatalogService::from_parts(
        vec![fiche.clone()],
        Vec::new(),
        true,
    )));
    let agent = https::start_engined(&env, parts).await;
    let ada = agent.actor(&env, "ada", Role::Admin).await;
    env.grant_services("ada").await;
    Bench {
        env,
        engine,
        safe_fs,
        deposits,
        agent,
        fiche,
        ada,
    }
}

impl Bench {
    fn request(
        &self,
        name: &str,
        variant: Option<&str>,
        pack: Option<&str>,
        accept: bool,
    ) -> Value {
        let mut source = json!({
            "type": "fiche",
            "id": self.fiche.id(),
            "version": self.fiche.file.version,
            "digest": self.fiche.digest_hex(),
            "agreements": if accept { json!(["eula"]) } else { json!([]) },
        });
        if let Some(variant) = variant {
            source["variant"] = json!(variant);
        }
        if let Some(pack) = pack {
            source["pack"] = json!({ "deposit": pack });
        }
        json!({
            "name": name,
            // Prominence II : une grosse mémoire, choisie au curseur de l'installation.
            "resources": { "cpu_millis": 4000, "memory_bytes": 10 * GIB },
            "source": source,
        })
    }

    async fn install(&self, body: Value) -> Wire {
        self.agent
            .confirmed(&self.ada, "POST", "/deployments", body)
            .await
            .send()
            .await
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
                started.elapsed() < Duration::from_secs(15),
                "l'installation ne finit pas : {operation:?}"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    async fn assert_nothing_created(&self) {
        let specs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM service_specs")
            .fetch_one(self.env.db.pool())
            .await
            .unwrap();
        assert_eq!(specs, 0, "description");
        assert!(self.engine.created().is_empty(), "conteneur");
    }
}

fn prominence_like_pack() -> Vec<u8> {
    zip_of(&[
        ("mods/a.jar", b"jar-a"),
        ("mods/b.jar", b"jar-b"),
        ("config/a.toml", b"x = 1"),
        ("defaultconfigs/b.toml", b"y = 2"),
        ("start.sh", b"#!/bin/sh"),
    ])
}

#[tokio::test]
async fn a_fabric_server_pack_like_prominence_ii_installs_with_a_big_memory_and_a_read_only_pack_folder()
 {
    let b = bench().await;
    let image = reference(&b.fiche, "fabric");
    assert!(image.contains(":java17@sha256:"), "{image}");
    b.engine.publish_image(&image, PullScript::two_layers());
    let bytes = prominence_like_pack();
    b.deposits.put("ada", "prominence", bytes.clone());
    // Lire d'abord : le client montre le genre lu par l'agent avant de lancer.
    let check = b
        .agent
        .request("POST", "/deployments/pack-check")
        .token(&b.ada.token)
        .json(&json!({ "deposit": "prominence" }))
        .send()
        .await;
    assert_eq!(check.status, 200, "{}", check.text);
    assert_eq!(check.body["genre"], "server_pack");
    assert_eq!(check.body["mods"], 2);
    // Installer : Fabric, Minecraft 1.20.1, le contrat accepté par le geste.
    let mut body = b.request("prominence", Some("fabric"), Some("prominence"), true);
    body["source"]["answers"] = json!([{ "id": "version", "value": "1.20.1" }]);
    let reply = b.install(body).await;
    assert_eq!(reply.status, 202, "{}", reply.text);
    let accepted: DeployAccepted = serde_json::from_value(reply.body).unwrap();
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Succeeded,
        "{operation:?}"
    );
    let plan = &b.engine.created()[0];
    // L'image Java 17, par l'empreinte ; la mémoire choisie ; le port du jeu seulement.
    assert_eq!(plan.image, image);
    assert_eq!((plan.cpu_millis, plan.memory_bytes), (4000, 10 * GIB));
    assert_eq!(plan.ports.len(), 1);
    assert_eq!((plan.ports[0].0, plan.ports[0].1), (25565, 25565));
    // Ce que l'image reçoit.
    let value = |name: &str| {
        plan.env
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.clone())
    };
    assert_eq!(value("TYPE").as_deref(), Some("FABRIC"));
    assert_eq!(value("VERSION").as_deref(), Some("1.20.1"));
    assert_eq!(value("EULA").as_deref(), Some("TRUE"));
    assert_eq!(value("GENERIC_PACK").as_deref(), Some("/packs/pack.zip"));
    assert_eq!(value("MEMORY").as_deref(), Some(""));
    assert_eq!(
        value("JVM_XX_OPTS").as_deref(),
        Some("-XX:MaxRAMPercentage=75")
    );
    assert_eq!(value("STOP_DURATION").as_deref(), Some("180"));
    assert!(
        value("RCON_PASSWORD").is_some_and(|p| p.len() >= 24),
        "mot de passe tiré par Hearth"
    );
    assert!(plan.env.iter().all(|(n, _)| !n.starts_with("CF_")));
    // Le pack est rangé tel quel, en lecture seule, et le dépôt est consommé.
    let key = ServiceKey::parse(&accepted.service_key).unwrap();
    assert_eq!(b.safe_fs.content(&key, "packs/pack.zip"), Some(bytes));
    assert!(!b.deposits.has("ada", "prominence"));
    let packs = plan
        .binds
        .iter()
        .find(|(_, target, _)| target == "/packs")
        .expect("dossier des packs");
    assert!(packs.2, "lecture seule");
    let data = plan
        .binds
        .iter()
        .find(|(_, target, _)| target == "/data")
        .expect("dossier des données");
    assert!(!data.2, "données inscriptibles");
    b.agent.shutdown().await;
}

#[tokio::test]
async fn without_the_contract_gesture_nothing_is_created_and_the_pack_stays_for_the_next_try() {
    let b = bench().await;
    b.deposits.put("ada", "p", prominence_like_pack());
    let reply = b
        .install(b.request("sans-contrat", Some("fabric"), Some("p"), false))
        .await;
    assert_eq!(reply.status, 422, "{}", reply.text);
    assert_eq!(reply.body["error"]["code"], "AGREEMENT_REQUIRED");
    b.assert_nothing_created().await;
    assert!(b.deposits.has("ada", "p"));
    b.agent.shutdown().await;
}

#[tokio::test]
async fn the_default_install_is_the_official_server_with_java_25_and_no_pack() {
    let b = bench().await;
    let image = reference(&b.fiche, "vanilla");
    assert!(image.contains(":java25@sha256:"), "{image}");
    b.engine.publish_image(&image, PullScript::two_layers());
    let reply = b.install(b.request("officiel", None, None, true)).await;
    assert_eq!(reply.status, 202, "{}", reply.text);
    let accepted: DeployAccepted = serde_json::from_value(reply.body).unwrap();
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Succeeded,
        "{operation:?}"
    );
    let plan = &b.engine.created()[0];
    let value = |name: &str| {
        plan.env
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.clone())
    };
    assert_eq!(value("TYPE").as_deref(), Some("VANILLA"));
    assert_eq!(value("VERSION").as_deref(), Some("LATEST"));
    assert!(value("GENERIC_PACK").is_none());
    b.agent.shutdown().await;
}

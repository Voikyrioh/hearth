//! Installer un serveur de jeu avec un pack de mods déposé, de bout en bout (HRT-124, ADR-0054, ADR-0055) : vrai
//! agent en HTTPS, vraie base, faux moteur, faux dossiers de données, fausse zone d'attente. Règles : BR-MOD-007,
//! BR-MOD-008, BR-MOD-012, BR-MOD-016, BR-MOD-017.
//!
//! Ce que les tests prouvent : le genre est lu dans le CONTENU, un pack refusé est retiré aussitôt et ne crée rien,
//! un pack accepté est rangé sous un nom FIXE dans un dossier monté en LECTURE SEULE, et le chemin passé au programme
//! du jeu est celui de ce fichier, jamais un nom venu du client.

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
const RICH_FICHE: &str = include_str!("fixtures/rich/fiche.toml");
const RICH_SETTINGS: &str = include_str!("fixtures/rich/settings.toml");

const PACKS_FOLDER: &str = "[[folder]]\nid = \"packs\"\nrole = \"packs\"\ntarget = \"/packs\"\nread_only = true\nbackup = false\n\n";
const MODS_BLOCK: &str = "[mods]\nways = [\"drop_files\", \"drop_pack\"]\n\n[mods.pack]\nfolder = \"packs\"\n\n[[mods.pack.genre]]\nkind = \"server_pack\"\nenv = \"GENERIC_PACK\"\nvariants = [\"java\"]\n\n[[mods.pack.genre]]\nkind = \"modrinth_pack\"\nenv = \"MODRINTH_MODPACK\"\nvariants = [\"bedrock\"]\n";

/// La fiche riche (Java par défaut, Bedrock en variante) avec un dossier de packs et le bloc `[mods]`.
fn with_mods() -> Fiche {
    let text = RICH_FICHE.replacen(
        "[[folder]]\nid = \"mods\"",
        &format!("{PACKS_FOLDER}[[folder]]\nid = \"mods\""),
        1,
    );
    load(
        Origin::Embedded,
        format!("{text}\n{MODS_BLOCK}\n").as_bytes(),
        RICH_SETTINGS.as_bytes(),
    )
    .expect("fiche valide")
}

fn without_mods() -> Fiche {
    load(
        Origin::Embedded,
        RICH_FICHE.as_bytes(),
        RICH_SETTINGS.as_bytes(),
    )
    .expect("fiche valide")
}

struct Bench {
    env: Env,
    engine: Arc<FakeEngine>,
    safe_fs: Arc<FakeSafeFs>,
    deposits: Arc<FakePackDeposits>,
    agent: Agent,
    fiche: Fiche,
    ada: Actor,
    /// Administrateur SANS le droit de gérer les services.
    bob: Actor,
}

async fn bench(fiche: Fiche) -> Bench {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    engine.set_info(Some(EngineInfo {
        cpus: 8,
        memory_bytes: 16 * GIB,
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
    let bob = agent.actor(&env, "bob", Role::Admin).await;
    env.grant_services("ada").await;
    Bench {
        env,
        engine,
        safe_fs,
        deposits,
        agent,
        fiche,
        ada,
        bob,
    }
}

impl Bench {
    fn request(&self, name: &str, pack: Option<&str>, variant: Option<&str>) -> Value {
        let agreements: Vec<String> = self
            .fiche
            .file
            .agreement
            .iter()
            .map(|a| a.id.clone())
            .collect();
        let mut source = json!({
            "type": "fiche",
            "id": self.fiche.id(),
            "version": self.fiche.file.version,
            "digest": self.fiche.digest_hex(),
            "agreements": agreements,
        });
        if let Some(pack) = pack {
            source["pack"] = json!({ "deposit": pack });
        }
        if let Some(variant) = variant {
            source["variant"] = json!(variant);
        }
        json!({
            "name": name,
            "resources": { "cpu_millis": 1000, "memory_bytes": 2 * GIB },
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

    async fn pack_check(&self, who: &Actor, deposit: &str) -> Wire {
        self.agent
            .request("POST", "/deployments/pack-check")
            .token(&who.token)
            .json(&json!({ "deposit": deposit }))
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

fn server_pack() -> Vec<u8> {
    zip_of(&[
        ("mods/a.jar", b"jar-a"),
        ("mods/b.jar", b"jar-b"),
        ("config/a.toml", b"x = 1"),
        ("start.sh", b"#!/bin/sh"),
    ])
}

const INDEX: &str = r#"{"formatVersion":1,"game":"minecraft","versionId":"1.2","name":"Mon pack",
    "files":[{"path":"mods/c.jar","downloads":["https://cdn.modrinth.com/data/x/c.jar"]}],
    "dependencies":{"minecraft":"1.20.1","fabric-loader":"0.15.0"}}"#;

// ---------------------------------------------------------------------------------------------
// Lire un pack avant d'installer (BR-MOD-007)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn the_agent_reads_the_kind_of_a_pack_from_its_content_and_nothing_is_stored() {
    let b = bench(with_mods()).await;
    // Le nom du dépôt ne dit rien : c'est le contenu.
    b.deposits.put("ada", "d-server", server_pack());
    let reply = b.pack_check(&b.ada, "d-server").await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["genre"], "server_pack");
    assert_eq!(
        (reply.body["files"].clone(), reply.body["mods"].clone()),
        (json!(4), json!(2))
    );
    assert_eq!(reply.body["top_dirs"], json!(["config", "mods"]));
    b.deposits.put(
        "ada",
        "d-mrpack",
        zip_of(&[("modrinth.index.json", INDEX.as_bytes())]),
    );
    let reply = b.pack_check(&b.ada, "d-mrpack").await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["genre"], "modrinth_pack");
    assert_eq!(reply.body["modrinth"]["minecraft"], "1.20.1");
    assert_eq!(reply.body["modrinth"]["loader"], "fabric-loader");
    assert_eq!(reply.body["modrinth"]["downloads"], 1);
    assert_eq!(reply.body["modrinth"]["hosts"], json!(["cdn.modrinth.com"]));
    // Une consultation : les dépôts sont toujours là, rien n'est rangé dans un service.
    assert!(b.deposits.has("ada", "d-server") && b.deposits.has("ada", "d-mrpack"));
    b.assert_nothing_created().await;
    b.agent.shutdown().await;
}

#[tokio::test]
async fn reading_a_pack_needs_the_right_to_manage_services_and_a_deposit_of_ones_own() {
    let b = bench(with_mods()).await;
    b.deposits.put("ada", "d1", server_pack());
    // Sans le droit : refusé, le dépôt n'est pas touché.
    let reply = b.pack_check(&b.bob, "d1").await;
    assert_eq!(reply.status, 403, "{}", reply.text);
    assert!(b.deposits.has("ada", "d1"));
    // Le dépôt d'un autre compte n'existe pas pour celui-ci, et n'est jamais retiré.
    b.env.grant_services("bob").await;
    let reply = b.pack_check(&b.bob, "d1").await;
    assert_eq!(
        (reply.status, code(&reply)),
        (404, "PACK_UNKNOWN".to_owned())
    );
    assert!(b.deposits.has("ada", "d1"));
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_curseforge_download_list_is_refused_with_its_mod_count_and_removed_at_once() {
    let b = bench(with_mods()).await;
    let manifest = br#"{"files":[{"projectID":1,"fileID":2},{"projectID":3,"fileID":4}]}"#;
    b.deposits.put(
        "ada",
        "cf",
        zip_of(&[("manifest.json", manifest), ("overrides/options.txt", b"x")]),
    );
    let reply = b.pack_check(&b.ada, "cf").await;
    assert_eq!(
        (reply.status, code(&reply)),
        (422, "PACK_REFUSED".to_owned())
    );
    assert_eq!(reason(&reply), "PACK_IS_A_DOWNLOAD_LIST");
    assert_eq!(reply.body["error"]["details"]["mods"], 2);
    assert!(
        !b.deposits.has("ada", "cf"),
        "retiré de la zone d'attente aussitôt"
    );
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_hostile_archive_and_an_unlisted_modrinth_host_are_refused_with_a_fixed_reason() {
    let b = bench(with_mods()).await;
    b.deposits.put(
        "ada",
        "slip",
        zip_of(&[("mods/a.jar", b"j"), ("../evil", b"x")]),
    );
    let reply = b.pack_check(&b.ada, "slip").await;
    assert_eq!(
        (reply.status, code(&reply)),
        (422, "PACK_REFUSED".to_owned())
    );
    assert_eq!(reason(&reply), "ARCHIVE_REFUSED");
    assert!(
        !reply.text.contains("evil"),
        "aucun nom d'entrée dans la réponse : {}",
        reply.text
    );
    let evil = INDEX.replace("cdn.modrinth.com", "evil.example");
    b.deposits.put(
        "ada",
        "host",
        zip_of(&[("modrinth.index.json", evil.as_bytes())]),
    );
    let reply = b.pack_check(&b.ada, "host").await;
    assert_eq!(reason(&reply), "PACK_HOST_FORBIDDEN");
    assert_eq!(reply.body["error"]["details"]["host"], "evil.example");
    assert_eq!(
        b.deposits.discarded(),
        vec!["slip".to_owned(), "host".to_owned()]
    );
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Installer avec un pack (BR-MOD-007, BR-MOD-012, BR-MOD-017)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_server_pack_is_stored_under_a_fixed_name_in_a_read_only_folder_and_its_path_goes_to_the_game()
 {
    let b = bench(with_mods()).await;
    b.engine
        .publish_image("example/rich:2.0", PullScript::two_layers());
    let bytes = server_pack();
    b.deposits.put("ada", "d-server", bytes.clone());
    let reply = b
        .install(b.request("avec-pack", Some("d-server"), None))
        .await;
    assert_eq!(reply.status, 202, "{}", reply.text);
    let accepted: DeployAccepted = serde_json::from_value(reply.body).unwrap();
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Succeeded,
        "{operation:?}"
    );
    let key = ServiceKey::parse(&accepted.service_key).unwrap();
    // Le pack est rangé tel quel, sous un nom FIXE, et le dépôt est consommé.
    assert_eq!(b.safe_fs.content(&key, "packs/pack.zip"), Some(bytes));
    assert!(!b.deposits.has("ada", "d-server"));
    // Le programme du jeu reçoit le chemin de CE fichier, par la variable que la fiche nomme pour ce genre.
    let plan = &b.engine.created()[0];
    assert!(
        plan.env
            .iter()
            .any(|(n, v)| n == "GENERIC_PACK" && v == "/packs/pack.zip"),
        "{:?}",
        plan.env.iter().map(|(n, _)| n).collect::<Vec<_>>()
    );
    // Le dossier des packs est monté en LECTURE SEULE, sous le dossier géré de CE service.
    let bind = plan
        .binds
        .iter()
        .find(|(_, target, _)| target == "/packs")
        .expect("dossier des packs monté");
    assert!(bind.2, "lecture seule");
    assert!(bind.0.contains(&accepted.service_key), "{}", bind.0);
    // Aucune variable de la famille CF_ ni type AUTO_CURSEFORGE nulle part.
    assert!(
        plan.env
            .iter()
            .all(|(n, v)| !n.starts_with("CF_") && v != "AUTO_CURSEFORGE")
    );
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_modrinth_pack_goes_to_the_game_by_its_own_variable_with_the_matching_variant() {
    let b = bench(with_mods()).await;
    b.engine
        .publish_image("example/rich:2.0", PullScript::two_layers());
    b.engine
        .publish_image("example/rich:bedrock-2.0", PullScript::two_layers());
    b.deposits.put(
        "ada",
        "mr",
        zip_of(&[("modrinth.index.json", INDEX.as_bytes())]),
    );
    let reply = b
        .install(b.request("mr", Some("mr"), Some("bedrock")))
        .await;
    assert_eq!(reply.status, 202, "{}", reply.text);
    let accepted: DeployAccepted = serde_json::from_value(reply.body).unwrap();
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Succeeded,
        "{operation:?}"
    );
    let key = ServiceKey::parse(&accepted.service_key).unwrap();
    assert!(b.safe_fs.content(&key, "packs/pack.mrpack").is_some());
    let plan = &b.engine.created()[0];
    assert!(
        plan.env
            .iter()
            .any(|(n, v)| n == "MODRINTH_MODPACK" && v == "/packs/pack.mrpack")
    );
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_pack_with_a_variant_the_fiche_does_not_allow_for_its_kind_creates_nothing_and_keeps_the_deposit()
 {
    let b = bench(with_mods()).await;
    // Un pack serveur avec la variante Bedrock : la fiche ne le permet pas.
    b.deposits.put("ada", "d1", server_pack());
    let reply = b
        .install(b.request("mauvais-moteur", Some("d1"), Some("bedrock")))
        .await;
    assert_eq!(reply.status, 422, "{}", reply.text);
    assert_eq!(reply.body["error"]["details"]["field"], "source.pack");
    b.assert_nothing_created().await;
    assert!(
        b.deposits.has("ada", "d1"),
        "l'utilisateur peut corriger sans redéposer"
    );
    b.agent.shutdown().await;
}

#[tokio::test]
async fn an_install_with_a_pack_is_refused_whole_when_the_pack_is_refused_or_unknown_or_not_offered()
 {
    let b = bench(with_mods()).await;
    // Inconnu.
    let reply = b.install(b.request("x", Some("nope"), None)).await;
    assert_eq!(
        (reply.status, code(&reply)),
        (404, "PACK_UNKNOWN".to_owned())
    );
    // Refusé : rien n'est créé, le dépôt est retiré.
    b.deposits
        .put("ada", "junk", zip_of(&[("readme.txt", b"x")]));
    let reply = b.install(b.request("y", Some("junk"), None)).await;
    assert_eq!(
        (reply.status, code(&reply)),
        (422, "PACK_REFUSED".to_owned())
    );
    assert_eq!(reason(&reply), "PACK_UNRECOGNIZED");
    assert!(!b.deposits.has("ada", "junk"));
    b.assert_nothing_created().await;
    b.agent.shutdown().await;

    // Une fiche qui n'offre aucune voie de pack refuse tout pack, sans le ranger.
    let plain = bench(without_mods()).await;
    plain.deposits.put("ada", "d1", server_pack());
    let reply = plain
        .install(plain.request("sans-voie", Some("d1"), None))
        .await;
    assert_eq!(reply.status, 422, "{}", reply.text);
    assert_eq!(reply.body["error"]["details"]["field"], "source.pack");
    plain.assert_nothing_created().await;
    plain.agent.shutdown().await;
}

#[tokio::test]
async fn an_install_without_a_pack_is_unchanged() {
    let b = bench(with_mods()).await;
    b.engine
        .publish_image("example/rich:2.0", PullScript::two_layers());
    let reply = b.install(b.request("sans-pack", None, None)).await;
    assert_eq!(reply.status, 202, "{}", reply.text);
    let accepted: DeployAccepted = serde_json::from_value(reply.body).unwrap();
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Succeeded,
        "{operation:?}"
    );
    let plan = &b.engine.created()[0];
    assert!(plan.env.iter().all(|(n, _)| n != "GENERIC_PACK"));
    b.agent.shutdown().await;
}

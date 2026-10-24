//! Installer un serveur de jeu depuis une fiche, de bout en bout (HRT-101, ADR-0050, ADR-0044) : vrai agent en
//! HTTPS, vraie base, faux moteur, faux dossiers de données, faux hôte, faux hasard. Règles : BR-FICHE-012, 013,
//! 017 à 024, 029, 032, 058, BR-DEPLOY-035 et 038. La preuve de clé et le mot de passe sont dans la matrice de
//! `admin_reauth.rs` ; ici : ce que fait la route et le cas d'usage, jusqu'au journal d'activité, à la base et
//! aux fichiers produits. Les secrets sont dans `fiche_install_secrets.rs`.

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
use support::fake_safe_fs::FakeSafeFs;
use support::fake_supervisor::FakeSupervisor;
use support::https::{self, Agent, Wire};
use support::{Env, env};

const GIB: u64 = 1024 * 1024 * 1024;
const RICH_FICHE: &str = include_str!("fixtures/rich/fiche.toml");
const RICH_SETTINGS: &str = include_str!("fixtures/rich/settings.toml");
const TERRARIA_FICHE: &str = include_str!("../catalog/terraria/fiche.toml");
const TERRARIA_SETTINGS: &str = include_str!("../catalog/terraria/settings.toml");
/// Une empreinte d'image valable pour le chargeur (`sha256:` et 64 chiffres hexadécimaux minuscules).
const IMAGE_DIGEST: &str =
    "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
/// Une ligne du journal : résultat, cible, raison, compte.
type JournalRow = (String, Option<String>, Option<String>, Option<String>);

fn rich_with(edit: impl Fn(&str) -> String) -> Fiche {
    load(
        Origin::Embedded,
        edit(RICH_FICHE).as_bytes(),
        RICH_SETTINGS.as_bytes(),
    )
    .expect("fiche de fabrication valide")
}

fn rich() -> Fiche {
    rich_with(str::to_owned)
}

/// Terraria, l'empreinte de l'image épinglée en plus.
fn pinned_terraria() -> Fiche {
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
    .expect("Terraria épinglée valide")
}

struct Bench {
    env: Env,
    engine: Arc<FakeEngine>,
    free_space: Arc<FakeFreeSpace>,
    safe_fs: Arc<FakeSafeFs>,
    agent: Agent,
    fiche: Fiche,
    /// Administrateur avec le droit de gérer les services.
    ada: Actor,
    /// Administrateur SANS le droit.
    bob: Actor,
    /// Lecture seule SANS le droit.
    lucas: Actor,
    /// Lecture seule AVEC le droit.
    dan: Actor,
}

async fn bench(fiche: Fiche, include_preview: bool) -> Bench {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    engine.set_info(Some(EngineInfo {
        cpus: 8,
        memory_bytes: 16 * GIB,
        storage_dir: Some("/var/lib/docker".into()),
    }));
    let free_space = Arc::new(FakeFreeSpace::new());
    let safe_fs = Arc::new(FakeSafeFs::new());
    let mut parts = parts_supervised(
        engine.clone(),
        Arc::new(FakeSupervisor::new()),
        fast_timings(),
    );
    parts.host_ports = Arc::new(FakeHostPorts::new());
    parts.free_space = free_space.clone();
    parts.safe_fs = Some(safe_fs.clone());
    parts.catalog.include_preview = include_preview;
    parts.catalog.service = Some(Arc::new(CatalogService::from_parts(
        vec![fiche.clone()],
        Vec::new(),
        include_preview,
    )));
    let agent = https::start_engined(&env, parts).await;
    let ada = agent.actor(&env, "ada", Role::Admin).await;
    let bob = agent.actor(&env, "bob", Role::Admin).await;
    let lucas = agent.actor(&env, "lucas", Role::ReadOnly).await;
    let dan = agent.actor(&env, "dan", Role::ReadOnly).await;
    env.grant_services("ada").await;
    env.grant_services("dan").await;
    Bench {
        env,
        engine,
        free_space,
        safe_fs,
        agent,
        fiche,
        ada,
        bob,
        lucas,
        dan,
    }
}

impl Bench {
    /// Une demande valable : le nom, les limites, la fiche telle que l'agent la propose, le contrat accepté.
    fn request(&self, name: &str) -> Value {
        let agreements: Vec<String> = self
            .fiche
            .file
            .agreement
            .iter()
            .map(|a| a.id.clone())
            .collect();
        json!({
            "name": name,
            "resources": { "cpu_millis": 1000, "memory_bytes": 2 * GIB },
            "source": {
                "type": "fiche",
                "id": self.fiche.id(),
                "version": self.fiche.file.version,
                "digest": self.fiche.digest_hex(),
                "agreements": agreements,
            }
        })
    }

    async fn install_as(&self, who: &Actor, body: Value) -> Wire {
        self.agent
            .confirmed(who, "POST", "/deployments", body)
            .await
            .send()
            .await
    }

    async fn install(&self, body: Value) -> Wire {
        self.install_as(&self.ada, body).await
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
                .token(&self.dan.token)
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

    async fn journal(&self) -> Vec<JournalRow> {
        self.agent.flush_journal().await;
        sqlx::query_as(
            "SELECT outcome, target, reason, account FROM audit_events
             WHERE action = 'service.deploy' ORDER BY id",
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

    /// Rien n'a été créé : ni description, ni conteneur, ni racine de service.
    async fn assert_nothing_created(&self) {
        assert_eq!(self.specs_count().await, 0, "description");
        assert!(self.engine.created().is_empty(), "conteneur");
        assert!(self.engine.pulls().is_empty(), "image récupérée");
    }
}

fn code(wire: &Wire) -> String {
    wire.body["error"]["code"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

fn key_of(accepted: &DeployAccepted) -> ServiceKey {
    ServiceKey::parse(&accepted.service_key).expect("clé")
}

// ---------------------------------------------------------------------------------------------
// Une installation qui réussit (BR-FICHE-013, 024, 029, 032)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_fiche_install_creates_a_service_with_the_fiche_digest_the_pinned_image_and_a_0600_root_config_file()
 {
    let b = bench(pinned_terraria(), true).await;
    let pinned = format!("ryshe/terraria:vanilla-1.4.5.8@{IMAGE_DIGEST}");
    b.engine.publish_image(&pinned, PullScript::two_layers());
    let accepted = b.accepted(b.request("mon-terraria")).await;
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Succeeded,
        "{operation:?}"
    );
    // L'image est créée PAR SON EMPREINTE, jamais par la seule version.
    assert_eq!(b.engine.pulls(), vec![pinned.clone()]);
    let created = b.engine.created();
    assert_eq!(created.len(), 1);
    let plan = &created[0];
    assert_eq!(plan.image, pinned);
    assert_eq!((plan.cpu_millis, plan.memory_bytes), (1000, 2 * GIB));
    assert_eq!(plan.labels["io.hearth.created-by"], "ada");
    // Le port de la fiche, et les dossiers gérés montés EN ENTIER (jamais le fichier seul).
    assert_eq!(plan.ports.len(), 1);
    assert_eq!((plan.ports[0].0, plan.ports[0].1), (7777, 7777));
    assert_eq!(plan.binds.len(), 2);
    assert!(plan.binds.iter().any(|(_, target, _)| target == "/config"));
    assert!(
        plan.binds
            .iter()
            .all(|(source, _, _)| source.contains(&accepted.service_key)),
        "tous sous le dossier géré de CE service : {:?}",
        plan.binds
    );
    // Le fichier produit : propriétaire root, droits 0600 (image root, BR-FICHE-024).
    let key = key_of(&accepted);
    assert_eq!(
        b.safe_fs.perms_of(&key, "config/serverconfig.txt"),
        Some((0, 0, 0o600))
    );
    let text = String::from_utf8(b.safe_fs.content(&key, "config/serverconfig.txt").unwrap())
        .expect("texte");
    assert!(text.contains("maxplayers=8"), "{text}");
    // La description garde la fiche, sa version et son empreinte (BR-FICHE-029).
    let row: (String, i64, String) = sqlx::query_as(
        "SELECT fiche_id, fiche_version, fiche_digest FROM service_specs WHERE current = 1",
    )
    .fetch_one(b.env.db.pool())
    .await
    .unwrap();
    assert_eq!(
        row,
        ("terraria".to_owned(), 2, b.fiche.digest_hex()),
        "la description du service"
    );
    // Le journal : une installation depuis une fiche, avec la fiche, et aucune valeur (BR-FICHE-032).
    let journal = b.journal().await;
    assert_eq!(journal.len(), 1, "{journal:?}");
    assert_eq!(journal[0].0, "ok");
    assert_eq!(journal[0].3.as_deref(), Some("ada"));
    let target = journal[0].1.clone().unwrap();
    assert!(
        target.contains("fiche terraria (version 2)") && target.contains("mon-terraria"),
        "{target}"
    );
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_rich_install_writes_each_file_with_the_declared_owner_and_never_wider_than_0640() {
    let b = bench(rich(), true).await;
    b.engine
        .publish_image("example/rich:2.0", PullScript::two_layers());
    let mut body = b.request("riche");
    body["source"]["answers"] = json!([
        {"id": "max_players", "value": "20"},
        {"id": "admin_password", "value": "Sentinel-pw-91Zx"}
    ]);
    body["source"]["ports"] = json!([{"id": "game", "host_port": 25570}]);
    let accepted = b.accepted(body).await;
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Succeeded,
        "{operation:?}"
    );
    let key = key_of(&accepted);
    // Propriétaire 1001:1002 déclaré par la fiche ; un `0644` déclaré est abaissé à `0640`.
    assert_eq!(
        b.safe_fs.perms_of(&key, "data+config/server.properties"),
        Some((1001, 1002, 0o640))
    );
    assert_eq!(
        b.safe_fs.perms_of(&key, "data+config/settings.xml"),
        Some((1001, 1002, 0o640))
    );
    // Les trois dossiers de la fiche existent, et le port choisi est celui du serveur.
    let paths = b.safe_fs.paths(&key);
    for folder in ["data+world", "data+config", "data+mods"] {
        assert!(paths.iter().any(|p| p == folder), "{folder} dans {paths:?}");
    }
    let plan = &b.engine.created()[0];
    assert_eq!(plan.ports[0].0, 25570);
    assert_eq!(plan.binds.len(), 3);
    // La variable de la fiche porte le secret tiré par l'agent (pour le moteur seulement).
    assert!(plan.env.iter().any(|(name, _)| name == "RCON_PASSWORD"));
    assert!(
        plan.env
            .iter()
            .any(|(name, value)| name == "EULA" && value == "TRUE"),
        "le contrat accepté par le geste de l'utilisateur"
    );
    b.agent.shutdown().await;
}

#[tokio::test]
async fn the_fiche_description_keeps_answers_and_frozen_questions() {
    let b = bench(rich(), true).await;
    b.engine
        .publish_image("example/rich:2.0", PullScript::two_layers());
    let mut body = b.request("riche");
    body["source"]["answers"] = json!([
        {"id": "max_players", "value": "20"},
        {"id": "admin_password", "value": "Sentinel-pw-91Zx"}
    ]);
    let accepted = b.accepted(body).await;
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(operation.status, OperationStatus::Succeeded);
    let spec_json: String =
        sqlx::query_scalar("SELECT spec_json FROM service_specs WHERE current = 1")
            .fetch_one(b.env.db.pool())
            .await
            .unwrap();
    let spec: Value = serde_json::from_str(&spec_json).unwrap();
    let extra = &spec["fiche_extra"];
    let answers = extra["answers"].as_array().unwrap();
    let answer = |id: &str| answers.iter().find(|a| a["id"] == id).unwrap();
    // Une question non secrète garde sa valeur ; la figée à la création du monde l'est.
    assert_eq!(answer("max_players")["value"], "20");
    assert_eq!(answer("max_players")["frozen"], false);
    assert_eq!(answer("level_name")["frozen"], true);
    // Un secret n'a ni valeur en clair ni trace dans le JSON : il est scellé à part.
    assert_eq!(answer("admin_password")["secret"], true);
    assert!(answer("admin_password").get("value").is_none());
    assert!(!spec_json.contains("Sentinel-pw-91Zx"));
    // Le contrat : qui l'a accepté.
    assert_eq!(extra["agreements"][0]["id"], "eula");
    assert_eq!(extra["agreements"][0]["accepted_by"], "ada");
    // Les valeurs scellées sont des champs de réponse : celle de l'utilisateur et celle que l'agent a tirée.
    let fields: Vec<String> = sqlx::query_scalar("SELECT field FROM service_spec_secrets")
        .fetch_all(b.env.db.pool())
        .await
        .unwrap();
    assert!(
        fields.iter().any(|f| f == "answer:admin_password"),
        "{fields:?}"
    );
    assert!(fields.iter().any(|f| f == "answer:gen-rcon"), "{fields:?}");
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Ce que le serveur refuse, tout de suite, sans rien créer
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn an_install_without_the_agreement_is_refused_even_from_a_modified_client_and_creates_nothing()
 {
    let b = bench(rich(), true).await;
    let mut body = b.request("riche");
    body["source"]["agreements"] = json!([]);
    let reply = b.install(body).await;
    assert_eq!(reply.status, 422, "{}", reply.text);
    assert_eq!(code(&reply), "AGREEMENT_REQUIRED");
    assert_eq!(reply.body["error"]["details"]["ids"], json!(["eula"]));
    b.assert_nothing_created().await;
    let journal = b.journal().await;
    assert_eq!(journal.len(), 1, "{journal:?}");
    assert_eq!(journal[0].0, "failed");
    assert_eq!(journal[0].2.as_deref(), Some("contrat non accepté"));
    // L'entrée vise la fiche (id et version), jamais une réponse (BR-FICHE-032).
    assert_eq!(journal[0].1.as_deref(), Some("fiche rich (version 3)"));
    b.agent.shutdown().await;
}

#[tokio::test]
async fn an_account_without_the_right_creates_no_service() {
    let b = bench(rich(), true).await;
    for who in [&b.bob, &b.lucas] {
        let reply = b.install_as(who, b.request("riche")).await;
        assert_eq!(reply.status, 403, "{}", reply.text);
    }
    b.assert_nothing_created().await;
    let journal = b.journal().await;
    assert!(
        journal
            .iter()
            .all(|row| row.0 == "denied" && row.2.as_deref() == Some("droits insuffisants")),
        "{journal:?}"
    );
    b.agent.shutdown().await;
}

#[tokio::test]
async fn an_invalid_answer_is_refused_naming_the_character_never_the_value() {
    let b = bench(rich(), true).await;
    let mut body = b.request("riche");
    body["source"]["answers"] = json!([{"id": "level_name", "value": "mon/monde-SENTINEL"}]);
    let reply = b.install(body).await;
    assert_eq!(reply.status, 422, "{}", reply.text);
    assert_eq!(code(&reply), "SETTING_REFUSED");
    let details = &reply.body["error"]["details"];
    assert_eq!(details["id"], "level_name");
    assert_eq!(details["reason"], "charset");
    assert_eq!(details["char"], "/");
    assert!(!reply.text.contains("SENTINEL") && !reply.text.contains("mon/monde"));
    b.assert_nothing_created().await;
    // Une question inconnue de la fiche : refusée sans que son nom revienne au client.
    let mut body = b.request("riche");
    body["source"]["answers"] = json!([{"id": "NOM-INVENTE", "value": "x"}]);
    let reply = b.install(body).await;
    assert_eq!(code(&reply), "SETTING_REFUSED");
    assert_eq!(reply.body["error"]["details"]["id"], Value::Null);
    assert!(!reply.text.contains("NOM-INVENTE"));
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_machine_below_a_measured_minimum_is_refused() {
    let big = rich_with(|text| {
        text.replace(
            "memory = { min_bytes = 1073741824, recommended_bytes = 2147483648, evidence = \"measured_idle\" }",
            "memory = { min_bytes = 34359738368, recommended_bytes = 68719476736, evidence = \"measured\" }",
        )
    });
    let b = bench(big, true).await;
    let reply = b.install(b.request("riche")).await;
    assert_eq!(reply.status, 409, "{}", reply.text);
    assert_eq!(code(&reply), "HOST_TOO_SMALL");
    let details = &reply.body["error"]["details"];
    assert_eq!(details["lack"], "memory");
    assert_eq!(details["needed"], 32 * GIB);
    assert_eq!(details["has"], 16 * GIB);
    b.assert_nothing_created().await;
    let journal = b.journal().await;
    assert_eq!(journal[0].2.as_deref(), Some("machine trop petite"));
    b.agent.shutdown().await;
}

#[tokio::test]
async fn an_unmeasured_minimum_never_blocks_the_install() {
    // Terraria : tout est « non mesuré », même sur une toute petite machine.
    let b = bench(pinned_terraria(), true).await;
    b.engine.set_info(Some(EngineInfo {
        cpus: 1,
        memory_bytes: 512 * 1024 * 1024,
        storage_dir: Some("/var/lib/docker".into()),
    }));
    b.engine.publish_image(
        &format!("ryshe/terraria:vanilla-1.4.5.8@{IMAGE_DIGEST}"),
        PullScript::two_layers(),
    );
    let mut body = b.request("petit");
    body["resources"] = json!({ "cpu_millis": 250, "memory_bytes": 256 * 1024 * 1024 });
    let accepted = b.accepted(body).await;
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(operation.status, OperationStatus::Succeeded);
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_disk_below_the_need_is_refused_without_a_box() {
    let measured = rich_with(|text| {
        text.replace(
            "disk = { install_bytes = 1073741824, evidence = { publisher = \"https://example.org/size\" } }",
            "disk = { install_bytes = 214748364800, evidence = \"measured\" }",
        )
    });
    let b = bench(measured, true).await;
    b.free_space.set(100 * GIB);
    for low_disk in [false, true] {
        let mut body = b.request("riche");
        body["accept"] = json!({ "low_disk": low_disk });
        let reply = b.install(body).await;
        assert_eq!(reply.status, 507, "{}", reply.text);
        assert_eq!(code(&reply), "NO_SPACE");
        let details = &reply.body["error"]["details"];
        assert_eq!(details["reason"], "install");
        assert_eq!(details["missing_bytes"], 100 * GIB);
    }
    b.assert_nothing_created().await;
    let journal = b.journal().await;
    assert_eq!(
        journal[0].2.as_deref(),
        Some("espace insuffisant pour l'installation")
    );
    b.agent.shutdown().await;
}

#[tokio::test]
async fn either_disk_below_the_measured_need_refuses_the_install() {
    // Le besoin est jugé sur le disque de la racine de données ET sur celui des images (BR-FICHE-023).
    let measured = rich_with(|text| {
        text.replace(
            "disk = { install_bytes = 1073741824, evidence = { publisher = \"https://example.org/size\" } }",
            "disk = { install_bytes = 214748364800, evidence = \"measured\" }",
        )
    });
    let b = bench(measured, true).await;
    // Les images ont de la place (500 Gio par défaut) ; la racine de données n'en a pas assez.
    b.free_space
        .set_under(&b.env.dir.path().to_string_lossy(), 100 * GIB);
    let reply = b.install(b.request("riche")).await;
    assert_eq!(reply.status, 507, "{}", reply.text);
    assert_eq!(code(&reply), "NO_SPACE");
    assert_eq!(reply.body["error"]["details"]["free_bytes"], 100 * GIB);
    b.assert_nothing_created().await;
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_publisher_disk_figure_asks_for_the_box_and_the_box_lets_it_through() {
    // Le chiffre vient de l'éditeur : un avertissement avec la case, jamais un refus sec (BR-FICHE-023).
    let b = bench(rich(), true).await;
    b.engine
        .publish_image("example/rich:2.0", PullScript::two_layers());
    b.free_space.set(1024);
    let reply = b.install(b.request("riche")).await;
    assert_eq!(reply.status, 507, "{}", reply.text);
    assert_eq!(reply.body["error"]["details"]["reason"], "low_disk");
    b.assert_nothing_created().await;
    let mut body = b.request("riche");
    body["accept"] = json!({ "low_disk": true });
    let accepted = b.accepted(body).await;
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(operation.status, OperationStatus::Succeeded);
    b.agent.shutdown().await;
}

#[tokio::test]
async fn an_install_without_memory_limit_is_refused() {
    let b = bench(rich(), true).await;
    let mut body = b.request("riche");
    body["resources"] = json!({ "cpu_millis": 1000 });
    let reply = b.install(body).await;
    assert_eq!(reply.status, 422, "{}", reply.text);
    assert_eq!(code(&reply), "LIMITS_REQUIRED");
    b.assert_nothing_created().await;
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_preview_fiche_is_refused_by_a_published_agent() {
    let b = bench(rich(), false).await;
    let reply = b.install(b.request("riche")).await;
    assert_eq!(reply.status, 404, "{}", reply.text);
    assert_eq!(code(&reply), "FICHE_UNKNOWN");
    b.assert_nothing_created().await;
    let journal = b.journal().await;
    assert_eq!(journal[0].2.as_deref(), Some("fiche inconnue"));
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_fiche_that_changed_since_the_client_saw_it_is_refused() {
    let b = bench(rich(), true).await;
    for (member, value) in [("digest", json!("00ff")), ("version", json!(99))] {
        let mut body = b.request("riche");
        body["source"][member] = value;
        let reply = b.install(body).await;
        assert_eq!(reply.status, 409, "{member} : {}", reply.text);
        assert_eq!(code(&reply), "FICHE_CHANGED");
    }
    b.assert_nothing_created().await;
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_fiche_request_decides_nothing_the_fiche_decides_and_cannot_resume_a_world() {
    let b = bench(rich(), true).await;
    // L'image, les variables, les ports, les dossiers et la commande sont ceux de la fiche, pas de la demande.
    let tampered: [(&str, Value); 5] = [
        ("image", json!("evil/image")),
        ("env", json!([{"name": "A", "value": "b"}])),
        ("ports", json!([{"host_port": 8080, "container_port": 80}])),
        ("mounts", json!([{"target": "/data"}])),
        ("command", json!("sh -c evil")),
    ];
    for (member, value) in tampered {
        let mut body = b.request("riche");
        body[member] = value;
        let reply = b.install(body).await;
        assert_eq!(reply.status, 422, "{member} : {}", reply.text);
        assert_eq!(code(&reply), "PLAN_SETTING_REFUSED", "{member}");
    }
    // Un membre que le type ne connaît pas : refusé, jamais ignoré.
    let mut body = b.request("riche");
    body["privileged"] = json!(true);
    let reply = b.install(body).await;
    assert_eq!(code(&reply), "PLAN_SETTING_REFUSED");
    // La reprise d'un monde existant n'est pas dans cette version.
    let mut body = b.request("riche");
    body["source"]["world_imported"] = json!(true);
    let reply = b.install(body).await;
    assert_eq!(reply.status, 422, "{}", reply.text);
    assert_eq!(code(&reply), "VALIDATION_ERROR");
    b.assert_nothing_created().await;
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_failed_start_leaves_no_file_and_no_description_behind() {
    let b = bench(rich(), true).await;
    b.engine
        .publish_image("example/rich:2.0", PullScript::two_layers());
    b.engine
        .fail_create(Some(hearth_agent::application::ports::EngineError::Refused));
    let mut body = b.request("riche");
    body["source"]["answers"] = json!([{"id": "admin_password", "value": "Sentinel-pw-91Zx"}]);
    let accepted = b.accepted(body).await;
    let operation = b.finished(&accepted.operation_id).await;
    assert_eq!(operation.status, OperationStatus::Failed, "{operation:?}");
    // Le fichier produit (qui portait le mot de passe) est parti avec le dossier du service.
    let key = key_of(&accepted);
    assert!(!b.safe_fs.has_root(&key), "{:?}", b.safe_fs.paths(&key));
    assert_eq!(b.specs_count().await, 0);
    b.agent.shutdown().await;
}

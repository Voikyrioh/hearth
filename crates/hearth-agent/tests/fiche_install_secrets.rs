//! Le test sentinelle d'une installation depuis une fiche (HRT-101, BR-FICHE-013, BR-FICHE-032, ADR-0044) :
//! une réponse secrète et un secret tiré par l'agent, avec des valeurs reconnaissables, ne sortent NULLE PART :
//! ni dans les octets bruts de la base et de son journal d'écriture, ni dans une trace (niveau le plus bavard,
//! chemin d'initialisation de la production), ni dans le journal d'activité, ni dans une réponse, un message
//! d'erreur ou une opération, ni dans un `Debug`.
//!
//! Le fichier produit à partir des réponses est, lui, VOLONTAIREMENT en clair dans le dossier géré du service
//! (droits `0640`, BR-FICHE-024) : il est exclu du balayage et vérifié à part.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use hearth_agent::application::catalog::CatalogService;
use hearth_agent::application::ports::EngineInfo;
use hearth_agent::domain::accounts::Role;
use hearth_agent::domain::catalog::translate::{Request, translate};
use hearth_agent::domain::catalog::{Origin, load};
use hearth_agent::domain::secret::Secret;
use hearth_agent::domain::services::ServiceKey;
use hearth_agent::domain::services::deploy::Limits;
use hearth_agent::domain::services::from_fiche::{InstallInputs, body_and_secrets_from};
use hearth_agent::infrastructure::logging::{self, LogFormat};
use hearth_proto::api::deploy::{DeployAccept, DeployAccepted, DeploySpec};
use hearth_proto::api::service_ops::{OperationStatus, OperationView};
use serde_json::{Value, json};
use support::engine::{fast_timings, parts_supervised};
use support::fake_engine::{FakeEngine, PullScript};
use support::fake_safe_fs::FakeSafeFs;
use support::fake_supervisor::FakeSupervisor;
use support::https;
use support::{Env, env};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::writer::BoxMakeWriter;

const GIB: u64 = 1024 * 1024 * 1024;
const RICH_FICHE: &str = include_str!("fixtures/rich/fiche.toml");
const RICH_SETTINGS: &str = include_str!("fixtures/rich/settings.toml");
/// Une réponse secrète (le mot de passe d'administration du serveur de jeu).
const SENTINEL_PASSWORD: &str = "Sentinel-Pw-7f3a91c2e4";
/// Une valeur refusée : elle ne doit pas revenir dans le message du refus.
const SENTINEL_REFUSED: &str = "refus/SENTINEL-REFUSED-b0c8d1";
/// Un témoin non secret : il doit, lui, se retrouver dans la base (le balayage voit).
const WITNESS_NAME: &str = "Witness-Fiche-Service";

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

/// Les octets BRUTS de la base et de ses compagnons (journal d'écriture, index partagé).
fn all_bytes(dir: &Path) -> Vec<u8> {
    let mut bytes = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.path().is_file() && name.starts_with("hearth.db") {
            bytes.extend(std::fs::read(entry.path()).unwrap_or_default());
        }
    }
    bytes
}

/// Les traces de tout le binaire de test, captées par le MÊME chemin d'initialisation que la production.
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
                EnvFilter::new("trace,sqlx=trace,sqlx::query=trace,hearth_agent=trace"),
            )
            .expect("première installation du journal de ce binaire de test");
            buffer
        })
        .clone()
}

fn rich_fiche() -> hearth_agent::domain::catalog::Fiche {
    load(
        Origin::Embedded,
        RICH_FICHE.as_bytes(),
        RICH_SETTINGS.as_bytes(),
    )
    .expect("fiche de fabrication valide")
}

#[tokio::test]
async fn no_answer_value_nor_generated_secret_reaches_the_database_the_journal_the_traces_or_a_response()
 {
    let traces = captured_traces();
    log::debug!(target: "hearth_witness", "witness-fiche-line");
    let fiche = rich_fiche();
    let env: Env = env().await;
    let engine = Arc::new(FakeEngine::new());
    engine.set_info(Some(EngineInfo {
        cpus: 8,
        memory_bytes: 16 * GIB,
        storage_dir: Some("/var/lib/docker".into()),
    }));
    engine.publish_image("example/rich:2.0", PullScript::two_layers());
    let safe_fs = Arc::new(FakeSafeFs::new());
    let mut parts = parts_supervised(
        engine.clone(),
        Arc::new(FakeSupervisor::new()),
        fast_timings(),
    );
    parts.safe_fs = Some(safe_fs.clone());
    parts.catalog.include_preview = true;
    parts.catalog.service = Some(Arc::new(CatalogService::from_parts(
        vec![fiche.clone()],
        Vec::new(),
        true,
    )));
    let agent = https::start_engined(&env, parts).await;
    let ada = agent.actor(&env, "ada", Role::Admin).await;
    env.grant_services("ada").await;

    let source = |answers: Value, agreements: Value| {
        json!({
            "name": WITNESS_NAME,
            "resources": { "cpu_millis": 1000, "memory_bytes": 2 * GIB },
            "source": {
                "type": "fiche", "id": "rich", "version": fiche.file.version,
                "digest": fiche.digest_hex(), "answers": answers, "agreements": agreements
            }
        })
    };
    let mut shown = String::new();

    // 1. Un refus : une valeur refusée, un contrat manquant. Rien de ce qui a été reçu ne revient.
    for body in [
        source(
            json!([{"id": "level_name", "value": SENTINEL_REFUSED}, {"id": "admin_password", "value": SENTINEL_PASSWORD}]),
            json!(["eula"]),
        ),
        source(
            json!([{"id": "admin_password", "value": SENTINEL_PASSWORD}]),
            json!([]),
        ),
    ] {
        let reply = agent
            .confirmed(&ada, "POST", "/deployments", body)
            .await
            .send()
            .await;
        assert!(reply.status == 422, "{}", reply.text);
        shown.push_str(&reply.text);
    }

    // 2. L'installation qui réussit, avec le mot de passe en réponse et le secret tiré par l'agent.
    let reply = agent
        .confirmed(
            &ada,
            "POST",
            "/deployments",
            source(
                json!([{"id": "admin_password", "value": SENTINEL_PASSWORD}]),
                json!(["eula"]),
            ),
        )
        .await
        .send()
        .await;
    assert_eq!(reply.status, 202, "{}", reply.text);
    shown.push_str(&reply.text);
    let accepted: DeployAccepted = serde_json::from_value(reply.body).unwrap();
    let started = Instant::now();
    let operation = loop {
        let reply = agent
            .request(
                "GET",
                &format!("/operations/services/{}", accepted.operation_id),
            )
            .token(&ada.token)
            .send()
            .await;
        shown.push_str(&reply.text);
        let operation: OperationView = serde_json::from_value(reply.body).unwrap();
        if !matches!(
            operation.status,
            OperationStatus::Accepted | OperationStatus::Running
        ) {
            break operation;
        }
        assert!(started.elapsed() < Duration::from_secs(15), "{operation:?}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert_eq!(
        operation.status,
        OperationStatus::Succeeded,
        "{operation:?}"
    );
    for path in ["/services", "/catalog/rich", "/audit"] {
        let reply = agent.request("GET", path).token(&ada.token).send().await;
        shown.push_str(&reply.text);
    }

    // Le fichier produit est EN CLAIR dans le dossier géré du service, et seulement là : il porte le mot de passe
    // saisi et le secret que l'agent a tiré (celui-ci sert de sentinelle au balayage).
    let key = ServiceKey::parse(&accepted.service_key).unwrap();
    let produced = String::from_utf8(
        safe_fs
            .content(&key, "data+config/server.properties")
            .expect("fichier produit"),
    )
    .unwrap();
    let generated = produced
        .lines()
        .find_map(|line| line.strip_prefix("rcon.password="))
        .expect("le secret tiré est dans le fichier produit")
        .to_owned();
    assert_eq!(generated.len(), 32);
    assert!(generated.bytes().all(|b| b.is_ascii_alphanumeric()));
    assert!(
        produced.contains(SENTINEL_PASSWORD),
        "le fichier produit porte les réponses, c'est voulu : {produced}"
    );
    assert_eq!(
        safe_fs.perms_of(&key, "data+config/server.properties"),
        Some((1001, 1002, 0o640)),
        "lisible du seul propriétaire et de son groupe"
    );

    // 3. Les Debug de tout ce que la demande traverse.
    let spec: DeploySpec = serde_json::from_value(source(
        json!([{"id": "admin_password", "value": SENTINEL_PASSWORD}]),
        json!(["eula"]),
    ))
    .unwrap();
    let answers = BTreeMap::from([("admin_password".to_owned(), SENTINEL_PASSWORD.to_owned())]);
    let agreements = BTreeSet::from(["eula".to_owned()]);
    let secrets = BTreeMap::from([("rcon".to_owned(), Secret::new(generated.clone()))]);
    let (none, optional) = (BTreeMap::new(), BTreeSet::new());
    let draft = translate(
        &fiche,
        &Request {
            variant: None,
            answers: &answers,
            agreements: &agreements,
            host_ports: &none,
            optional_ports: &optional,
            generated: &secrets,
            world_imported: false,
        },
    )
    .expect("traduite");
    let built = body_and_secrets_from(
        &InstallInputs {
            fiche: &fiche,
            answers: &answers,
            generated: &secrets,
            accepted_by: "ada",
            accepted_at: 0,
            accept: DeployAccept::default(),
        },
        &draft,
        WITNESS_NAME,
        Limits {
            cpu_millis: 1000,
            memory_bytes: 2 * GIB,
        },
    )
    .expect("jugée");
    let debug = format!(
        "{spec:?} {draft:?} {:?} {:?} {:?}",
        built.valid.secrets, built.files, built.valid.body
    );
    assert!(
        !debug.contains(SENTINEL_PASSWORD) && !debug.contains(&generated),
        "un secret dans un Debug : {debug}"
    );

    // Le journal d'activité, lu directement : le témoin y est (le balayage voit), jamais un secret.
    agent.flush_journal().await;
    let rows: Vec<(Option<String>, Option<String>)> =
        sqlx::query_as("SELECT target, reason FROM audit_events")
            .fetch_all(env.db.pool())
            .await
            .unwrap();
    let journal = format!("{rows:?}");
    assert!(
        journal.contains(WITNESS_NAME) && journal.contains("fiche rich (version 3)"),
        "témoin : le journal dit l'installation : {journal}"
    );

    // Les secrets sont bien scellés en base : leurs champs existent (le test ne passe pas à vide).
    let fields: Vec<String> = sqlx::query_scalar("SELECT field FROM service_spec_secrets")
        .fetch_all(env.db.pool())
        .await
        .unwrap();
    assert!(
        fields.iter().any(|f| f == "answer:admin_password"),
        "{fields:?}"
    );
    assert!(fields.iter().any(|f| f == "answer:gen-rcon"), "{fields:?}");

    agent.shutdown().await;
    let data_dir = env.dir.path().to_owned();
    let live = all_bytes(&data_dir);
    let logged = traces.lock().unwrap().clone();
    assert!(
        contains(&live, WITNESS_NAME),
        "témoin : le balayage voit la base et son journal d'écriture"
    );
    assert!(
        contains(&logged, "witness-fiche-line"),
        "témoin : le suivi voit passer les traces"
    );
    assert!(
        contains(&logged, "déploiement terminé"),
        "la trace de fin existe : le test n'est pas aveugle"
    );
    for sentinel in [
        SENTINEL_PASSWORD,
        SENTINEL_REFUSED,
        generated.as_str(),
        "SENTINEL-REFUSED",
    ] {
        assert!(
            !contains(&live, sentinel),
            "{sentinel} dans la base ou son journal d'écriture"
        );
        assert!(!contains(&logged, sentinel), "{sentinel} dans une trace");
        assert!(
            !shown.contains(sentinel),
            "{sentinel} dans une réponse, une opération ou le journal rendu"
        );
        assert!(
            !journal.contains(sentinel),
            "{sentinel} au journal d'activité"
        );
    }
}

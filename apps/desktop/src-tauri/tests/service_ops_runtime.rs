//! HRT-84 : le suivi des opérations de service dans la coquille. La chaîne réelle jusqu'à la sortie de la
//! coquille : un VRAI agent (TLS 1.3, SQLite, faux moteur de conteneurs) accepte une opération dans son
//! journal, `hearth-link` reçoit le message `operation` du sujet `services`, le relais de la coquille
//! l'écrit dans la fenêtre sous le nom `link://service-operations`, et une fenêtre qui arrive après coup
//! retrouve l'état (`list_service_operations`). L'issue d'une opération terminée est retenue jusqu'à son
//! acquittement : annoncée une fois (BR-RESIL-010).
//!
//! Ce que ce test ne prouve PAS : l'émission réelle vers la WebView (`window.emit`), l'écoute (`listen`) et
//! l'appel des commandes par `invoke` : le relais écrit dans un enregistreur à la place de la fenêtre.
#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "../../../../crates/hearth-link/tests/support/agent.rs"]
mod agent;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agent::fake_engine::FakeEngine;
use agent::{PASSWORD, TestAgent};
use hearth_agent::application::service_ops::AcceptRequest;
use hearth_agent::domain::accounts::Role;
use hearth_agent::domain::ops::{OpKind, OpPlan, OpStatus, PlannedStep, StepName};
use hearth_agent::domain::services::ServiceKey;
use hearth_desktop_lib::link::{LinkRuntime, UiSink};
use hearth_desktop_lib::service_ops::book::OpsBook;
use hearth_desktop_lib::service_ops::dto::{OperationChangeDto, OperationStatusDto};
use hearth_desktop_lib::vault::{CredentialBackend, CredentialVault};
use hearth_link::LinkConfig;
use hearth_link::domain::state::Thresholds;
use hearth_proto::api::service_ops::OperationView;
use serde_json::Value;

const GUARD: Duration = Duration::from_secs(60);
const KEY: &str = "h_01J9ZY0G3Q8M2K6W4T7V5N1B9D";

#[derive(Default)]
struct Memory(Mutex<HashMap<String, Vec<u8>>>);

#[derive(Clone)]
struct Shared(Arc<Memory>);

impl CredentialBackend for Shared {
    fn read(&self, target: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(self.0.0.lock().unwrap().get(target).cloned())
    }
    fn write(&self, target: &str, secret: &[u8]) -> Result<(), String> {
        self.0
            .0
            .lock()
            .unwrap()
            .insert(target.into(), secret.into());
        Ok(())
    }
    fn remove(&self, target: &str) -> Result<(), String> {
        self.0.0.lock().unwrap().remove(target);
        Ok(())
    }
}

struct Nowhere;

impl UiSink for Nowhere {
    fn emit(&self, _: &str, _: serde_json::Value) {}
}

#[derive(Default)]
struct Recorder(Mutex<Vec<(String, serde_json::Value)>>);

impl UiSink for Recorder {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        self.0.lock().unwrap().push((event.to_owned(), payload));
    }
}

impl Recorder {
    fn operations(&self) -> Vec<Value> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|(event, _)| event == "link://service-operations")
            .map(|(_, payload)| payload.clone())
            .collect()
    }
}

fn config() -> LinkConfig {
    let hour = Duration::from_secs(3_600);
    LinkConfig {
        thresholds: Thresholds {
            silence: hour,
            reconnecting_after: hour,
            offline_after: hour * 2,
            ..Thresholds::scaled(6)
        },
        attempt_timeout: Duration::from_secs(30),
        request_timeout: Duration::from_secs(30),
        persist_timeout: Duration::from_secs(120),
        ..LinkConfig::default()
    }
}

async fn wait_for(sink: &Recorder, what: &str, predicate: impl Fn(&Value) -> bool) -> Value {
    tokio::time::timeout(GUARD, async {
        loop {
            if let Some(found) = sink.operations().into_iter().find(|event| predicate(event)) {
                return found;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("délai dépassé en attendant : {what}"))
}

fn plan() -> OpPlan {
    let step = |text: &str, destructive| PlannedStep {
        name: StepName::parse(text).unwrap(),
        destructive,
    };
    OpPlan::new(vec![step("check", false), step("create_new", true)], None).unwrap()
}

#[tokio::test]
async fn an_operation_reaches_the_window_is_replayed_late_and_its_outcome_is_kept_until_acknowledged()
 {
    let agent = TestAgent::install_engined(Arc::new(FakeEngine::new())).await;
    agent.create_account("marie", Role::Admin).await;
    agent.grant_services("marie").await;

    let dir = agent::tmp::tempdir().unwrap();
    let vault = Arc::new(CredentialVault::new(Shared(Arc::new(Memory::default()))));
    let runtime = Arc::new(
        LinkRuntime::open_with(dir.path(), vault, "poste-test/0.1", config())
            .await
            .unwrap(),
    );
    let sink = Arc::new(Recorder::default());
    let relay = {
        let runtime = runtime.clone();
        let sink = sink.clone();
        let stream = runtime.manager().subscribe();
        tokio::spawn(async move { runtime.forward(stream, &*sink).await })
    };
    let probe = runtime
        .probe("127.0.0.1", Some(agent.addr.port()))
        .await
        .unwrap();
    let server = runtime
        .add_and_login(
            "forge".into(),
            2,
            "127.0.0.1".into(),
            Some(agent.addr.port()),
            &probe.fingerprint,
            probe.mac_addresses,
            "marie",
            PASSWORD.into(),
            false,
            &Nowhere,
        )
        .await
        .unwrap();

    // Le flux vit : le sujet `services` est abonné (sinon l'opération serait acceptée avant l'abonnement).
    tokio::time::timeout(GUARD, async {
        while !sink
            .0
            .lock()
            .unwrap()
            .iter()
            .any(|(event, _)| event == "link://services")
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("le sujet services");

    // 1. L'opération acceptée par l'agent arrive à la fenêtre, avec le serveur et un numéro.
    let ops = agent.ops();
    let id = ops
        .accept(AcceptRequest {
            op_key: "k-1".to_owned(),
            service: ServiceKey::parse(KEY).unwrap(),
            service_name: "terraria".to_owned(),
            kind: OpKind::Deploy,
            account: "marie".to_owned(),
            plan: plan(),
            forced: false,
            reserve_name: None,
            heavy_io: false,
        })
        .await
        .unwrap();
    ops.start(&id).await.unwrap();
    let running = wait_for(&sink, "l'opération en cours", |event| {
        event["change"]["operation"]["id"] == id.as_str()
            && event["change"]["operation"]["status"] == "running"
    })
    .await;
    assert_eq!(running["serverId"], server.id.as_str());
    assert_eq!(running["change"]["kind"], "upsert");
    assert_eq!(running["change"]["operation"]["key"], KEY);
    assert_eq!(running["change"]["operation"]["kind"], "deploy");
    // Jamais un nom de service, un compte ou un chemin dans l'opération : elle ne porte que sa clé.
    let text = running.to_string();
    assert!(
        !text.contains("terraria") && !text.contains("marie"),
        "{text}"
    );

    // 2. Une fenêtre qui arrive après coup retrouve l'opération en cours.
    let replay = runtime.service_operations();
    assert_eq!(replay.len(), 1);
    assert!(matches!(
        &replay[0].change,
        OperationChangeDto::Upsert { operation } if operation.status == OperationStatusDto::Running
    ));

    // 3. Elle se termine : l'issue arrive une fois, et elle est retenue jusqu'à l'acquittement.
    ops.finish(&id, OpStatus::Succeeded, None).await.unwrap();
    wait_for(&sink, "l'issue", |event| {
        event["change"]["operation"]["id"] == id.as_str()
            && event["change"]["operation"]["status"] == "succeeded"
    })
    .await;
    let late = runtime.service_operations();
    assert_eq!(late.len(), 1, "l'issue non acquittée est rejouée");
    runtime.ack_service_operations(&server.id, std::slice::from_ref(&id));
    assert!(runtime.service_operations().is_empty());

    runtime.manager().shutdown().await;
    relay.abort();
}

// ── La forme : la conversion contre la fiche de référence ──────────────────────────────────────

fn fixture() -> Value {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/service-operation-event.json"
    ))
    .unwrap();
    serde_json::from_str(&text).unwrap()
}

/// Ce que reçoit l'interface est comparé, champ par champ, à la fiche que les tests Vitest relisent aussi pour
/// contrôler le pont simulé (`src/link/simulated-service-ops.test.ts`).
#[test]
fn the_event_the_window_receives_is_the_reference_file_field_by_field() {
    let running: OperationView = serde_json::from_value(serde_json::json!({
        "id": "01J9ZY0G3Q8M2K6W4T7V5N1B9D", "key": "h_01J9ZY0G3Q8M2K6W4T7V5N1B9E",
        "kind": "deploy", "status": "running", "step": "pull",
        "steps": [{ "step": "check", "state": "done" }, { "step": "pull", "state": "intent" }],
        "percent": 40, "items_done": 3, "bytes_done": 400, "bytes_total": 1000, "eta_s": 12,
        "cancellable": true
    }))
    .unwrap();
    let awaiting: OperationView = serde_json::from_value(serde_json::json!({
        "id": "01J9ZY0G3Q8M2K6W4T7V5N1B9F", "key": "h_01J9ZY0G3Q8M2K6W4T7V5N1B9G",
        "kind": "update", "status": "awaiting_decision", "cancellable": false,
        "decision": { "choices": ["revert", "keep"], "expires_at": "2026-10-14T10:15:00Z" },
        "cause": "decided_by_account"
    }))
    .unwrap();
    let mut book = OpsBook::default();
    let reference = fixture();
    let first = serde_json::to_value(book.on_view("forge", &running)).unwrap();
    let second = serde_json::to_value(book.on_view("forge", &awaiting)).unwrap();
    let gone = serde_json::to_value(book.on_gone("forge", "01J9ZY0G3Q8M2K6W4T7V5N1B9D")).unwrap();
    assert_eq!(first, reference["running"]);
    assert_eq!(second, reference["awaiting"]);
    assert_eq!(gone, reference["gone"]);
}

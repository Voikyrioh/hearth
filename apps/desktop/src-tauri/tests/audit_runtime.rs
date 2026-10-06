//! Le journal d'activité côté coquille contre un VRAI agent (TLS 1.3, SQLite, WebSocket) : une page
//! typée (types, conversions, curseur), le refus d'un compte lecture seule, les filtres refusés avant
//! tout envoi, l'export vers un fichier choisi par l'UTILISATEUR (jamais par la page), le relais des
//! entrées en direct (`link://audit`). HRT-14.
#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "../../../../crates/hearth-link/tests/support/agent.rs"]
mod agent;
#[path = "../../../../crates/hearth-link/tests/support/proxy.rs"]
mod proxy;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use agent::{PASSWORD, TestAgent};
use async_trait::async_trait;
use hearth_agent::domain::accounts::Role;
use hearth_desktop_lib::audit::{
    self, AuditEntryDto, AuditFilterDto, AuditKindDto, AuditOutcomeDto, SaveDialog,
};
use hearth_desktop_lib::link::{LinkRuntime, UiSink};
use hearth_desktop_lib::link_dto::{InvalidField, LinkFailure, LinkStateName};
use hearth_desktop_lib::vault::{CredentialBackend, CredentialVault};
use hearth_link::LinkConfig;
use hearth_link::adapters::{HttpTransport, HttpTransportConfig};
use hearth_link::ports::Transport as _;
use hearth_link::ports::transport::{Pin, Target};
use hearth_proto::api::sessions::LoginRequest;
use hearth_proto::fingerprint::Fingerprint;
use proxy::FaultProxy;
use serde_json::Value;

#[derive(Default, Clone)]
struct Memory(Arc<Mutex<HashMap<String, Vec<u8>>>>);

impl CredentialBackend for Memory {
    fn read(&self, target: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(self.0.lock().unwrap().get(target).cloned())
    }
    fn write(&self, target: &str, secret: &[u8]) -> Result<(), String> {
        self.0.lock().unwrap().insert(target.into(), secret.into());
        Ok(())
    }
    fn remove(&self, target: &str) -> Result<(), String> {
        self.0.lock().unwrap().remove(target);
        Ok(())
    }
}

#[derive(Default)]
struct Recorder(Mutex<Vec<(String, Value)>>);

impl UiSink for Recorder {
    fn emit(&self, event: &str, payload: Value) {
        self.0.lock().unwrap().push((event.to_owned(), payload));
    }
}

impl Recorder {
    fn of(&self, event: &str) -> Vec<Value> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|(name, _)| name == event)
            .map(|(_, payload)| payload.clone())
            .collect()
    }
}

async fn eventually(what: &str, mut condition: impl FnMut() -> bool) {
    let started = Instant::now();
    while !condition() {
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "attendu : {what}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// La boîte de dialogue d'enregistrement du test : l'« utilisateur » choisit (ou non) un fichier.
struct Chooser {
    choice: Mutex<Option<PathBuf>>,
    asked: Mutex<Vec<String>>,
}

impl Chooser {
    fn new(choice: Option<PathBuf>) -> Self {
        Self {
            choice: Mutex::new(choice),
            asked: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl SaveDialog for Chooser {
    async fn choose(&self, suggested_name: &str) -> Option<PathBuf> {
        self.asked.lock().unwrap().push(suggested_name.to_owned());
        self.choice.lock().unwrap().clone()
    }
}

struct Rig {
    agent: TestAgent,
    proxy: FaultProxy,
    runtime: Arc<LinkRuntime>,
    sink: Arc<Recorder>,
    server: String,
    fingerprint: String,
    _dir: tempfile::TempDir,
}

async fn rig(role: Role) -> Rig {
    let agent = TestAgent::install().await;
    agent.create_account("marie", role).await;
    let proxy = FaultProxy::start(agent.addr).await;
    let dir = tempfile::tempdir().unwrap();
    let vault = Arc::new(CredentialVault::new(Memory::default()));
    let config = LinkConfig {
        subscribe_audit: true,
        ..LinkConfig::default()
    };
    let runtime = Arc::new(
        LinkRuntime::open_with(dir.path(), vault, "poste-test/0.1", config)
            .await
            .unwrap(),
    );
    let sink = Arc::new(Recorder::default());
    let stream = runtime.manager().subscribe();
    let (forwarded, to) = (runtime.clone(), sink.clone());
    tokio::spawn(async move { forwarded.forward(stream, &*to).await });
    let probe = runtime
        .probe("127.0.0.1", Some(proxy.port()))
        .await
        .unwrap();
    let server = runtime
        .add_and_login(
            "Forge".into(),
            2,
            "127.0.0.1".into(),
            Some(proxy.port()),
            &probe.fingerprint,
            probe.mac_addresses,
            "marie",
            PASSWORD.into(),
            false,
            &*sink,
        )
        .await
        .unwrap();
    let id = server.id.clone();
    eventually("lien connecté", || {
        runtime
            .states()
            .iter()
            .any(|state| state.server_id == id && state.state == LinkStateName::Connected)
    })
    .await;
    Rig {
        agent,
        proxy,
        runtime,
        sink,
        server: id,
        fingerprint: probe.fingerprint,
        _dir: dir,
    }
}

fn filter() -> AuditFilterDto {
    AuditFilterDto::default()
}

#[tokio::test]
async fn a_page_is_typed_newest_first_with_the_utc_source_and_a_cursor_that_walks_on() {
    let rig = rig(Role::Admin).await;
    for n in 0..30 {
        rig.agent
            .create_account(&format!("compte-{n:02}"), Role::ReadOnly)
            .await;
    }
    let page = audit::read_page(&rig.runtime, &rig.server, filter(), None)
        .await
        .unwrap();
    // 30 créations, plus la création et la connexion de marie.
    assert_eq!(page.events.len(), 32);
    let first: &AuditEntryDto = &page.events[0];
    assert_eq!(first.action, "account.create");
    assert_eq!(first.target.as_deref(), Some("compte-29"));
    assert!(
        first.at.ends_with('Z'),
        "la source reste en UTC : {}",
        first.at
    );
    assert!(page.events.windows(2).all(|w| w[0].id > w[1].id));
    // Le JSON que reçoit l'interface : noms en camelCase, identifiant numérique.
    let json = serde_json::to_value(first).unwrap();
    assert!(json["id"].is_number());
    assert!(json["actionLabel"].is_string());
    assert!(json["repeatCount"].is_number());
    assert_eq!(json["origin"]["kind"], "client");
    // Page de 100 au plus : le curseur d'une page à l'autre.
    let all = audit::read_page(
        &rig.runtime,
        &rig.server,
        AuditFilterDto {
            outcomes: vec![AuditOutcomeDto::Ok],
            ..filter()
        },
        None,
    )
    .await
    .unwrap();
    assert!(all.next_before.is_none(), "peu d'entrées : une seule page");
}

#[tokio::test]
async fn filters_reach_the_agent_typed_and_the_kinds_of_the_spec_are_exact() {
    let rig = rig(Role::Admin).await;
    rig.agent.create_account("paul", Role::ReadOnly).await;
    let creations = audit::read_page(
        &rig.runtime,
        &rig.server,
        AuditFilterDto {
            kinds: vec![AuditKindDto::Accounts],
            ..filter()
        },
        None,
    )
    .await
    .unwrap();
    assert!(!creations.events.is_empty());
    assert!(
        creations
            .events
            .iter()
            .all(|e| e.action.starts_with("account."))
    );
    let searched = audit::read_page(
        &rig.runtime,
        &rig.server,
        AuditFilterDto {
            text: Some("PAUL".into()),
            ..filter()
        },
        None,
    )
    .await
    .unwrap();
    assert!(
        searched
            .events
            .iter()
            .any(|e| e.target.as_deref() == Some("paul"))
    );
}

#[tokio::test]
async fn an_invalid_filter_or_cursor_is_refused_before_anything_is_sent() {
    let rig = rig(Role::Admin).await;
    let invalid = LinkFailure::InvalidInput {
        field: InvalidField::Other,
    };
    for bad in [
        AuditFilterDto {
            text: Some("x".repeat(201)),
            ..filter()
        },
        AuditFilterDto {
            accounts: vec!["a,b".into()],
            ..filter()
        },
        AuditFilterDto {
            from_s: Some(f64::NAN),
            ..filter()
        },
        AuditFilterDto {
            from_s: Some(1_790_000_100.0),
            to_s: Some(1_790_000_000.0),
            ..filter()
        },
        AuditFilterDto {
            from_s: Some(-1.0),
            ..filter()
        },
    ] {
        let result = audit::read_page(&rig.runtime, &rig.server, bad, None).await;
        assert_eq!(result.unwrap_err(), invalid);
    }
    for cursor in [0.0, -3.0, 1.5, f64::NAN, f64::INFINITY] {
        let result = audit::read_page(&rig.runtime, &rig.server, filter(), Some(cursor)).await;
        assert_eq!(result.unwrap_err(), invalid, "curseur {cursor}");
    }
    let unknown = audit::read_page(&rig.runtime, "pas-un-serveur", filter(), None).await;
    assert_eq!(unknown.unwrap_err(), LinkFailure::UnknownServer);
}

#[tokio::test]
async fn a_read_only_account_gets_a_typed_refusal_and_no_file() {
    let rig = rig(Role::ReadOnly).await;
    let refused = audit::read_page(&rig.runtime, &rig.server, filter(), None)
        .await
        .unwrap_err();
    assert_eq!(refused, LinkFailure::Forbidden);
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("journal.csv");
    let chooser = Chooser::new(Some(target.clone()));
    let export = audit::export(&rig.runtime, &rig.server, filter(), &chooser)
        .await
        .unwrap_err();
    assert_eq!(export, LinkFailure::Forbidden);
    // Refusée : la boîte de dialogue n'a même pas été ouverte, rien n'est écrit.
    assert!(chooser.asked.lock().unwrap().is_empty());
    assert!(!target.exists());
}

#[tokio::test]
async fn the_export_goes_where_the_user_chose_and_nowhere_else() {
    let rig = rig(Role::Admin).await;
    rig.agent.create_account("-cmd-calc", Role::ReadOnly).await;
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("choisi par l-utilisateur.csv");
    let chooser = Chooser::new(Some(target.clone()));
    let result = audit::export(
        &rig.runtime,
        &rig.server,
        AuditFilterDto {
            kinds: vec![AuditKindDto::Accounts],
            ..filter()
        },
        &chooser,
    )
    .await
    .unwrap();
    assert!(result.saved);
    assert!(!result.truncated);
    assert_eq!(
        *chooser.asked.lock().unwrap(),
        vec!["journal-hearth.csv".to_owned()],
        "seul le nom SUGGÉRÉ vient de l'application"
    );
    let bytes = std::fs::read(&target).unwrap();
    assert!(bytes.starts_with(&[0xEF, 0xBB, 0xBF]), "UTF-8 avec BOM");
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.contains("Date et heure;Compte;Origine;Action;Cible;Résultat;Raison\r\n"));
    // Valeur piégée : neutralisée UNE fois (par l'agent), rien de plus à faire côté client.
    assert!(text.contains(";'-cmd-calc;"), "{text}");
    assert!(!text.contains("''-cmd-calc"));
    // Rien d'autre n'a été écrit à côté.
    let entries: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
    assert_eq!(entries.len(), 1);
}

fn names_in(dir: &std::path::Path) -> Vec<std::ffi::OsString> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect()
}

#[tokio::test]
async fn an_export_replaces_the_previous_file_whole_and_leaves_no_temporary_file() {
    let rig = rig(Role::Admin).await;
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("journal.csv");
    std::fs::write(&target, "ancien contenu ".repeat(10_000)).unwrap();
    let chooser = Chooser::new(Some(target.clone()));
    let result = audit::export(&rig.runtime, &rig.server, filter(), &chooser)
        .await
        .unwrap();
    assert!(result.saved);
    let text = std::fs::read_to_string(&target).unwrap();
    assert!(text.starts_with('\u{feff}'), "le nouveau fichier, entier");
    assert!(!text.contains("ancien contenu"));
    assert!(
        text.ends_with("\r\n"),
        "un fichier complet finit par une fin de ligne"
    );
    assert_eq!(names_in(dir.path()).len(), 1, "aucun fichier temporaire");
}

#[tokio::test]
async fn a_failed_write_keeps_what_was_there_and_leaves_no_temporary_file() {
    let rig = rig(Role::Admin).await;
    let dir = tempfile::tempdir().unwrap();
    // Le renommage final échoue (la destination est un dossier non vide) : rien n'est détruit.
    let target = dir.path().join("journal.csv");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("precieux.txt"), "à garder").unwrap();
    let chooser = Chooser::new(Some(target.clone()));
    let failed = audit::export(&rig.runtime, &rig.server, filter(), &chooser)
        .await
        .unwrap_err();
    assert_eq!(failed, LinkFailure::Storage);
    assert_eq!(
        std::fs::read_to_string(target.join("precieux.txt")).unwrap(),
        "à garder"
    );
    assert_eq!(names_in(dir.path()).len(), 1, "aucun fichier temporaire");
}

#[tokio::test]
async fn cancelling_the_save_dialog_writes_nothing_and_a_bad_destination_is_a_storage_failure() {
    let rig = rig(Role::Admin).await;
    let cancelled = audit::export(&rig.runtime, &rig.server, filter(), &Chooser::new(None))
        .await
        .unwrap();
    assert!(!cancelled.saved);
    // Une « destination » qui est un dossier : l'écriture échoue, l'échec est typé.
    let dir = tempfile::tempdir().unwrap();
    let failed = audit::export(
        &rig.runtime,
        &rig.server,
        filter(),
        &Chooser::new(Some(dir.path().to_owned())),
    )
    .await
    .unwrap_err();
    assert_eq!(failed, LinkFailure::Storage);
}

#[tokio::test]
async fn entries_written_by_the_agent_reach_the_window_as_link_audit_events() {
    let rig = rig(Role::Admin).await;
    let target = Target {
        host: "127.0.0.1".into(),
        port: rig.proxy.port(),
        pin: Pin::Pinned(Fingerprint::from_hex(&rig.fingerprint).unwrap()),
    };
    let transport = HttpTransport::new(HttpTransportConfig::default());
    let refused = transport
        .login(
            &target,
            &LoginRequest {
                username: "marie".into(),
                password: "Mauvais-mot-de-passe-1".into(),
            },
        )
        .await;
    assert!(refused.is_err());
    eventually("l'entrée en direct", || {
        !rig.sink.of(audit::EVENT).is_empty()
    })
    .await;
    let live = rig.sink.of(audit::EVENT);
    assert_eq!(live.len(), 1, "une seule fois");
    assert_eq!(live[0]["serverId"], rig.server.as_str());
    assert_eq!(live[0]["event"]["action"], "login");
    assert_eq!(live[0]["event"]["outcome"], "denied");
    // Et la lecture retrouve la même entrée, avec le même identifiant.
    let page = audit::read_page(&rig.runtime, &rig.server, filter(), None)
        .await
        .unwrap();
    let id = live[0]["event"]["id"].as_f64().unwrap();
    assert_eq!(page.events.iter().filter(|e| e.id == id).count(), 1);
}

#[tokio::test]
async fn a_read_only_account_receives_no_audit_event() {
    let rig = rig(Role::ReadOnly).await;
    let before = rig.sink.of("link://metrics").len();
    rig.agent.create_account("paul", Role::ReadOnly).await;
    // Pas de sommeil : on attend un FAIT, dix mesures de plus sur ce même flux APRÈS l'écriture de
    // l'entrée (si l'agent la lui envoyait, elle serait passée devant).
    eventually("dix mesures de plus sur le flux", || {
        rig.sink.of("link://metrics").len() >= before + 10
    })
    .await;
    assert!(rig.sink.of(audit::EVENT).is_empty());
}

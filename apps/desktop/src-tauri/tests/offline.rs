//! HRT-12 : la coquille contre un vrai agent (TLS 1.3, SQLite, WebSocket) à travers le mandataire à
//! pannes : ce que l'interface reçoit pendant une coupure (rien si elle est courte, « Reconnexion… »
//! puis « Hors ligne » sinon), les notifications système et l'icône de la zone de notification
//! qui en découlent, une action lancée à la coupure (résultat inconnu, jamais rejouée, issue au
//! retour du lien), refus hors « Connecté ».
//!
//! Déterminisme : aucune assertion de durée. Les seuils du lien qui n'ont pas à jouer sont
//! relevés à une heure ; chaque attente porte sur un fait observable ; l'agent retient les
//! actions tant que le test ne les relâche pas.
#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "../../../../crates/hearth-link/tests/support/agent.rs"]
mod agent;
#[path = "../../../../crates/hearth-link/tests/support/proxy.rs"]
mod proxy;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use agent::{PASSWORD, TestAgent};
use hearth_agent::domain::accounts::Role;
use hearth_desktop_lib::alerts::{Alerts, Notifier, TrayPort};
use hearth_desktop_lib::link::{LinkRuntime, UiSink};
use hearth_desktop_lib::link_dto::{ActionInput, ActionMethod, ActionResultDto, LinkFailure};
use hearth_desktop_lib::presence::TrayStatus;
use hearth_desktop_lib::vault::{CredentialBackend, CredentialVault};
use hearth_link::LinkConfig;
use hearth_link::domain::state::{LinkState, Thresholds};
use proxy::FaultProxy;
use serde_json::Value;

const GUARD: Duration = Duration::from_secs(60);

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

#[derive(Default)]
struct Screen(Mutex<Vec<(String, Value)>>);

impl UiSink for Screen {
    fn emit(&self, event: &str, payload: Value) {
        self.0.lock().unwrap().push((event.to_owned(), payload));
    }
}

impl Screen {
    fn mark(&self) -> usize {
        self.0.lock().unwrap().len()
    }

    /// Les états annoncés à l'écran depuis `mark`.
    fn states_since(&self, mark: usize) -> Vec<String> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .skip(mark)
            .filter(|(name, _)| name == "link://state")
            .map(|(_, payload)| payload["state"].as_str().unwrap().to_owned())
            .collect()
    }

    fn operations(&self) -> Vec<Value> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|(name, _)| name == "link://operation")
            .map(|(_, payload)| payload.clone())
            .collect()
    }
}

#[derive(Default)]
struct Spy {
    notes: Mutex<Vec<String>>,
    icons: Mutex<Vec<(TrayStatus, String)>>,
}

impl Notifier for Spy {
    fn notify(&self, _: &str, body: &str) {
        self.notes.lock().unwrap().push(body.to_owned());
    }
}

impl TrayPort for Spy {
    fn show(&self, status: TrayStatus, tooltip: &str) {
        self.icons
            .lock()
            .unwrap()
            .push((status, tooltip.to_owned()));
    }
}

impl Spy {
    fn notes(&self) -> Vec<String> {
        self.notes.lock().unwrap().clone()
    }

    fn last_status(&self) -> Option<TrayStatus> {
        self.icons.lock().unwrap().last().map(|(status, _)| *status)
    }
}

async fn eventually(what: &str, mut condition: impl FnMut() -> bool) {
    let started = Instant::now();
    while !condition() {
        assert!(started.elapsed() < GUARD, "attendu : {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Seuils du lien : `reconnecting` / `offline` selon ce que le scénario éprouve, sinon une heure.
fn config(reconnecting: bool, offline: bool) -> LinkConfig {
    let hour = Duration::from_secs(3_600);
    let scaled = Thresholds::scaled(6);
    LinkConfig {
        thresholds: Thresholds {
            silence: hour,
            reconnecting_after: if reconnecting {
                scaled.reconnecting_after
            } else {
                hour
            },
            offline_after: if offline {
                scaled.offline_after
            } else {
                hour * 2
            },
            ..scaled
        },
        attempt_timeout: Duration::from_secs(30),
        request_timeout: Duration::from_secs(30),
        ..LinkConfig::default()
    }
}

struct Rig {
    agent: TestAgent,
    proxy: FaultProxy,
    runtime: Arc<LinkRuntime>,
    screen: Arc<Screen>,
    spy: Arc<Spy>,
    clock: Arc<AtomicU64>,
    alerts: Arc<Alerts>,
    id: String,
    _dir: tempfile::TempDir,
}

async fn rig(config: LinkConfig) -> Rig {
    let agent = TestAgent::install().await;
    agent.create_account("marie", Role::Admin).await;
    let proxy = FaultProxy::start(agent.addr).await;
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(Memory::default());
    let vault = Arc::new(CredentialVault::new(Shared(secrets)));
    let runtime = Arc::new(
        LinkRuntime::open_with(dir.path(), vault, "poste-test/0.1", config)
            .await
            .unwrap(),
    );
    let screen = Arc::new(Screen::default());
    let stream = runtime.manager().subscribe();
    let (forwarded, to) = (runtime.clone(), screen.clone());
    tokio::spawn(async move { forwarded.forward(stream, &*to).await });
    let spy = Arc::new(Spy::default());
    let clock = Arc::new(AtomicU64::new(0));
    let reading = clock.clone();
    let alerts = Arc::new(Alerts::new(spy.clone(), spy.clone(), true, move || {
        reading.load(Ordering::SeqCst)
    }));
    runtime.set_observer(alerts.clone());
    let probe = runtime
        .probe("127.0.0.1", Some(proxy.port()))
        .await
        .unwrap();
    let server = runtime
        .add_and_login(
            "forge".into(),
            2,
            "127.0.0.1".into(),
            Some(proxy.port()),
            &probe.fingerprint,
            probe.mac_addresses,
            "marie",
            PASSWORD.into(),
            true,
            &*screen,
        )
        .await
        .unwrap();
    let id = server.id;
    let rig = Rig {
        agent,
        proxy,
        runtime,
        screen,
        spy,
        clock,
        alerts,
        id,
        _dir: dir,
    };
    rig.alerts.set_displayed(Some(&rig.id));
    rig.wait_for(LinkState::Connected).await;
    rig
}

impl Rig {
    async fn wait_for(&self, state: LinkState) {
        let id = hearth_link::domain::server::ServerId::parse(&self.id).unwrap();
        eventually(&format!("état {state:?}"), || {
            self.runtime.manager().state(&id).unwrap().state == state
        })
        .await;
    }

    async fn wait_attempts(&self, n: u64) {
        let target = self.proxy.accepted() + n;
        eventually("des tentatives de reconnexion", || {
            self.proxy.accepted() >= target
        })
        .await;
    }
}

/// Une action réelle de l'API : changer le mot de passe du compte (`current` doit être le bon).
fn change_password(current: &str, new: &str) -> ActionInput {
    ActionInput {
        method: ActionMethod::Put,
        path: "/me/password".into(),
        body: Some(format!(r#"{{"current":"{current}","password":"{new}"}}"#)),
    }
}

fn ping() -> ActionInput {
    change_password(PASSWORD, "New-Password-12")
}

#[tokio::test]
async fn a_cut_shorter_than_the_threshold_never_reaches_the_screen_nor_the_notifications() {
    // Aucun seuil ne peut être franchi : la coupure dure ce qu'elle dure, elle est « courte ».
    let rig = rig(config(false, false)).await;
    let mark = rig.screen.mark();
    let notes = rig.spy.notes().len();
    rig.proxy.cut();
    rig.wait_attempts(2).await;
    rig.proxy.heal();
    rig.wait_attempts(1).await;
    rig.wait_for(LinkState::Connected).await;
    assert_eq!(rig.screen.states_since(mark), Vec::<String>::new());
    assert_eq!(rig.spy.notes().len(), notes, "aucune notification");
    assert_eq!(rig.spy.last_status(), Some(TrayStatus::Connected));
}

#[tokio::test]
async fn a_long_cut_shows_reconnecting_then_offline_notifies_once_and_turns_the_icon_red_then_green()
 {
    let rig = rig(config(true, true)).await;
    let mark = rig.screen.mark();
    rig.proxy.cut();
    rig.wait_for(LinkState::Offline).await;
    // À l'écran : « Reconnexion… » puis « Hors ligne », dans cet ordre.
    let seen = rig.screen.states_since(mark);
    let mut dedup = seen.clone();
    dedup.dedup();
    assert_eq!(dedup, ["reconnecting", "offline"], "{seen:?}");
    // Une notification, l'icône rouge, l'infobulle dit lequel.
    assert_eq!(rig.spy.notes(), ["forge est hors ligne."]);
    assert_eq!(rig.spy.last_status(), Some(TrayStatus::Critical));
    assert_eq!(
        rig.spy.icons.lock().unwrap().last().unwrap().1,
        "Hearth : forge, Hors ligne"
    );
    // Les tentatives continuent : pas une notification de plus.
    rig.wait_attempts(2).await;
    assert_eq!(rig.spy.notes().len(), 1);
    // Le retour : icône verte, et la notification « de retour » (sa propre limite).
    rig.proxy.heal();
    rig.wait_for(LinkState::Connected).await;
    eventually("icône verte", || {
        rig.spy.last_status() == Some(TrayStatus::Connected)
    })
    .await;
    assert_eq!(
        rig.spy.notes(),
        ["forge est hors ligne.", "forge est de nouveau connecté."]
    );
    // Une deuxième coupure dans la minute : retenue, jamais perdue ; elle part à l'échéance.
    rig.proxy.cut();
    rig.wait_for(LinkState::Offline).await;
    assert_eq!(rig.spy.notes().len(), 2, "limite d'une par minute");
    rig.clock.store(60_000, Ordering::SeqCst);
    rig.alerts.tick();
    assert_eq!(
        rig.spy.notes().last().unwrap(),
        "forge est hors ligne. Le lien a changé 1 fois depuis la dernière alerte."
    );
}

#[tokio::test]
async fn an_action_is_refused_without_anything_sent_when_the_link_is_not_connected() {
    let rig = rig(config(true, true)).await;
    rig.proxy.cut();
    rig.wait_for(LinkState::Offline).await;
    let result = rig.runtime.execute(&rig.id, ping()).await;
    assert_eq!(result.unwrap_err(), LinkFailure::NotConnected);
}

#[tokio::test]
async fn an_action_cut_before_the_answer_is_unknown_never_replayed_and_its_outcome_comes_back() {
    let rig = rig(config(true, true)).await;
    // La réponse correcte quand le lien tient (le mot de passe devient `New-Password-12`).
    match rig.runtime.execute(&rig.id, ping()).await.unwrap() {
        ActionResultDto::Completed { status, .. } => assert_eq!(status, 200),
        other => panic!("réponse attendue, reçue {other:?}"),
    }
    // L'agent retient l'action : « en cours » côté agent tant que le test ne la relâche pas.
    rig.agent.hold_actions();
    let started = rig.agent.verifications_started();
    let runtime = rig.runtime.clone();
    let id = rig.id.clone();
    let call = tokio::spawn(async move {
        runtime
            .execute(
                &id,
                change_password("New-Password-12", "Another-Password-34"),
            )
            .await
    });
    rig.agent.wait_action_started(started).await;
    rig.proxy.cut();
    let ActionResultDto::Unknown { op_id } = tokio::time::timeout(GUARD, call)
        .await
        .expect("l'appel ne reste pas suspendu")
        .unwrap()
        .unwrap()
    else {
        panic!("résultat inconnu attendu");
    };
    // Jamais rejouée : l'agent n'a vu qu'une seule exécution de cette action.
    assert_eq!(rig.agent.verifications_started(), started + 1);
    // L'agent finit pendant la coupure ; le lien revient ; l'issue arrive à l'écran, une fois.
    rig.agent.release_actions();
    rig.agent.wait_operation_settled("marie", &op_id).await;
    rig.proxy.heal();
    eventually("issue de l'opération à l'écran", || {
        rig.screen
            .operations()
            .iter()
            .any(|event| event["opId"] == op_id.as_str())
    })
    .await;
    let events: Vec<Value> = rig
        .screen
        .operations()
        .into_iter()
        .filter(|event| event["opId"] == op_id.as_str())
        .collect();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["outcome"], "done", "« fait pendant la coupure »");
    assert_eq!(rig.agent.verifications_started(), started + 1);
}

#[tokio::test]
async fn an_invalid_action_is_refused_before_it_leaves_the_pc() {
    let rig = rig(config(false, false)).await;
    for path in ["me/password", "/../etc/passwd", "/a\nb"] {
        let bad = ActionInput {
            path: path.into(),
            ..ping()
        };
        assert!(matches!(
            rig.runtime.execute(&rig.id, bad).await.unwrap_err(),
            LinkFailure::InvalidInput { .. }
        ));
    }
    let bad_body = ActionInput {
        body: Some("{pas du json".into()),
        ..ping()
    };
    assert!(matches!(
        rig.runtime.execute(&rig.id, bad_body).await.unwrap_err(),
        LinkFailure::InvalidInput { .. }
    ));
}

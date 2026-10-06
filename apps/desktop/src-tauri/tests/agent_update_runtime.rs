//! HRT-17, lot interface : la mise à jour de l'agent de la coquille contre un VRAI agent (TLS 1.3,
//! SQLite, WebSocket ; seuls le téléchargement et la machine sont simulés par le banc de l'agent) :
//! l'état et la version disponible, la demande d'un administrateur avec la cible que la COQUILLE
//! retient, le refus de rôle de l'agent pour un compte Lecture seule, l'installation gérée, « déjà
//! en cours », les refus avant envoi (aucune cible, cible changée, rétrogradation : rien ne part), la
//! progression relayée à l'interface, et l'agent qui reste l'arbitre d'une cible dont la somme est
//! fausse.
//!
//! Déterminisme : aucune assertion de durée ; chaque attente porte sur un fait observable.
#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "../../../../crates/hearth-link/tests/support/agent.rs"]
mod agent;
#[path = "../../../../crates/hearth-link/tests/support/proxy.rs"]
mod proxy;
#[path = "../../../../crates/hearth-link/tests/support/update_rig.rs"]
mod update_rig;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use agent::{PASSWORD, TestAgent};
use hearth_agent::domain::accounts::Role;
use hearth_desktop_lib::agent_update::domain::{AgentCandidate, AgentTarget, validate_target};
use hearth_desktop_lib::agent_update::dto::{
    AgentUpdateOutcome, AgentUpdateOutcomeDto, AgentUpdateReasonDto, AgentUpdateRefusal,
    AgentUpdateStepDto, AgentUpdateView,
};
use hearth_desktop_lib::agent_update::service;
use hearth_desktop_lib::link::{LinkRuntime, UiSink};
use hearth_desktop_lib::link_dto::LinkFailure;
use hearth_desktop_lib::update::domain::DownloadPolicy;
use hearth_desktop_lib::vault::{CredentialBackend, CredentialVault};
use hearth_link::LinkConfig;
use hearth_link::domain::server::ServerId;
use hearth_link::domain::state::LinkState;
use proxy::FaultProxy;
use update_rig::{Rig, rig, sha256_hex};

const GUARD: Duration = Duration::from_secs(60);
const BINARY: &[u8] = b"nouvel agent";
const NOW: i64 = 1_790_000_100;

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

/// Ce que l'interface reçoit : chaque signal, avec sa charge.
#[derive(Default, Clone)]
struct Recorder(Arc<Mutex<Vec<(String, serde_json::Value)>>>);

impl UiSink for Recorder {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        self.0.lock().unwrap().push((event.to_owned(), payload));
    }
}

impl Recorder {
    /// Les étapes de mise à jour relayées, dans l'ordre, sans répétition consécutive.
    fn steps(&self) -> Vec<String> {
        let mut steps: Vec<String> = self
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|(event, _)| event == "agent-update://progress")
            .map(|(_, payload)| payload["progress"]["step"].as_str().unwrap().to_owned())
            .collect();
        steps.dedup();
        steps
    }

    fn last_progress(&self) -> Option<serde_json::Value> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .rfind(|(event, _)| event == "agent-update://progress")
            .map(|(_, payload)| payload.clone())
    }
}

async fn eventually(what: &str, mut condition: impl FnMut() -> bool) {
    let started = Instant::now();
    while !condition() {
        assert!(started.elapsed() < GUARD, "attendu : {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn config() -> LinkConfig {
    let hour = Duration::from_secs(3_600);
    let mut thresholds = hearth_link::domain::state::Thresholds::scaled(6);
    thresholds.silence = hour;
    thresholds.reconnecting_after = hour;
    thresholds.offline_after = hour * 2;
    LinkConfig {
        thresholds,
        attempt_timeout: Duration::from_secs(30),
        request_timeout: Duration::from_secs(30),
        persist_timeout: Duration::from_secs(120),
        ..LinkConfig::default()
    }
}

struct Client {
    runtime: Arc<LinkRuntime>,
    id: ServerId,
    ui: Recorder,
    dir: tempfile::TempDir,
}

async fn client(agent: &TestAgent, username: &str) -> Client {
    client_via(agent.addr.port(), username).await
}

async fn client_via(port: u16, username: &str) -> Client {
    let dir = tempfile::tempdir().unwrap();
    let vault = Arc::new(CredentialVault::new(Shared(Arc::new(Memory::default()))));
    let runtime = Arc::new(
        LinkRuntime::open_with(dir.path(), vault, "poste-test/0.1", config())
            .await
            .unwrap(),
    );
    let ui = Recorder::default();
    let probe = runtime.probe("127.0.0.1", Some(port)).await.unwrap();
    let server = runtime
        .add_and_login(
            "forge".into(),
            2,
            "127.0.0.1".into(),
            Some(port),
            &probe.fingerprint,
            probe.mac_addresses,
            username,
            PASSWORD.into(),
            false,
            &ui,
        )
        .await
        .unwrap();
    let id = ServerId::parse(&server.id).unwrap();
    // Le relais des événements vers l'interface (ce que fait la coquille au démarrage).
    let stream = runtime.manager().subscribe();
    let forwarder = runtime.clone();
    let sink = ui.clone();
    tokio::spawn(async move { forwarder.forward(stream, &sink).await });
    eventually("le lien connecté", || {
        runtime.manager().state(&id).unwrap().state == LinkState::Connected
    })
    .await;
    Client {
        runtime,
        id,
        ui,
        dir,
    }
}

/// La cible que la coquille retiendrait pour ce banc : l'adresse que le faux téléchargeur sert, la
/// signature de la clé jetable du banc, la somme du binaire.
fn target_for(rig: &Rig, version: &str, checksum: Option<String>) -> AgentTarget {
    validate_target(
        &AgentCandidate {
            version: version.into(),
            url: "https://exemple.org/hearth-agent-linux-x86_64".into(),
            signature: rig.keys.sign(BINARY),
            sha256: checksum.unwrap_or_else(|| sha256_hex(BINARY)),
        },
        &DownloadPolicy::host_for_tests("exemple.org"),
    )
    .unwrap()
}

struct World {
    agent: TestAgent,
    rig: Arc<Rig>,
}

async fn world(allowed: bool, gated: bool) -> World {
    let rig = Arc::new(rig(allowed, gated));
    let factory = rig.clone();
    let agent = TestAgent::install_with(Some(update_rig::factory(factory))).await;
    agent.create_account("marie", Role::Admin).await;
    agent.create_account("lucas", Role::ReadOnly).await;
    World { agent, rig }
}

impl Client {
    async fn view(&self, target: Option<&AgentTarget>) -> AgentUpdateView {
        service::view(self.runtime.manager(), &self.id, target, NOW, None)
            .await
            .unwrap()
    }

    async fn start(
        &self,
        target: Option<&AgentTarget>,
        version: &str,
    ) -> Result<AgentUpdateOutcome, LinkFailure> {
        service::start(self.runtime.manager(), &self.id, target, version).await
    }
}

fn refusal_of(outcome: Result<AgentUpdateOutcome, LinkFailure>) -> AgentUpdateRefusal {
    match outcome.unwrap() {
        AgentUpdateOutcome::Refused { refusal } => refusal,
        other => panic!("refus attendu, reçu {other:?}"),
    }
}

#[tokio::test]
async fn the_state_shows_the_version_of_the_agent_and_a_newer_version_from_the_feed() {
    let world = world(true, false).await;
    for username in ["marie", "lucas"] {
        let client = client(&world.agent, username).await;
        let newer = target_for(&world.rig, "0.2.0", None);
        let view = client.view(Some(&newer)).await;
        assert!(!view.current.is_empty());
        assert!(!view.managed && !view.in_progress);
        assert!(view.progress.is_none() && view.last.is_none());
        assert_eq!(view.available.unwrap().version, "0.2.0");
        // Sans cible : rien n'est disponible.
        assert!(client.view(None).await.available.is_none());
        // Pas plus récente que l'agent (la même, ou plus ancienne) : jamais proposée.
        let same = target_for(&world.rig, &view.current, None);
        assert!(client.view(Some(&same)).await.available.is_none());
        let older = target_for(&world.rig, "0.0.1", None);
        assert!(client.view(Some(&older)).await.available.is_none());
    }
}

#[tokio::test]
async fn an_administrator_starts_the_update_and_the_agent_gets_the_target_the_shell_holds() {
    let world = world(true, true).await;
    let client = client(&world.agent, "marie").await;
    let target = target_for(&world.rig, "0.2.0", None);
    let outcome = client.start(Some(&target), "0.2.0").await.unwrap();
    assert_eq!(
        outcome,
        AgentUpdateOutcome::Accepted {
            version: "0.2.0".into()
        }
    );
    // La progression arrive à l'interface, du téléchargement jusqu'au redémarrage.
    world.rig.release_gate();
    eventually("le redémarrage relayé à l'interface", || {
        client.ui.steps().last().map(String::as_str) == Some("restart")
    })
    .await;
    assert_eq!(
        client.ui.steps(),
        ["download", "verify", "install", "restart"]
    );
    let last = client.ui.last_progress().unwrap();
    assert_eq!(last["serverId"], client.id.as_str());
    assert_eq!(last["progress"]["version"], "0.2.0");
    // Ce que l'agent a téléchargé : l'adresse de la cible retenue par la coquille, pas une autre.
    let fetched = world.rig.downloader.fetched.lock().unwrap().clone();
    assert_eq!(fetched, [target.url().to_string()]);
    // En cours : la lecture le dit (lue pendant l'attente de l'agent).
    let view = client.view(Some(&target)).await;
    assert!(view.in_progress || view.progress.is_some());
}

#[tokio::test]
async fn a_second_request_while_one_runs_is_refused_by_the_agent() {
    let world = world(true, true).await;
    let client = client(&world.agent, "marie").await;
    let target = target_for(&world.rig, "0.2.0", None);
    client.start(Some(&target), "0.2.0").await.unwrap();
    // Porte fermée : la première est toujours en cours.
    let second = refusal_of(client.start(Some(&target), "0.2.0").await);
    assert_eq!(second, AgentUpdateRefusal::InProgress);
    let view = client.view(Some(&target)).await;
    assert!(view.in_progress);
    world.rig.release_gate();
}

#[tokio::test]
async fn a_read_only_account_is_refused_by_the_agent_with_the_typed_role_failure() {
    let world = world(true, false).await;
    let client = client(&world.agent, "lucas").await;
    let target = target_for(&world.rig, "0.2.0", None);
    let outcome = client.start(Some(&target), "0.2.0").await;
    assert_eq!(outcome.unwrap_err(), LinkFailure::Forbidden);
    // Rien n'a démarré chez l'agent.
    assert!(world.rig.downloader.fetched.lock().unwrap().is_empty());
    assert!(!client.view(Some(&target)).await.in_progress);
}

#[tokio::test]
async fn a_managed_installation_offers_no_update_and_the_agent_refuses_a_forced_request() {
    let world = world(false, false).await;
    let client = client(&world.agent, "marie").await;
    let target = target_for(&world.rig, "0.2.0", None);
    let view = client.view(Some(&target)).await;
    assert!(view.managed);
    assert!(
        view.available.is_none(),
        "installation gérée : aucun bouton à montrer"
    );
    assert_eq!(
        refusal_of(client.start(Some(&target), "0.2.0").await),
        AgentUpdateRefusal::ManagedInstall
    );
    assert!(world.rig.downloader.fetched.lock().unwrap().is_empty());
}

#[tokio::test]
async fn nothing_is_sent_without_a_target_for_another_version_or_for_a_downgrade() {
    let world = world(true, false).await;
    let client = client(&world.agent, "marie").await;
    let current = client.view(None).await.current;
    // Aucune cible dans le flux.
    assert_eq!(
        refusal_of(client.start(None, "0.2.0").await),
        AgentUpdateRefusal::NoTarget
    );
    // L'utilisateur a vu 0.2.0, le client retient maintenant autre chose.
    let held = target_for(&world.rig, "0.3.0", None);
    assert_eq!(
        refusal_of(client.start(Some(&held), "0.2.0").await),
        AgentUpdateRefusal::TargetChanged
    );
    assert_eq!(
        refusal_of(client.start(Some(&held), "n'importe quoi").await),
        AgentUpdateRefusal::TargetChanged
    );
    // Rétrogradation et même version : jamais envoyées.
    let older = target_for(&world.rig, "0.0.1", None);
    assert_eq!(
        refusal_of(client.start(Some(&older), "0.0.1").await),
        AgentUpdateRefusal::NotNewer
    );
    let same = target_for(&world.rig, &current, None);
    assert_eq!(
        refusal_of(client.start(Some(&same), &current).await),
        AgentUpdateRefusal::NotNewer
    );
    assert!(
        world.rig.downloader.fetched.lock().unwrap().is_empty(),
        "aucune demande n'est partie"
    );
}

#[tokio::test]
async fn the_agent_stays_the_arbiter_of_a_target_with_a_wrong_checksum() {
    let world = world(true, false).await;
    let client = client(&world.agent, "marie").await;
    // Le client ne peut pas savoir que la somme est fausse : il l'envoie, l'agent refuse après
    // téléchargement, rien n'est installé.
    let wrong = target_for(&world.rig, "0.2.0", Some("11".repeat(32)));
    assert!(matches!(
        client.start(Some(&wrong), "0.2.0").await.unwrap(),
        AgentUpdateOutcome::Accepted { .. }
    ));
    eventually("la fin de la mise à jour relayée", || {
        client
            .ui
            .last_progress()
            .is_some_and(|event| event["progress"]["step"] == "done")
    })
    .await;
    let done = client.ui.last_progress().unwrap();
    assert_eq!(done["progress"]["outcome"], "failed");
    assert_eq!(done["progress"]["reason"], "bad_checksum");
    // Le résultat se relit, avec ses codes typés, et il est « récent ».
    let view = service::view(
        client.runtime.manager(),
        &client.id,
        None,
        // « Maintenant » vu par le serveur : l'horloge pilotée du banc (1 790 000 000 s + 100 s).
        NOW,
        None,
    )
    .await
    .unwrap();
    let last = view.last.expect("un dernier résultat");
    assert_eq!(last.outcome, AgentUpdateOutcomeDto::Failed);
    assert_eq!(last.reason, Some(AgentUpdateReasonDto::BadChecksum));
    assert!(last.recent);
    assert_eq!(last.version.as_deref(), Some("0.2.0"));
    assert!(matches!(
        AgentUpdateStepDto::from(hearth_proto::api::update::UpdateStep::Done),
        AgentUpdateStepDto::Done
    ));
}

#[test]
fn a_result_is_recent_for_a_day_and_an_unreadable_or_future_date_is_not() {
    let now = 1_790_000_000;
    let at = |seconds_ago: i64| {
        time::OffsetDateTime::from_unix_timestamp(now - seconds_ago)
            .unwrap()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap()
    };
    assert!(service::is_recent(&at(0), now));
    assert!(service::is_recent(&at(23 * 3600), now));
    assert!(service::is_recent(&at(24 * 3600), now));
    assert!(!service::is_recent(&at(24 * 3600 + 1), now));
    // Une date un peu dans le futur est celle d'un serveur en avance : récente (tolérance explicite,
    // `a_server_clock_a_little_ahead_or_behind…`) ; au-delà de la tolérance, non.
    assert!(service::is_recent(&at(-5), now), "serveur en avance de 5 s");
    assert!(
        !service::is_recent(&at(-(2 * 24 * 3600)), now),
        "très loin dans le futur"
    );
    assert!(!service::is_recent("hier", now));
    assert!(!service::is_recent("", now));
}

#[tokio::test]
async fn a_result_never_seen_on_the_stream_is_read_as_not_announced_until_it_is_acknowledged() {
    let world = world(true, false).await;
    let client = client(&world.agent, "marie").await;
    // La mise à jour se termine chez l'agent sans que le client soit là pour voir `done` sur le flux.
    let wrong = target_for(&world.rig, "0.2.0", Some("11".repeat(32)));
    client.start(Some(&wrong), "0.2.0").await.unwrap();
    eventually("la fin chez l'agent", || {
        world.rig.host.with(|state| state.last.is_some())
    })
    .await;
    let read = |seen: Option<&str>| {
        let (runtime, id) = (client.runtime.clone(), client.id.clone());
        let seen = seen.map(str::to_owned);
        async move {
            service::view(runtime.manager(), &id, None, NOW, seen.as_deref())
                .await
                .unwrap()
        }
    };
    let first = read(None).await.last.expect("un résultat");
    assert!(
        !first.announced,
        "jamais annoncé tant que personne ne l'a noté"
    );
    // Noté par sa date : annoncé, y compris pour une lecture après un redémarrage du client.
    assert!(read(Some(&first.at)).await.last.unwrap().announced);
    // Un AUTRE résultat (autre date) n'est pas couvert par la note du précédent.
    assert!(
        !read(Some("2026-01-01T00:00:00Z"))
            .await
            .last
            .unwrap()
            .announced
    );
}

#[tokio::test]
async fn an_update_request_cut_before_its_answer_is_unknown_and_never_replayed() {
    let world = world(true, false).await;
    let proxy = FaultProxy::start(world.agent.addr).await;
    let client = client_via(proxy.port(), "marie").await;
    let target = target_for(&world.rig, "0.2.0", None);
    // Rien ne passe : la demande reste sans réponse ; on la coupe une fois son suivi écrit.
    proxy.freeze();
    let (runtime, id, held) = (client.runtime.clone(), client.id.clone(), target.clone());
    let call = tokio::spawn(async move { service::send(runtime.manager(), &id, &held).await });
    let tracking = client.dir.path().join("operations");
    eventually("le suivi de l'action écrit", || {
        std::fs::read_dir(&tracking).is_ok_and(|mut entries| entries.next().is_some())
    })
    .await;
    proxy.cut();
    let outcome = tokio::time::timeout(GUARD, call)
        .await
        .expect("l'appel ne reste pas suspendu")
        .unwrap()
        .unwrap();
    assert!(
        matches!(outcome, AgentUpdateOutcome::Unknown { .. }),
        "{outcome:?}"
    );
    // Jamais rejouée : le lien revient, l'agent n'a reçu aucune demande.
    proxy.heal();
    client.runtime.manager().retry_now(&client.id).unwrap();
    eventually("le lien revenu", || {
        client.runtime.manager().state(&client.id).unwrap().state == LinkState::Connected
    })
    .await;
    assert!(world.rig.downloader.fetched.lock().unwrap().is_empty());
    assert!(!client.view(Some(&target)).await.in_progress);
}

#[test]
fn a_server_clock_a_little_ahead_or_behind_never_loses_a_fresh_result() {
    // Horloges injectées : `at` est la date du SERVEUR, `now` celle du PC. Un serveur maison est
    // souvent en avance de quelques secondes ou minutes sur un PC Windows.
    let now = 1_790_000_000;
    let at = |server_minus_pc: i64| {
        time::OffsetDateTime::from_unix_timestamp(now + server_minus_pc)
            .unwrap()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap()
    };
    // Serveur en avance de 1 s et de 5 min : le résultat vient d'être écrit, il est récent.
    assert!(service::is_recent(&at(1), now), "serveur en avance de 1 s");
    assert!(
        service::is_recent(&at(300), now),
        "serveur en avance de 5 min"
    );
    // Serveur en retard de 5 min : le résultat a 5 minutes pour le PC.
    assert!(
        service::is_recent(&at(-300), now),
        "serveur en retard de 5 min"
    );
    // Au-delà de la tolérance (1 h) sur le futur, ou de 24 h sur le passé : pas « récent » (une ligne
    // d'historique) ; l'ANNONCE, elle, ne dépend pas de cette comparaison (store de l'interface).
    assert!(service::is_recent(
        &at(service::FUTURE_TOLERANCE_SECONDS),
        now
    ));
    assert!(!service::is_recent(
        &at(service::FUTURE_TOLERANCE_SECONDS + 1),
        now
    ));
    assert!(
        !service::is_recent(&at(2 * 24 * 3600), now),
        "serveur en avance de 2 jours"
    );
}

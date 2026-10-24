//! Arrêter un serveur de jeu en enregistrant son monde, de bout en bout (HRT-103, ADR-0052) : vrai agent en HTTPS,
//! vraie base, faux moteur dont le « jeu » lit son entrée, écrit dans ses journaux et sort. Un serveur Terraria est
//! INSTALLÉ par la route de l'installation (fiche dont la suite d'arrêt est `tested` : le seul mot que l'essai de
//! Terraria change), puis arrêté, redémarré, ou son arrêt est décidé.
//!
//! Règles : BR-JEU-001 à BR-JEU-007, BR-JEU-009, BR-JEU-011, BR-JEU-032, BR-JEU-034, BR-JEU-038. La preuve de clé et le
//! mot de passe des actes sont dans la matrice de `admin_reauth.rs`.

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
use hearth_proto::api::service_ops::{OperationCause, OperationStatus, OperationView};
use serde_json::json;
use support::device::Actor;
use support::engine::{fast_timings, parts_supervised};
use support::fake_engine::{Call, FakeEngine, PullScript, StdinReaction};
use support::fake_host::{FakeFreeSpace, FakeHostPorts};
use support::fake_safe_fs::FakeSafeFs;
use support::fake_supervisor::FakeSupervisor;
use support::https::{self, Agent, Wire};
use support::{Env, env};

const GIB: u64 = 1024 * 1024 * 1024;
const TERRARIA_FICHE: &str = include_str!("../catalog/terraria/fiche.toml");
const TERRARIA_SETTINGS: &str = include_str!("../catalog/terraria/settings.toml");
const IMAGE_DIGEST: &str =
    "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
/// Un mot de passe qui ne doit apparaître NULLE PART sauf dans le fichier du jeu.
const SENTINEL: &str = "Sentinelle-pw-77QaZ";

/// La fiche de Terraria avec l'état d'essai de sa suite d'arrêt et les retouches du test.
fn terraria(sequence: &str, change: impl FnOnce(String) -> String) -> Fiche {
    let text = TERRARIA_FICHE
        .replacen(
            "game_updates = \"never\"",
            &format!("digest = \"{IMAGE_DIGEST}\"\ngame_updates = \"never\""),
            1,
        )
        .replacen(
            "[stop.sequence]\nstatus = \"untested\"",
            &format!("[stop.sequence]\nstatus = \"{sequence}\""),
            1,
        );
    let text = change(text);
    load(
        Origin::Embedded,
        text.as_bytes(),
        TERRARIA_SETTINGS.as_bytes(),
    )
    .expect("Terraria valide")
}

/// Le seul mot que l'essai change : la suite d'arrêt est essayée.
fn tested(text: String) -> String {
    text
}

/// L'attente de 15 secondes devient l'attente d'un signe OBLIGATOIRE dans les journaux du jeu.
fn waits_for_sign(text: String) -> String {
    text.replacen(
        "kind = \"wait\"\nseconds = 15",
        "kind = \"wait_log\"\ncontains = \"World saved\"\ntimeout_s = 30\nrequired = true",
        1,
    )
}

/// L'image enregistre elle-même sur le signal (7 Days to Die) : aucune suite.
fn saves_by_itself(text: String) -> String {
    let start = text.find("[stop]").expect("section stop");
    let end = text.find("[resources]").expect("section resources");
    format!(
        "{}[stop]\ngrace_s = 120\nsignal = \"image_default\"\nworld_saved = \"by_image_on_signal\"\n\n{}",
        &text[..start],
        &text[end..]
    )
}

struct Bench {
    env: Env,
    engine: Arc<FakeEngine>,
    agent: Agent,
    fiche: Fiche,
    ada: Actor,
    dan: Actor,
}

async fn bench_with(fiche: Fiche) -> Bench {
    bench_timed(fiche, 1).await
}

/// Une « seconde de fiche » dure `unit_ms` millisecondes.
async fn bench_timed(fiche: Fiche, unit_ms: u64) -> Bench {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    // Les exécutions que le faux lance commencent une seconde APRÈS l'ordre (l'horloge de l'agent est figée).
    engine.set_info(Some(EngineInfo {
        cpus: 8,
        memory_bytes: 16 * GIB,
        storage_dir: Some("/var/lib/docker".into()),
    }));
    let mut parts = parts_supervised(
        engine.clone(),
        Arc::new(FakeSupervisor::new()),
        fast_timings(),
    );
    parts.lifecycle.game.unit = Duration::from_millis(unit_ms);
    parts.host_ports = Arc::new(FakeHostPorts::new());
    parts.free_space = Arc::new(FakeFreeSpace::new());
    parts.safe_fs = Some(Arc::new(FakeSafeFs::new()));
    parts.catalog.include_preview = true;
    parts.catalog.service = Some(Arc::new(CatalogService::from_parts(
        vec![fiche.clone()],
        Vec::new(),
        true,
    )));
    let agent = https::start_engined(&env, parts).await;
    let ada = agent.actor(&env, "ada", Role::Admin).await;
    let dan = agent.actor(&env, "dan", Role::ReadOnly).await;
    env.grant_services("ada").await;
    env.grant_services("dan").await;
    Bench {
        env,
        engine,
        agent,
        fiche,
        ada,
        dan,
    }
}

async fn bench() -> Bench {
    bench_with(terraria("tested", tested)).await
}

/// Le serveur installé : sa clé, l'identifiant de son conteneur.
struct Server {
    key: ServiceKey,
    id: String,
    /// Combien d'ordres le moteur avait reçus quand l'installation s'est terminée.
    calls_before: usize,
}

impl Bench {
    /// Installe un serveur Terraria par la route de l'installation ; son conteneur tourne.
    async fn install(&self) -> Server {
        let pinned = format!("ryshe/terraria:vanilla-1.4.5.8@{IMAGE_DIGEST}");
        self.engine.publish_image(&pinned, PullScript::two_layers());
        let agreements: Vec<String> = self
            .fiche
            .file
            .agreement
            .iter()
            .map(|a| a.id.clone())
            .collect();
        let body = json!({
            "name": "mon-terraria",
            "resources": { "cpu_millis": 1000, "memory_bytes": 2 * GIB },
            "source": {
                "type": "fiche", "id": self.fiche.id(), "version": self.fiche.file.version,
                "digest": self.fiche.digest_hex(), "agreements": agreements,
                "answers": [
                    {"id": "password", "value": SENTINEL},
                    {"id": "motd", "value": "Salut"}
                ]
            }
        });
        let reply = self
            .agent
            .confirmed(&self.ada, "POST", "/deployments", body)
            .await
            .send()
            .await;
        assert_eq!(reply.status, 202, "{}", reply.text);
        let accepted: DeployAccepted = serde_json::from_value(reply.body).expect("DeployAccepted");
        let operation = self.finished(&accepted.operation_id).await;
        assert_eq!(
            operation.status,
            OperationStatus::Succeeded,
            "{operation:?}"
        );
        let id = self.engine.container_ids().remove(0);
        // La « seconde » d'un jeu : l'heure du moteur et celle de l'agent (figée) sont les mêmes.
        Server {
            key: ServiceKey::parse(&accepted.service_key).expect("clé"),
            id,
            calls_before: self.engine.calls().len(),
        }
    }

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

    /// Attend que l'opération ne soit plus ni acceptée ni en cours.
    async fn finished(&self, id: &str) -> OperationView {
        self.until(id, |status| {
            !matches!(
                status,
                OperationStatus::Accepted
                    | OperationStatus::Running
                    | OperationStatus::AwaitingDecision
            )
        })
        .await
    }

    async fn until(&self, id: &str, done: impl Fn(OperationStatus) -> bool) -> OperationView {
        let started = Instant::now();
        loop {
            let operation = self.operation(id).await;
            if done(operation.status) {
                return operation;
            }
            assert!(
                started.elapsed() < Duration::from_secs(15),
                "l'opération n'arrive pas là : {operation:?}"
            );
            tokio::time::sleep(Duration::from_millis(15)).await;
        }
    }

    async fn gesture(&self, verb: &str, server: &Server) -> String {
        let reply = self
            .agent
            .confirmed(
                &self.ada,
                "POST",
                &format!("/services/{}/{verb}", server.key.as_str()),
                json!({}),
            )
            .await
            .send()
            .await;
        assert_eq!(reply.status, 202, "{}", reply.text);
        reply.body["action_id"].as_str().expect("action").to_owned()
    }

    async fn decide(&self, id: &str, choice: &str) -> Wire {
        self.agent
            .confirmed(
                &self.ada,
                "POST",
                &format!("/operations/services/{id}/decision"),
                json!({ "choice": choice }),
            )
            .await
            .send()
            .await
    }

    /// Les ordres reçus par le moteur depuis l'installation.
    fn calls(&self, server: &Server) -> Vec<Call> {
        self.engine.calls()[server.calls_before..].to_vec()
    }

    fn run_state(&self, server: &Server) -> Option<RunState> {
        self.agent
            .services()
            .snapshot()
            .services
            .iter()
            .find(|view| view.key == server.key)
            .map(|view| view.observation.run_state)
    }

    /// L'heure de l'agent (figée) en millisecondes, comme les lignes de journaux du moteur.
    fn now_ms(&self) -> i64 {
        use hearth_agent::application::ports::Clock;
        i64::try_from(self.env.clock.now().unix_timestamp_nanos() / 1_000_000).unwrap()
    }

    /// Le jeu lit « save » (et écrit son signe), puis « exit » (et sort après 20 ms).
    fn plays_like_terraria(&self, server: &Server, sign: bool) {
        self.engine.on_stdin(
            &server.id,
            StdinReaction {
                line: "save".into(),
                log: sign.then(|| (self.now_ms() + 1, "World saved".to_owned())),
                exit_after: None,
            },
        );
        self.engine.on_stdin(
            &server.id,
            StdinReaction {
                line: "exit".into(),
                log: None,
                exit_after: Some((Duration::from_millis(20), 0)),
            },
        );
    }

    /// Les entrées du journal d'activité de cycle de vie : action, résultat, cible, raison.
    async fn journal(&self) -> Vec<(String, String, Option<String>, Option<String>)> {
        self.agent.flush_journal().await;
        sqlx::query_as(
            "SELECT action, outcome, target, reason FROM audit_events
             WHERE action IN ('service.start', 'service.stop', 'service.restart') ORDER BY id",
        )
        .fetch_all(self.env.db.pool())
        .await
        .unwrap()
    }
}

fn stdin(server: &Server, line: &str) -> Call {
    Call::Stdin(server.id.clone(), line.to_owned())
}

fn stop(server: &Server, seconds: u64) -> Call {
    Call::Stop(server.id.clone(), Duration::from_secs(seconds))
}

// ---------------------------------------------------------------------------------------------
// Le premier test : la séquence de Terraria (BR-JEU-034)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn terraria_saves_waits_exits_then_the_engine_is_asked_to_stop_right_away() {
    let b = bench().await;
    let server = b.install().await;
    b.plays_like_terraria(&server, false);
    // La règle de redémarrage du serveur relance un programme qui sort de lui-même, sauf arrêt demandé au moteur.
    b.engine
        .relaunch_after_stop(&server.id, Duration::from_millis(60));
    let id = b.gesture("stop", &server).await;
    let operation = b.finished(&id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Succeeded,
        "{operation:?}"
    );
    assert_eq!(operation.cause, None, "arrêt propre : rien de forcé");
    // « save », l'attente, « exit », puis l'arrêt demandé au moteur avec le délai de la fiche : dans cet ordre.
    assert_eq!(
        b.calls(&server),
        vec![
            stdin(&server, "save"),
            stdin(&server, "exit"),
            stop(&server, 10)
        ]
    );
    // Le conteneur est arrêté, et reste arrêté.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(b.run_state(&server), Some(RunState::Exited));
    // Les étapes du plan : l'enregistrement, puis l'arrêt.
    let steps: Vec<_> = operation.steps.iter().map(|s| s.step.as_str()).collect();
    assert!(
        steps.contains(&"save_world") && steps.contains(&"stop"),
        "{steps:?}"
    );
    // Le journal d'activité dit ce qu'il est advenu de l'enregistrement, sans le texte d'un ordre (BR-JEU-032) :
    // aucun signe n'est déclaré, l'enregistrement n'est donc pas confirmé.
    let journal = b.journal().await;
    let last = journal.last().expect("une entrée");
    assert_eq!((last.0.as_str(), last.1.as_str()), ("service.stop", "ok"));
    assert_eq!(
        last.2.as_deref(),
        Some("service mon-terraria (enregistrement non confirmé)")
    );
}

#[tokio::test]
async fn the_sign_in_the_games_logs_makes_the_world_saved_before_the_exit_order() {
    let b = bench_with(terraria("tested", waits_for_sign)).await;
    let server = b.install().await;
    b.plays_like_terraria(&server, true);
    let id = b.gesture("stop", &server).await;
    assert_eq!(b.finished(&id).await.status, OperationStatus::Succeeded);
    assert_eq!(
        b.calls(&server),
        vec![
            stdin(&server, "save"),
            stdin(&server, "exit"),
            stop(&server, 10)
        ]
    );
    let journal = b.journal().await;
    assert_eq!(
        journal.last().unwrap().2.as_deref(),
        Some("service mon-terraria (monde enregistré)")
    );
}

// ---------------------------------------------------------------------------------------------
// Pas de confirmation : rien n'est arrêté, une décision (BR-JEU-006)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_sign_that_does_not_come_stops_nothing_and_asks_which_way_to_go() {
    let b = bench_with(terraria("tested", waits_for_sign)).await;
    let server = b.install().await;
    // Le jeu reçoit « save » mais n'écrit rien.
    b.plays_like_terraria(&server, false);
    let id = b.gesture("stop", &server).await;
    let waiting = b
        .until(&id, |status| status == OperationStatus::AwaitingDecision)
        .await;
    assert_eq!(waiting.decision.expect("choix").choices.len(), 2);
    // Rien n'est arrêté, ni quitté : seul « save » est parti.
    assert_eq!(b.calls(&server), vec![stdin(&server, "save")]);
    assert_eq!(b.run_state(&server), Some(RunState::Running));
    // « Arrêter quand même » : l'ordre de sortie que la fiche déclare part aussi (le canal marche, c'est le signe
    // qui manquait), puis l'arrêt du moteur.
    let decided = b.decide(&id, "continue").await;
    assert_eq!(decided.status, 200, "{}", decided.text);
    let operation = b.finished(&id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Succeeded,
        "{operation:?}"
    );
    assert_eq!(
        b.calls(&server),
        vec![
            stdin(&server, "save"),
            stdin(&server, "exit"),
            stop(&server, 10)
        ]
    );
    let journal = b.journal().await;
    assert!(
        journal
            .iter()
            .any(|e| e.2.as_deref() == Some("service mon-terraria (arrêt malgré tout)")),
        "{journal:?}"
    );
}

#[tokio::test]
async fn cancelling_the_decision_leaves_the_service_running_releases_the_lock_and_never_resends() {
    let b = bench_with(terraria("tested", waits_for_sign)).await;
    let server = b.install().await;
    b.plays_like_terraria(&server, false);
    let id = b.gesture("stop", &server).await;
    b.until(&id, |status| status == OperationStatus::AwaitingDecision)
        .await;
    let decided = b.decide(&id, "cancel").await;
    assert_eq!(decided.status, 200, "{}", decided.text);
    let operation = b.finished(&id).await;
    assert_eq!(operation.status, OperationStatus::Cancelled);
    // Le service est comme avant, rien n'a été renvoyé, l'arrêt voulu est oublié.
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(b.calls(&server), vec![stdin(&server, "save")]);
    assert_eq!(b.run_state(&server), Some(RunState::Running));
    let intents: Vec<(String,)> = sqlx::query_as("SELECT service_key FROM service_intents")
        .fetch_all(b.env.db.pool())
        .await
        .unwrap();
    assert!(intents.is_empty(), "{intents:?}");
    // Le verrou est rendu : un nouvel arrêt est accepté.
    b.engine.on_stdin(
        &server.id,
        StdinReaction {
            line: "save".into(),
            log: Some((b.now_ms() + 1, "World saved".into())),
            exit_after: None,
        },
    );
    let again = b.gesture("stop", &server).await;
    assert_ne!(again, id);
}

#[tokio::test]
async fn a_decision_left_unanswered_expires_in_15_minutes_the_service_keeps_running() {
    let b = bench_with(terraria("tested", waits_for_sign)).await;
    let server = b.install().await;
    b.plays_like_terraria(&server, false);
    let id = b.gesture("stop", &server).await;
    b.until(&id, |status| status == OperationStatus::AwaitingDecision)
        .await;
    b.env.clock.advance(time::Duration::minutes(14));
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        b.operation(&id).await.status,
        OperationStatus::AwaitingDecision
    );
    b.env.clock.advance(time::Duration::minutes(1));
    // L'expiration est celle du balayage des opérations, la seule (BR-JEU-006).
    assert_eq!(b.agent.ops().sweep().await.unwrap().expired, 1);
    let operation = b.finished(&id).await;
    assert_eq!(operation.status, OperationStatus::Cancelled);
    assert_eq!(operation.cause, Some(OperationCause::DecisionExpired));
    assert_eq!(b.calls(&server), vec![stdin(&server, "save")]);
    assert_eq!(b.run_state(&server), Some(RunState::Running));
}

#[tokio::test]
async fn an_untested_sequence_is_never_sent_the_stop_asks_for_a_decision() {
    // BR-JEU-038 : la fiche embarquée de Terraria déclare « save » et « exit » mais sa suite est `untested`.
    let b = bench_with(terraria("untested", tested)).await;
    let server = b.install().await;
    let id = b.gesture("stop", &server).await;
    let waiting = b
        .until(&id, |status| status == OperationStatus::AwaitingDecision)
        .await;
    assert!(waiting.decision.is_some());
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(b.calls(&server).is_empty(), "rien n'est envoyé ni arrêté");
    // L'entrée du conteneur est OUVERTE (la fiche déclare des ordres : l'ouvrir plus tard exigerait de le recréer),
    // mais rien n'y est écrit tant que la suite n'est pas essayée (BR-JEU-038).
    assert!(b.engine.created().iter().all(|plan| plan.stdin_open));
    assert!(b.engine.stdin_lines(&server.id).is_empty());
    b.decide(&id, "continue").await;
    let operation = b.finished(&id).await;
    assert_eq!(operation.status, OperationStatus::Succeeded);
    // « Arrêter quand même » est l'arrêt du moteur, sans aucun ordre.
    assert_eq!(b.calls(&server), vec![stop(&server, 10)]);
    let journal = b.journal().await;
    assert_eq!(
        journal.last().unwrap().2.as_deref(),
        Some("service mon-terraria (enregistrement impossible)")
    );
}

#[tokio::test]
async fn an_order_the_game_does_not_accept_is_a_decision_and_nothing_is_stopped() {
    // BR-JEU-007 : l'entrée du service n'est pas ouverte (ou le service ne répond pas).
    let b = bench().await;
    let server = b.install().await;
    b.engine.close_stdin(&server.id);
    let id = b.gesture("stop", &server).await;
    b.until(&id, |status| status == OperationStatus::AwaitingDecision)
        .await;
    assert!(b.calls(&server).is_empty());
    assert_eq!(b.run_state(&server), Some(RunState::Running));
    b.decide(&id, "continue").await;
    assert_eq!(b.finished(&id).await.status, OperationStatus::Succeeded);
    // L'ordre de sortie n'est pas retenté : le compte a choisi l'arrêt du moteur, rien de plus.
    assert_eq!(b.calls(&server), vec![stop(&server, 10)]);
    let journal = b.journal().await;
    assert_eq!(
        journal.last().unwrap().2.as_deref(),
        Some("service mon-terraria (enregistrement impossible)")
    );
}

#[tokio::test]
async fn a_game_that_saves_by_its_image_is_stopped_with_the_signal_and_no_order() {
    // BR-JEU-034, 7 Days to Die : aucun ordre, l'arrêt du moteur avec le délai de 120 secondes.
    let b = bench_with(terraria("untested", saves_by_itself)).await;
    let server = b.install().await;
    b.engine
        .update(&server.id, |c| c.stop_timeout_s = Some(120));
    b.agent.services().resync().await.expect("liste");
    let id = b.gesture("stop", &server).await;
    assert_eq!(b.finished(&id).await.status, OperationStatus::Succeeded);
    assert_eq!(b.calls(&server), vec![stop(&server, 120)]);
    assert!(b.engine.stdin_lines(&server.id).is_empty());
    let journal = b.journal().await;
    assert_eq!(
        journal.last().unwrap().2.as_deref(),
        Some("service mon-terraria (non nécessaire (le jeu enregistre lui-même))")
    );
}

#[tokio::test]
async fn an_adopted_service_receives_no_order_even_when_it_runs_the_same_game() {
    // BR-JEU-001 : un conteneur adopté, même nommé comme le jeu, s'arrête comme avant, sans ordre.
    let b = bench().await;
    let mut observation = support::fake_engine::container("terraria-forge", RunState::Running);
    observation.tty = false;
    let id = observation.id.clone();
    b.engine.put_silently(observation);
    b.agent.services().resync().await.expect("liste");
    let key = format!("c_{id}");
    b.env.adopt(&key, "terraria-forge", "ada").await;
    b.agent.services().reload_adopted().await;
    let before = b.engine.calls().len();
    let reply = b
        .agent
        .confirmed(&b.ada, "POST", &format!("/services/{key}/stop"), json!({}))
        .await
        .send()
        .await;
    assert_eq!(reply.status, 202, "{}", reply.text);
    let action = reply.body["action_id"].as_str().unwrap().to_owned();
    assert_eq!(b.finished(&action).await.status, OperationStatus::Succeeded);
    let calls = b.engine.calls()[before..].to_vec();
    assert_eq!(calls, vec![Call::Stop(id.clone(), Duration::from_secs(10))]);
    assert!(b.engine.stdin_lines(&id).is_empty());
}

// ---------------------------------------------------------------------------------------------
// Redémarrer, annuler, ne rien divulguer
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_restart_follows_the_same_sequence_then_starts_the_container_again() {
    // BR-JEU-035 : appliquer des réglages est un redémarrage ordinaire, avec la séquence d'arrêt de la fiche.
    let b = bench().await;
    let server = b.install().await;
    b.plays_like_terraria(&server, false);
    b.engine
        .relaunch_after_stop(&server.id, Duration::from_millis(60));
    let id = b.gesture("restart", &server).await;
    let operation = b.finished(&id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Succeeded,
        "{operation:?}"
    );
    assert_eq!(
        b.calls(&server),
        vec![
            stdin(&server, "save"),
            stdin(&server, "exit"),
            stop(&server, 10),
            Call::Start(server.id.clone())
        ]
    );
    assert_eq!(b.run_state(&server), Some(RunState::Running));
    let intents: Vec<(String,)> = sqlx::query_as("SELECT service_key FROM service_intents")
        .fetch_all(b.env.db.pool())
        .await
        .unwrap();
    assert!(
        intents.is_empty(),
        "un redémarrage laisse le service voulu en marche"
    );
}

#[tokio::test]
async fn cancelling_during_the_wait_stops_the_sequence_before_the_exit_order() {
    // Quatre pauses de 120 « secondes de fiche » de 10 ms : le compte annule pendant ces presque 5 secondes.
    let b = bench_timed(
        terraria("tested", |t| {
            t.replacen(
                "[[stop.sequence.step]]
kind = \"wait\"
seconds = 15",
                &"[[stop.sequence.step]]
kind = \"wait\"
seconds = 120

"
                .repeat(4),
                1,
            )
        }),
        10,
    )
    .await;
    let server = b.install().await;
    b.plays_like_terraria(&server, false);
    let id = b.gesture("stop", &server).await;
    b.until(&id, |_| b.engine.stdin_lines(&server.id).len() == 1)
        .await;
    let cancelled = b
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
    assert!(
        cancelled.status == 200 || cancelled.status == 202,
        "{}",
        cancelled.text
    );
    let operation = b.finished_or_cancelling(&id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Cancelled,
        "{operation:?}"
    );
    // « exit » n'est jamais parti, le service tourne toujours.
    assert_eq!(b.calls(&server), vec![stdin(&server, "save")]);
    assert_eq!(b.run_state(&server), Some(RunState::Running));
}

impl Bench {
    async fn finished_or_cancelling(&self, id: &str) -> OperationView {
        self.until(id, |status| {
            !matches!(
                status,
                OperationStatus::Accepted | OperationStatus::Running | OperationStatus::Cancelling
            )
        })
        .await
    }
}

#[tokio::test]
async fn no_order_text_and_no_password_reaches_the_activity_journal_or_the_operations() {
    // BR-JEU-016 : le journal nomme l'enregistrement, jamais le texte d'un ordre ; le mot de passe n'y est pas.
    let b = bench().await;
    let server = b.install().await;
    b.plays_like_terraria(&server, false);
    let id = b.gesture("stop", &server).await;
    b.finished(&id).await;
    b.agent.flush_journal().await;
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT COALESCE(target, '') || ' ' || COALESCE(reason, '') FROM audit_events",
    )
    .fetch_all(b.env.db.pool())
    .await
    .unwrap();
    for (row,) in &rows {
        assert!(!row.contains("exit") && !row.contains(SENTINEL), "{row}");
    }
    let ops: Vec<(String,)> =
        sqlx::query_as("SELECT plan_json || COALESCE(decision_json, '') FROM service_ops")
            .fetch_all(b.env.db.pool())
            .await
            .unwrap();
    for (row,) in &ops {
        assert!(
            !row.contains("exit") && !row.contains("\"save\"") && !row.contains(SENTINEL),
            "{row}"
        );
    }
}

#[tokio::test]
async fn an_agent_restart_in_the_middle_of_a_stop_interrupts_it_and_replays_nothing() {
    // BR-JEU-011 : l'opération restée en plein geste est `interrupted` au démarrage, son verrou est rendu, aucun
    // ordre n'est renvoyé au jeu ni au moteur ; le service tourne toujours.
    use hearth_agent::application::service_ops::AcceptRequest;
    use hearth_agent::domain::catalog::stop_plan::stop_plan;
    use hearth_agent::domain::ops::{GameStopKind, Gesture, OpKind, UndoInfo, op_plan};
    let b = bench().await;
    let server = b.install().await;
    let ops = b.agent.ops();
    let plan = op_plan(Gesture::Stop, &GameStopKind::of(stop_plan(&b.fiche))).unwrap();
    let id = ops
        .accept(AcceptRequest {
            op_key: "k-restart".into(),
            service: server.key.clone(),
            service_name: "mon-terraria".into(),
            kind: OpKind::Stop,
            account: "ada".into(),
            plan,
            forced: false,
            reserve_name: None,
            heavy_io: false,
        })
        .await
        .unwrap();
    ops.start(&id).await.unwrap();
    ops.intent(&id, "save_world", &UndoInfo::default())
        .await
        .unwrap();
    let observer = hearth_agent::infrastructure::ops_observer::UnobservedState;
    ops.recover_at_startup(&observer).await.unwrap();
    // Le processus est le même ici : le verrou en mémoire de l'ancien agent disparaîtrait avec lui.
    ops.locks().with(|table| table.release_all_of(&id));
    ops.rebuild_index().await.unwrap();
    let operation = b.operation(&id).await;
    assert_eq!(operation.status, OperationStatus::Interrupted);
    assert_eq!(operation.cause, Some(OperationCause::Interrupted));
    assert!(b.calls(&server).is_empty(), "rien n'est rejoué");
    assert_eq!(b.run_state(&server), Some(RunState::Running));
    // Le verrou est rendu : le compte peut arrêter de nouveau, de lui-même.
    b.plays_like_terraria(&server, false);
    let again = b.gesture("stop", &server).await;
    assert_eq!(b.finished(&again).await.status, OperationStatus::Succeeded);
}

// ---------------------------------------------------------------------------------------------
// Le mot de passe du serveur de jeu (BR-JEU-025, BR-JEU-026)
// ---------------------------------------------------------------------------------------------

impl Bench {
    async fn secret_as(&self, who: &Actor, key: &str) -> Wire {
        self.agent
            .request("GET", &format!("/services/{key}/game-secret"))
            .token(&who.token)
            .send()
            .await
    }
}

#[tokio::test]
async fn the_password_is_revealed_to_the_right_only_from_the_description_and_journaled_without_it()
{
    let b = bench().await;
    let server = b.install().await;
    let bob = b.agent.actor(&b.env, "bob", Role::Admin).await;
    // Un compte qui a le droit, quel que soit son rôle (dan est en lecture seule).
    for who in [&b.ada, &b.dan] {
        let reply = b.secret_as(who, server.key.as_str()).await;
        assert_eq!(reply.status, 200, "{}", reply.text);
        assert_eq!(reply.body["password"], SENTINEL);
        assert_eq!(reply.header("cache-control"), Some("no-store"));
    }
    // Un administrateur SANS le droit : refus, consigné.
    let denied = b.secret_as(&bob, server.key.as_str()).await;
    assert_eq!(denied.status, 403, "{}", denied.text);
    assert!(!denied.text.contains(SENTINEL));
    // Un service inconnu, ou adopté (sans description), ressemble à un service inconnu (BR-JEU-001).
    let unknown = b.secret_as(&b.ada, "h_01J9ZY0G3Q8M2K6W4T7V5N1B9D").await;
    assert_eq!(unknown.status, 404);
    b.agent.flush_journal().await;
    let rows: Vec<(String, String, Option<String>, String)> = sqlx::query_as(
        "SELECT outcome, account, target, COALESCE(details, '') FROM audit_events
         WHERE action = 'service.game.password.reveal' ORDER BY id",
    )
    .fetch_all(b.env.db.pool())
    .await
    .unwrap();
    let kinds: Vec<_> = rows.iter().map(|r| (r.0.as_str(), r.1.as_str())).collect();
    assert_eq!(
        kinds,
        vec![("ok", "ada"), ("ok", "dan"), ("denied", "bob")],
        "{rows:?}"
    );
    assert_eq!(rows[0].2.as_deref(), Some("service mon-terraria"));
    // Jamais la valeur, dans aucune entrée du journal d'activité.
    let all: Vec<(String,)> = sqlx::query_as(
        "SELECT COALESCE(target, '') || COALESCE(reason, '') || COALESCE(details, '') FROM audit_events",
    )
    .fetch_all(b.env.db.pool())
    .await
    .unwrap();
    assert!(all.iter().all(|(row,)| !row.contains(SENTINEL)));
}

#[tokio::test]
async fn a_server_installed_with_a_closed_input_gets_a_decision_never_an_order_sent_to_nowhere() {
    // Un Terraria installé avant que la fiche ouvre l'entrée : l'adaptateur refuse l'ordre, la suite demande une
    // décision, rien n'est écrit ni arrêté (BR-JEU-007).
    let b = bench().await;
    let server = b.install().await;
    b.engine.close_stdin(&server.id);
    let id = b.gesture("stop", &server).await;
    b.until(&id, |status| status == OperationStatus::AwaitingDecision)
        .await;
    assert!(b.engine.stdin_lines(&server.id).is_empty());
    assert!(b.calls(&server).is_empty());
}

#[tokio::test]
async fn cancelling_while_the_sign_is_awaited_closes_the_operation_at_once() {
    // Un signe obligatoire attendu 300 « secondes de fiche » de 10 ms (3 s) : l'annulation ne l'attend pas.
    let b = bench_timed(
        terraria("tested", |t| {
            waits_for_sign(t).replacen("timeout_s = 30", "timeout_s = 300", 1)
        }),
        10,
    )
    .await;
    let server = b.install().await;
    b.plays_like_terraria(&server, false);
    let id = b.gesture("stop", &server).await;
    b.until(&id, |_| b.engine.stdin_lines(&server.id).len() == 1)
        .await;
    let started = Instant::now();
    let cancelled = b
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
    assert!(
        cancelled.status == 200 || cancelled.status == 202,
        "{}",
        cancelled.text
    );
    let operation = b.finished_or_cancelling(&id).await;
    assert_eq!(
        operation.status,
        OperationStatus::Cancelled,
        "{operation:?}"
    );
    assert!(started.elapsed() < Duration::from_millis(2500));
    assert_eq!(b.calls(&server), vec![stdin(&server, "save")]);
}

#[tokio::test]
async fn a_cut_between_the_exit_order_and_the_engine_stop_leaves_the_wanted_stop_and_replays_nothing()
 {
    // BR-JEU-011 : l'agent s'arrête après l'ordre de sortie, l'arrêt voulu et l'étape `stop` sont déjà écrits.
    let b = bench().await;
    let server = b.install().await;
    b.plays_like_terraria(&server, false);
    // Le moteur ne répond jamais à l'arrêt : l'opération reste entre l'ordre et l'arrêt.
    b.engine.make_hang_on_stop(&server.id);
    let id = b.gesture("stop", &server).await;
    b.until(&id, |_| b.calls(&server).contains(&stop(&server, 10)))
        .await;
    let intents: Vec<(String,)> = sqlx::query_as("SELECT service_key FROM service_intents")
        .fetch_all(b.env.db.pool())
        .await
        .unwrap();
    assert_eq!(intents.len(), 1, "l'arrêt voulu est en base avant l'ordre");
    let calls = b.calls(&server);
    // Le redémarrage de l'agent.
    let ops = b.agent.ops();
    let observer = hearth_agent::infrastructure::ops_observer::UnobservedState;
    ops.recover_at_startup(&observer).await.unwrap();
    ops.locks().with(|table| table.release_all_of(&id));
    ops.rebuild_index().await.unwrap();
    let operation = b.operation(&id).await;
    assert_eq!(operation.status, OperationStatus::Interrupted);
    assert_eq!(operation.cause, Some(OperationCause::Interrupted));
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(b.calls(&server), calls, "rien n'est rejoué ni renvoyé");
    let steps: Vec<_> = operation.steps.iter().map(|s| s.step.as_str()).collect();
    assert!(steps.contains(&"stop"), "{steps:?}");
}

#[tokio::test]
async fn a_fiche_without_a_game_password_answers_200_without_one() {
    // BR-JEU-027 : pas de mot de passe de jeu dans la fiche, pas un service inconnu.
    let settings = TERRARIA_SETTINGS.replacen(
        "secret_kind = \"game_password\"",
        "secret_kind = \"admin_password\"",
        1,
    );
    let text = TERRARIA_FICHE.replacen(
        "game_updates = \"never\"",
        &format!("digest = \"{IMAGE_DIGEST}\"\ngame_updates = \"never\""),
        1,
    );
    let fiche = load(Origin::Embedded, text.as_bytes(), settings.as_bytes()).expect("valide");
    let b = bench_with(fiche).await;
    let server = b.install().await;
    let reply = b.secret_as(&b.ada, server.key.as_str()).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert!(reply.body.get("password").is_none(), "{}", reply.text);
}

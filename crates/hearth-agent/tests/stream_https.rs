//! Flux temps réel de bout en bout : vrai agent en HTTPS (TLS 1.3), vrai WebSocket, vraie base,
//! sondes simulées échantillonnées toutes les 20 ms (BR-DASH-001, 002, 011, 013).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use hearth_agent::application::ports::AuditFeed;
use hearth_agent::domain::accounts::Role;
use hearth_proto::api::metrics::Sample;
use hearth_proto::error::ErrorCode;
use hearth_proto::stream::{ServerMessage, SessionNotice, Topic};
use serde_json::{Value, json};
use support::https::{self, Agent};
use support::probe::{fast_stream, metering, metering_with};
use support::ws::{self, End, WsClient};
use support::{Env, PASSWORD, env};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tokio::sync::broadcast;

async fn token(env: &Env, agent: &Agent, username: &str, role: Role) -> String {
    env.create(username, role).await;
    let reply = agent
        .request("POST", "/sessions")
        .json(&json!({ "username": username, "password": PASSWORD }))
        .send()
        .await;
    assert_eq!(reply.status, 201, "{:?}", reply.body);
    reply.body["token"].as_str().unwrap().to_owned()
}

/// Flux ouvert, authentifié et abonné aux mesures ; le snapshot est déjà lu.
async fn subscribed(agent: &Agent, token: &str) -> (WsClient, Vec<Sample>) {
    let mut client = ws::open(agent).await;
    client.auth(token).await;
    client.subscribe(&[Topic::Metrics]).await;
    let ServerMessage::Snapshot { history, .. } = client.expect().await else {
        panic!("le snapshot vient en premier");
    };
    (client, history)
}

fn when(sample: &Sample) -> OffsetDateTime {
    OffsetDateTime::parse(&sample.at, &Rfc3339).unwrap()
}

/// Prochain échantillon `metrics` (les autres messages sont des échecs).
async fn metrics(client: &mut WsClient) -> Sample {
    match client.expect().await {
        ServerMessage::Metrics(sample) => sample,
        other => panic!("un échantillon était attendu, reçu {other:?}"),
    }
}

#[tokio::test]
async fn an_authenticated_client_gets_a_snapshot_then_metrics() {
    let env = env().await;
    let agent = https::start_metered(&env, metering()).await;
    let token = token(&env, &agent, "lucas", Role::ReadOnly).await;

    let mut client = ws::open(&agent).await;
    client.auth(&token).await;
    client.subscribe(&[Topic::Metrics]).await;

    let ServerMessage::Snapshot { machine, history } = client.expect().await else {
        panic!("le snapshot vient en premier");
    };
    assert_eq!(machine.name, "forge-test");
    assert!(machine.capabilities.gpu);
    assert!(!machine.capabilities.temps);
    assert_eq!(machine.disks[0].mount, "/");
    assert_eq!(machine.gpus[0].name, "Test GPU");

    let mut previous = history.last().map(when);
    for _ in 0..4 {
        let sample = metrics(&mut client).await;
        assert_eq!(sample.cores.len(), 4);
        assert_eq!(sample.gpus[0].load_percent, Some(12.0));
        assert_eq!(sample.gpus[0].temp_c, None, "température absente, pas zéro");
        assert_eq!(sample.mem.total_bytes, 16 << 30);
        let at = when(&sample);
        if let Some(previous) = previous {
            assert!(at > previous, "{at} doit suivre {previous}");
        }
        previous = Some(at);
    }
    agent.shutdown().await;
}

#[tokio::test]
async fn ping_is_answered_with_the_same_number_and_a_bad_message_does_not_close_the_stream() {
    let env = env().await;
    let agent = https::start_metered(&env, metering()).await;
    let token = token(&env, &agent, "lucas", Role::ReadOnly).await;
    let mut client = ws::open(&agent).await;
    client.auth(&token).await;

    client
        .send(&hearth_proto::stream::ClientMessage::Ping { n: 41 })
        .await;
    assert_eq!(client.expect().await, ServerMessage::Pong { n: 41 });

    client.send_text("{ pas du json").await;
    let ServerMessage::Error(error) = client.expect().await else {
        panic!("une erreur était attendue");
    };
    assert_eq!(error.code, ErrorCode::ValidationError);

    client.send_text(r#"{"type":"nope"}"#).await;
    assert!(matches!(client.expect().await, ServerMessage::Error(_)));

    client
        .send(&hearth_proto::stream::ClientMessage::Ping { n: 42 })
        .await;
    assert_eq!(client.expect().await, ServerMessage::Pong { n: 42 });
    agent.shutdown().await;
}

#[tokio::test]
async fn a_client_that_does_not_authenticate_first_is_refused_then_closed() {
    let env = env().await;
    let agent = https::start_metered(&env, metering()).await;
    let token = token(&env, &agent, "lucas", Role::ReadOnly).await;

    // Un premier message qui n'est pas `auth`.
    let mut client = ws::open(&agent).await;
    client.subscribe(&[Topic::Metrics]).await;
    let ServerMessage::Error(error) = client.expect().await else {
        panic!("une erreur était attendue");
    };
    assert_eq!(error.code, ErrorCode::Unauthenticated);
    assert_eq!(client.until_end().await, End::Closed(1008));

    // Un jeton illisible, puis un jeton inconnu.
    let mut client = ws::open(&agent).await;
    client.auth("pas-un-jeton").await;
    let ServerMessage::Error(error) = client.expect().await else {
        panic!("une erreur était attendue");
    };
    assert_eq!(error.code, ErrorCode::Unauthenticated);
    assert_eq!(client.until_end().await, End::Closed(1008));

    let mut client = ws::open(&agent).await;
    client.auth(&"ab".repeat(32)).await;
    let ServerMessage::Error(error) = client.expect().await else {
        panic!("une erreur était attendue");
    };
    assert_eq!(error.code, ErrorCode::SessionExpired);
    assert_eq!(client.until_end().await, End::Closed(1008));

    // Un client qui ne dit rien est coupé après le délai d'authentification.
    let mut silent = ws::open(&agent).await;
    let started = Instant::now();
    let ServerMessage::Error(error) = silent.expect().await else {
        panic!("une erreur était attendue");
    };
    assert_eq!(error.code, ErrorCode::Unauthenticated);
    assert!(started.elapsed() >= Duration::from_millis(300));
    assert_eq!(silent.until_end().await, End::Closed(1008));

    // Et rien de tout cela n'empêche un client légitime.
    let (mut good, _) = subscribed(&agent, &token).await;
    metrics(&mut good).await;
    agent.shutdown().await;
}

#[tokio::test]
async fn the_interface_version_applies_to_the_stream_too() {
    let env = env().await;
    let agent = https::start_metered(&env, metering()).await;
    assert_eq!(ws::connect(&agent, None).await.err().unwrap().status, 422);
    assert_eq!(
        ws::connect(&agent, Some("2")).await.err().unwrap().status,
        426
    );
    assert!(ws::connect(&agent, Some("1")).await.is_ok());
    agent.shutdown().await;
}

#[tokio::test]
async fn an_oversized_message_closes_the_stream() {
    let env = env().await;
    let agent = https::start_metered(&env, metering()).await;
    let token = token(&env, &agent, "lucas", Role::ReadOnly).await;
    let mut client = ws::open(&agent).await;
    client.auth(&token).await;
    client
        .send_text(&format!(
            r#"{{"type":"ping","n":1,"pad":"{}"}}"#,
            "x".repeat(8_000)
        ))
        .await;
    assert!(matches!(
        client.until_end().await,
        End::Closed(_) | End::Dropped
    ));
    agent.shutdown().await;
}

#[tokio::test]
async fn a_revoked_session_is_told_then_closed_during_the_stream() {
    let env = env().await;
    let agent = https::start_metered(&env, metering()).await;
    let account = env.create("lucas", Role::ReadOnly).await;
    let token = {
        let reply = agent
            .request("POST", "/sessions")
            .json(&json!({ "username": "lucas", "password": PASSWORD }))
            .send()
            .await;
        reply.body["token"].as_str().unwrap().to_owned()
    };
    let (mut client, _) = subscribed(&agent, &token).await;
    metrics(&mut client).await;

    env.service.revoke_sessions(&account.id).await.unwrap();
    let notice = loop {
        match client.expect().await {
            ServerMessage::Metrics(_) => {}
            other => break other,
        }
    };
    assert_eq!(
        notice,
        ServerMessage::Session {
            kind: SessionNotice::Revoked
        }
    );
    assert_eq!(client.until_end().await, End::Closed(1008));
    agent.shutdown().await;
}

#[tokio::test]
async fn an_expired_session_is_told_then_closed_during_the_stream() {
    let env = env().await;
    let agent = https::start_metered(&env, metering()).await;
    let token = token(&env, &agent, "lucas", Role::ReadOnly).await;
    let (mut client, _) = subscribed(&agent, &token).await;
    metrics(&mut client).await;

    env.clock.advance(time::Duration::days(31));
    let notice = loop {
        match client.expect().await {
            ServerMessage::Metrics(_) => {}
            other => break other,
        }
    };
    assert_eq!(
        notice,
        ServerMessage::Session {
            kind: SessionNotice::Expired
        }
    );
    assert_eq!(client.until_end().await, End::Closed(1008));
    agent.shutdown().await;
}

#[tokio::test]
async fn a_subscriber_that_never_reads_blocks_nobody() {
    let env = env().await;
    let agent = https::start_metered(&env, metering()).await;
    let token = token(&env, &agent, "lucas", Role::ReadOnly).await;
    let (_asleep, _) = subscribed(&agent, &token).await;
    let (mut reading, _) = subscribed(&agent, &token).await;

    // L'abonné qui lit reçoit 100 échantillons consécutifs (2 s de mesures) pendant que l'autre
    // laisse les siens s'entasser.
    let started = Instant::now();
    let mut seconds = Vec::new();
    for _ in 0..100 {
        seconds.push(metrics(&mut reading).await.uptime_s);
    }
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "l'échantillonnage n'a pas ralenti : {:?}",
        started.elapsed()
    );
    assert!(seconds.windows(2).all(|pair| pair[1] > pair[0]));
    // Aucune seconde n'est perdue pour celui qui lit.
    assert!(
        seconds.windows(2).all(|pair| pair[1] == pair[0] + 1),
        "trou dans la série : {seconds:?}"
    );
    agent.shutdown().await;
}

#[tokio::test]
async fn a_new_subscription_resumes_without_gap_or_duplicate() {
    let env = env().await;
    // 50 ms entre deux échantillons : marge confortable pour un test sans trou.
    let mut config = metering();
    config.period = Duration::from_millis(50);
    let agent = https::start_metered(&env, config).await;
    let token = token(&env, &agent, "lucas", Role::ReadOnly).await;

    let (mut client, first) = subscribed(&agent, &token).await;
    // L'historique du snapshot est continu : une seconde de plus à chaque échantillon.
    assert!(first.windows(2).all(|p| p[1].uptime_s == p[0].uptime_s + 1));
    for _ in 0..3 {
        metrics(&mut client).await;
    }

    // Le client se réabonne (ce que fait une reconnexion) : nouveau snapshot, puis le flux
    // reprend juste après son dernier échantillon.
    client.subscribe(&[Topic::Metrics]).await;
    let history = loop {
        match client.expect().await {
            ServerMessage::Metrics(_) => {}
            ServerMessage::Snapshot { history, .. } => break history,
            other => panic!("inattendu : {other:?}"),
        }
    };
    let last = history.last().expect("l'historique n'est pas vide");
    assert!(
        history
            .windows(2)
            .all(|p| p[1].uptime_s == p[0].uptime_s + 1)
    );
    for expected in last.uptime_s + 1..last.uptime_s + 6 {
        let sample = metrics(&mut client).await;
        assert!(when(&sample) > when(last));
        assert_eq!(sample.uptime_s, expected, "ni trou ni doublon");
    }
    agent.shutdown().await;
}

struct Feed(broadcast::Sender<Arc<Value>>);

impl AuditFeed for Feed {
    fn subscribe(&self) -> Option<broadcast::Receiver<Arc<Value>>> {
        Some(self.0.subscribe())
    }
}

#[tokio::test]
async fn the_audit_topic_is_for_administrators_and_carries_the_feed() {
    let env = env().await;
    let (sender, _) = broadcast::channel(16);
    let config = metering_with(Arc::new(Feed(sender.clone())), fast_stream());
    let agent = https::start_metered(&env, config).await;
    let readonly = token(&env, &agent, "lucas", Role::ReadOnly).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;

    let mut reader = ws::open(&agent).await;
    reader.auth(&readonly).await;
    reader.subscribe(&[Topic::Audit]).await;
    let ServerMessage::Error(error) = reader.expect().await else {
        panic!("une erreur était attendue");
    };
    assert_eq!(error.code, ErrorCode::ForbiddenRole);
    // Le flux reste ouvert pour le reste.
    reader
        .send(&hearth_proto::stream::ClientMessage::Ping { n: 1 })
        .await;
    assert_eq!(reader.expect().await, ServerMessage::Pong { n: 1 });

    let mut owner = ws::open(&agent).await;
    owner.auth(&admin).await;
    owner.subscribe(&[Topic::Audit]).await;
    // Laisse l'abonnement s'établir, puis publie un événement.
    owner
        .send(&hearth_proto::stream::ClientMessage::Ping { n: 2 })
        .await;
    assert_eq!(owner.expect().await, ServerMessage::Pong { n: 2 });
    sender
        .send(Arc::new(json!({ "id": 7, "action": "login" })))
        .unwrap();
    assert_eq!(
        owner.expect().await,
        ServerMessage::Audit {
            event: json!({ "id": 7, "action": "login" })
        }
    );
    agent.shutdown().await;
}

#[tokio::test]
async fn stopping_the_agent_closes_the_open_streams_cleanly_and_quickly() {
    let env = env().await;
    let agent = https::start_metered(&env, metering()).await;
    let token = token(&env, &agent, "lucas", Role::ReadOnly).await;
    let (mut client, _) = subscribed(&agent, &token).await;
    // Un second flux, encore sans authentification, est fermé lui aussi.
    let mut waiting = ws::open(&agent).await;

    let started = Instant::now();
    let (_, end, waiting_end) =
        tokio::join!(agent.shutdown(), client.until_end(), waiting.until_end());
    assert_eq!(end, End::Closed(1001));
    assert_eq!(waiting_end, End::Closed(1001));
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "l'arrêt n'attend pas le délai de grâce : {:?}",
        started.elapsed()
    );
}

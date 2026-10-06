//! Flux temps réel de bout en bout : vrai agent en HTTPS (TLS 1.3), vrai WebSocket, vraie base,
//! sondes simulées échantillonnées toutes les 20 ms (BR-DASH-001, 002, 011, 013).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use hearth_agent::domain::accounts::Role;
use hearth_proto::api::metrics::Sample;
use hearth_proto::error::ErrorCode;
use hearth_proto::stream::{ServerMessage, SessionNotice, Topic};
use serde_json::json;
use support::https::{self, Agent};
use support::probe::{FakeSystem, ToggleGpu, fast_stream, metering, metering_with};
use support::ws::{self, End, WsClient};
use support::{Env, PASSWORD, env};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

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

    env.service
        .revoke_sessions(&account.id, support::by())
        .await
        .unwrap();
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

/// Attend qu'une condition devienne vraie (délai large : seul un vrai blocage échoue).
async fn eventually<F: std::future::Future<Output = bool>>(
    what: &str,
    mut attempt: impl FnMut() -> F,
) {
    let started = Instant::now();
    while !attempt().await {
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "délai dépassé : {what}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn a_client_that_stops_reading_is_dropped_and_blocks_nobody() {
    let env = env().await;
    // Échantillons très gros (1 500 cœurs) toutes les 2 ms, envoi limité à 300 ms, deux flux au
    // plus : le client qui ne lit pas remplit les tampons TCP, l'envoi ne passe plus, il est
    // abandonné et sa place est rendue.
    let mut config = metering();
    config.system = Arc::new(FakeSystem::wide(1_500));
    config.period = Duration::from_millis(2);
    config.stream.send_timeout = Duration::from_millis(300);
    config.stream.max_total = 2;
    let agent = https::start_metered(&env, config).await;
    let token = token(&env, &agent, "lucas", Role::ReadOnly).await;

    let mut stuck = ws::connect_with(&agent, Some("1"), Some(1_024))
        .await
        .expect("flux ouvert");
    stuck.auth(&token).await;
    stuck.subscribe(&[Topic::Metrics]).await;
    // Il ne lira plus rien : `stuck` reste ouvert, jamais lu.

    let (mut reading, _) = subscribed(&agent, &token).await;
    let progress = Arc::new(AtomicU64::new(0));
    let seen = progress.clone();
    let reader = tokio::spawn(async move {
        while let Ok(message) = reading.next().await {
            if let ServerMessage::Metrics(sample) = message {
                seen.store(sample.uptime_s, Ordering::SeqCst);
            }
        }
    });

    // Les deux places de flux sont prises : un troisième flux authentifié est refusé tant que le
    // bloqué est là, puis accepté quand il est abandonné et sa place rendue.
    assert!(try_stream(&agent, &token).await.is_none());
    eventually(
        "le client bloqué est abandonné et sa place rendue",
        || async { try_stream(&agent, &token).await.is_some() },
    )
    .await;

    // Celui qui lit n'a pas été ralenti : ses échantillons n'ont cessé d'arriver.
    let before = progress.load(Ordering::SeqCst);
    eventually("le lecteur reçoit encore des échantillons", || async {
        progress.load(Ordering::SeqCst) > before
    })
    .await;
    assert!(!reader.is_finished(), "le lecteur n'a pas été coupé");
    drop(stuck);
    reader.abort();
    agent.shutdown().await;
}

/// Ouvre un flux authentifié et abonné aux mesures : `Some` s'il est accepté (snapshot reçu),
/// `None` s'il est refusé (ouverture refusée, ou erreur `BUSY` après `auth`).
async fn try_stream(agent: &Agent, token: &str) -> Option<WsClient> {
    let mut client = ws::connect(agent, Some("1")).await.ok()?;
    client.auth(token).await;
    client.subscribe(&[Topic::Metrics]).await;
    match client.next().await {
        Ok(ServerMessage::Snapshot { .. }) => Some(client),
        _ => None,
    }
}

#[tokio::test]
async fn authenticated_streams_are_capped_in_total_and_per_account() {
    let env = env().await;
    let mut config = metering();
    config.stream.max_total = 3;
    config.stream.max_per_account = 2;
    let agent = https::start_metered(&env, config).await;
    let lucas = token(&env, &agent, "lucas", Role::ReadOnly).await;
    let marie = token(&env, &agent, "marie", Role::ReadOnly).await;
    let paul = token(&env, &agent, "paul", Role::ReadOnly).await;

    let first = try_stream(&agent, &lucas).await.expect("1er flux de lucas");
    let _second = try_stream(&agent, &lucas).await.expect("2e flux de lucas");

    // Un troisième flux de lucas : refusé après l'authentification, au format d'erreur, BUSY.
    let mut third = ws::open(&agent).await;
    third.auth(&lucas).await;
    let ServerMessage::Error(error) = third.expect().await else {
        panic!("une erreur était attendue");
    };
    assert_eq!(error.code, ErrorCode::Busy);
    assert_eq!(third.until_end().await, End::Closed(1008));

    // Un autre compte passe tant que l'agent n'est pas plein.
    let _marie = try_stream(&agent, &marie).await.expect("flux de marie");
    // L'agent a 3 flux : un quatrième, même d'un autre compte, est refusé.
    assert!(try_stream(&agent, &paul).await.is_none());

    // Un flux fermé rend sa place.
    drop(first);
    eventually("une place de flux est rendue", || async {
        try_stream(&agent, &paul).await.is_some()
    })
    .await;
    agent.shutdown().await;
}

#[tokio::test]
async fn an_address_holds_at_most_two_waiting_connections_and_every_path_gives_them_back() {
    let env = env().await;
    let mut config = metering();
    config.stream.max_pending_per_address = 2;
    let agent = https::start_metered(&env, config).await;
    let status = |result: Result<WsClient, support::ws::Refused>| result.err().map(|r| r.status);

    // Deux connexions muettes tiennent les deux places d'attente de cette adresse.
    let mut silent_a = ws::open(&agent).await;
    let silent_b = ws::open(&agent).await;
    assert_eq!(status(ws::connect(&agent, Some("1")).await), Some(503));

    // Délai d'authentification dépassé : erreur, fermeture, place rendue.
    assert!(matches!(silent_a.expect().await, ServerMessage::Error(_)));
    assert_eq!(silent_a.until_end().await, End::Closed(1008));
    eventually("place rendue après le délai d'authentification", || async {
        ws::connect(&agent, Some("1")).await.is_ok()
    })
    .await;

    // Coupure du client : place rendue.
    drop(silent_b);
    let mut bad = loop {
        if let Ok(client) = ws::connect(&agent, Some("1")).await {
            break client;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    // Erreur d'authentification : message, fermeture, place rendue.
    bad.auth("pas-un-jeton").await;
    assert!(matches!(bad.expect().await, ServerMessage::Error(_)));
    assert_eq!(bad.until_end().await, End::Closed(1008));
    eventually("deux places rendues", || async {
        let one = ws::connect(&agent, Some("1")).await;
        let two = ws::connect(&agent, Some("1")).await;
        one.is_ok() && two.is_ok()
    })
    .await;
    agent.shutdown().await;
}

#[tokio::test]
async fn silent_anonymous_connections_never_take_a_stream_place() {
    let env = env().await;
    let mut config = metering();
    // Les anonymes ne sont pas coupés pendant le test ; les quotas d'attente sont ceux de
    // production (16 au total) ; un seul compte, deux places de flux.
    config.stream.auth_timeout = Duration::from_secs(60);
    config.stream.max_pending_per_address = 16;
    config.stream.max_total = 2;
    let agent = https::start_metered(&env, config).await;
    let token = token(&env, &agent, "lucas", Role::ReadOnly).await;

    // 15 anonymes muets (plus un client légitime : le quota d'attente est de 16).
    let mut silent = Vec::new();
    for _ in 0..15 {
        silent.push(ws::open(&agent).await);
    }
    // Le client légitime s'authentifie et obtient sa place de flux : les anonymes n'en
    // consomment aucune. Il en obtient même une deuxième dès qu'une place d'attente se libère.
    let mut legit = ws::open(&agent).await;
    legit.auth(&token).await;
    legit.subscribe(&[Topic::Metrics]).await;
    assert!(matches!(
        legit.expect().await,
        ServerMessage::Snapshot { .. }
    ));
    // Au-delà de 16 attentes, refus dès l'ouverture, 503 BUSY.
    let extra = ws::open(&agent).await;
    assert_eq!(
        ws::connect(&agent, Some("1")).await.err().map(|r| r.status),
        Some(503)
    );
    drop(extra);
    drop(silent);
    agent.shutdown().await;
}

#[tokio::test]
async fn an_unknown_address_never_takes_the_waiting_places_reserved_for_known_ones() {
    let env = env().await;
    let mut config = metering();
    // Quotas d'attente de production (16 au total, dont 4 réservées) ; une seule adresse peut les
    // prendre toutes ; les anonymes ne sont pas coupés pendant le test.
    config.stream.auth_timeout = Duration::from_secs(60);
    config.stream.max_pending_per_address = 16;
    let agent = https::start_metered(&env, config).await;
    let status = |result: Result<WsClient, support::ws::Refused>| result.err().map(|r| r.status);

    // Cette adresse n'a ni session ni connexion réussie : elle n'a droit qu'aux 12 places hors
    // réserve, la 13e est refusée (503 BUSY).
    let mut silent = Vec::new();
    for _ in 0..12 {
        silent.push(ws::open(&agent).await);
    }
    assert_eq!(status(ws::connect(&agent, Some("1")).await), Some(503));

    // Un client se connecte (session ouverte depuis cette adresse) : l'adresse est connue, elle
    // prend les places réservées (4). L'agent est alors plein pour tous.
    let _token = token(&env, &agent, "lucas", Role::ReadOnly).await;
    for _ in 0..4 {
        silent.push(ws::open(&agent).await);
    }
    assert_eq!(status(ws::connect(&agent, Some("1")).await), Some(503));

    drop(silent);
    agent.shutdown().await;
}

#[tokio::test]
async fn a_card_appearing_after_the_snapshot_gets_the_subscriber_a_new_snapshot() {
    let env = env().await;
    let gpu = Arc::new(ToggleGpu::default());
    let mut config = metering();
    config.gpu = gpu.clone();
    let agent = https::start_metered(&env, config).await;
    let token = token(&env, &agent, "lucas", Role::ReadOnly).await;

    let mut client = ws::open(&agent).await;
    client.auth(&token).await;
    client.subscribe(&[Topic::Metrics]).await;
    let ServerMessage::Snapshot { machine, .. } = client.expect().await else {
        panic!("le snapshot vient en premier");
    };
    assert!(!machine.capabilities.gpu);

    // Une carte apparaît : le client reçoit un nouveau snapshot avec la carte.
    gpu.set(&["RTX nouvelle"]);
    let updated = loop {
        match client.expect().await {
            ServerMessage::Metrics(_) => {}
            ServerMessage::Snapshot { machine, .. } => break machine,
            other => panic!("inattendu : {other:?}"),
        }
    };
    assert!(updated.capabilities.gpu);
    assert_eq!(updated.gpus[0].name, "RTX nouvelle");
    // Et le flux continue ensuite, sans nouveau snapshot tant que rien ne change.
    metrics(&mut client).await;
    agent.shutdown().await;
}

#[tokio::test]
async fn a_connection_subscribes_once_per_interval_without_being_closed() {
    let env = env().await;
    let mut config = metering();
    config.stream.min_subscribe_interval = Duration::from_secs(30);
    let agent = https::start_metered(&env, config).await;
    let token = token(&env, &agent, "lucas", Role::ReadOnly).await;
    let (mut client, _) = subscribed(&agent, &token).await;

    client.subscribe(&[Topic::Metrics]).await;
    let busy = loop {
        match client.expect().await {
            ServerMessage::Metrics(_) => {}
            other => break other,
        }
    };
    let ServerMessage::Error(error) = busy else {
        panic!("une erreur était attendue, reçu {busy:?}");
    };
    assert_eq!(error.code, ErrorCode::Busy);
    // Pas de second snapshot, le flux reste ouvert et l'abonnement d'origine continue.
    client
        .send(&hearth_proto::stream::ClientMessage::Ping { n: 9 })
        .await;
    loop {
        match client.expect().await {
            ServerMessage::Metrics(_) => {}
            ServerMessage::Pong { n } => {
                assert_eq!(n, 9);
                break;
            }
            other => panic!("inattendu : {other:?}"),
        }
    }
    agent.shutdown().await;
}

/// Un administrateur crée un compte par l'API : l'agent écrit l'événement et le diffuse.
async fn create_account(agent: &Agent, admin: &str, username: &str) {
    let reply = agent
        .request("POST", "/accounts")
        .token(admin)
        .json(&json!({ "username": username, "password": PASSWORD, "role": "readonly" }))
        .send()
        .await;
    assert_eq!(reply.status, 201, "{:?}", reply.body);
}

#[tokio::test]
async fn an_administrator_demoted_during_the_stream_loses_the_audit_topic() {
    let env = env().await;
    let agent = https::start_metered(&env, metering_with(fast_stream())).await;
    // Un deuxième administrateur : on ne rétrograde pas le dernier.
    let root = token(&env, &agent, "root", Role::Admin).await;
    let marie = env.create("marie", Role::Admin).await;
    let reply = agent
        .request("POST", "/sessions")
        .json(&json!({ "username": "marie", "password": PASSWORD }))
        .send()
        .await;
    let marie_token = reply.body["token"].as_str().unwrap().to_owned();

    let mut client = ws::open(&agent).await;
    client.auth(&marie_token).await;
    client.subscribe(&[Topic::Audit]).await;
    client
        .send(&hearth_proto::stream::ClientMessage::Ping { n: 1 })
        .await;
    assert_eq!(client.expect().await, ServerMessage::Pong { n: 1 });
    create_account(&agent, &root, "paul").await;
    let ServerMessage::Audit { event } = client.expect().await else {
        panic!("un événement du journal était attendu");
    };
    assert_eq!(
        (event.action.as_str(), event.target.as_deref()),
        ("account.create", Some("paul"))
    );

    // Marie devient lecture seule pendant le flux : son abonnement se perd, avec un message.
    env.service
        .change_role(&marie.id, Role::ReadOnly, support::by())
        .await
        .unwrap();
    let ServerMessage::Error(error) = client.expect().await else {
        panic!("une erreur était attendue");
    };
    assert_eq!(error.code, ErrorCode::ForbiddenRole);
    // Plus aucun événement du journal ne lui parvient (le prochain message est la réponse au ping).
    create_account(&agent, &root, "carl").await;
    client
        .send(&hearth_proto::stream::ClientMessage::Ping { n: 2 })
        .await;
    assert_eq!(client.expect().await, ServerMessage::Pong { n: 2 });
    agent.shutdown().await;
}

#[tokio::test]
async fn a_new_subscription_resumes_without_gap_or_duplicate() {
    let env = env().await;
    // 100 ms entre deux échantillons : une pause de la CI de plusieurs centaines de ms ne fait
    // pas perdre d'échantillon à l'abonné (le canal en garde 16).
    let mut config = metering();
    config.period = Duration::from_millis(100);
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

#[tokio::test]
async fn the_audit_topic_is_for_administrators_and_carries_the_account_creation() {
    let env = env().await;
    let agent = https::start_metered(&env, metering_with(fast_stream())).await;
    let readonly = token(&env, &agent, "lucas", Role::ReadOnly).await;
    let admin = token(&env, &agent, "marie", Role::Admin).await;

    // Un compte lecture seule ne peut pas s'abonner ; le flux reste ouvert pour le reste.
    let mut reader = ws::open(&agent).await;
    reader.auth(&readonly).await;
    reader.subscribe(&[Topic::Audit]).await;
    let ServerMessage::Error(error) = reader.expect().await else {
        panic!("une erreur était attendue");
    };
    assert_eq!(error.code, ErrorCode::ForbiddenRole);
    reader
        .send(&hearth_proto::stream::ClientMessage::Ping { n: 1 })
        .await;
    assert_eq!(reader.expect().await, ServerMessage::Pong { n: 1 });

    let mut owner = ws::open(&agent).await;
    owner.auth(&admin).await;
    owner.subscribe(&[Topic::Audit]).await;
    // Laisse l'abonnement s'établir, puis crée un compte.
    owner
        .send(&hearth_proto::stream::ClientMessage::Ping { n: 2 })
        .await;
    assert_eq!(owner.expect().await, ServerMessage::Pong { n: 2 });
    create_account(&agent, &admin, "paul").await;
    let ServerMessage::Audit { event } = owner.expect().await else {
        panic!("un événement du journal était attendu");
    };
    assert_eq!(event.action, "account.create");
    assert_eq!(event.action_label, "Création de compte");
    assert_eq!(event.account.as_deref(), Some("marie"));
    assert_eq!(event.target.as_deref(), Some("paul"));
    // Même forme que `GET /audit` : l'entrée relue par l'API est celle du flux.
    let listed = agent.request("GET", "/audit").token(&admin).send().await;
    assert_eq!(listed.body["events"][0]["id"], event.id);
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
        started.elapsed() < Duration::from_secs(4),
        "l'arrêt n'attend pas le délai de grâce : {:?}",
        started.elapsed()
    );
}

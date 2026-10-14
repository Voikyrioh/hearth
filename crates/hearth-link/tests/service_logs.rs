//! La lecture des journaux d'un service contre un VRAI agent (TLS 1.3, vrai WebSocket, faux moteur) :
//! première page, direct, reprise après coupure sans doublon ni perte, plafond du compte, place rendue à la
//! libération, refus sans le droit, hors connexion, débit élevé (HRT-67, `BR-SLOG-*`). Aucune attente de
//! durée : des attentes sur des faits observables.
//!
//! Règles : BR-SLOG-001, 007, 014, 018, 020, 026.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;

use hearth_agent::domain::services::RunState;
use hearth_agent::domain::services::logs::LogSource;
use hearth_link::{LinkError, LogEnd, LogOutput, ServiceLogReader};
use hearth_proto::api::service_logs::{LogState, LogsErrorCode};
use support::agent::fake_engine::{FakeEngine, container};
use support::{Options, WAIT, World};
use tokio::time::timeout;

/// Une minute avant l horloge de l agent de test (1 790 000 000 s) : la reprise ne refuse pas un curseur du futur.
const T0: i64 = 1_789_999_900_000;

struct Rig {
    world: World,
    engine: Arc<FakeEngine>,
}

/// Un agent connecté dont ces services sont adoptés par « marie » (qui a le droit de les lire).
async fn rig(names: &[&str]) -> Rig {
    rig_braked(names, None).await
}

async fn rig_braked(
    names: &[&str],
    brake: Option<hearth_agent::domain::services::BrakeSettings>,
) -> Rig {
    let engine = Arc::new(FakeEngine::new());
    for name in names {
        let mut web = container(name, RunState::Running);
        // Démarré avant les lignes de test : la lecture ne croit pas à un conteneur recréé.
        web.created_at = Some(time::OffsetDateTime::from_unix_timestamp(1_789_998_000).unwrap());
        web.started_at = Some(time::OffsetDateTime::from_unix_timestamp(1_789_999_000).unwrap());
        engine.put_silently(web);
    }
    let world = World::connected(Options {
        engine: Some(engine.clone()),
        brake,
        ..Options::default()
    })
    .await;
    world.agent.grant_services("marie").await;
    for name in names {
        let id = container(name, RunState::Running).id;
        world.agent.adopt_container(&id, name, "marie").await;
    }
    Rig { world, engine }
}

fn key_of(name: &str) -> String {
    format!("c_{}", container(name, RunState::Running).id)
}

fn push(engine: &FakeEngine, name: &str, from: usize, to: usize) {
    let id = container(name, RunState::Running).id;
    for n in from..to {
        engine.push_log(&id, LogSource::Out, T0 + n as i64, &format!("ligne {n}"));
    }
}

/// Lit jusqu'à ce que `done` dise vrai ; rend tous les messages lus.
async fn read_until(
    reader: &mut ServiceLogReader,
    mut done: impl FnMut(&[LogOutput]) -> bool,
) -> Vec<LogOutput> {
    let mut seen = Vec::new();
    let waited = timeout(WAIT, async {
        loop {
            match reader.next().await {
                Some(output) => seen.push(output),
                None => return,
            }
            if done(&seen) {
                return;
            }
        }
    })
    .await;
    assert!(waited.is_ok(), "attente des journaux dépassée : {seen:?}");
    seen
}

fn texts(seen: &[LogOutput]) -> Vec<String> {
    seen.iter()
        .filter_map(|output| match output {
            LogOutput::Lines(lines) => Some(lines.items.iter().map(|item| item.x.clone())),
            _ => None,
        })
        .flatten()
        .collect()
}

fn count(seen: &[LogOutput]) -> usize {
    texts(seen).len()
}

#[tokio::test]
async fn the_first_page_then_the_live_lines_arrive_in_order() {
    let rig = rig(&["web"]).await;
    push(&rig.engine, "web", 0, 3);
    let mut reader = rig
        .world
        .manager
        .open_service_logs(&rig.world.id, &key_of("web"))
        .unwrap();
    let seen = read_until(&mut reader, |seen| count(seen) >= 3).await;
    assert!(matches!(
        seen.first(),
        Some(LogOutput::Status(status)) if status.state == LogState::Live
    ));
    push(&rig.engine, "web", 3, 5);
    let more = read_until(&mut reader, |seen| count(seen) >= 2).await;
    let mut all = texts(&seen);
    all.extend(texts(&more));
    assert_eq!(all, ["ligne 0", "ligne 1", "ligne 2", "ligne 3", "ligne 4"]);
}

#[tokio::test]
async fn a_cut_link_resumes_by_the_cursor_without_a_duplicate_or_a_loss() {
    let rig = rig(&["web"]).await;
    push(&rig.engine, "web", 0, 10);
    let mut reader = rig
        .world
        .manager
        .open_service_logs(&rig.world.id, &key_of("web"))
        .unwrap();
    let first = read_until(&mut reader, |seen| count(seen) >= 10).await;

    // Le lien tombe, des lignes sont écrites pendant la coupure, le lien revient.
    rig.world.proxy.cut();
    let cut = read_until(&mut reader, |seen| {
        seen.iter().any(|o| matches!(o, LogOutput::Interrupted))
    })
    .await;
    assert!(
        cut.iter().all(|o| !matches!(o, LogOutput::Ended(_))),
        "la lecture ne se termine pas sur une coupure"
    );
    push(&rig.engine, "web", 10, 25);
    rig.world.proxy.heal();

    let resumed = read_until(&mut reader, |seen| count(seen) >= 15).await;
    let mut all = texts(&first);
    all.extend(texts(&resumed));
    let expected: Vec<String> = (0..25).map(|n| format!("ligne {n}")).collect();
    assert_eq!(
        all, expected,
        "ni doublon ni perte à la reprise : {resumed:?}"
    );
}

#[tokio::test]
async fn the_fourth_reading_of_an_account_is_refused_with_its_code_and_a_freed_place_is_given_back()
{
    // Les nombres de production : trois lectures par compte, douze ouvertures en rafale.
    let rig = rig_braked(
        &["a", "b", "c", "d"],
        Some(hearth_agent::domain::services::BrakeSettings::production()),
    )
    .await;
    let mut held = Vec::new();
    for name in ["a", "b", "c"] {
        let mut reader = rig
            .world
            .manager
            .open_service_logs(&rig.world.id, &key_of(name))
            .unwrap();
        read_until(&mut reader, |seen| {
            seen.iter().any(|o| matches!(o, LogOutput::Status(_)))
        })
        .await;
        held.push(reader);
    }
    let mut fourth = rig
        .world
        .manager
        .open_service_logs(&rig.world.id, &key_of("d"))
        .unwrap();
    let seen = read_until(&mut fourth, |seen| {
        seen.iter().any(|o| matches!(o, LogOutput::Ended(_)))
    })
    .await;
    assert!(
        seen.iter().any(|o| matches!(
            o,
            LogOutput::Ended(LogEnd::Refused {
                code: LogsErrorCode::LogsAccountLimit,
                ..
            })
        )),
        "{seen:?}"
    );
    // Une refusée ne se rouvre pas seule : la tâche est finie.
    assert!(timeout(WAIT, fourth.next()).await.unwrap().is_none());

    // Libérer une lecture rend sa place tout de suite.
    drop(held.pop());
    let mut again = rig
        .world
        .manager
        .open_service_logs(&rig.world.id, &key_of("d"))
        .unwrap();
    // L'agent voit la fermeture du socket libéré avec un léger retard : on attend la place.
    let seen = read_until(&mut again, |seen| {
        seen.iter()
            .any(|o| matches!(o, LogOutput::Status(_) | LogOutput::Ended(_)))
    })
    .await;
    assert!(
        matches!(seen.last(), Some(LogOutput::Status(_))),
        "la place libérée est rendue : {seen:?}"
    );
}

#[tokio::test]
async fn an_adopted_service_is_refused_to_an_account_without_the_right() {
    let rig = rig(&["web"]).await;
    rig.world.agent.revoke_services("marie").await;
    let mut reader = rig
        .world
        .manager
        .open_service_logs(&rig.world.id, &key_of("web"))
        .unwrap();
    let seen = read_until(&mut reader, |seen| {
        seen.iter().any(|o| matches!(o, LogOutput::Ended(_)))
    })
    .await;
    assert!(
        matches!(
            seen.last(),
            Some(LogOutput::Ended(LogEnd::Refused {
                code: LogsErrorCode::MissingRight,
                ..
            }))
        ),
        "{seen:?}"
    );
}

#[tokio::test]
async fn opening_needs_a_connected_link_and_a_service_key() {
    let rig = rig(&["web"]).await;
    assert!(matches!(
        rig.world
            .manager
            .open_service_logs(&rig.world.id, "pas une clé"),
        Err(LinkError::InvalidInput(_))
    ));
    rig.world.manager.logout(&rig.world.id).await.unwrap();
    assert!(matches!(
        rig.world
            .manager
            .open_service_logs(&rig.world.id, &key_of("web")),
        Err(LinkError::NotConnected)
    ));
}

#[tokio::test]
async fn a_high_rate_is_delivered_in_batches_without_loss_for_a_reader_that_follows() {
    let rig = rig(&["web"]).await;
    let mut reader = rig
        .world
        .manager
        .open_service_logs(&rig.world.id, &key_of("web"))
        .unwrap();
    read_until(&mut reader, |seen| {
        seen.iter().any(|o| matches!(o, LogOutput::Status(_)))
    })
    .await;
    const TOTAL: usize = 5_000;
    push(&rig.engine, "web", 0, TOTAL);
    let mut batches = 0usize;
    let mut lines = 0usize;
    let mut last = String::new();
    timeout(WAIT, async {
        while lines < TOTAL {
            match reader.next().await {
                Some(LogOutput::Lines(batch)) => {
                    batches += 1;
                    lines += batch.items.len();
                    last = batch.items.last().unwrap().x.clone();
                }
                Some(LogOutput::Gap(_)) | Some(LogOutput::Status(_)) => {}
                other => panic!("inattendu : {other:?}"),
            }
        }
    })
    .await
    .expect("débit élevé non livré");
    assert_eq!(last, format!("ligne {}", TOTAL - 1));
    assert!(
        batches < TOTAL / 4,
        "les lignes arrivent par lots : {batches}"
    );
}

#[tokio::test]
async fn nothing_of_a_line_appears_in_the_debug_of_what_a_reader_gives() {
    let rig = rig(&["web"]).await;
    let id = container("web", RunState::Running).id;
    rig.engine
        .push_log(&id, LogSource::Out, T0, "SENTINEL-TEXTE-DE-LIGNE-91f2");
    let mut reader = rig
        .world
        .manager
        .open_service_logs(&rig.world.id, &key_of("web"))
        .unwrap();
    let seen = read_until(&mut reader, |seen| count(seen) >= 1).await;
    let shown = format!("{seen:?} {reader:?}");
    assert!(!shown.contains("SENTINEL"), "{shown}");
}

#[tokio::test]
async fn too_many_openings_in_a_row_meet_the_brake_with_its_wait_and_it_is_not_retried() {
    let rig = rig_braked(
        &["web"],
        Some(hearth_agent::domain::services::BrakeSettings::production()),
    )
    .await;
    let key = key_of("web");
    let mut refusal = None;
    for _ in 0..14 {
        let mut reader = rig
            .world
            .manager
            .open_service_logs(&rig.world.id, &key)
            .unwrap();
        let seen = read_until(&mut reader, |seen| {
            seen.iter()
                .any(|o| matches!(o, LogOutput::Status(_) | LogOutput::Ended(_)))
        })
        .await;
        if let Some(LogOutput::Ended(end)) = seen.last() {
            refusal = Some((*end, reader));
            break;
        }
        // Libérée tout de suite : seul le frein des ouvertures est éprouvé, pas le plafond.
        drop(reader);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let (end, mut reader) = refusal.expect("le frein doit arrêter la rafale");
    match end {
        LogEnd::Refused {
            code: LogsErrorCode::LogsRateLimited,
            retry_after_s: Some(wait),
        } => assert!((1..=5).contains(&wait), "{wait}"),
        other => panic!("attendu le frein avec son délai : {other:?}"),
    }
    // Aucune reprise automatique : la tâche est finie, rien ne repart seul.
    assert!(timeout(WAIT, reader.next()).await.unwrap().is_none());
}

//! L'heure écoulée à l'ouverture (HRT-18, BR-DASH-010, ADR-0015 §5) : à la connexion, `hearth-link` lit
//! `GET /metrics/history?window=1h` et annonce (`Event::History`) ce qui précède l'instantané du flux, pour que
//! la courbe d'une heure soit déjà remplie. Contre un VRAI agent ; seule la réponse de l'heure est scriptée
//! (l'anneau d'un agent qui vient de démarrer est presque vide). Aucune assertion de durée.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use hearth_link::domain::event::Event;
use hearth_link::domain::state::LinkState;
use hearth_proto::api::metrics::{HistoryResponse, HistoryWindow, MemorySample, Sample};
use support::{Options, WAIT, World};

fn sample(at: &str) -> Sample {
    Sample {
        at: at.into(),
        uptime_s: 1,
        cpu: 42.0,
        cores: vec![],
        mem: MemorySample {
            used_bytes: 1,
            total_bytes: 2,
        },
        disks: vec![],
        net: None,
        gpus: vec![],
        temps: vec![],
    }
}

fn hour(samples: Vec<Sample>) -> HistoryResponse {
    HistoryResponse {
        window: HistoryWindow::OneHour,
        step_s: 10,
        samples,
    }
}

#[tokio::test]
async fn the_hour_before_the_snapshot_is_announced_after_the_snapshot_at_the_opening() {
    // Deux échantillons d'il y a bien longtemps : plus anciens que tout instantané. (Ce qui n'est pas plus
    // ancien que l'instantané est écarté : `domain::history::tests`.)
    let world = World::connected(Options {
        hour: Some(hour(vec![
            sample("2020-01-01T00:00:00Z"),
            sample("2020-01-01T00:00:10Z"),
        ])),
        ..Options::default()
    })
    .await;
    let (_, event) = world
        .recorder
        .wait_for(0, "l'heure écoulée", WAIT, |event| {
            matches!(event, Event::History { .. })
        })
        .await;
    let Event::History { samples, .. } = event else {
        unreachable!()
    };
    assert_eq!(samples.len(), 2);
    assert_eq!(samples[0].at, "2020-01-01T00:00:00Z");
    // Annoncée APRÈS l'instantané de la même connexion.
    let log = world.recorder.since(0);
    let snapshot = log
        .iter()
        .position(|(_, event)| matches!(event, Event::Snapshot { .. }))
        .expect("instantané");
    let history = log
        .iter()
        .position(|(_, event)| matches!(event, Event::History { .. }))
        .expect("heure");
    assert!(snapshot < history, "l'heure suit l'instantané");
}

#[tokio::test]
async fn a_failed_read_of_the_hour_never_prevents_the_connection() {
    let world = World::connected(Options {
        hour_fails: true,
        ..Options::default()
    })
    .await;
    assert_eq!(world.state().state, LinkState::Connected);
    assert!(
        world
            .recorder
            .since(0)
            .iter()
            .all(|(_, event)| !matches!(event, Event::History { .. })),
        "aucune heure annoncée"
    );
}

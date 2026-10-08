//! L'heure écoulée à l'ouverture (HRT-18, BR-DASH-010, ADR-0015 §5) : à la connexion, `hearth-link` lit
//! `GET /metrics/history?window=1h` et annonce (`Event::History`) ce qui précède l'instantané du flux, pour que
//! la courbe d'une heure soit déjà remplie. Contre un VRAI agent ; seule la réponse de l'heure est scriptée
//! (l'anneau d'un agent qui vient de démarrer est presque vide). L'heure est lue À CÔTÉ de la connexion, jamais dans
//! la tentative : une route muette ou lente ne retarde ni « Connecté » ni le direct. Aucune assertion de durée.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use hearth_link::domain::event::Event;
use hearth_link::domain::state::LinkState;
use hearth_proto::api::metrics::{HistoryResponse, HistoryWindow, MemorySample, Sample};
use std::sync::atomic::Ordering;
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
        peaks: Vec::new(),
    }
}

#[tokio::test]
async fn the_hour_before_the_snapshot_is_announced_after_the_snapshot_at_the_opening() {
    // Deux échantillons d'il y a bien longtemps : plus anciens que tout instantané. (Ce qui n'est pas plus
    // ancien que l'instantané est écarté : `domain::history::tests`.)
    let world = World::connected(Options {
        hours: vec![hour(vec![
            sample("2020-01-01T00:00:00Z"),
            sample("2020-01-01T00:00:10Z"),
        ])],
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

fn history_events(world: &World) -> Vec<Vec<String>> {
    world
        .recorder
        .since(0)
        .iter()
        .filter_map(|(_, event)| match event {
            Event::History { samples, .. } => {
                Some(samples.iter().map(|sample| sample.at.clone()).collect())
            }
            _ => None,
        })
        .collect()
}

/// B1 : une route d'historique muette ne retarde ni « Connecté » ni le direct, ne fait échouer aucune
/// tentative ; une fois qu'elle répond, l'heure est recollée après coup.
#[tokio::test]
async fn a_mute_history_route_never_delays_the_connection_and_the_hour_is_pasted_when_it_answers() {
    let world = World::connected(Options {
        hours: vec![hour(vec![sample("2020-01-01T00:00:00Z")])],
        hour_hold_first: true,
        ..Options::default()
    })
    .await;
    // `World::connected` a rendu : « Connecté » ET une mesure du direct reçue, la lecture de l'heure tient encore.
    assert_eq!(world.state().state, LinkState::Connected);
    assert!(
        history_events(&world).is_empty(),
        "pas d'historique tant que la route se tait"
    );
    // Après le premier « Connecté », aucun état « Reconnexion » ni « Hors ligne » : aucune tentative n'a échoué.
    let states: Vec<LinkState> = world
        .recorder
        .since(0)
        .iter()
        .filter_map(|(_, event)| match event {
            Event::State { info, .. } => Some(info.state),
            _ => None,
        })
        .collect();
    let connected = states
        .iter()
        .position(|state| *state == LinkState::Connected)
        .expect("connecté");
    assert!(
        states[connected..]
            .iter()
            .all(|state| *state == LinkState::Connected),
        "aucune tentative échouée : {states:?}"
    );
    // La route répond enfin : l'heure arrive après coup, le lien n'a pas bougé.
    world.spy.hour_release.notify_one();
    world
        .recorder
        .wait_for(0, "l'heure écoulée", WAIT, |event| {
            matches!(event, Event::History { .. })
        })
        .await;
    assert_eq!(
        history_events(&world),
        vec![vec!["2020-01-01T00:00:00Z".to_owned()]]
    );
    assert_eq!(world.state().state, LinkState::Connected);
}

/// B1 : le lien retombe pendant la lecture : elle est abandonnée, rien n'est annoncé pour l'ancienne session ;
/// la session suivante lit sa propre heure.
#[tokio::test]
async fn a_read_in_flight_when_the_link_drops_is_abandoned_and_announces_nothing_for_the_old_session()
 {
    let world = World::connected(Options {
        hours: vec![
            hour(vec![sample("2020-01-01T00:00:00Z")]),
            hour(vec![sample("2021-01-01T00:00:00Z")]),
        ],
        hour_hold_first: true,
        ..Options::default()
    })
    .await;
    let mark = world.recorder.mark();
    world.proxy.cut();
    world
        .recorder
        .wait_state(mark, LinkState::Reconnecting, WAIT)
        .await;
    world.proxy.heal();
    // La nouvelle session lit sa propre heure (la deuxième lecture ne tient pas).
    world
        .recorder
        .wait_for(mark, "l'heure de la nouvelle session", WAIT, |event| {
            matches!(event, Event::History { .. })
        })
        .await;
    // FAIT : l'ancienne lecture a été ABANDONNÉE (son futur a été jeté pendant qu'elle attendait) ; sans
    // l'abandon ce compteur resterait à 0.
    assert_eq!(world.spy.hour_aborted.load(Ordering::SeqCst), 1);
    // On la libère : plus personne n'attend, elle ne finit jamais. On attend ensuite un fait POSTÉRIEUR (la
    // mesure suivante du direct) avant de compter les événements d'historique.
    world.spy.hour_release.notify_one();
    let after = world.recorder.mark();
    world.recorder.wait_metrics(after, WAIT).await;
    assert_eq!(world.spy.hour_completed.load(Ordering::SeqCst), 0);
    assert_eq!(
        history_events(&world),
        vec![vec!["2021-01-01T00:00:00Z".to_owned()]]
    );
    assert!(world.spy.hour_calls.load(Ordering::SeqCst) >= 2);
}

/// Le maximum de chaque pas remplace la moyenne : un pic d'une seconde d'il y a une heure est tracé à sa hauteur.
#[tokio::test]
async fn the_peaks_of_the_agent_replace_the_means_in_the_hour_announced() {
    use hearth_proto::api::metrics::StepPeak;
    let mut response = hour(vec![sample("2020-01-01T00:00:00Z")]);
    response.peaks = vec![StepPeak {
        cpu: 100.0,
        mem_used_bytes: 2,
        net: None,
        gpus: vec![],
        temps: vec![],
    }];
    let world = World::connected(Options {
        hours: vec![response],
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
    assert_eq!(
        samples[0].cpu, 100.0,
        "le maximum du pas, pas la moyenne (42)"
    );
}

//! Adopter et relâcher PAR LA LIAISON contre un VRAI agent (TLS 1.3, SQLite, vrai flux WebSocket, faux
//! moteur piloté par le test) : le défi, la preuve de la clé de ce poste et le mot de passe sont ceux de
//! l'acte réel, la liste des candidats est lue par la liaison, et le service apparaît puis disparaît dans le
//! flux `services`. Une unité dont le nom demande un encodage dans le chemin (`\x2d`) est adoptée par sa clé
//! en clair et relâchée par son chemin encodé (HRT-80, ADR-0043, BR-ADOPT-008, BR-ADOPT-013). Aucune attente
//! de durée : des attentes sur des faits observables.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::time::Duration;

use hearth_agent::application::ports::EngineError;
use hearth_agent::domain::services::RunState;
use hearth_link::domain::event::Event;
use hearth_link::domain::secret::Secret;
use hearth_link::{Acted, InputField, LinkError};
use hearth_proto::api::services::{AdoptionResult, EngineState, ServicesState};
use support::agent::StaticSupervisor;
use support::agent::fake_engine::{FakeEngine, container};
use support::{Options, PASSWORD, WAIT, World};

const ESCAPED_UNIT: &str = "docker-my\\x2dapp.service";

fn services_of(event: &Event) -> Option<&ServicesState> {
    match event {
        Event::Services { state, .. } => Some(state),
        _ => None,
    }
}

fn names(state: &ServicesState) -> Vec<String> {
    state
        .services
        .iter()
        .map(|item| item.name.clone())
        .collect()
}

async fn wait_state(
    world: &World,
    mark: usize,
    what: &str,
    predicate: impl Fn(&ServicesState) -> bool,
) -> ServicesState {
    let (_, event) = world
        .recorder
        .wait_for(mark, what, WAIT, |event| {
            services_of(event).is_some_and(&predicate)
        })
        .await;
    services_of(&event).unwrap().clone()
}

/// Un agent qui exige la confirmation, avec `web` (sans unité) et `app` (lancé par l'unité au nom
/// échappé), tous deux non adoptés.
fn options() -> Options {
    let engine = Arc::new(FakeEngine::new());
    for name in ["web", "app"] {
        engine.put_silently(container(name, RunState::Running));
    }
    let app_id = container("app", RunState::Running).id;
    Options {
        device_key: true,
        engine: Some(engine),
        supervisor: Some(StaticSupervisor::new(&[(&app_id, ESCAPED_UNIT)])),
        ..Options::default()
    }
}

/// Attend que la liaison lise les deux candidats (le registre de l'agent les a vus).
async fn candidates(world: &World) -> Vec<String> {
    for _ in 0..500 {
        let found = world.manager.services_external(&world.id).await.unwrap();
        if found.engine == EngineState::Ready && found.containers.len() == 2 {
            return found.containers.into_iter().map(|c| c.key).collect();
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("les deux candidats n'ont pas été lus");
}

#[tokio::test]
async fn an_adoption_and_a_release_sent_by_the_link_show_and_hide_the_service_in_its_stream() {
    let world = World::connected(options()).await;
    world.agent.grant_services("marie").await;
    let password = Secret::from(PASSWORD);

    // Rien n'est publié tant que rien n'est adopté.
    let first = wait_state(&world, 0, "l'état initial", |state| {
        state.engine.state == EngineState::Ready
    })
    .await;
    assert!(first.services.is_empty(), "{first:?}");

    // La liaison lit les candidats, dont la clé au nom échappé, en clair.
    let keys = candidates(&world).await;
    let escaped = format!("u_{ESCAPED_UNIT}");
    assert!(keys.contains(&escaped), "{keys:?}");
    let web = keys
        .iter()
        .find(|key| key.starts_with("c_"))
        .unwrap()
        .clone();

    // Adopter les deux : acte signé (défi neuf, preuve, mot de passe), puis ils apparaissent dans le flux.
    let mark = world.recorder.mark();
    let Acted::Done(reply) = world
        .manager
        .services_adopt(&world.id, &[web.clone(), escaped.clone()], &password)
        .await
        .unwrap()
    else {
        panic!("réponse attendue");
    };
    assert_eq!(reply.adopted, 2);
    assert!(
        reply
            .results
            .iter()
            .all(|item| item.result == AdoptionResult::Adopted),
        "{reply:?}"
    );
    let listed = wait_state(&world, mark, "les deux services adoptés", |state| {
        state.services.len() == 2
    })
    .await;
    assert_eq!(names(&listed), ["app", "web"]);

    // Relâcher celui dont la clé demande un encodage : la liaison encode le chemin et signe la clé en clair.
    let mark = world.recorder.mark();
    let Acted::Done(released) = world
        .manager
        .services_release(&world.id, &escaped, &password)
        .await
        .unwrap()
    else {
        panic!("réponse attendue");
    };
    assert!(released.released);
    let left = wait_state(&world, mark, "un service relâché", |state| {
        state.services.len() == 1
    })
    .await;
    assert_eq!(names(&left), ["web"]);

    // Le relâcher de nouveau ne fait rien, et le dit.
    let Acted::Done(again) = world
        .manager
        .services_release(&world.id, &escaped, &password)
        .await
        .unwrap()
    else {
        panic!("réponse attendue");
    };
    assert!(!again.released);

    // Relâché, il est de nouveau un candidat.
    let found = world.manager.services_external(&world.id).await.unwrap();
    assert!(found.containers.iter().any(|c| c.key == escaped));
}

#[tokio::test]
async fn the_refusals_of_the_agent_come_back_typed_and_nothing_is_adopted() {
    let world = World::connected(options()).await;
    let password = Secret::from(PASSWORD);
    let web = format!("c_{}", container("web", RunState::Running).id);

    // Sans le droit « gérer les services » : la lecture des candidats et l'acte sont refusés, le droit
    // manquant est nommé.
    assert!(matches!(
        world.manager.services_external(&world.id).await,
        Err(LinkError::MissingRight(_))
    ));
    assert!(matches!(
        world
            .manager
            .services_adopt(&world.id, std::slice::from_ref(&web), &password)
            .await,
        Err(LinkError::MissingRight(_))
    ));

    // Avec le droit mais un mot de passe faux : refus de la confirmation, rien n'est adopté.
    world.agent.grant_services("marie").await;
    let wrong = Secret::from("Wrong-Horse-9999");
    let refused = world
        .manager
        .services_adopt(&world.id, std::slice::from_ref(&web), &wrong)
        .await;
    assert!(
        matches!(refused, Err(LinkError::Rejected(Some(_)))),
        "{refused:?}"
    );

    // Une clé qui n'a pas la forme d'une clé de service ne part pas.
    assert_eq!(
        world
            .manager
            .services_release(&world.id, "../accounts", &password)
            .await,
        Err(LinkError::UnreadableAct)
    );
}

// ── Raisons des refus (revue de la PR 89) ───────────────────────────────────────────────────

#[tokio::test]
async fn the_reason_of_a_refusal_comes_back_typed_for_every_refusal_the_code_alone_does_not_tell_apart()
 {
    use hearth_link::RefusalReason;
    let world = World::connected(options()).await;
    world.agent.grant_services("marie").await;
    let password = Secret::from(PASSWORD);
    let key = format!("c_{}", container("web", RunState::Running).id);
    let because =
        |result: Result<Acted<hearth_proto::api::services::AdoptResponse>, LinkError>| match result
        {
            Err(LinkError::RejectedBecause { reason, .. }) => reason,
            other => panic!("un refus avec sa raison était attendu : {other:?}"),
        };
    // Une demande vide, trop grande, avec un doublon : trois validations que le code seul ne sépare pas.
    assert_eq!(
        because(
            world
                .manager
                .services_adopt(&world.id, &[], &password)
                .await
        ),
        RefusalReason::KeysEmpty
    );
    let many: Vec<String> = (0..51).map(|index| format!("c_{index:064x}")).collect();
    assert_eq!(
        because(
            world
                .manager
                .services_adopt(&world.id, &many, &password)
                .await
        ),
        RefusalReason::KeysTooMany
    );
    assert_eq!(
        because(
            world
                .manager
                .services_adopt(&world.id, &[key.clone(), key.clone()], &password)
                .await
        ),
        RefusalReason::KeysDuplicate
    );
    // Rien n'a été adopté par ces refus.
    let found = world.manager.services_external(&world.id).await.unwrap();
    assert_eq!(found.containers.len(), 2);
}

#[tokio::test]
async fn an_agent_whose_engine_is_not_ready_refuses_an_adoption_with_that_reason() {
    use hearth_link::RefusalReason;
    // Sans moteur branché, le pilotage des services est désactivé : rien ne s'adopte.
    let world = World::connected(Options {
        device_key: true,
        ..Options::default()
    })
    .await;
    world.agent.grant_services("marie").await;
    let refused = world
        .manager
        .services_adopt(
            &world.id,
            &["c_0123abcd".to_owned()],
            &Secret::from(PASSWORD),
        )
        .await;
    assert!(
        matches!(
            refused,
            Err(LinkError::RejectedBecause {
                reason: RefusalReason::EngineNotReady,
                ..
            })
        ),
        "{refused:?}"
    );
}

// ── Coupure pendant l'acte (revue de la PR 89) ──────────────────────────────────────────────

#[tokio::test]
async fn an_adoption_cut_before_the_answer_is_unknown_never_replayed_and_done_once_by_the_agent() {
    use hearth_link::domain::pending_ops::Outcome;
    let world = World::connected(options()).await;
    world.agent.grant_services("marie").await;
    let password = Secret::from(PASSWORD);
    let keys = candidates(&world).await;
    let web = keys
        .iter()
        .find(|key| key.starts_with("c_"))
        .unwrap()
        .clone();
    // L'agent retient l'acte (confirmation en cours) tant que le test ne le relâche pas.
    world.agent.hold_actions();
    let started = world.agent.verifications_started();
    let mut sent = tokio::spawn({
        let manager = world.manager.clone();
        let id = world.id.clone();
        let web = web.clone();
        async move { manager.services_adopt(&id, &[web], &password).await }
    });
    support::wait_started_or_returned(&world.agent, started, &mut sent).await;
    let mark = world.recorder.mark();
    world.proxy.cut();
    let outcome = tokio::time::timeout(WAIT, sent)
        .await
        .expect("l'appel ne reste pas suspendu")
        .unwrap()
        .unwrap();
    let Acted::ResultUnknown { id: operation } = outcome else {
        panic!("résultat inconnu attendu, reçu {outcome:?}");
    };
    // L'agent finit l'acte pendant la coupure ; le lien revient ; l'issue arrive, jamais rejouée.
    world.agent.release_actions();
    world
        .agent
        .wait_operation_settled("marie", operation.as_str())
        .await;
    world.proxy.heal();
    let (_, event) = world
        .recorder
        .wait_for(mark, "issue de l'opération", WAIT, |event| {
            matches!(event, Event::Operation { .. })
        })
        .await;
    let Event::Operation {
        id: got, outcome, ..
    } = event
    else {
        unreachable!()
    };
    assert_eq!(got, operation);
    assert!(
        matches!(outcome, Outcome::DoneDuringOutage { .. }),
        "{outcome:?}"
    );
    // Adopté une seule fois : le service est publié, et n'est plus un candidat.
    let listed = wait_state(&world, 0, "le service adopté", |state| {
        state.services.len() == 1
    })
    .await;
    assert_eq!(names(&listed), ["web"]);
    let found = world.manager.services_external(&world.id).await.unwrap();
    assert!(
        found
            .containers
            .iter()
            .all(|container| container.key != web)
    );
}

// ── Traces (revue de la PR 89) ──────────────────────────────────────────────────────────────

/// Capture tout ce que `tracing` reçoit, au niveau le plus bavard (message, champs), pour y chercher un nom
/// de conteneur ou un secret.
#[derive(Clone, Default)]
struct Capture(Arc<std::sync::Mutex<Vec<String>>>);

struct Line(String);

impl tracing::field::Visit for Line {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.0.push_str(&format!("{}={value:?} ", field.name()));
    }
}

impl tracing::Subscriber for Capture {
    fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }

    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}

    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}

    fn event(&self, event: &tracing::Event<'_>) {
        let mut line = Line(format!("{} ", event.metadata().target()));
        event.record(&mut line);
        self.0.lock().unwrap().push(line.0);
    }

    fn enter(&self, _: &tracing::span::Id) {}

    fn exit(&self, _: &tracing::span::Id) {}
}

/// Test sentinelle : ni le nom ni l'image d'un conteneur que le compte n'a pas adopté, ni un mot de passe,
/// n'entrent dans une trace de la liaison, au niveau le plus bavard, pendant la lecture des candidats, un
/// refus et une relâche.
#[tokio::test]
async fn no_unadopted_container_name_image_or_secret_enters_a_trace_of_the_link() {
    const HIDDEN_NAME: &str = "sentinelle-nom-cache-91c4";
    const HIDDEN_IMAGE: &str = "registre.invalide/sentinelle-image-cachee-55ab";
    const WRONG: &str = "Faux-Mot-De-Passe-77";
    let capture = Capture::default();
    let _guard = tracing::subscriber::set_default(capture.clone());
    let engine = Arc::new(FakeEngine::new());
    let mut hidden = container(HIDDEN_NAME, RunState::Running);
    hidden.image = hearth_agent::domain::services::image::image_name(HIDDEN_IMAGE, &"b".repeat(64));
    engine.put_silently(hidden.clone());
    let world = World::connected(Options {
        device_key: true,
        engine: Some(engine),
        ..Options::default()
    })
    .await;
    world.agent.grant_services("marie").await;
    let found = loop {
        let found = world.manager.services_external(&world.id).await.unwrap();
        if found.containers.len() == 1 {
            break found;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    // La réponse nomme le conteneur : c'est son rôle. Les traces, non.
    assert_eq!(found.containers[0].name, HIDDEN_NAME);
    let _ = world
        .manager
        .services_adopt(&world.id, &[], &Secret::from(PASSWORD))
        .await;
    let _ = world
        .manager
        .services_adopt(
            &world.id,
            std::slice::from_ref(&found.containers[0].key),
            &Secret::from(WRONG),
        )
        .await;
    let _ = world
        .manager
        .services_release(&world.id, &found.containers[0].key, &Secret::from(PASSWORD))
        .await;
    let traces = capture.0.lock().unwrap().join("\n");
    assert!(!traces.is_empty(), "le niveau le plus bavard a bien parlé");
    for needle in [HIDDEN_NAME, HIDDEN_IMAGE, WRONG, PASSWORD] {
        assert!(
            !traces.contains(needle),
            "visible dans une trace : {needle}"
        );
    }
}

// ── Révéler les valeurs des variables d'un service (HRT-63, BR-SVC-009) ───────────────────────

/// `reveal_service_env` contre un vrai agent : refus sans le droit « gérer les services », réussite avec le
/// droit, clé inconnue, moteur muet, clé mal formée. Et, test sentinelle, ni un nom de variable ni une valeur
/// n'entre dans une trace de la liaison, au niveau le plus bavard, ni dans le texte d'une erreur.
#[tokio::test]
async fn revealing_the_values_follows_the_right_and_never_leaves_a_trace() {
    const NAME: &str = "SENTINEL_NOM_VARIABLE_6b1d";
    const VALUE: &str = "sentinelle-valeur-secrete-92ce";
    let capture = Capture::default();
    let _guard = tracing::subscriber::set_default(capture.clone());
    let engine = Arc::new(FakeEngine::new());
    let web = container("web", RunState::Running);
    engine.put_silently(web.clone());
    engine.set_env(&web.id, &[(NAME, VALUE)]);
    let world = World::connected(Options {
        device_key: true,
        engine: Some(engine.clone()),
        ..Options::default()
    })
    .await;
    world.agent.adopt_container(&web.id, "web", "marie").await;
    let key = format!("c_{}", web.id);

    // Sans le droit : refusé, le droit manquant est nommé, rien n'est lu dans le moteur.
    let refused = world.manager.reveal_service_env(&world.id, &key).await;
    assert!(
        matches!(refused, Err(LinkError::MissingRight(_))),
        "{refused:?}"
    );
    assert_eq!(engine.env_read_count(), 0);

    // Avec le droit : les noms et les valeurs reviennent, dans l'ordre du moteur.
    world.agent.grant_services("marie").await;
    let revealed = world
        .manager
        .reveal_service_env(&world.id, &key)
        .await
        .unwrap();
    assert_eq!(revealed.variables.len(), 1);
    assert_eq!(revealed.variables[0].name.expose(), NAME);
    assert_eq!(revealed.variables[0].value.expose(), VALUE);
    assert_eq!(engine.env_read_count(), 1);
    // Le Debug de la réponse ne montre rien.
    let shown = format!("{revealed:?}");
    assert!(!shown.contains(NAME) && !shown.contains(VALUE), "{shown}");

    // Un service que l'agent ne connaît pas : une erreur, pas des valeurs.
    let unknown = format!("c_{}", "e".repeat(64));
    let missing = world.manager.reveal_service_env(&world.id, &unknown).await;
    assert!(missing.is_err(), "{missing:?}");
    assert_eq!(engine.env_read_count(), 1, "rien n'a été lu pour l'inconnu");

    // Le moteur ne répond plus : une erreur, et l'agent n'invente pas de valeurs.
    engine.fail_inspect(&web.id, Some(EngineError::Unreachable));
    let mute = world.manager.reveal_service_env(&world.id, &key).await;
    assert!(mute.is_err(), "{mute:?}");
    engine.fail_inspect(&web.id, None);

    // Une clé qui n'a pas la forme d'une clé de service ne part pas.
    for bad in ["../accounts", "web", "", "c_a/b"] {
        assert_eq!(
            world.manager.reveal_service_env(&world.id, bad).await,
            Err(LinkError::InvalidInput(InputField::Filter)),
            "{bad:?}"
        );
    }

    let traces = capture.0.lock().unwrap().join("\n");
    assert!(!traces.is_empty(), "le niveau le plus bavard a bien parlé");
    for error in [&refused, &missing, &mute] {
        let text = format!("{error:?}");
        assert!(!text.contains(NAME) && !text.contains(VALUE), "{text}");
    }
    for needle in [NAME, VALUE] {
        assert!(
            !traces.contains(needle),
            "visible dans une trace : {needle}"
        );
    }
}

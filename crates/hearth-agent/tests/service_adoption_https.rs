//! Adopter un conteneur existant et le relâcher, de bout en bout (HRT-80, ADR-0043, `BR-ADOPT-005` à
//! `BR-ADOPT-026`) : vrai agent en HTTPS, vraie base, vrai WebSocket pour les journaux, faux moteur. Les
//! actes passent par la confirmation complète (preuve de la clé du poste puis mot de passe). Le contrôle
//! de chaque preuve, du mot de passe à chaque fois et de l'élévation, pour TOUS les actes dont ceux-ci, est
//! dans `admin_reauth.rs` ; ici : ce que font les deux actes et la liste des candidats.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cloned_ref_to_slice_refs
)]

mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use hearth_agent::domain::accounts::Role;
use hearth_agent::domain::services::{Observation, ProbeOutcome, RunState};
use hearth_proto::api::service_logs::{LogSource, LogsClientMessage, LogsServerMessage};
use hearth_proto::api::services::ServicesState;
use serde_json::{Value, json};
use support::device::Actor;
use support::engine::{fast_timings, parts_supervised};
use support::fake_engine::{FakeEngine, container};
use support::fake_supervisor::{FakeSupervisor, running_unit, unit_named};
use support::https::{self, Agent, Wire};
use support::ws;
use support::{Env, env};

const T0: i64 = 1_791_885_600_000;

/// Un faux serveur : un agent qui tourne sur un faux moteur, quatre comptes aux quatre profils de
/// `BR-GRANT` (A : administrateur avec le droit ; B : administrateur sans ; C : lecture seule sans ;
/// D : lecture seule avec).
struct Bench {
    env: Env,
    engine: Arc<FakeEngine>,
    supervisor: Arc<FakeSupervisor>,
    agent: Agent,
    ada: Actor,
    bob: Actor,
    lucas: Actor,
    dan: Actor,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Normal,
    /// `services.adoption = false`.
    AdoptionOff,
}

async fn bench(mode: Mode, setup: impl FnOnce(&Arc<FakeEngine>, &Arc<FakeSupervisor>)) -> Bench {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let supervisor = Arc::new(FakeSupervisor::new());
    setup(&engine, &supervisor);
    let engine_parts = parts_supervised(engine.clone(), supervisor.clone(), fast_timings());
    let agent = match mode {
        Mode::Normal => https::start_engined(&env, engine_parts).await,
        Mode::AdoptionOff => https::start_without_adoption(&env, engine_parts).await,
    };
    let ada = agent.actor(&env, "ada", Role::Admin).await;
    let bob = agent.actor(&env, "bob", Role::Admin).await;
    let lucas = agent.actor(&env, "lucas", Role::ReadOnly).await;
    let dan = agent.actor(&env, "dan", Role::ReadOnly).await;
    env.grant_services("ada").await;
    env.grant_services("dan").await;
    Bench {
        env,
        engine,
        supervisor,
        agent,
        ada,
        bob,
        lucas,
        dan,
    }
}

async fn eventually<F, Fut>(what: &str, mut check: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let started = Instant::now();
    while !check().await {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "délai dépassé : {what}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn key_of(observation: &Observation) -> String {
    format!("c_{}", observation.id)
}

/// Un conteneur lancé par une unité du système, et la clé du service que cette unité désigne.
fn under_unit(
    supervisor: &FakeSupervisor,
    name: &str,
    unit: &str,
    id_char: char,
) -> (Observation, String) {
    let mut observation = container(name, RunState::Running);
    observation.id = id_char.to_string().repeat(64);
    let unit = unit_named(unit);
    supervisor.set_owner(&observation.id, &unit);
    supervisor.set_status(&unit, running_unit());
    (observation, format!("u_{unit}"))
}

impl Bench {
    /// Attend que le registre ait lu `count` conteneurs, publiés ou non.
    async fn read(&self, count: usize) {
        let registry = self.agent.services();
        eventually("les conteneurs sont lus", || {
            let registry = registry.clone();
            async move {
                let snapshot = registry.snapshot();
                snapshot.services.len() + snapshot.engine.external_count as usize == count
                    && snapshot.engine.list_read_at.is_some()
            }
        })
        .await;
    }

    async fn external(&self, who: &Actor) -> Wire {
        self.agent
            .request("GET", "/services/external")
            .token(&who.token)
            .send()
            .await
    }

    async fn adopt(&self, who: &Actor, keys: &[String]) -> Wire {
        self.agent
            .confirmed(who, "POST", "/services/adopt", json!({ "keys": keys }))
            .await
            .send()
            .await
    }

    async fn release(&self, who: &Actor, key: &str) -> Wire {
        self.agent
            .confirmed(who, "POST", &format!("/services/{key}/release"), json!({}))
            .await
            .send()
            .await
    }

    async fn services(&self, who: &Actor) -> ServicesState {
        let reply = self
            .agent
            .request("GET", "/services")
            .token(&who.token)
            .send()
            .await;
        assert_eq!(reply.status, 200, "{}", reply.text);
        serde_json::from_value(reply.body).expect("ServicesState")
    }

    /// Les entrées du journal d'activité qui touchent l'adoption : action, résultat, cible, raison, compte.
    async fn journal(
        &self,
    ) -> Vec<(
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    )> {
        self.agent.flush_journal().await;
        sqlx::query_as(
            "SELECT action, outcome, target, reason, account FROM audit_events
             WHERE action LIKE 'service.%' ORDER BY id",
        )
        .fetch_all(self.env.db.pool())
        .await
        .unwrap()
    }

    /// Ce que le moteur dit d'un conteneur, pour prouver qu'un acte n'y a rien changé.
    async fn observed(&self, id: &str) -> Observation {
        use hearth_agent::application::ports::ContainerEngine;
        self.engine
            .inspect(id)
            .await
            .expect("inspection")
            .expect("conteneur")
    }

    /// Ouvre la lecture des journaux d'un service et rend les messages reçus jusqu'à la première ligne,
    /// l'erreur terminale ou un silence.
    async fn logs(&self, who: &Actor, key: &str) -> String {
        let mut client = ws::open_logs(&self.agent, key)
            .await
            .expect("socket des journaux");
        client
            .send_value(&json!({ "type": "auth", "token": who.token }))
            .await;
        client
            .send_value(&LogsClientMessage::Open {
                tail: 1000,
                since: None,
            })
            .await;
        let mut text = String::new();
        while let Some(next) = client.try_next_logs(Duration::from_millis(800)).await {
            match next {
                Ok(message) => {
                    text.push_str(&format!("{message:?}\n"));
                    if let LogsServerMessage::Lines(lines) = message {
                        for item in &lines.items {
                            text.push_str(&item.x);
                            text.push('\n');
                        }
                        break;
                    }
                }
                Err(end) => {
                    text.push_str(&format!("{end:?}\n"));
                    break;
                }
            }
        }
        text
    }
}

fn names_of(body: &Value) -> Vec<String> {
    body["containers"]
        .as_array()
        .expect("containers")
        .iter()
        .map(|c| c["name"].as_str().expect("nom").to_owned())
        .collect()
}

// ---------------------------------------------------------------------------------------------
// La liste des candidats (BR-ADOPT-005, BR-ADOPT-006)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn the_candidates_are_for_the_right_only_sorted_with_their_unit_and_a_successful_read_is_journaled_without_any_name()
 {
    let kept = container("deja-adopte", RunState::Running);
    let kept_key = key_of(&kept);
    let b = bench(Mode::Normal, |engine, supervisor| {
        engine.put_silently(container("Zeta", RunState::Running));
        engine.put_silently(container("alpha", RunState::Exited));
        let (terraria, _) = under_unit(supervisor, "terraria", "docker-terraria.service", 'a');
        engine.put_silently(terraria);
        engine.put_silently(kept.clone());
    })
    .await;
    b.env.adopt(&kept_key, "deja-adopte", "marie").await;
    b.agent.services().reload_adopted().await;
    b.read(4).await;

    // A et D (le droit, quel que soit le rôle) lisent la même liste.
    let for_ada = b.external(&b.ada).await;
    let for_dan = b.external(&b.dan).await;
    assert_eq!(for_ada.status, 200, "{}", for_ada.text);
    assert_eq!(for_ada.body, for_dan.body);
    assert_eq!(for_ada.body["engine"], "ready");
    // Triés sans tenir compte de la casse ; l'adopté n'en fait pas partie.
    assert_eq!(names_of(&for_ada.body), ["alpha", "terraria", "Zeta"]);
    // Exactement ce qu'il faut pour choisir (BR-ADOPT-006) : la clé, le nom, l'image, l'unité, l'état.
    let terraria = &for_ada.body["containers"][1];
    assert_eq!(terraria["key"], "u_docker-terraria.service");
    assert_eq!(terraria["unit"], "docker-terraria.service");
    assert_eq!(terraria["state"], "running");
    assert_eq!(for_ada.body["containers"][0]["state"], "stopped");
    assert!(for_ada.body["containers"][0].get("unit").is_none());
    for candidate in for_ada.body["containers"].as_array().unwrap() {
        let mut fields: Vec<&str> = candidate
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        fields.sort_unstable();
        assert!(
            fields == ["image", "key", "name", "state"]
                || fields == ["image", "key", "name", "state", "unit"],
            "{fields:?}"
        );
    }
    // Jamais dans un cache.
    assert_eq!(for_ada.header("cache-control"), Some("no-store"));

    // B et C (pas le droit) sont refusés, administrateur ou non, et n'apprennent aucun nom.
    for (who, actor) in [("bob", &b.bob), ("lucas", &b.lucas)] {
        let refused = b.external(actor).await;
        assert_eq!(refused.status, 403, "{who} : {}", refused.text);
        assert_eq!(refused.code(), "FORBIDDEN_ROLE", "{who}");
        assert_eq!(
            refused.body["error"]["message"],
            "Tu n'as pas le droit de gérer les services de ce serveur."
        );
        for name in ["alpha", "terraria", "Zeta", "deja-adopte"] {
            assert!(!refused.text.contains(name), "{who} : {}", refused.text);
        }
    }
    let anonymous = b.agent.request("GET", "/services/external").send().await;
    assert_eq!(anonymous.status, 401);

    // Les deux lectures réussies (décision par défaut du propriétaire : gérer des conteneurs déjà présents
    // est sensible) et les deux refus sont au journal, sous la même action, sans nom ni clé de conteneur :
    // la cible est la route, et rien de ce que la liste contenait n'y figure.
    let journal = b.journal().await;
    assert_eq!(journal.len(), 4, "{journal:?}");
    let mut read_by: Vec<&str> = journal
        .iter()
        .filter(|row| row.1 == "ok")
        .map(|row| row.4.as_deref().unwrap())
        .collect();
    read_by.sort_unstable();
    assert_eq!(read_by, ["ada", "dan"]);
    assert_eq!(journal.iter().filter(|row| row.1 == "denied").count(), 2);
    for (action, outcome, target, reason, _) in &journal {
        assert_eq!(action, "service.external.read");
        assert_eq!(target.as_deref(), Some("/services/external"));
        match outcome.as_str() {
            "ok" => assert_eq!(reason, &None),
            _ => assert_eq!(reason.as_deref(), Some("droits insuffisants")),
        }
    }
    let everything = format!("{journal:?}");
    for name in [
        "alpha",
        "terraria",
        "Zeta",
        "deja-adopte",
        "docker-terraria",
    ] {
        assert!(!everything.contains(name), "{name} : {everything}");
    }
    b.agent.shutdown().await;
}

#[tokio::test]
async fn the_candidates_say_the_engine_state_when_it_is_not_ready_and_nothing_is_offered() {
    let b = bench(Mode::Normal, |engine, _| {
        engine.put_silently(container("web", RunState::Running));
        engine.set_outcome(ProbeOutcome::SocketMissing, false);
    })
    .await;
    let registry = b.agent.services();
    eventually("le moteur est absent", || {
        let registry = registry.clone();
        async move {
            registry.engine_status().state == hearth_agent::domain::services::EngineState::Absent
        }
    })
    .await;
    let reply = b.external(&b.ada).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["engine"], "absent");
    assert_eq!(reply.body["containers"], json!([]));
    // Rien ne peut être adopté sans moteur (BR-ADOPT-026) : la demande est refusée, tel quel.
    let adopted = b.adopt(&b.ada, &["c_0000".to_owned()]).await;
    assert_eq!(adopted.status, 409, "{}", adopted.text);
    assert_eq!(
        adopted.body["error"]["details"]["reason"],
        "engine_not_ready"
    );
    b.agent.shutdown().await;
}

#[tokio::test]
async fn with_adoption_switched_off_nothing_is_offered_nothing_is_adopted_and_an_adopted_service_can_be_released()
 {
    let web = container("web", RunState::Running);
    let key = key_of(&web);
    let b = bench(Mode::AdoptionOff, |engine, _| {
        engine.put_silently(web.clone());
        engine.put_silently(container("db", RunState::Running));
    })
    .await;
    b.env.adopt(&key, "web", "marie").await;
    b.agent.services().reload_adopted().await;
    b.read(2).await;
    let message = "L'adoption de services est désactivée sur ce serveur. Un administrateur du serveur peut la réactiver dans la configuration de l'agent Hearth.";
    let candidates = b.external(&b.ada).await;
    assert_eq!(candidates.status, 409, "{}", candidates.text);
    assert_eq!(candidates.body["error"]["message"], message);
    assert_eq!(
        candidates.body["error"]["details"]["reason"],
        "adoption_disabled"
    );
    let db = key_of(&container("db", RunState::Running));
    let adopt = b.adopt(&b.ada, &[db.clone()]).await;
    assert_eq!(adopt.status, 409, "{}", adopt.text);
    assert_eq!(adopt.body["error"]["message"], message);
    assert_eq!(b.env.adopted_keys().await, [key.clone()]);
    // Les services déjà adoptés restent gérés, et « ne plus gérer » reste possible.
    assert_eq!(b.services(&b.ada).await.services.len(), 1);
    let released = b.release(&b.ada, &key).await;
    assert_eq!(released.status, 200, "{}", released.text);
    assert_eq!(released.body, json!({ "released": true }));
    assert!(b.services(&b.ada).await.services.is_empty());
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Adopter (BR-ADOPT-008 à BR-ADOPT-010, BR-ADOPT-019, BR-ADOPT-024)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn adopting_changes_nothing_on_the_service_publishes_it_and_journals_one_entry_per_service() {
    let web = container("web", RunState::Running);
    let db = container("db", RunState::Exited);
    let web_key = key_of(&web);
    let b = bench(Mode::Normal, |engine, supervisor| {
        engine.put_silently(web.clone());
        engine.put_silently(db.clone());
        let (terraria, _) = under_unit(supervisor, "terraria", "docker-terraria.service", 'a');
        engine.put_silently(terraria);
    })
    .await;
    b.read(3).await;
    let before = (
        b.observed(&web.id).await,
        b.observed(&db.id).await,
        b.observed(&"a".repeat(64)).await,
    );
    assert!(b.services(&b.bob).await.services.is_empty());
    assert_eq!(b.services(&b.bob).await.engine.external_count, 3);

    let keys = vec![
        web_key.clone(),
        key_of(&db),
        "u_docker-terraria.service".to_owned(),
    ];
    let reply = b.adopt(&b.ada, &keys).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["adopted"], 3);
    let results = reply.body["results"].as_array().unwrap();
    assert_eq!(
        results
            .iter()
            .map(|r| r["result"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["adopted", "adopted", "adopted"]
    );
    assert_eq!(
        results
            .iter()
            .map(|r| r["key"].as_str().unwrap())
            .collect::<Vec<_>>(),
        keys.iter().map(String::as_str).collect::<Vec<_>>(),
        "une issue par clé, dans l'ordre de la demande"
    );

    // Rien n'a changé sur les conteneurs (BR-ADOPT-009) : le port du moteur n'a même aucun geste
    // d'écriture, et ce que le moteur en dit est identique.
    let after = (
        b.observed(&web.id).await,
        b.observed(&db.id).await,
        b.observed(&"a".repeat(64)).await,
    );
    assert_eq!(before, after);
    // Les trois services sont publiés pour tous, avec leur repère d'adoption, et plus comptés.
    for actor in [&b.ada, &b.bob, &b.lucas, &b.dan] {
        let state = b.services(actor).await;
        assert_eq!(state.services.len(), 3);
        assert_eq!(state.engine.external_count, 0);
        assert!(
            state
                .services
                .iter()
                .all(|s| s.adopted.as_ref().is_some_and(|a| a.by == "ada"))
        );
    }
    // Le détail réservé reste réservé à qui a le droit (BR-ADOPT-015).
    assert!(
        b.services(&b.bob)
            .await
            .services
            .iter()
            .all(|s| s.details_reserved)
    );
    assert!(
        b.external(&b.ada).await.body["containers"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        b.env.adopted_keys().await,
        [
            web_key.clone(),
            key_of(&db),
            "u_docker-terraria.service".to_owned()
        ]
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
    );

    // Une entrée par service : le nom (et l'unité), le compte, l'origine, le résultat. Rien d'autre.
    let journal = b.journal().await;
    let adopts: Vec<_> = journal
        .iter()
        .filter(|row| row.0 == "service.adopt")
        .collect();
    assert_eq!(adopts.len(), 3, "{journal:?}");
    let mut targets: Vec<&str> = adopts.iter().map(|row| row.2.as_deref().unwrap()).collect();
    targets.sort_unstable();
    assert_eq!(
        targets,
        [
            "service db",
            "service terraria (docker-terraria.service)",
            "service web"
        ]
    );
    for row in adopts {
        assert_eq!(row.1, "ok");
        assert_eq!(row.4.as_deref(), Some("ada"));
    }
    let origin: Vec<(String,)> =
        sqlx::query_as("SELECT origin_kind FROM audit_events WHERE action = 'service.adopt'")
            .fetch_all(b.env.db.pool())
            .await
            .unwrap();
    assert!(origin.iter().all(|(kind,)| kind == "client"));
    b.agent.shutdown().await;
}

#[tokio::test]
async fn the_result_is_said_service_by_service_a_vanished_one_is_not_adopted_an_adopted_one_is_not_touched()
 {
    let vanishing = container("vanishing", RunState::Running);
    let taken = container("taken", RunState::Running);
    let fresh = container("fresh", RunState::Running);
    let b = bench(Mode::Normal, |engine, _| {
        for each in [&vanishing, &taken, &fresh] {
            engine.put_silently(each.clone());
        }
    })
    .await;
    b.read(3).await;
    // Le candidat disparaît entre la liste et l'acte ; un autre compte en adopte un autre entre-temps.
    let mut asked = vec![key_of(&vanishing), key_of(&taken), key_of(&fresh)];
    let first = b.adopt(&b.dan, &[key_of(&taken)]).await;
    assert_eq!(first.body["adopted"], 1, "{}", first.text);
    b.engine.remove(&vanishing.id);
    let registry = b.agent.services();
    eventually("le conteneur disparu n'est plus lu", || {
        let registry = registry.clone();
        async move { registry.candidates().iter().all(|c| c.name != "vanishing") }
    })
    .await;
    asked.push("c_inventee0123".to_owned());
    asked.push("h_01ARZ3NDEKTSV4RRFFQ69G5FAV".to_owned());
    asked.push("pas une cle".to_owned());
    let reply = b.adopt(&b.ada, &asked).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["adopted"], 1);
    let results: Vec<(String, Option<String>)> = reply.body["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["result"].as_str().unwrap().to_owned(),
                r["by"].as_str().map(str::to_owned),
            )
        })
        .collect();
    assert_eq!(
        results,
        [
            ("not_found".to_owned(), None),
            ("already_adopted".to_owned(), Some("dan".to_owned())),
            ("adopted".to_owned(), None),
            // Une clé inventée, une clé `h_` et un texte qui n'est pas une clé : la même issue.
            ("not_found".to_owned(), None),
            ("not_found".to_owned(), None),
            ("not_found".to_owned(), None),
        ]
    );
    assert_eq!(b.services(&b.bob).await.services.len(), 2);
    // Le service déjà adopté n'est pas touché : il garde son adoptant d'origine.
    let state = b.services(&b.bob).await;
    let taken_item = state.services.iter().find(|s| s.name == "taken").unwrap();
    assert_eq!(taken_item.adopted.as_ref().unwrap().by, "dan");

    // Le journal : une entrée réussie, une « déjà adopté » avec le nom (déjà publié), et les « service
    // introuvable » ne nomment rien, identiques pour le disparu, l'inventé et le mal formé.
    let journal = b.journal().await;
    let by_ada: Vec<_> = journal
        .iter()
        .filter(|row| row.0 == "service.adopt" && row.4.as_deref() == Some("ada"))
        .collect();
    assert_eq!(by_ada.len(), 6, "{journal:?}");
    assert_eq!(
        by_ada
            .iter()
            .filter(|row| row.1 == "ok" && row.2.as_deref() == Some("service fresh"))
            .count(),
        1
    );
    assert_eq!(
        by_ada
            .iter()
            .filter(|row| row.1 == "failed"
                && row.3.as_deref() == Some("déjà adopté")
                && row.2.as_deref() == Some("service taken"))
            .count(),
        1
    );
    let not_found: Vec<_> = by_ada
        .iter()
        .filter(|row| row.3.as_deref() == Some("service introuvable"))
        .collect();
    assert_eq!(not_found.len(), 4);
    assert!(
        not_found
            .iter()
            .all(|row| row.1 == "failed" && row.2.as_deref() == Some("/services/adopt")),
        "{not_found:?}"
    );
    for row in &journal {
        assert!(
            !row.2.as_deref().unwrap_or("").contains("vanishing"),
            "le nom d'un conteneur disparu n'est jamais écrit : {row:?}"
        );
    }
    b.agent.shutdown().await;
}

#[tokio::test]
async fn too_many_keys_duplicates_and_a_target_that_does_not_match_the_keys_adopt_nothing() {
    let web = container("web", RunState::Running);
    let b = bench(Mode::Normal, |engine, _| {
        engine.put_silently(web.clone());
    })
    .await;
    b.read(1).await;
    let key = key_of(&web);
    // Plus de 50 clés : refusé avec le texte de la spec, rien n'est adopté.
    let many: Vec<String> = std::iter::once(key.clone())
        .chain((0..50).map(|n| format!("c_inventee{n:04}")))
        .collect();
    let reply = b.adopt(&b.ada, &many).await;
    assert_eq!(reply.status, 422, "{}", reply.text);
    assert_eq!(
        reply.body["error"]["message"],
        "Tu peux adopter 50 services à la fois."
    );
    // Exactement 50 : accepté (une adoptée, les autres inconnues).
    let fifty: Vec<String> = std::iter::once(key.clone())
        .chain((0..49).map(|n| format!("c_inventee{n:04}")))
        .collect();
    assert_eq!(fifty.len(), 50);
    let reply = b.adopt(&b.ada, &fifty).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["adopted"], 1);
    b.env.unadopt(&key).await;
    b.agent.services().reload_adopted().await;
    // Une liste vide, un doublon : refusés.
    let reply = b.adopt(&b.ada, &[]).await;
    assert_eq!(reply.status, 422, "{}", reply.text);
    let reply = b.adopt(&b.ada, &[key.clone(), key.clone()]).await;
    assert_eq!(reply.status, 422, "{}", reply.text);
    assert!(b.env.adopted_keys().await.is_empty());
    // Une cible qui ne correspond pas aux clés envoyées : la preuve signe le nombre et l'empreinte des clés
    // de la confirmation, pas celles du corps (BR-ADOPT-008).
    let mut body = b
        .agent
        .confirm(
            &b.ada,
            "POST",
            "/services/adopt",
            json!({ "keys": [key.clone()] }),
        )
        .await;
    body["keys"] = json!([key.clone(), "c_inventee0001"]);
    let reply = b
        .agent
        .request("POST", "/services/adopt")
        .token(&b.ada.token)
        .json(&body)
        .send()
        .await;
    assert_eq!(reply.status, 409, "{}", reply.text);
    assert_eq!(reply.body["error"]["details"]["reason"], "proof_invalid");
    let mut body = b
        .agent
        .confirm(
            &b.ada,
            "POST",
            "/services/adopt",
            json!({ "keys": [key.clone()] }),
        )
        .await;
    body["keys"] = json!(["c_inventee0001"]);
    let reply = b
        .agent
        .request("POST", "/services/adopt")
        .token(&b.ada.token)
        .json(&body)
        .send()
        .await;
    assert_eq!(reply.status, 409, "{}", reply.text);
    assert!(b.env.adopted_keys().await.is_empty());
    // Sans le droit, la confirmation la plus complète ne change rien.
    let reply = b.adopt(&b.bob, &[key.clone()]).await;
    assert_eq!(reply.status, 403, "{}", reply.text);
    assert!(b.env.adopted_keys().await.is_empty());
    b.agent.shutdown().await;
}

#[tokio::test]
async fn two_accounts_adopting_the_same_service_at_the_same_time_adopt_it_once() {
    let web = container("web", RunState::Running);
    let key = key_of(&web);
    let b = bench(Mode::Normal, |engine, _| {
        engine.put_silently(web.clone());
    })
    .await;
    b.read(1).await;
    let keys = [key.clone()];
    let (first, second) = tokio::join!(b.adopt(&b.ada, &keys), b.adopt(&b.dan, &keys));
    let results: Vec<String> = [&first, &second]
        .iter()
        .map(|reply| {
            assert_eq!(reply.status, 200, "{}", reply.text);
            reply.body["results"][0]["result"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    let mut sorted = results.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, ["adopted", "already_adopted"], "{results:?}");
    let winner = if results[0] == "adopted" {
        "ada"
    } else {
        "dan"
    };
    let loser = if winner == "ada" { &second } else { &first };
    assert_eq!(loser.body["results"][0]["by"], winner);
    assert_eq!(b.env.adopted_keys().await, [key]);
    let journal = b.journal().await;
    assert_eq!(
        journal.iter().filter(|row| row.1 == "ok").count(),
        1,
        "une seule adoption réussie : {journal:?}"
    );
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Ne plus gérer (BR-ADOPT-013, BR-ADOPT-024)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn releasing_takes_the_service_off_the_list_of_everyone_touches_nothing_and_is_journaled_once()
 {
    let web = container("web", RunState::Running);
    let key = key_of(&web);
    let b = bench(Mode::Normal, |engine, _| {
        engine.put_silently(web.clone());
        engine.put_silently(container("hidden-one", RunState::Running));
        engine.push_log(&web.id, LogSource::Out, T0, "ligne du service");
    })
    .await;
    b.read(2).await;
    assert_eq!(b.adopt(&b.ada, &[key.clone()]).await.body["adopted"], 1);
    let before = b.observed(&web.id).await;
    // Une lecture de journaux est ouverte quand on relâche : elle se ferme.
    let mut reading = ws::open_logs(&b.agent, &key).await.expect("socket");
    reading
        .send_value(&json!({ "type": "auth", "token": b.ada.token }))
        .await;
    reading
        .send_value(&LogsClientMessage::Open {
            tail: 10,
            since: None,
        })
        .await;
    let mut got_lines = false;
    while let Some(Ok(message)) = reading.try_next_logs(Duration::from_secs(2)).await {
        if matches!(message, LogsServerMessage::Lines(_)) {
            got_lines = true;
            break;
        }
    }
    assert!(got_lines, "la lecture ouverte rend d'abord les lignes");

    let reply = b.release(&b.dan, &key).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body, json!({ "released": true }));
    // Plus dans la liste de personne, de nouveau compté, la base l'a oublié, rien n'a bougé sur le service.
    for actor in [&b.ada, &b.bob, &b.lucas, &b.dan] {
        let state = b.services(actor).await;
        assert!(state.services.is_empty());
        assert_eq!(state.engine.external_count, 2);
    }
    assert!(b.env.adopted_keys().await.is_empty());
    assert_eq!(b.observed(&web.id).await, before);
    // La lecture des journaux ouverte se ferme : le service est maintenant un conteneur non géré.
    let mut closed = false;
    while let Some(next) = reading.try_next_logs(Duration::from_secs(4)).await {
        match next {
            Ok(LogsServerMessage::Error(error)) => {
                assert_eq!(format!("{:?}", error.code), "ServiceNotFound");
                closed = true;
                break;
            }
            Ok(_) => {}
            Err(_) => {
                closed = true;
                break;
            }
        }
    }
    assert!(closed, "la lecture ouverte doit se fermer");
    // Une seule entrée, au nom du compte qui a relâché.
    let journal = b.journal().await;
    let releases: Vec<_> = journal
        .iter()
        .filter(|row| row.0 == "service.release")
        .collect();
    assert_eq!(releases.len(), 1, "{journal:?}");
    assert_eq!(releases[0].1, "ok");
    assert_eq!(releases[0].2.as_deref(), Some("service web"));
    assert_eq!(releases[0].4.as_deref(), Some("dan"));
    // Le service peut être adopté de nouveau.
    assert_eq!(b.adopt(&b.ada, &[key]).await.body["adopted"], 1);
    b.agent.shutdown().await;
}

#[tokio::test]
async fn releasing_what_is_not_adopted_does_nothing_says_the_same_for_every_reason_and_journals_nothing()
 {
    let web = container("web", RunState::Running);
    let key = key_of(&web);
    let hidden = container("hidden-one", RunState::Running);
    let b = bench(Mode::Normal, |engine, _| {
        engine.put_silently(web.clone());
        engine.put_silently(hidden.clone());
    })
    .await;
    b.read(2).await;
    assert_eq!(b.adopt(&b.ada, &[key.clone()]).await.body["adopted"], 1);
    assert_eq!(
        b.release(&b.ada, &key).await.body,
        json!({ "released": true })
    );
    let entries = b.journal().await.len();
    let mut bodies = Vec::new();
    // Déjà relâché, caché (conteneur qui existe), inventé, mal formé, créé par Hearth : la même réponse.
    for target in [
        key.clone(),
        key_of(&hidden),
        "c_inventee0123".to_owned(),
        "u_docker-inconnu.service".to_owned(),
        "h_01ARZ3NDEKTSV4RRFFQ69G5FAV".to_owned(),
    ] {
        let reply = b.release(&b.ada, &target).await;
        assert_eq!(reply.status, 200, "{target} : {}", reply.text);
        bodies.push(reply.text);
    }
    assert!(
        bodies.windows(2).all(|pair| pair[0] == pair[1]),
        "la même réponse : {bodies:?}"
    );
    assert_eq!(bodies[0], r#"{"released":false}"#);
    assert_eq!(b.journal().await.len(), entries, "aucune entrée de plus");
    // Sans le droit : refusé, avant toute lecture de la clé.
    let reply = b.release(&b.bob, &key_of(&hidden)).await;
    assert_eq!(reply.status, 403, "{}", reply.text);
    // Le conteneur caché n'a pas bougé : toujours non géré.
    assert_eq!(b.services(&b.ada).await.engine.external_count, 2);
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Identité (BR-ADOPT-017, BR-ADOPT-018)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_container_without_a_unit_is_adopted_for_its_life_and_a_unit_for_good() {
    let plain = container("plain", RunState::Running);
    let b = bench(Mode::Normal, |engine, supervisor| {
        engine.put_silently(plain.clone());
        let (first, _) = under_unit(supervisor, "terraria", "docker-terraria.service", 'a');
        engine.put_silently(first);
    })
    .await;
    b.read(2).await;
    let unit_key = "u_docker-terraria.service".to_owned();
    let reply = b.adopt(&b.ada, &[key_of(&plain), unit_key.clone()]).await;
    assert_eq!(reply.body["adopted"], 2, "{}", reply.text);
    // Un tiers recrée « plain » (un autre identifiant, le même nom) ; le système recrée le conteneur de
    // l'unité : le premier redevient un conteneur non géré, le second reste le même service.
    b.engine.remove(&plain.id);
    let mut namesake = container("plain", RunState::Running);
    namesake.id = "9".repeat(64);
    b.engine.put(namesake, "start");
    b.engine.remove(&"a".repeat(64));
    let (second, _) = under_unit(&b.supervisor, "terraria", "docker-terraria.service", 'b');
    b.engine.put(second, "start");
    let registry = b.agent.services();
    eventually("l'homonyme est compté, l'unité reste publiée", || {
        let registry = registry.clone();
        async move {
            let snapshot = registry.snapshot();
            snapshot.engine.external_count == 1
                && snapshot.services.len() == 1
                && snapshot.services[0].key.as_str() == "u_docker-terraria.service"
        }
    })
    .await;
    eventually("l'adoption de l'ancien conteneur est oubliée", || {
        let env = &b.env;
        async move { env.adopted_keys().await == ["u_docker-terraria.service".to_owned()] }
    })
    .await;
    // L'homonyme est un autre conteneur : il est proposé, avec un autre identifiant.
    let candidates = b.external(&b.ada).await;
    assert_eq!(names_of(&candidates.body), ["plain"]);
    assert_ne!(
        candidates.body["containers"][0]["key"].as_str().unwrap(),
        key_of(&plain)
    );
    // La fin de l'adoption de l'ancien conteneur n'écrit aucune entrée au journal : aucun compte n'agit.
    let journal = b.journal().await;
    assert!(
        journal
            .iter()
            .all(|row| row.0 == "service.adopt" || row.0 == "service.external.read"),
        "seulement l'acte d'adopter et la lecture des candidats : {journal:?}"
    );
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Journaux d'un service adopté lus sans le droit (BR-SLOG-018, BR-SLOG-024)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn an_adopted_service_read_without_the_right_is_refused_in_plain_words_and_journaled_while_a_hidden_container_stays_an_unknown_service()
 {
    let web = container("web", RunState::Running);
    let hidden = container("hidden-one", RunState::Running);
    let key = key_of(&web);
    let b = bench(Mode::Normal, |engine, _| {
        engine.put_silently(web.clone());
        engine.put_silently(hidden.clone());
        engine.push_log(&web.id, LogSource::Out, T0, "ligne du service");
        engine.push_log(&hidden.id, LogSource::Out, T0, "ligne du caché");
    })
    .await;
    b.read(2).await;
    assert_eq!(b.adopt(&b.ada, &[key.clone()]).await.body["adopted"], 1);
    // Avec le droit : les lignes sortent.
    let with_right = b.logs(&b.dan, &key).await;
    assert!(with_right.contains("ligne du service"), "{with_right}");
    // Sans le droit, le service est dans sa liste : refus explicite, pas « introuvable ».
    for actor in [&b.bob, &b.lucas] {
        let refused = b.logs(actor, &key).await;
        assert!(refused.contains("MissingRight"), "{refused}");
        assert!(!refused.contains("ligne du service"), "{refused}");
    }
    // Un conteneur caché et une clé inventée reçoivent « introuvable », mot pour mot la même chose :
    // le refus explicite ne crée aucun oracle sur ce que le serveur fait tourner.
    let hidden_key = key_of(&hidden);
    let mut transcripts = Vec::new();
    for key in [
        hidden_key.as_str(),
        "c_inventee0123",
        "u_docker-inconnu.service",
    ] {
        for actor in [&b.bob, &b.ada] {
            let text = b.logs(actor, key).await;
            assert!(text.contains("ServiceNotFound"), "{key} : {text}");
            assert!(!text.contains("ligne du caché"));
            transcripts.push(text);
        }
    }
    assert!(
        transcripts.windows(2).all(|pair| pair[0] == pair[1]),
        "{transcripts:?}"
    );
    // Le journal d'activité : les deux refus du service adopté (BR-SLOG-024), avec son nom, au nom des
    // comptes refusés ; rien pour le caché ni l'inventé, ni pour les lectures réussies.
    let journal = b.journal().await;
    let reads: Vec<_> = journal
        .iter()
        .filter(|row| row.0 == "service.logs.read")
        .collect();
    assert_eq!(reads.len(), 2, "{journal:?}");
    assert!(reads.iter().all(|row| row.1 == "denied"
        && row.3.as_deref() == Some("droits insuffisants")
        && row.2.as_deref() == Some("service web")));
    let mut who: Vec<&str> = reads.iter().map(|row| row.4.as_deref().unwrap()).collect();
    who.sort_unstable();
    assert_eq!(who, ["bob", "lucas"]);
    for row in &journal {
        assert!(!row.2.as_deref().unwrap_or("").contains("hidden-one"));
    }
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_right_withdrawn_during_a_reading_ends_it_with_the_explicit_refusal_and_the_entry() {
    let web = container("web", RunState::Running);
    let key = key_of(&web);
    let b = bench(Mode::Normal, |engine, _| {
        engine.put_silently(web.clone());
        engine.push_log(&web.id, LogSource::Out, T0, "première");
    })
    .await;
    b.read(1).await;
    assert_eq!(b.adopt(&b.ada, &[key.clone()]).await.body["adopted"], 1);
    let mut reading = ws::open_logs(&b.agent, &key).await.expect("socket");
    reading
        .send_value(&json!({ "type": "auth", "token": b.dan.token }))
        .await;
    reading
        .send_value(&LogsClientMessage::Open {
            tail: 10,
            since: None,
        })
        .await;
    while let Some(Ok(message)) = reading.try_next_logs(Duration::from_secs(2)).await {
        if matches!(message, LogsServerMessage::Lines(_)) {
            break;
        }
    }
    b.env.revoke_services("dan").await;
    let mut refused = false;
    while let Some(next) = reading.try_next_logs(Duration::from_secs(5)).await {
        if let Ok(LogsServerMessage::Error(error)) = next {
            assert_eq!(format!("{:?}", error.code), "MissingRight");
            refused = true;
            break;
        }
    }
    assert!(refused, "la lecture doit se fermer sur le refus explicite");
    let journal = b.journal().await;
    assert_eq!(
        journal
            .iter()
            .filter(|row| row.0 == "service.logs.read" && row.4.as_deref() == Some("dan"))
            .count(),
        1,
        "{journal:?}"
    );
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Lien coupé (BR-ADOPT-025)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_link_cut_after_the_request_leaves_the_act_done_once_and_a_replay_never_does_it_twice() {
    let web = container("web", RunState::Running);
    let key = key_of(&web);
    let b = bench(Mode::Normal, |engine, _| {
        engine.put_silently(web.clone());
    })
    .await;
    b.read(1).await;
    // L'acte part avec sa clé d'opération, puis le lien tombe avant la réponse : l'agent va au bout.
    let request = b
        .agent
        .confirmed(
            &b.ada,
            "POST",
            "/services/adopt",
            json!({ "keys": [key.clone()] }),
        )
        .await
        .header("idempotency-key", "adopt-cut-1");
    request.send_and_cut().await;
    eventually("l'adoption est faite malgré la coupure", || {
        let env = &b.env;
        let key = key.clone();
        async move { env.adopted_keys().await == [key] }
    })
    .await;
    // Rejouer la même requête (même clé d'opération, nouvelle preuve) rend le premier résultat : rien n'est
    // refait ni consigné de nouveau, et rien n'est jamais rejoué tout seul.
    let again = b
        .agent
        .confirmed(
            &b.ada,
            "POST",
            "/services/adopt",
            json!({ "keys": [key.clone()] }),
        )
        .await
        .header("idempotency-key", "adopt-cut-1")
        .send()
        .await;
    assert!(again.status < 500, "{}", again.text);
    let journal = b.journal().await;
    assert_eq!(
        journal
            .iter()
            .filter(|row| row.0 == "service.adopt")
            .count(),
        1,
        "{journal:?}"
    );
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Rafales : les lectures répétées et les refus répétés ne remplissent pas le journal plafonné
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn repeated_reads_of_the_candidates_by_one_account_are_grouped_and_the_first_entry_stays() {
    let b = bench(Mode::Normal, |engine, _| {
        engine.put_silently(container("alpha", RunState::Running));
    })
    .await;
    b.read(1).await;
    for _ in 0..100 {
        assert_eq!(b.external(&b.ada).await.status, 200);
    }
    let journal = b.journal().await;
    let reads: Vec<_> = journal
        .iter()
        .filter(|row| row.0 == "service.external.read" && row.1 == "ok")
        .collect();
    // La première lecture, puis UNE entrée de synthèse pour les quatre-vingt-dix-neuf suivantes.
    assert!(
        (1..=2).contains(&reads.len()),
        "cent lectures, au plus deux entrées : {journal:?}"
    );
    assert!(reads.iter().all(|row| row.4.as_deref() == Some("ada")));
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_hundred_identical_refusals_leave_a_bounded_number_of_entries_and_the_first_one_stays() {
    let web = container("web", RunState::Running);
    let key = key_of(&web);
    let b = bench(Mode::Normal, |engine, _| {
        engine.put_silently(web.clone());
    })
    .await;
    b.read(1).await;
    assert_eq!(b.adopt(&b.ada, &[key.clone()]).await.body["adopted"], 1);
    // Cent refus de lire les candidats, puis des refus de lire les journaux d'un service adopté.
    for _ in 0..100 {
        assert_eq!(b.external(&b.lucas).await.status, 403);
    }
    for _ in 0..30 {
        assert!(b.logs(&b.lucas, &key).await.contains("MissingRight"));
    }
    let journal = b.journal().await;
    for action in ["service.external.read", "service.logs.read"] {
        let rows: Vec<_> = journal
            .iter()
            .filter(|row| row.0 == action && row.1 == "denied")
            .collect();
        assert!(
            (1..=2).contains(&rows.len()),
            "{action} : refus identiques, au plus deux entrées : {journal:?}"
        );
        // La première entrée est celle du premier refus, écrite tout de suite.
        assert_eq!(rows[0].4.as_deref(), Some("lucas"));
    }
    b.agent.shutdown().await;
}

// ---------------------------------------------------------------------------------------------
// Clé contenant une barre oblique inverse (nom d'unité échappé par le système)
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_unit_with_an_escaped_name_is_adopted_and_released_through_its_encoded_key() {
    let b = bench(Mode::Normal, |engine, supervisor| {
        let (service, _) = under_unit(supervisor, "web", "docker-my\\x2dapp.service", 'a');
        engine.put_silently(service);
    })
    .await;
    b.read(1).await;
    let key = "u_docker-my\\x2dapp.service".to_owned();
    let listed = b.external(&b.ada).await;
    assert_eq!(listed.body["containers"][0]["key"], key.as_str());
    assert_eq!(b.adopt(&b.ada, &[key.clone()]).await.body["adopted"], 1);
    // Le chemin porte la barre oblique inverse encodée ; l'agent la décode comme la liaison, la preuve
    // signe la clé en clair et l'acte relâche bien ce service.
    let reply = b.release(&b.ada, "u_docker-my%5Cx2dapp.service").await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body, json!({ "released": true }));
    assert!(b.env.adopted_keys().await.is_empty());
    // Un segment encodé qui sort de la forme d'une clé ne relâche rien et ne change pas de route.
    for segment in ["u_docker-my%2Fx.service", "..", "u_a%2e%2e%2fb.service"] {
        let reply = b.release(&b.ada, segment).await;
        assert_ne!(reply.status, 500, "{segment} : {}", reply.text);
        assert_ne!(reply.body, json!({ "released": true }), "{segment}");
    }
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_base_ahead_of_the_memory_never_puts_an_unpublished_name_in_the_journal_and_the_memory_catches_up()
 {
    let web = container("sentinel-divergent-web", RunState::Running);
    let key = key_of(&web);
    let b = bench(Mode::Normal, |engine, _| {
        engine.put_silently(web.clone());
    })
    .await;
    b.read(1).await;
    // La base retient l'adoption, la mémoire du registre ne l'a pas relue : le conteneur y est encore un
    // candidat caché.
    b.env.adopt(&key, "sentinel-divergent-web", "marie").await;
    assert!(b.services(&b.bob).await.services.is_empty());
    let reply = b.adopt(&b.ada, &[key.clone()]).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["adopted"], 0);
    assert_eq!(reply.body["results"][0]["result"], "already_adopted");
    // Aucune entrée ne porte le nom d'un conteneur qui n'était pas publié : la cible est la route.
    let journal = b.journal().await;
    assert!(
        journal
            .iter()
            .all(|row| !row.2.as_deref().unwrap_or("").contains("sentinel")),
        "{journal:?}"
    );
    assert_eq!(journal.len(), 1, "{journal:?}");
    assert_eq!(journal[0].2.as_deref(), Some("/services/adopt"));
    // La mémoire a rejoint la base après l'acte : le service est maintenant publié.
    eventually("la mémoire rejoint la base", || async {
        !b.services(&b.bob).await.services.is_empty()
    })
    .await;
    b.agent.shutdown().await;
}

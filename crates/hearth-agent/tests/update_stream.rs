//! La progression d'une mise à jour sur le flux temps réel (BR-UPDATE-013, BR-UPDATE-017) : vrai
//! agent en HTTPS, vrai WebSocket. Le sujet `update` est ouvert à tout compte authentifié ; un
//! client qui s'abonne en cours de route reçoit d'abord l'état courant.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use hearth_agent::domain::accounts::Role;
use hearth_proto::api::update::{UpdateOutcome, UpdateReason, UpdateStep};
use hearth_proto::stream::{Topic, UpdateMessage};
use serde_json::json;
use support::https::Agent;
use support::probe::metering;
use support::update::Rig;
use support::{PASSWORD, env};

const BINARY: &[u8] = b"nouvel agent";

async fn login(agent: &Agent, name: &str) -> String {
    let reply = agent
        .request("POST", "/sessions")
        .json(&json!({ "username": name, "password": PASSWORD }))
        .send()
        .await;
    reply.body["token"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn an_administrator_follows_every_step_and_a_read_only_account_sees_them_too() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    env.create("lucas", Role::ReadOnly).await;
    let rig = Rig::new(&env, true, true);
    let agent = support::https::start_updating(&env, metering(), Some(rig.updating())).await;
    let admin = login(&agent, "marie").await;
    let readonly = login(&agent, "lucas").await;

    let mut watcher = support::ws::open(&agent).await;
    watcher.auth(&readonly).await;
    watcher.subscribe(&[Topic::Update]).await;

    let reply = agent
        .request("POST", "/agent/update")
        .token(&admin)
        .json(&rig.request("0.2.0", BINARY))
        .send()
        .await;
    assert_eq!(reply.status, 202, "{:?}", reply.body);

    // Téléchargement avec pourcentage (porte fermée : le premier message est à 0 %), puis les
    // étapes dans l'ordre.
    let UpdateMessage::Update(first) = watcher.next_update().await;
    assert_eq!(
        (first.step, first.version.as_str()),
        (UpdateStep::Download, "0.2.0")
    );
    rig.release_gate();
    let mut steps = vec![first.step];
    loop {
        let UpdateMessage::Update(progress) = watcher.next_update().await;
        if steps.last() != Some(&progress.step) {
            steps.push(progress.step);
        }
        if progress.step == UpdateStep::Restart {
            break;
        }
    }
    assert_eq!(
        steps,
        [
            UpdateStep::Download,
            UpdateStep::Verify,
            UpdateStep::Install,
            UpdateStep::Restart
        ]
    );
    agent.shutdown().await;
}

#[tokio::test]
async fn a_client_that_subscribes_in_the_middle_gets_the_current_step_then_the_result() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let rig = Rig::new(&env, true, true);
    let agent = support::https::start_updating(&env, metering(), Some(rig.updating())).await;
    let token = login(&agent, "marie").await;
    // Une somme fausse : la mise à jour échoue après le téléchargement.
    let mut body = rig.request("0.2.0", BINARY);
    body["sha256"] = json!("11".repeat(32));
    let accepted = agent
        .request("POST", "/agent/update")
        .token(&token)
        .json(&body)
        .send()
        .await;
    assert_eq!(accepted.status, 202);

    // Le client « revient » : l'état courant d'abord, la fin ensuite (BR-UPDATE-017).
    let mut late = support::ws::open(&agent).await;
    late.auth(&token).await;
    late.subscribe(&[Topic::Update]).await;
    let UpdateMessage::Update(current) = late.next_update().await;
    assert_eq!(current.step, UpdateStep::Download);
    rig.release_gate();
    loop {
        let UpdateMessage::Update(progress) = late.next_update().await;
        if progress.step == UpdateStep::Done {
            assert_eq!(progress.outcome, Some(UpdateOutcome::Failed));
            assert_eq!(progress.reason, Some(UpdateReason::BadChecksum));
            break;
        }
    }
    agent.shutdown().await;
}

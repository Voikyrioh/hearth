//! Les actions qui terminent ou modifient leur propre session, contre un VRAI agent : fermer ses
//! propres sessions, supprimer son compte, définir son mot de passe par la route d'administration,
//! changer son propre mot de passe (la session courante est gardée), se rétrograder. Dans chaque cas
//! l'appelant reçoit le résultat de la RÉPONSE (jamais « résultat inconnu ») que l'avis de fin de
//! session du flux arrive avant ou après elle ; l'ordre exact est prouvé de façon déterministe par
//! `tracking.rs::a_session_end_notice_before_the_response_never_turns_a_delivered_result_into_unknown`.
//! (FIX:01M47PCYX3BY3YV84R9WW3KAQ3)
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use hearth_agent::domain::accounts::Role;
use hearth_link::domain::state::LinkState;
use hearth_link::ports::transport::Method;
use hearth_link::{ActionOutcome, ActionRequest};
use serde_json::json;
use support::{Options, WAIT, World};

async fn world() -> (World, String) {
    let world = World::connected(Options::default().accepting_bare_acts()).await;
    world.agent.create_account("paul", Role::Admin).await;
    let me = world.manager.accounts_list(&world.id).await.unwrap().me;
    (world, me)
}

async fn run(world: &World, request: ActionRequest) -> ActionOutcome {
    tokio::time::timeout(WAIT, world.manager.execute_raw(&world.id, request))
        .await
        .expect("l'action rend la main")
        .unwrap()
}

fn assert_completed(outcome: &ActionOutcome) {
    assert!(
        matches!(outcome, ActionOutcome::Completed { status, .. } if (200..300).contains(status)),
        "{outcome:?}"
    );
}

async fn session_ended(world: &World) {
    world
        .recorder
        .wait_state(0, LinkState::AccessRevoked, WAIT)
        .await;
}

#[tokio::test]
async fn closing_your_own_sessions_returns_its_result_and_ends_the_session() {
    let (world, me) = world().await;
    let outcome = run(
        &world,
        ActionRequest {
            method: Method::Delete,
            path: format!("/accounts/{me}/sessions"),
            body: None,
        },
    )
    .await;
    assert_completed(&outcome);
    session_ended(&world).await;
}

#[tokio::test]
async fn deleting_your_own_account_returns_its_result_and_ends_the_session() {
    let (world, me) = world().await;
    let outcome = run(
        &world,
        ActionRequest {
            method: Method::Delete,
            path: format!("/accounts/{me}"),
            body: Some(json!({ "confirmation": "marie" })),
        },
    )
    .await;
    assert_completed(&outcome);
    session_ended(&world).await;
}

#[tokio::test]
async fn setting_your_own_password_by_the_admin_route_returns_its_result_and_ends_the_session() {
    let (world, me) = world().await;
    let outcome = run(
        &world,
        ActionRequest {
            method: Method::Put,
            path: format!("/accounts/{me}/password"),
            body: Some(json!({ "password": "Sunny-Walk-Home-42" })),
        },
    )
    .await;
    assert_completed(&outcome);
    session_ended(&world).await;
}

#[tokio::test]
async fn changing_your_own_password_keeps_the_current_session() {
    let (world, _) = world().await;
    let outcome = run(
        &world,
        ActionRequest {
            method: Method::Put,
            path: "/me/password".into(),
            body: Some(json!({ "current": support::PASSWORD, "password": "Sunny-Walk-Home-42" })),
        },
    )
    .await;
    assert_completed(&outcome);
    // Aucun avis ne fait tomber la session courante : la liste se lit encore.
    world.manager.accounts_list(&world.id).await.unwrap();
    assert_eq!(world.state().state, LinkState::Connected);
}

#[tokio::test]
async fn demoting_yourself_returns_its_result_and_keeps_the_link() {
    let (world, me) = world().await;
    let outcome = run(
        &world,
        ActionRequest {
            method: Method::Patch,
            path: format!("/accounts/{me}"),
            body: Some(json!({ "role": "readonly" })),
        },
    )
    .await;
    assert_completed(&outcome);
    assert_eq!(world.state().state, LinkState::Connected);
}

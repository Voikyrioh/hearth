//! HRT-78 : le droit « gérer les services » du compte connecté, lu contre un VRAI agent (TLS 1.3,
//! SQLite) : `rights` de `GET /me` et capacité `service-rights` de `GET /hello` (BR-GRANT-011,
//! BR-GRANT-016) et le même droit sur chaque compte de la liste (BR-GRANT-013). Aucune attente de durée.
//! (Le refus faute du droit, qui garde son `details.missing`, est éprouvé par `manager::audit::tests` et par la
//! coquille.)
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use hearth_agent::domain::accounts::Role;
use hearth_link::LinkError;
use hearth_proto::api::accounts::{RIGHT_SERVICES, has_right};
use support::{Options, World};

#[tokio::test]
async fn a_fresh_account_has_no_right_and_the_agent_announces_that_it_knows_the_right() {
    let world = World::connected(Options::default()).await;
    let read = world.manager.service_rights(&world.id).await.unwrap();
    assert!(read.agent_knows);
    assert!(
        !read.granted,
        "aucun compte n'a le droit à l'arrivée (BR-GRANT-002)"
    );
    assert_eq!(read.account, "marie");
}

#[tokio::test]
async fn the_right_is_read_again_each_time_and_follows_the_server() {
    let world = World::connected(Options::default()).await;
    world.agent.grant_services("marie").await;
    assert!(
        world
            .manager
            .service_rights(&world.id)
            .await
            .unwrap()
            .granted
    );
    world.agent.revoke_services("marie").await;
    assert!(
        !world
            .manager
            .service_rights(&world.id)
            .await
            .unwrap()
            .granted
    );
}

#[tokio::test]
async fn a_read_only_account_with_the_right_can_read_it() {
    let world = World::connected(Options {
        role: Role::ReadOnly,
        ..Options::default()
    })
    .await;
    world.agent.grant_services("marie").await;
    let read = world.manager.service_rights(&world.id).await.unwrap();
    assert!(read.agent_knows && read.granted);
}

#[tokio::test]
async fn an_agent_that_does_not_announce_the_capability_is_read_as_granting_nobody() {
    let world = World::connected(Options {
        hide_capabilities: true,
        ..Options::default()
    })
    .await;
    world.agent.grant_services("marie").await;
    let read = world.manager.service_rights(&world.id).await.unwrap();
    assert!(!read.agent_knows);
    assert!(!read.granted, "BR-GRANT-016 : non accordé pour tous");
}

#[tokio::test]
async fn the_list_of_accounts_carries_the_right_of_each_account_and_whether_the_agent_knows_it() {
    let world = World::connected(Options::default()).await;
    world.agent.create_account("paul", Role::ReadOnly).await;
    world.agent.grant_services("paul").await;
    let read = world.manager.accounts_list(&world.id).await.unwrap();
    assert!(read.rights_known);
    let by_name = |name: &str| read.accounts.iter().find(|a| a.username == name).unwrap();
    assert!(has_right(&by_name("paul").rights, RIGHT_SERVICES));
    assert!(!has_right(&by_name("marie").rights, RIGHT_SERVICES));
}

#[tokio::test]
async fn the_list_says_when_the_agent_does_not_know_the_right() {
    let world = World::connected(Options {
        hide_capabilities: true,
        ..Options::default()
    })
    .await;
    assert!(
        !world
            .manager
            .accounts_list(&world.id)
            .await
            .unwrap()
            .rights_known
    );
}

#[tokio::test]
async fn nothing_is_sent_for_a_server_that_is_not_connected() {
    let world = World::connected(Options::default()).await;
    world.manager.logout(&world.id).await.unwrap();
    assert_eq!(
        world.manager.service_rights(&world.id).await.unwrap_err(),
        LinkError::NotConnected
    );
}

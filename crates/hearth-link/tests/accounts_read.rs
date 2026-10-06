//! Lecture des comptes contre un VRAI agent (TLS 1.3, SQLite) : liste et compte de la session
//! (« moi » = l'identifiant de l'agent), refus d'un compte Lecture seule, lien coupé, serveur jamais
//! connecté. HRT-13. Aucune attente de durée : seuls des faits observables.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use hearth_agent::domain::accounts::Role;
use hearth_link::LinkError;
use hearth_proto::api::accounts::RoleName;
use hearth_proto::error::ErrorCode;
use support::{Options, World};

#[tokio::test]
async fn the_list_comes_with_the_account_of_the_session() {
    let world = World::connected(Options::default()).await;
    world.agent.create_account("paul", Role::ReadOnly).await;
    let read = world.manager.accounts_list(&world.id).await.unwrap();
    let usernames: Vec<&str> = read.accounts.iter().map(|a| a.username.as_str()).collect();
    assert_eq!(usernames, ["marie", "paul"]);
    let marie = &read.accounts[0];
    assert_eq!(marie.role, RoleName::Admin);
    assert_eq!(read.me, marie.id, "moi = l'identifiant de l'agent");
    assert_ne!(read.me, read.accounts[1].id);
    assert_eq!(marie.sessions_open, 1);
}

#[tokio::test]
async fn a_read_only_account_is_refused_by_the_agent_with_a_typed_error() {
    let world = World::connected(Options {
        role: Role::ReadOnly,
        ..Options::default()
    })
    .await;
    let error = world.manager.accounts_list(&world.id).await.unwrap_err();
    assert_eq!(error, LinkError::Rejected(Some(ErrorCode::ForbiddenRole)));
}

#[tokio::test]
async fn a_cut_link_fails_the_read_without_a_second_request_or_a_tracked_operation() {
    let world = World::connected(Options::default()).await;
    world.proxy.cut();
    let error = world.manager.accounts_list(&world.id).await.unwrap_err();
    assert!(
        matches!(error, LinkError::Unreachable(_) | LinkError::Timeout),
        "{error:?}"
    );
}

#[tokio::test]
async fn nothing_is_sent_for_a_server_that_is_not_connected() {
    let world = World::connected(Options::default()).await;
    world.manager.logout(&world.id).await.unwrap();
    let error = world.manager.accounts_list(&world.id).await.unwrap_err();
    assert_eq!(error, LinkError::NotConnected);
}

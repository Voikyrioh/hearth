//! HRT-77, BR-GRANT-008 et BR-GRANT-009 : la commande `account services {compte} --grant | --revoke`.
//!
//! Le vrai cas d'usage (`ServicesRightService`) et le vrai point d'entrée (`account::execute`) tournent
//! sur une vraie base temporaire. On vérifie ce qui est dit, ce qui est écrit en base, et surtout ce que
//! le journal d'activité contient : une entrée par changement EFFECTIF, avec l'origine « ligne de
//! commande du serveur », et aucune quand la commande est sans effet ou refusée.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;

use hearth_agent::application::ports::{AuditFeed, Store};
use hearth_agent::application::services_right::ServicesRightService;
use hearth_agent::domain::accounts::Role;
use hearth_agent::domain::secret::Secret;
use hearth_agent::entrypoint::account::{self, AccountCliError, PasswordInput};
use hearth_agent::entrypoint::cli::AccountAction;
use hearth_agent::infrastructure::sqlite::{SqliteAccountRepo, SqliteStore};
use support::Env;

/// La commande n'a jamais besoin d'un mot de passe : en demander un est une erreur.
struct NoPassword;

impl PasswordInput for NoPassword {
    fn new_password(&self) -> Result<Secret, AccountCliError> {
        Err(AccountCliError::PasswordInput(
            "aucun mot de passe attendu".into(),
        ))
    }
}

fn rights_service(env: &Env) -> ServicesRightService {
    let store: Arc<dyn Store> = Arc::new(SqliteStore::new(env.db.pool().clone()));
    ServicesRightService::new(
        Arc::new(SqliteAccountRepo::new(env.db.pool().clone())),
        store,
        env.clock.clone(),
        env.trail.clone(),
    )
}

fn services(username: &str, grant: bool, revoke: bool) -> AccountAction {
    AccountAction::Services {
        username: username.to_owned(),
        grant,
        revoke,
    }
}

async fn run(
    env: &Env,
    action: &AccountAction,
    privileged: bool,
) -> (Result<(), AccountCliError>, String) {
    let rights = rights_service(env);
    let mut out = Vec::new();
    let result = account::execute(
        action,
        &env.service,
        &rights,
        &NoPassword,
        privileged,
        &mut out,
    )
    .await;
    (result, String::from_utf8(out).unwrap())
}

/// Les entrées du journal qui parlent du droit : (code, origine, compte acteur, cible, résultat).
async fn right_entries(env: &Env) -> Vec<(String, String, Option<String>, Option<String>, String)> {
    sqlx::query_as(
        "SELECT action, origin_kind, account, target, outcome FROM audit_events
         WHERE action LIKE 'account.services.%' ORDER BY id",
    )
    .fetch_all(env.db.pool())
    .await
    .unwrap()
}

async fn audit_count(env: &Env) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM audit_events")
        .fetch_one(env.db.pool())
        .await
        .unwrap()
}

#[tokio::test]
async fn grant_then_revoke_change_the_right_say_so_and_are_journaled_from_the_command_line() {
    let env = support::env().await;
    env.create("marie", Role::Admin).await;
    assert_eq!(env.services_right_of("marie").await, "none");

    let (result, out) = run(&env, &services("marie", true, false), true).await;
    result.unwrap();
    assert_eq!(
        out,
        "Le compte marie peut maintenant gérer les services de ce serveur.\n"
    );
    assert_eq!(env.services_right_of("marie").await, "manage");

    let (result, out) = run(&env, &services("marie", false, true), true).await;
    result.unwrap();
    assert_eq!(
        out,
        "Le compte marie ne peut plus gérer les services de ce serveur.\n"
    );
    assert_eq!(env.services_right_of("marie").await, "none");

    let entries = right_entries(&env).await;
    assert_eq!(
        entries,
        vec![
            (
                "account.services.grant".to_owned(),
                "cli".to_owned(),
                None,
                Some("marie".to_owned()),
                "ok".to_owned()
            ),
            (
                "account.services.revoke".to_owned(),
                "cli".to_owned(),
                None,
                Some("marie".to_owned()),
                "ok".to_owned()
            ),
        ],
        "BR-GRANT-009 : origine « ligne de commande », cible le compte, résultat réussi"
    );
}

#[tokio::test]
async fn the_labels_of_the_journal_are_those_of_the_spec() {
    let env = support::env().await;
    env.create("marie", Role::ReadOnly).await;
    run(&env, &services("marie", true, false), true)
        .await
        .0
        .unwrap();
    run(&env, &services("marie", false, true), true)
        .await
        .0
        .unwrap();
    let labels: Vec<String> = sqlx::query_scalar(
        "SELECT action_label FROM audit_events WHERE action LIKE 'account.services.%' ORDER BY id",
    )
    .fetch_all(env.db.pool())
    .await
    .unwrap();
    assert_eq!(
        labels,
        [
            "Droit de gérer les services accordé",
            "Droit de gérer les services retiré"
        ]
    );
}

#[tokio::test]
async fn a_read_only_account_can_be_granted_and_keeps_its_role() {
    let env = support::env().await;
    env.create("lecteur", Role::ReadOnly).await;
    run(&env, &services("lecteur", true, false), true)
        .await
        .0
        .unwrap();
    assert_eq!(env.services_right_of("lecteur").await, "manage");
    let role: String = sqlx::query_scalar("SELECT role FROM accounts WHERE username = 'lecteur'")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert_eq!(role, "readonly");
}

#[tokio::test]
async fn a_command_without_effect_says_so_and_adds_no_journal_entry() {
    let env = support::env().await;
    env.create("marie", Role::Admin).await;

    // Déjà « non accordé » : le retrait est sans effet.
    let before = audit_count(&env).await;
    let (result, out) = run(&env, &services("marie", false, true), true).await;
    result.unwrap();
    assert_eq!(
        out,
        "Le compte marie ne pouvait pas gérer les services de ce serveur.\n"
    );
    assert_eq!(audit_count(&env).await, before, "aucune entrée de journal");

    run(&env, &services("marie", true, false), true)
        .await
        .0
        .unwrap();
    let before = audit_count(&env).await;
    let (result, out) = run(&env, &services("marie", true, false), true).await;
    result.unwrap();
    assert_eq!(
        out,
        "Le compte marie peut déjà gérer les services de ce serveur.\n"
    );
    assert_eq!(audit_count(&env).await, before, "aucune entrée de journal");
    assert_eq!(env.services_right_of("marie").await, "manage");
}

#[tokio::test]
async fn an_unknown_account_is_refused_and_nothing_is_written_or_journaled() {
    let env = support::env().await;
    env.create("marie", Role::Admin).await;
    let before = audit_count(&env).await;

    for name in ["paul", "x", "Marie Dupont", "../etc/passwd", ""] {
        let (result, out) = run(&env, &services(name, true, false), true).await;
        let error = result.unwrap_err();
        assert!(
            matches!(error, AccountCliError::UnknownAccount(_)),
            "{name:?} : {error}"
        );
        assert_eq!(
            error.to_string(),
            format!("Aucun compte « {name} » sur ce serveur.")
        );
        assert!(out.is_empty());
    }
    assert_eq!(env.services_right_of("marie").await, "none");
    assert_eq!(audit_count(&env).await, before);
}

#[tokio::test]
async fn the_name_is_never_replayed_with_terminal_control_characters() {
    let env = support::env().await;
    let (result, _) = run(&env, &services("a\u{1b}[2Jb", true, false), true).await;
    let message = result.unwrap_err().to_string();
    assert!(!message.contains('\u{1b}'), "{message:?}");
    assert_eq!(message, "Aucun compte « a?[2Jb » sur ce serveur.");
}

#[tokio::test]
async fn an_option_is_required_and_nothing_is_written_without_one() {
    let env = support::env().await;
    env.create("marie", Role::Admin).await;
    let before = audit_count(&env).await;
    let (result, out) = run(&env, &services("marie", false, false), true).await;
    let error = result.unwrap_err();
    assert!(matches!(error, AccountCliError::NoOption));
    assert_eq!(error.to_string(), "Précise --grant ou --revoke.");
    assert!(out.is_empty());
    // Les deux à la fois (impossible par la ligne de commande, que clap refuse) ne devine rien non plus.
    let (result, _) = run(&env, &services("marie", true, true), true).await;
    assert!(matches!(result, Err(AccountCliError::NoOption)));
    assert_eq!(env.services_right_of("marie").await, "none");
    assert_eq!(audit_count(&env).await, before);
}

#[tokio::test]
async fn outside_root_the_command_is_refused_first_and_changes_nothing() {
    let env = support::env().await;
    env.create("marie", Role::Admin).await;
    let before = audit_count(&env).await;
    // Hors root, même sans option et même pour un compte inconnu : le premier refus est celui de root,
    // qui ne laisse rien deviner de l'existence des comptes.
    for action in [
        services("marie", true, false),
        services("marie", false, true),
        services("marie", false, false),
        services("inconnu", true, false),
    ] {
        let (result, out) = run(&env, &action, false).await;
        let error = result.unwrap_err();
        assert!(matches!(error, AccountCliError::NotRoot), "{error}");
        assert_eq!(
            error.to_string(),
            "Cette commande doit être lancée en root sur le serveur."
        );
        assert!(out.is_empty());
    }
    assert_eq!(env.services_right_of("marie").await, "none");
    assert_eq!(audit_count(&env).await, before);
}

#[tokio::test]
async fn an_account_created_on_the_command_line_is_born_without_the_right() {
    let env = support::env().await;
    let (result, _) = {
        let rights = rights_service(&env);
        let mut out = Vec::new();
        struct Fixed;
        impl PasswordInput for Fixed {
            fn new_password(&self) -> Result<Secret, AccountCliError> {
                Ok(Secret::new(support::PASSWORD.to_owned()))
            }
        }
        let result = account::execute(
            &AccountAction::Add {
                username: "nouveau".to_owned(),
                role: Role::Admin,
            },
            &env.service,
            &rights,
            &Fixed,
            true,
            &mut out,
        )
        .await;
        (result, out)
    };
    result.unwrap();
    assert_eq!(
        env.services_right_of("nouveau").await,
        "none",
        "BR-GRANT-002"
    );
    assert!(right_entries(&env).await.is_empty());
}

#[tokio::test]
async fn the_entry_is_published_to_the_live_feed_only_after_it_is_written() {
    // Le journal est écrit dans la transaction du changement : quand la commande répond, l'entrée est
    // déjà en base ; le direct (abonnés du flux) la reçoit ensuite.
    let env = support::env().await;
    env.create("marie", Role::Admin).await;
    let mut live = env.feed.subscribe();
    let (result, _) = run(&env, &services("marie", true, false), true).await;
    result.unwrap();
    assert_eq!(right_entries(&env).await.len(), 1);
    let event = live.try_recv().expect("l'entrée est publiée");
    assert_eq!(event.action, "account.services.grant");
}

/// Le compte disparaît entre la lecture et l'écriture (supprimé par l'agent qui tourne) : la requête
/// n'écrit aucune ligne, c'est une ERREUR, et la transaction n'est pas validée. Aucune entrée « droit
/// accordé » n'est laissée pour un droit qui n'existe pas.
#[tokio::test]
async fn a_right_written_for_a_vanished_account_is_an_error_and_journals_nothing() {
    use hearth_agent::domain::accounts::{AccountId, ServicesRight};

    let env = support::env().await;
    let store = SqliteStore::new(env.db.pool().clone());
    let mut tx = store.begin().await.unwrap();
    let result = tx
        .services_right()
        .set_services_right(
            &AccountId::new("01JVANISHED000000000000000"),
            ServicesRight::Manage,
        )
        .await;
    assert!(result.is_err(), "aucune ligne écrite : une erreur");
    drop(tx);
    assert!(right_entries(&env).await.is_empty());
}

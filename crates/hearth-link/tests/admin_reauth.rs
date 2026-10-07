//! La confirmation des actes d'administration côté liaison, contre un VRAI agent qui EXIGE la
//! confirmation (TLS 1.3, SQLite) : mot de passe ET preuve de la clé de ce poste (usage `0x05`) sur les
//! dix actes, défi neuf et clé d'opération neuve à chaque essai, rien d'envoyé sans clé, élévation de
//! 5 minutes, agent ancien, client ancien. HRT-30 (ADR-0031, ADR-0033, BR-TRUST-036 à 046).
//! Aucune attente de durée.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use hearth_link::domain::secret::Secret;
use hearth_link::ports::transport::Method;
use hearth_link::{ActionOutcome, ActionRequest, LinkError};
use hearth_proto::admin_act::{Contract, ROUTES};
use hearth_proto::api::sessions::ChallengePurpose;
use serde_json::{Value, json};
use support::{ChallengeMode, Options, PASSWORD, World};

const OTHER: &str = "Another-Pass-77";
const NEW_PASSWORD: &str = "Sunny-Walk-Home-42";

fn requiring() -> Options {
    Options {
        device_key: true,
        reauth_required: true,
        ..Options::default()
    }
}

fn request(method: Method, path: &str, body: Option<Value>) -> ActionRequest {
    ActionRequest {
        method,
        path: path.to_owned(),
        body,
    }
}

fn completed(outcome: &ActionOutcome) -> (u16, Value) {
    let ActionOutcome::Completed { status, body, .. } = outcome else {
        panic!("réponse attendue, reçu {outcome:?}");
    };
    (*status, body.clone())
}

fn code_of(body: &Value) -> String {
    body["error"]["code"].as_str().unwrap_or("").to_owned()
}

fn reason_of(body: &Value) -> String {
    body["error"]["details"]["reason"]
        .as_str()
        .unwrap_or("")
        .to_owned()
}

async fn act(
    world: &World,
    method: Method,
    path: &str,
    body: Option<Value>,
    password: Option<&str>,
) -> (u16, Value) {
    let password = password.map(Secret::from);
    let outcome = world
        .manager
        .execute_act(&world.id, request(method, path, body), password.as_ref())
        .await
        .unwrap();
    completed(&outcome)
}

async fn account_id(world: &World, username: &str) -> String {
    world
        .manager
        .accounts_list(&world.id)
        .await
        .unwrap()
        .accounts
        .into_iter()
        .find(|account| account.username == username)
        .unwrap_or_else(|| panic!("compte {username} absent"))
        .id
}

async fn account_count(world: &World) -> usize {
    world
        .manager
        .accounts_list(&world.id)
        .await
        .unwrap()
        .accounts
        .len()
}

fn create_body(name: &str) -> Value {
    json!({ "username": name, "password": NEW_PASSWORD, "role": "readonly" })
}

#[tokio::test]
async fn the_ten_acts_are_signed_as_they_are_sent_and_the_agent_accepts_each_one() {
    let world = World::connected(requiring()).await;
    let mut challenges = 0;
    let mut expect_challenge = |world: &World| {
        challenges += 1;
        assert_eq!(
            world.spy.calls_for(ChallengePurpose::AdminAct),
            challenges,
            "un défi neuf par acte"
        );
    };

    let (status, _) = act(
        &world,
        Method::Post,
        "/accounts",
        Some(create_body("paul")),
        Some(PASSWORD),
    )
    .await;
    assert_eq!(status, 201);
    expect_challenge(&world);
    let paul = account_id(&world, "paul").await;

    let (status, _) = act(
        &world,
        Method::Patch,
        &format!("/accounts/{paul}"),
        Some(json!({ "role": "admin" })),
        Some(PASSWORD),
    )
    .await;
    assert!((200..300).contains(&status), "{status}");
    expect_challenge(&world);

    let (status, _) = act(
        &world,
        Method::Put,
        &format!("/accounts/{paul}/password"),
        Some(json!({ "password": OTHER })),
        Some(PASSWORD),
    )
    .await;
    assert!((200..300).contains(&status), "{status}");
    expect_challenge(&world);

    let (status, _) = act(
        &world,
        Method::Delete,
        &format!("/accounts/{paul}/sessions"),
        None,
        Some(PASSWORD),
    )
    .await;
    assert!((200..300).contains(&status), "{status}");
    expect_challenge(&world);

    let (status, _) = act(
        &world,
        Method::Delete,
        &format!("/accounts/{paul}"),
        None,
        Some(PASSWORD),
    )
    .await;
    assert!((200..300).contains(&status), "{status}");
    expect_challenge(&world);

    for active in [true, false] {
        let outcome = world
            .manager
            .set_attack_mode(&world.id, active, &Secret::from(PASSWORD))
            .await
            .unwrap();
        assert!((200..300).contains(&completed(&outcome).0));
        expect_challenge(&world);
    }

    let (status, body) = act(
        &world,
        Method::Put,
        "/me/reauth",
        Some(json!({ "password": "each" })),
        Some(PASSWORD),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["password"], "each");
    expect_challenge(&world);

    let (status, _) = act(
        &world,
        Method::Put,
        "/me/password",
        Some(json!({ "current": PASSWORD, "password": NEW_PASSWORD })),
        Some(PASSWORD),
    )
    .await;
    assert!((200..300).contains(&status), "{status}");
    expect_challenge(&world);
}

#[tokio::test]
async fn each_try_takes_a_new_challenge_and_a_new_operation_key_so_a_refusal_is_never_replayed() {
    let world = World::connected(requiring()).await;
    for _ in 0..2 {
        let outcome = world
            .manager
            .execute_act(
                &world.id,
                request(Method::Post, "/accounts", Some(create_body("paul"))),
                Some(&Secret::from("Faux-Mot-De-Passe-1")),
            )
            .await
            .unwrap();
        let ActionOutcome::Completed {
            status,
            body,
            replayed,
        } = outcome
        else {
            panic!("réponse attendue");
        };
        assert_eq!((status, code_of(&body).as_str()), (422, "WRONG_PASSWORD"));
        assert!(!replayed, "un refus de confirmation n'est pas rejoué");
    }
    assert_eq!(account_count(&world).await, 1, "rien n'a été créé");
    let (status, _) = act(
        &world,
        Method::Post,
        "/accounts",
        Some(create_body("paul")),
        Some(PASSWORD),
    )
    .await;
    assert_eq!(status, 201, "le bon mot de passe passe, avec un défi neuf");
    assert_eq!(world.spy.calls_for(ChallengePurpose::AdminAct), 3);
}

#[tokio::test]
async fn a_wrong_password_counts_like_a_failed_login_until_the_agent_makes_you_wait() {
    let world = World::connected(requiring()).await;
    let mut codes = Vec::new();
    for _ in 0..8 {
        let outcome = world
            .manager
            .set_attack_mode(&world.id, true, &Secret::from("Faux-Mot-De-Passe-1"))
            .await
            .unwrap();
        let (status, body) = completed(&outcome);
        codes.push((status, code_of(&body)));
        if status == 429 {
            assert!(
                body["error"]["details"]["retry_after_s"].as_u64().unwrap() > 0,
                "{body}"
            );
            break;
        }
    }
    let last = codes.last().unwrap();
    assert_eq!(last.0, 429, "{codes:?}");
    assert_eq!(last.1, "TOO_MANY_ATTEMPTS");
    assert!(
        codes[..codes.len() - 1]
            .iter()
            .all(|(status, code)| *status == 422 && code == "WRONG_PASSWORD"),
        "{codes:?}"
    );
    assert!(
        codes.len() > 1,
        "au moins un faux mot de passe avant l'attente"
    );
}

#[tokio::test]
async fn without_a_key_no_call_leaves_not_even_a_challenge() {
    let world = World::connected(Options {
        device_key: false,
        reauth_required: true,
        ..Options::default()
    })
    .await;
    assert!(!world.manager.has_device_key(&world.id));
    let writes = world.spy.write_count();
    let challenges = world.spy.calls();
    let before = account_count(&world).await;
    let error = world
        .manager
        .execute_act(
            &world.id,
            request(Method::Post, "/accounts", Some(create_body("paul"))),
            Some(&Secret::from(PASSWORD)),
        )
        .await
        .unwrap_err();
    assert_eq!(error, LinkError::NoDeviceKey);
    assert_eq!(world.spy.calls(), challenges, "pas même un défi");
    assert_eq!(
        world.spy.write_count(),
        writes,
        "aucune écriture n'est partie"
    );
    assert_eq!(account_count(&world).await, before);
}

#[tokio::test]
async fn an_unavailable_challenge_is_said_so_and_nothing_else_leaves() {
    for mode in [ChallengeMode::Unreachable, ChallengeMode::Garbage] {
        let world = World::connected(requiring()).await;
        let writes = world.spy.write_count();
        world.spy.set(mode);
        let error = world
            .manager
            .execute_act(
                &world.id,
                request(Method::Post, "/accounts", Some(create_body("paul"))),
                Some(&Secret::from(PASSWORD)),
            )
            .await
            .unwrap_err();
        assert_eq!(error, LinkError::DeviceChallengeUnavailable, "{mode:?}");
        // Un seul défi demandé (aucun repli sans preuve, aucune nouvelle tentative) et aucune requête qui
        // modifie n'est partie : ni mot de passe, ni preuve.
        assert_eq!(
            world.spy.calls_for(ChallengePurpose::AdminAct),
            1,
            "{mode:?}"
        );
        assert_eq!(world.spy.write_count(), writes, "{mode:?}");
        world.spy.set(ChallengeMode::Real);
        assert_eq!(account_count(&world).await, 1, "{mode:?}");
    }
}

/// Un exemple lisible pour chaque route de la liste fermée.
fn sample(method: &str, pattern: &str) -> (Method, String, Option<Value>) {
    let method = match method {
        "POST" => Method::Post,
        "PUT" => Method::Put,
        "PATCH" => Method::Patch,
        "DELETE" => Method::Delete,
        other => panic!("méthode {other}"),
    };
    let path = pattern.replace("{id}", "01ABC");
    let body = match pattern {
        "/accounts" => Some(create_body("paul")),
        "/accounts/{id}" if method == Method::Patch => Some(json!({ "role": "readonly" })),
        "/agent/update" => Some(json!({ "version": "0.2.0", "sha256": "ab" })),
        "/security/attack-mode" => Some(json!({ "active": true })),
        "/me/password" => Some(json!({ "current": "x", "password": "y" })),
        "/me/reauth" => Some(json!({ "password": "each" })),
        _ => None,
    };
    (method, path, body)
}

#[tokio::test]
async fn no_admin_act_leaves_the_link_without_a_confirmation() {
    let world = World::connected(requiring()).await;
    let writes = world.spy.write_count();
    let challenges = world.spy.calls();
    for route in ROUTES {
        let (method, path, body) = sample(route.method, route.pattern);
        let error = world
            .manager
            .execute(&world.id, request(method, &path, body))
            .await
            .unwrap_err();
        assert_eq!(error, LinkError::ActionUnconfirmed, "{route:?}");
        // La porte des actes ne sert pas à une route libre, ni au retrait d'un poste (sa propre
        // fonction, son propre contrat).
        if route.contract == Contract::LegacyRemoval {
            let (method, path, body) = sample(route.method, route.pattern);
            let error = world
                .manager
                .execute_act(
                    &world.id,
                    request(method, &path, body),
                    Some(&Secret::from(PASSWORD)),
                )
                .await
                .unwrap_err();
            assert_eq!(error, LinkError::UnreadableAct, "{route:?}");
        }
    }
    assert_eq!(world.spy.write_count(), writes, "rien n'est parti");
    assert_eq!(world.spy.calls(), challenges, "pas même un défi");
    // Une route libre part par `execute`.
    let outcome = world
        .manager
        .execute(&world.id, request(Method::Delete, "/accounts/NOPE/x", None))
        .await
        .unwrap();
    assert_eq!(completed(&outcome).0, 404);
}

#[tokio::test]
async fn an_act_without_a_password_and_not_covered_by_the_elevation_is_not_sent() {
    let world = World::connected(requiring()).await;
    let writes = world.spy.write_count();
    let challenges = world.spy.calls();
    for (method, path, body) in [
        (
            Method::Put,
            "/me/password",
            json!({ "current": PASSWORD, "password": NEW_PASSWORD }),
        ),
        (Method::Put, "/me/reauth", json!({ "password": "each" })),
        (
            Method::Put,
            "/security/attack-mode",
            json!({ "active": true }),
        ),
    ] {
        let error = world
            .manager
            .execute_act(&world.id, request(method, path, Some(body)), None)
            .await
            .unwrap_err();
        assert_eq!(
            error,
            LinkError::InvalidInput(hearth_link::InputField::Credentials),
            "{path}"
        );
    }
    assert_eq!(world.spy.calls(), challenges);
    assert_eq!(world.spy.write_count(), writes);
}

#[tokio::test]
async fn a_covered_act_passes_without_a_password_during_the_elevation_and_asks_again_when_it_closes()
 {
    let world = World::connected(requiring()).await;
    let state = world.manager.admin_reauth(&world.id).await.unwrap();
    assert!(state.has_device_key);
    let info = state.agent.expect("l'agent annonce la confirmation");
    assert!(info.required);
    assert_eq!(info.elevated_for_s, 0);
    assert_eq!(info.password.as_str(), "window");

    // Un mot de passe juste ouvre l'élévation (créer un compte en lecture seule : couvert).
    let (status, _) = act(
        &world,
        Method::Post,
        "/accounts",
        Some(create_body("paul")),
        Some(PASSWORD),
    )
    .await;
    assert_eq!(status, 201);
    let info = world
        .manager
        .admin_reauth(&world.id)
        .await
        .unwrap()
        .agent
        .unwrap();
    assert!(
        info.elevated_for_s > 0 && info.elevated_for_s <= 300,
        "{info:?}"
    );
    let paul = account_id(&world, "paul").await;

    // Pendant l'élévation : la preuve de clé suffit (un défi neuf, aucun mot de passe).
    let (status, _) = act(
        &world,
        Method::Delete,
        &format!("/accounts/{paul}/sessions"),
        None,
        None,
    )
    .await;
    assert!((200..300).contains(&status), "{status}");

    // Passer le réglage à « à chaque action » ferme l'élévation (BR-TRUST-043) : la même action sans mot
    // de passe est refusée, `password_required`, et rien n'a changé.
    let (status, _) = act(
        &world,
        Method::Put,
        "/me/reauth",
        Some(json!({ "password": "each" })),
        Some(PASSWORD),
    )
    .await;
    assert_eq!(status, 200);
    let writes = world.spy.write_count();
    let (status, body) = act(
        &world,
        Method::Delete,
        &format!("/accounts/{paul}/sessions"),
        None,
        None,
    )
    .await;
    assert_eq!(
        (status, code_of(&body).as_str()),
        (409, "POST_NOT_RECOGNIZED")
    );
    assert_eq!(reason_of(&body), "password_required");
    assert_eq!(world.spy.write_count(), writes + 1);

    // Le client redemande, puis renvoie avec un défi neuf et une clé d'opération neuve.
    let (status, _) = act(
        &world,
        Method::Delete,
        &format!("/accounts/{paul}/sessions"),
        None,
        Some(PASSWORD),
    )
    .await;
    assert!((200..300).contains(&status), "{status}");
}

#[tokio::test]
async fn an_old_client_is_told_to_update_and_keeps_its_link_and_its_reads() {
    let world = World::connected(requiring()).await;
    let outcome = world
        .manager
        .execute_raw(
            &world.id,
            request(Method::Post, "/accounts", Some(create_body("paul"))),
        )
        .await
        .unwrap();
    let (status, body) = completed(&outcome);
    assert_eq!(
        (status, code_of(&body).as_str()),
        (426, "INCOMPATIBLE_VERSION")
    );
    assert_eq!(body["error"]["details"]["upgrade"], "client");
    assert_eq!(reason_of(&body), "reauth_required");
    // Le lien reste connecté et la lecture marche : ni alerte, ni état bloqué.
    assert_eq!(
        world.state().state,
        hearth_link::domain::state::LinkState::Connected
    );
    assert_eq!(world.state().blocked, None);
    assert_eq!(account_count(&world).await, 1);
}

#[tokio::test]
async fn an_agent_from_before_sends_the_act_as_it_always_did_with_no_extra_demand() {
    let world = World::connected(Options {
        device_key: false,
        ..Options::default()
    })
    .await;
    world.spy.hide_reauth(true);
    let challenges = world.spy.calls();
    let state = world.manager.admin_reauth(&world.id).await.unwrap();
    assert!(state.agent.is_none(), "aucune capacité annoncée");
    let (status, _) = act(
        &world,
        Method::Post,
        "/accounts",
        Some(create_body("paul")),
        None,
    )
    .await;
    assert_eq!(status, 201);
    assert_eq!(
        world.spy.calls(),
        challenges,
        "aucun défi : tout comme avant"
    );
}

#[tokio::test]
async fn an_agent_from_before_gets_the_attack_mode_in_its_delivered_flat_form() {
    let world = World::connected(Options {
        device_key: true,
        ..Options::default()
    })
    .await;
    world.spy.hide_reauth(true);
    let outcome = world
        .manager
        .set_attack_mode(&world.id, true, &Secret::from(PASSWORD))
        .await
        .unwrap();
    assert_eq!(completed(&outcome).0, 200);
    assert_eq!(world.spy.calls_for(ChallengePurpose::AttackMode), 1);
    assert_eq!(world.spy.calls_for(ChallengePurpose::AdminAct), 0);
}

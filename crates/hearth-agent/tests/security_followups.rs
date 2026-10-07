//! Suivis de HRT-24 : le choix « garder l'adresse d'où part cette requête » au changement de son
//! propre mot de passe (Q15, BR-CONN-019), et la fermeture des sessions sans lien au retrait d'un
//! poste (revue de la PR #25, BR-TRUST-022).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use hearth_agent::application::sessions::AuthError;
use hearth_agent::domain::accounts::Role;
use hearth_agent::domain::sessions::SessionEnd;
use serde_json::json;
use support::api::Api;
use support::device::DeviceKey;
use support::{Env, PASSWORD, by, client_at, env, secret};

const NEW_PASSWORD: &str = "Brand-New-Secret-4321";

async fn addresses(env: &Env) -> Vec<String> {
    let mut rows: Vec<String> = sqlx::query_scalar("SELECT address FROM known_addresses")
        .fetch_all(env.db.pool())
        .await
        .unwrap();
    rows.sort();
    rows
}

/// `marie` a deux adresses apprises sans clé (A, B) et un poste à clé (K).
async fn marie_with_three_addresses() -> Env {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    for from in ["10.7.7.1", "10.7.7.2"] {
        env.sessions
            .login("marie", secret(PASSWORD), &client_at(from))
            .await
            .unwrap();
    }
    let key = DeviceKey::new();
    let proof = key.login_proof(&env, "marie", "10.7.7.3");
    env.sessions
        .login_with_device(
            "marie",
            secret(PASSWORD),
            &client_at("10.7.7.3"),
            Some(&proof),
        )
        .await
        .unwrap();
    env
}

#[tokio::test]
async fn keeping_the_address_keeps_it_and_the_key_poste_and_forgets_the_others() {
    let env = marie_with_three_addresses().await;
    let id = env.service.find("marie").await.unwrap().id;
    env.service
        .change_own_password_keeping(
            &id,
            secret(PASSWORD),
            secret(NEW_PASSWORD),
            None,
            Some("10.7.7.1"),
            by(),
        )
        .await
        .unwrap();
    assert_eq!(addresses(&env).await, ["10.7.7.1", "10.7.7.3"]);
}

#[tokio::test]
async fn keeping_an_address_that_was_never_retained_learns_nothing() {
    let env = marie_with_three_addresses().await;
    let id = env.service.find("marie").await.unwrap().id;
    env.service
        .change_own_password_keeping(
            &id,
            secret(PASSWORD),
            secret(NEW_PASSWORD),
            None,
            Some("10.9.9.9"),
            by(),
        )
        .await
        .unwrap();
    assert_eq!(addresses(&env).await, ["10.7.7.3"]);
}

#[tokio::test]
async fn without_the_choice_every_address_without_a_key_is_forgotten_as_before() {
    let env = marie_with_three_addresses().await;
    let id = env.service.find("marie").await.unwrap().id;
    env.service
        .change_own_password(&id, secret(PASSWORD), secret(NEW_PASSWORD), None, by())
        .await
        .unwrap();
    assert_eq!(addresses(&env).await, ["10.7.7.3"]);
}

/// Sur le fil : le champ est additif. Absent ou faux, comportement d'avant ; vrai, l'adresse d'où part
/// la requête (la connexion, jamais le corps) est gardée.
#[tokio::test]
async fn on_the_wire_the_field_is_additive_and_keeps_the_address_the_request_comes_from() {
    for (extra, kept) in [
        (json!({}), false),
        (json!({ "keep_address": false }), false),
        (json!({ "keep_address": true }), true),
    ] {
        let env = env().await;
        let api = Api::new(&env);
        env.create("marie", Role::Admin).await;
        let token = api.token_of("marie").await;
        let before = addresses(&env).await;
        assert_eq!(before.len(), 1, "la connexion a retenu l'adresse du client");
        let mut body = json!({ "current": PASSWORD, "password": NEW_PASSWORD });
        for (name, value) in extra.as_object().unwrap() {
            body[name] = value.clone();
        }
        let reply = api
            .put("/me/password")
            .token(&token)
            .json(&body)
            .send()
            .await;
        assert_eq!(reply.status, 200, "{:?}", reply.body);
        let after = addresses(&env).await;
        assert_eq!(after, if kept { before } else { vec![] }, "{body}");
    }
}

#[tokio::test]
async fn removing_a_device_closes_every_session_without_a_link_except_the_current_one() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let account = env.service.find("marie").await.unwrap().id;
    // Deux postes à clé : A (qui reste), B (à retirer).
    let (key_a, key_b) = (DeviceKey::new(), DeviceKey::new());
    let proof = key_a.login_proof(&env, "marie", "10.7.7.1");
    let on_a = env
        .sessions
        .login_with_device(
            "marie",
            secret(PASSWORD),
            &client_at("10.7.7.1"),
            Some(&proof),
        )
        .await
        .unwrap();
    let proof = key_b.login_proof(&env, "marie", "10.7.7.2");
    let on_b = env
        .sessions
        .login_with_device(
            "marie",
            secret(PASSWORD),
            &client_at("10.7.7.2"),
            Some(&proof),
        )
        .await
        .unwrap();
    // Trois sessions sans clé, depuis trois adresses (aucune n'est celle du poste B).
    let mut plain = Vec::new();
    for from in ["10.7.7.5", "10.7.7.6", "10.7.7.7"] {
        plain.push(
            env.sessions
                .login("marie", secret(PASSWORD), &client_at(from))
                .await
                .unwrap(),
        );
    }
    let devices = env.trust.list(&account, &on_a.session_id).await.unwrap();
    let b = devices
        .iter()
        .find(|device| !device.current)
        .unwrap()
        .id
        .clone();
    // La session courante est l'une des trois sans lien : elle reste.
    env.trust
        .remove(&account, &plain[0].session_id, b.as_str(), by())
        .await
        .unwrap();
    let check = |outcome: &hearth_agent::application::sessions::LoginOutcome| {
        let token = outcome.token.encode();
        let sessions = env.sessions.clone();
        async move { sessions.authenticate(&token).await }
    };
    assert!(check(&on_b).await.is_err(), "les sessions du poste retiré");
    for gone in [&plain[1], &plain[2]] {
        let error = check(gone).await.unwrap_err();
        assert!(
            matches!(error, AuthError::Ended(SessionEnd::Revoked)),
            "une session sans lien, d'une autre adresse, ne survit plus : {error:?}"
        );
    }
    check(&plain[0]).await.expect("la session courante reste");
    check(&on_a)
        .await
        .expect("la session d'un autre poste à clé reste");
}

//! Le poste d'une session (HRT-28, tranche F, BR-TRUST-048) : posé à la connexion par mot de passe,
//! jamais réécrit par une preuve de session. Sans cela, la règle « clé du poste courant » du retrait
//! d'un poste se contournait avec le jeton d'un poste et la clé d'un autre.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use hearth_agent::application::trust::RemoveError;
use hearth_agent::domain::accounts::Role;
use hearth_proto::device_proof::Binding;
use support::device::DeviceKey;
use support::{Env, PASSWORD, by, client_at, env, secret};

async fn login_with_key(
    env: &Env,
    key: &DeviceKey,
    from: &str,
) -> hearth_agent::application::sessions::LoginOutcome {
    let proof = key.login_proof(env, "marie", from);
    env.sessions
        .login_with_device("marie", secret(PASSWORD), &client_at(from), Some(&proof))
        .await
        .expect("connexion")
}

fn session_binding(
    outcome: &hearth_agent::application::sessions::LoginOutcome,
) -> Binding<'static> {
    let hash: &'static [u8; 32] = Box::leak(Box::new(*outcome.token.hash().as_bytes()));
    Binding::Session { token_hash: hash }
}

async fn session_device(
    env: &Env,
    session: &hearth_agent::domain::sessions::SessionId,
) -> Option<String> {
    sqlx::query_scalar("SELECT device_id FROM sessions WHERE id = ?")
        .bind(session.as_str())
        .fetch_one(env.db.pool())
        .await
        .unwrap()
}

async fn addresses(env: &Env) -> Vec<String> {
    let mut rows: Vec<String> = sqlx::query_scalar("SELECT address FROM known_addresses")
        .fetch_all(env.db.pool())
        .await
        .unwrap();
    rows.sort();
    rows
}

#[tokio::test]
async fn a_login_with_a_proof_links_the_session_to_the_device_of_the_key() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let key = DeviceKey::new();
    let outcome = login_with_key(&env, &key, "10.7.7.1").await;
    let device: String = sqlx::query_scalar("SELECT id FROM trusted_devices")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert_eq!(
        session_device(&env, &outcome.session_id).await.as_deref(),
        Some(device.as_str())
    );
}

#[tokio::test]
async fn a_session_proof_under_the_key_of_another_device_changes_nothing_and_does_not_unlock_the_removal()
 {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let (key_a, key_b) = (DeviceKey::new(), DeviceKey::new());
    let on_a = login_with_key(&env, &key_a, "10.7.7.1").await;
    let on_b = login_with_key(&env, &key_b, "10.7.7.2").await;
    let linked_before = session_device(&env, &on_a.session_id).await;
    let known_before = addresses(&env).await;
    assert!(linked_before.is_some());
    let account = env.service.find("marie").await.unwrap().id;
    let device_a = linked_before.clone().unwrap();
    let device_b = session_device(&env, &on_b.session_id).await.unwrap();

    // Le jeton de la session du poste A, la clé du poste B : une preuve de session valide.
    let token_a = on_a.token.encode();
    let swapped = key_b.prove(&env.trust, session_binding(&on_a), "marie", "10.9.9.9");
    env.sessions
        .authenticate_proved(&token_a, "10.9.9.9", &swapped)
        .await
        .expect("la session fonctionne, la preuve est ignorée");

    assert_eq!(
        session_device(&env, &on_a.session_id).await,
        linked_before,
        "le poste de la session ne change pas"
    );
    assert_eq!(
        addresses(&env).await,
        known_before,
        "aucune adresse n'est apprise"
    );

    // Le retrait du poste A (courant) ne se refait pas depuis B : retirer A avec la clé de B, ou B avec
    // la clé de B (qui n'est pas le poste de la session), est refusé et rien n'est retiré.
    let session = env.sessions.authenticate(&token_a).await.unwrap();
    let hash = *on_a.token.hash().as_bytes();
    for target in [device_a.as_str(), device_b.as_str()] {
        let proof = key_b.prove(
            &env.trust,
            Binding::DeviceRemoval {
                token_hash: &hash,
                target,
            },
            "marie",
            "10.9.9.9",
        );
        let error = env
            .sessions
            .remove_device(
                &session,
                &token_a,
                target,
                secret(PASSWORD),
                Some(&proof),
                &client_at("10.9.9.9"),
                by(),
            )
            .await
            .unwrap_err();
        assert!(matches!(error, RemoveError::ProofInvalid), "{error:?}");
    }
    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM trusted_devices")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert_eq!(remaining, 2, "rien n'a été retiré");
    let _ = account;
}

#[tokio::test]
async fn a_session_without_a_device_stays_without_after_a_valid_proof_and_retains_the_address() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let key = DeviceKey::new();
    let enrolled = login_with_key(&env, &key, "10.7.7.1").await;
    // Une seconde session, ouverte sans clé.
    let plain = env
        .sessions
        .login("marie", secret(PASSWORD), &client_at("10.7.7.3"))
        .await
        .unwrap();
    assert_eq!(session_device(&env, &plain.session_id).await, None);
    let token = plain.token.encode();
    let proof = key.prove(&env.trust, session_binding(&plain), "marie", "10.8.8.8");
    env.sessions
        .authenticate_proved(&token, "10.8.8.8", &proof)
        .await
        .unwrap();
    assert_eq!(
        session_device(&env, &plain.session_id).await,
        None,
        "une preuve de session ne relie plus rien"
    );
    assert!(
        addresses(&env).await.contains(&"10.8.8.8".to_owned()),
        "l'adresse est retenue (BR-TRUST-007)"
    );

    // Le retrait répond « poste requis » depuis cette session.
    let session = env.sessions.authenticate(&token).await.unwrap();
    let target = session_device(&env, &enrolled.session_id).await.unwrap();
    let hash = *plain.token.hash().as_bytes();
    let removal = key.prove(
        &env.trust,
        Binding::DeviceRemoval {
            token_hash: &hash,
            target: &target,
        },
        "marie",
        "10.8.8.8",
    );
    let error = env
        .sessions
        .remove_device(
            &session,
            &token,
            &target,
            secret(PASSWORD),
            Some(&removal),
            &client_at("10.8.8.8"),
            by(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, RemoveError::DeviceRequired), "{error:?}");
}

#[tokio::test]
async fn the_removal_from_the_linked_device_still_passes() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let (key_a, key_b) = (DeviceKey::new(), DeviceKey::new());
    let on_a = login_with_key(&env, &key_a, "10.7.7.1").await;
    let on_b = login_with_key(&env, &key_b, "10.7.7.2").await;
    let target = session_device(&env, &on_b.session_id).await.unwrap();
    let token = on_a.token.encode();
    let session = env.sessions.authenticate(&token).await.unwrap();
    let hash = *on_a.token.hash().as_bytes();
    let proof = key_a.prove(
        &env.trust,
        Binding::DeviceRemoval {
            token_hash: &hash,
            target: &target,
        },
        "marie",
        "10.7.7.1",
    );
    env.sessions
        .remove_device(
            &session,
            &token,
            &target,
            secret(PASSWORD),
            Some(&proof),
            &client_at("10.7.7.1"),
            by(),
        )
        .await
        .expect("le poste relié à la session retire un autre poste");
    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM trusted_devices")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert_eq!(remaining, 1);
}

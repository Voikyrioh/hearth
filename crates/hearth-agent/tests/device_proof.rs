//! Identité d'appareil côté agent (HRT-22, ADR-0023) : la preuve de possession de la clé, l'inscription
//! d'un poste dans la transaction de la connexion par mot de passe, les adresses retenues, la liste et
//! le retrait des postes, les oublis, le journal. De bout en bout sur une vraie base SQLite temporaire,
//! avec de vraies signatures Ed25519 (`ring`) ; temps contrôlé (horloges de test, aucun `sleep`).
//!
//! **La clé ne change aucune décision d'accès** : les tests de connexion existants tournent avec
//! l'identité d'appareil branchée (`support::env`) et passent sans être modifiés.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use hearth_agent::application::ports::{Clock, DeviceRepo, KnownAddressRepo};
use hearth_agent::application::sessions::{AuthError, LoginError, LoginOutcome};
use hearth_agent::application::trust::RemoveError;
use hearth_agent::domain::accounts::{AccountId, Role};
use hearth_agent::domain::sessions::{RENEWAL_INTERVAL, SessionEnd};
use hearth_agent::domain::trust::DeviceId;
use hearth_agent::infrastructure::sqlite::{SqliteDeviceRepo, SqliteKnownAddressRepo};
use hearth_proto::api::devices::MAX_DEVICES_PER_ACCOUNT;
use hearth_proto::api::sessions::{ChallengePurpose, DeviceProof, DeviceStatus};
use hearth_proto::device_proof::Binding;
use hearth_proto::fingerprint::Fingerprint;
use support::device::DeviceKey;
use support::{CLIENT_ADDR, Env, PASSWORD, SERVER_FINGERPRINT, by, client_at, env, secret};
use time::Duration;

const WRONG: &str = "Wrong-Horse-9999";

/// action, résultat, compte, cible, adresse d'origine, raison.
type AuditRow = (
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

async fn scalar(env: &Env, sql: &'static str) -> i64 {
    sqlx::query_scalar(sql)
        .fetch_one(env.db.pool())
        .await
        .unwrap()
}

async fn login(
    env: &Env,
    username: &str,
    password: &str,
    from: &str,
    proof: Option<&DeviceProof>,
) -> Result<LoginOutcome, LoginError> {
    env.sessions
        .login_with_device(username, secret(password), &client_at(from), proof)
        .await
}

/// Connexion réussie de `marie` depuis `from`, avec la preuve de `key`.
async fn login_with_key(env: &Env, key: &DeviceKey, from: &str) -> LoginOutcome {
    let proof = key.login_proof(env, "marie", from);
    login(env, "marie", PASSWORD, from, Some(&proof))
        .await
        .expect("connexion")
}

async fn marie(env: &Env) -> AccountId {
    env.service.find("marie").await.unwrap().id
}

async fn devices(env: &Env) -> i64 {
    scalar(env, "SELECT COUNT(*) FROM trusted_devices").await
}

fn session_binding(outcome: &LoginOutcome) -> hearth_proto::device_proof::Binding<'static> {
    // Les octets du hachage vivent le temps du test : on les fuit volontairement (un test, 32 octets).
    let hash: &'static [u8; 32] = Box::leak(Box::new(*outcome.token.hash().as_bytes()));
    Binding::Session { token_hash: hash }
}

fn server() -> Fingerprint {
    Fingerprint::from_bytes(SERVER_FINGERPRINT)
}

// ---------------------------------------------------------------------------------------------
// La preuve : ce qu'elle lie, ce qu'elle refuse
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_valid_proof_is_verified_under_the_key_it_carries() {
    let env = env().await;
    let key = DeviceKey::new();
    let proof = key.login_proof(&env, "marie", CLIENT_ADDR);
    let verified = env
        .trust
        .verify(&proof, Binding::Login, "marie", CLIENT_ADDR)
        .expect("preuve valide");
    assert_eq!(verified.key_id, key.key_id());
    assert_eq!(verified.public_key, key.public_key());
}

#[tokio::test]
async fn a_proof_for_one_usage_identifier_server_or_address_is_worth_nothing_for_another() {
    let env = env().await;
    let key = DeviceKey::new();
    let token_hash = [0x42; 32];

    // Usage : une preuve de connexion ne vaut ni pour authentifier un flux ni pour le mode attaque.
    let login_proof = key.login_proof(&env, "marie", CLIENT_ADDR);
    for binding in [
        Binding::Session {
            token_hash: &token_hash,
        },
        Binding::AttackMode {
            token_hash: &token_hash,
            activate: true,
        },
    ] {
        assert!(
            env.trust
                .verify(&login_proof, binding, "marie", CLIENT_ADDR)
                .is_none(),
            "{binding:?}"
        );
    }
    // ... et réciproquement : une preuve de session ne vaut pas pour une connexion.
    let session_proof = key.prove(
        &env.trust,
        Binding::Session {
            token_hash: &token_hash,
        },
        "marie",
        CLIENT_ADDR,
    );
    assert!(
        env.trust
            .verify(&session_proof, Binding::Login, "marie", CLIENT_ADDR)
            .is_none()
    );
    // Jeton : une preuve de session pour un jeton ne vaut pas pour un autre.
    assert!(
        env.trust
            .verify(
                &session_proof,
                Binding::Session {
                    token_hash: &[0x43; 32]
                },
                "marie",
                CLIENT_ADDR
            )
            .is_none()
    );
    // Geste du mode attaque : une preuve d'activation ne vaut pas pour la désactivation.
    let on = key.prove(
        &env.trust,
        Binding::AttackMode {
            token_hash: &token_hash,
            activate: true,
        },
        "marie",
        CLIENT_ADDR,
    );
    assert!(
        env.trust
            .verify(
                &on,
                Binding::AttackMode {
                    token_hash: &token_hash,
                    activate: false
                },
                "marie",
                CLIENT_ADDR
            )
            .is_none()
    );

    // Identifiant : la preuve faite pour « marie » ne vaut pas pour « paul ».
    let for_marie = key.login_proof(&env, "marie", CLIENT_ADDR);
    assert!(
        env.trust
            .verify(&for_marie, Binding::Login, "paul", CLIENT_ADDR)
            .is_none()
    );

    // Serveur : une preuve signée pour l'empreinte d'un autre serveur ne vaut rien ici, même avec
    // un défi authentique.
    let challenge = env
        .trust
        .issue_challenge("marie", ChallengePurpose::Login, CLIENT_ADDR)
        .unwrap()
        .challenge;
    let other_server = Fingerprint::from_bytes([0x11; 32]);
    let foreign = key.sign(&other_server, Binding::Login, "marie", &challenge);
    assert!(
        env.trust
            .verify(&foreign, Binding::Login, "marie", CLIENT_ADDR)
            .is_none()
    );
    // Contrôle : signée pour CE serveur, la même preuve passe.
    let ours = key.sign(&server(), Binding::Login, "marie", &challenge);
    assert!(
        env.trust
            .verify(&ours, Binding::Login, "marie", CLIENT_ADDR)
            .is_some()
    );

    // Adresse : le défi n'est valable que depuis l'adresse qui l'a demandé.
    let asked_from_a = key.login_proof(&env, "marie", "10.0.0.7");
    assert!(
        env.trust
            .verify(&asked_from_a, Binding::Login, "marie", "10.0.0.8")
            .is_none()
    );
}

#[tokio::test]
async fn an_expired_challenge_is_refused_and_one_at_the_limit_is_not() {
    let env = env().await;
    let key = DeviceKey::new();
    let fresh = key.login_proof(&env, "marie", CLIENT_ADDR);
    env.monotonic.advance(Duration::seconds(60));
    assert!(
        env.trust
            .verify(&fresh, Binding::Login, "marie", CLIENT_ADDR)
            .is_some(),
        "à 60 s exactement, le défi vaut encore"
    );
    let late = key.login_proof(&env, "marie", CLIENT_ADDR);
    env.monotonic
        .advance(Duration::seconds(60) + Duration::milliseconds(1));
    assert!(
        env.trust
            .verify(&late, Binding::Login, "marie", CLIENT_ADDR)
            .is_none(),
        "au-delà de 60 s, le défi est expiré"
    );
}

#[tokio::test]
async fn a_replayed_challenge_is_refused_the_proof_serves_once() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let key = DeviceKey::new();
    let proof = key.login_proof(&env, "marie", CLIENT_ADDR);

    // Vérifier une preuve n'écrit rien : seul le service qu'elle rend retient le défi.
    for _ in 0..3 {
        assert!(
            env.trust
                .verify(&proof, Binding::Login, "marie", CLIENT_ADDR)
                .is_some()
        );
    }
    // Un mot de passe faux ne consomme rien : le même défi sert ensuite avec le bon.
    let refused = login(&env, "marie", WRONG, CLIENT_ADDR, Some(&proof))
        .await
        .unwrap_err();
    assert!(matches!(refused, LoginError::InvalidCredentials));
    let served = login(&env, "marie", PASSWORD, CLIENT_ADDR, Some(&proof))
        .await
        .unwrap();
    assert_eq!(served.device, Some(DeviceStatus::Enrolled));
    // La preuve a servi : rejouée avec le bon mot de passe, elle ne vaut plus rien (la connexion,
    // elle, réussit comme sans clé).
    env.clock.advance(Duration::minutes(1));
    let replayed = login(&env, "marie", PASSWORD, CLIENT_ADDR, Some(&proof))
        .await
        .unwrap();
    assert_eq!(replayed.device, None, "rejeu");
    assert!(
        env.trust
            .verify(&proof, Binding::Login, "marie", CLIENT_ADDR)
            .is_none(),
        "un défi consommé ne se vérifie plus"
    );
    assert_eq!(devices(&env).await, 1);
}

/// Une connexion lancée sur sa propre tâche (donc, sur l'exécuteur à plusieurs fils, sur son propre fil
/// quand il y en a de libres) : les contenants sont des `Arc`, rien n'est emprunté à la tâche du test.
fn spawn_login(
    env: &Env,
    from: String,
    proof: DeviceProof,
) -> tokio::task::JoinHandle<Result<LoginOutcome, LoginError>> {
    let sessions = env.sessions.clone();
    tokio::spawn(async move {
        sessions
            .login_with_device("marie", secret(PASSWORD), &client_at(&from), Some(&proof))
            .await
    })
}

/// Deux requêtes simultanées avec le même défi et le bon mot de passe : une seule gagne. Sur un
/// exécuteur à plusieurs fils (HRT-24, suivi de la revue de la PR #25). Le défi est lié à l'adresse et le
/// tour par adresse met deux connexions d'une même adresse l'une derrière l'autre : la garantie tient à
/// `consume` (vérification et insertion sous un même verrou) et à la transaction ; le test prouve le
/// résultat, la course entre adresses est dans le test suivant.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_simultaneous_logins_with_the_same_challenge_and_the_right_password_one_wins() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let key = DeviceKey::new();
    let proof = key.login_proof(&env, "marie", CLIENT_ADDR);
    let (a, b) = tokio::join!(
        spawn_login(&env, CLIENT_ADDR.to_owned(), proof.clone()),
        spawn_login(&env, CLIENT_ADDR.to_owned(), proof.clone()),
    );
    let statuses = [a.unwrap().unwrap().device, b.unwrap().unwrap().device];
    assert_eq!(
        statuses
            .iter()
            .filter(|s| **s == Some(DeviceStatus::Enrolled))
            .count(),
        1,
        "{statuses:?}"
    );
    assert_eq!(
        statuses.iter().filter(|s| s.is_none()).count(),
        1,
        "{statuses:?}"
    );
    assert_eq!(devices(&env).await, 1);
}

/// La même course sur le flux, **pour de vrai** (plusieurs fils, aucune file par adresse : le flux n'a pas
/// de tour) : huit ouvertures simultanées avec la même preuve de session ; la preuve rejouée ensuite ne
/// sert plus (les sessions, elles, fonctionnent toujours).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_simultaneous_stream_proofs_with_the_same_challenge_one_serves() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let key = DeviceKey::new();
    let outcome = login_with_key(&env, &key, "10.7.7.1").await;
    env.clock.advance(Duration::minutes(10));
    let token = outcome.token.encode();
    let proof = key.prove(&env.trust, session_binding(&outcome), "marie", "10.7.7.2");
    let tasks: Vec<_> = (0..8)
        .map(|_| {
            let (sessions, token, proof) = (env.sessions.clone(), token.clone(), proof.clone());
            tokio::spawn(async move {
                sessions
                    .authenticate_proved(&token, "10.7.7.2", &proof)
                    .await
            })
        })
        .collect();
    for task in tasks {
        task.await.unwrap().unwrap();
    }
    let rows: Vec<String> = sqlx::query_scalar("SELECT address FROM known_addresses")
        .fetch_all(env.db.pool())
        .await
        .unwrap();
    assert_eq!(rows, vec!["10.7.7.2".to_owned()]);
    sqlx::query("UPDATE known_addresses SET address = '10.7.7.9'")
        .execute(env.db.pool())
        .await
        .unwrap();
    env.sessions
        .authenticate_proved(&token, "10.7.7.2", &proof)
        .await
        .unwrap();
    let rows: Vec<String> = sqlx::query_scalar("SELECT address FROM known_addresses")
        .fetch_all(env.db.pool())
        .await
        .unwrap();
    assert_eq!(
        rows,
        vec!["10.7.7.9".to_owned()],
        "preuve rejouée : rien n'est appris"
    );
}

#[tokio::test]
async fn a_forged_or_altered_challenge_is_refused() {
    let env = env().await;
    let key = DeviceKey::new();
    let issued = env
        .trust
        .issue_challenge("marie", ChallengePurpose::Login, CLIENT_ADDR)
        .unwrap()
        .challenge;
    let raw = STANDARD.decode(&issued).unwrap();
    assert_eq!(raw.len(), 56);
    // Chaque octet du défi compte : le nonce, l'émission et le code (le client signe ce qu'il a
    // reçu, ici altéré : la signature est valide, le défi ne l'est pas).
    for index in [0, 7, 15, 16, 23, 24, 40, 55] {
        let mut altered = raw.clone();
        altered[index] ^= 0x01;
        let challenge = STANDARD.encode(&altered);
        let proof = key.sign(&server(), Binding::Login, "marie", &challenge);
        assert!(
            env.trust
                .verify(&proof, Binding::Login, "marie", CLIENT_ADDR)
                .is_none(),
            "octet {index}"
        );
    }
    // Un défi inventé de toutes pièces, signé par une clé quelconque.
    let invented = STANDARD.encode([0x5a_u8; 56]);
    let proof = key.sign(&server(), Binding::Login, "marie", &invented);
    assert!(
        env.trust
            .verify(&proof, Binding::Login, "marie", CLIENT_ADDR)
            .is_none()
    );
    // Un défi d'un autre service (autre clé de code) : même forme, code faux.
    let other_env = support::env().await;
    let foreign = other_env
        .trust
        .issue_challenge("marie", ChallengePurpose::Login, CLIENT_ADDR)
        .unwrap()
        .challenge;
    let proof = key.sign(&server(), Binding::Login, "marie", &foreign);
    assert!(
        env.trust
            .verify(&proof, Binding::Login, "marie", CLIENT_ADDR)
            .is_none()
    );
}

#[tokio::test]
async fn a_signature_by_another_key_or_over_other_bytes_is_refused() {
    let env = env().await;
    let key = DeviceKey::new();
    let other = DeviceKey::new();
    let mut proof = key.login_proof(&env, "marie", CLIENT_ADDR);
    // La clé publique d'un autre sous la signature de la première.
    proof.public_key = STANDARD.encode(other.public_key());
    assert!(
        env.trust
            .verify(&proof, Binding::Login, "marie", CLIENT_ADDR)
            .is_none()
    );
    // Une signature altérée d'un seul bit.
    let mut proof = key.login_proof(&env, "marie", CLIENT_ADDR);
    let mut signature = STANDARD.decode(&proof.signature).unwrap();
    signature[10] ^= 0x01;
    proof.signature = STANDARD.encode(signature);
    assert!(
        env.trust
            .verify(&proof, Binding::Login, "marie", CLIENT_ADDR)
            .is_none()
    );
    // Un algorithme inconnu.
    let mut proof = key.login_proof(&env, "marie", CLIENT_ADDR);
    proof.algorithm = "p256".into();
    assert!(
        env.trust
            .verify(&proof, Binding::Login, "marie", CLIENT_ADDR)
            .is_none()
    );
}

#[tokio::test]
async fn a_malformed_proof_never_panics_and_is_refused() {
    let env = env().await;
    let key = DeviceKey::new();
    let good = key.login_proof(&env, "marie", CLIENT_ADDR);
    let huge = "A".repeat(1_000_000);
    let texts = [
        "",
        " ",
        "!!!!",
        "é",
        "====",
        "AAAA\nAAAA",
        huge.as_str(),
        "\u{0}",
        "AAAA AAAA",
    ];
    for text in texts {
        for field in 0..3 {
            let mut proof = good.clone();
            match field {
                0 => proof.public_key = text.to_owned(),
                1 => proof.challenge = text.to_owned(),
                _ => proof.signature = text.to_owned(),
            }
            assert!(
                env.trust
                    .verify(&proof, Binding::Login, "marie", CLIENT_ADDR)
                    .is_none(),
                "champ {field} : {:?}",
                text.chars().take(6).collect::<String>()
            );
        }
    }
    // Longueurs voisines de la bonne.
    for len in [0, 1, 31, 33, 55, 57, 63, 65] {
        for field in 0..3 {
            let mut proof = good.clone();
            let bytes = STANDARD.encode(vec![9_u8; len]);
            match field {
                0 => proof.public_key = bytes,
                1 => proof.challenge = bytes,
                _ => proof.signature = bytes,
            }
            assert!(
                env.trust
                    .verify(&proof, Binding::Login, "marie", CLIENT_ADDR)
                    .is_none(),
                "{len} octets, champ {field}"
            );
        }
    }
    // Un identifiant démesuré ou vide dans la vérification ne fait pas paniquer non plus.
    for name in ["", &"é".repeat(100_000)] {
        assert!(
            env.trust
                .verify(&good, Binding::Login, name, CLIENT_ADDR)
                .is_none()
        );
    }
    // Contrôle : la preuve intacte passe (aucune des tentatives ne l'a consommée).
    assert!(
        env.trust
            .verify(&good, Binding::Login, "marie", CLIENT_ADDR)
            .is_some()
    );
}

// ---------------------------------------------------------------------------------------------
// Absence d'oracle
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn the_challenge_reads_nothing_in_the_database_and_is_the_same_for_any_identifier() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    // La base est fermée : un défi qui lirait quoi que ce soit échouerait.
    env.db.pool().close().await;
    let mut shapes = Vec::new();
    for username in ["marie", "fantome", "", "MARIE", &"x".repeat(5_000)] {
        for purpose in [
            ChallengePurpose::Login,
            ChallengePurpose::Session,
            ChallengePurpose::AttackMode,
        ] {
            let response = env
                .trust
                .issue_challenge(username, purpose, CLIENT_ADDR)
                .expect("défi sans lecture en base");
            let raw = STANDARD.decode(&response.challenge).unwrap();
            shapes.push((raw.len(), response.expires_in_s, response.challenge.len()));
        }
    }
    assert!(
        shapes.iter().all(|shape| *shape == (56, 60, 76)),
        "{shapes:?}"
    );
}

/// Ce que le chemin de connexion fait de coûteux, relevé avant et après.
struct Work {
    hashes: u64,
    decoys: u64,
    signatures: u64,
}

fn work(env: &Env) -> Work {
    Work {
        hashes: env.hasher.verifications(),
        decoys: env.hasher.against_decoy(),
        signatures: env.verifier.calls(),
    }
}

#[tokio::test]
async fn a_wrong_password_looks_the_same_with_or_without_a_proof_for_an_existing_or_missing_account()
 {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let key = DeviceKey::new();

    let mut observed = Vec::new();
    for (username, with_proof) in [
        ("marie", true),
        ("marie", false),
        ("fantome", true),
        ("fantome", false),
    ] {
        let from = "10.7.7.7";
        let proof = with_proof.then(|| key.login_proof(&env, username, from));
        let before = work(&env);
        let error = login(&env, username, WRONG, from, proof.as_ref())
            .await
            .expect_err("mauvais mot de passe");
        let after = work(&env);
        observed.push((
            username,
            with_proof,
            format!("{error:?}"),
            after.hashes - before.hashes,
            after.decoys - before.decoys,
            after.signatures - before.signatures,
        ));
        env.clock.advance(Duration::minutes(5));
    }
    for (username, with_proof, error, hashes, decoys, signatures) in &observed {
        assert_eq!(error, "InvalidCredentials", "{username}");
        assert_eq!(*hashes, 1, "un seul calcul Argon2 : {username}");
        assert_eq!(
            *decoys,
            u64::from(*username == "fantome"),
            "le haché factice sert à l'identifiant inconnu seulement"
        );
        // La signature est vérifiée sous la clé fournie, que l'identifiant existe ou non.
        assert_eq!(*signatures, u64::from(*with_proof), "{username}");
    }
    assert_eq!(
        devices(&env).await,
        0,
        "un mot de passe faux n'inscrit rien"
    );
    assert_eq!(
        scalar(
            &env,
            "SELECT COUNT(*) FROM audit_events WHERE action LIKE 'device.%'"
        )
        .await,
        0
    );
}

#[tokio::test]
async fn a_right_password_with_a_false_or_missing_proof_connects_as_if_there_were_no_key() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let key = DeviceKey::new();

    // Aucune preuve.
    let outcome = login(&env, "marie", PASSWORD, "10.7.7.1", None)
        .await
        .unwrap();
    assert_eq!(outcome.device, None);
    env.sessions
        .authenticate(&outcome.token.encode())
        .await
        .expect("la session fonctionne");

    // Preuves fausses : signature d'un autre message, défi expiré, mauvais usage (le rejeu : voir plus bas).
    let expired = key.login_proof(&env, "marie", "10.7.7.4");
    env.monotonic.advance(Duration::seconds(61));
    let wrong_usage = key.prove(
        &env.trust,
        Binding::Session {
            token_hash: &[1; 32],
        },
        "marie",
        "10.7.7.5",
    );
    let wrong_signature = {
        let mut proof = key.login_proof(&env, "marie", "10.7.7.2");
        proof.signature = STANDARD.encode([1_u8; 64]);
        proof
    };
    for (proof, from) in [
        (&wrong_signature, "10.7.7.2"),
        (&expired, "10.7.7.4"),
        (&wrong_usage, "10.7.7.5"),
    ] {
        let outcome = login(&env, "marie", PASSWORD, from, Some(proof))
            .await
            .expect("la connexion ne dépend pas de la clé");
        assert_eq!(outcome.device, None, "{from}");
    }
    assert_eq!(devices(&env).await, 0);
    assert_eq!(
        scalar(
            &env,
            "SELECT COUNT(*) FROM audit_events WHERE action LIKE 'device.%'"
        )
        .await,
        0
    );
}

// ---------------------------------------------------------------------------------------------
// Inscription : seulement par mot de passe, dans la transaction de la connexion
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_valid_proof_with_the_right_password_enrolls_the_device_and_learns_its_address() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let key = DeviceKey::new();
    let outcome = login_with_key(&env, &key, "10.7.7.7").await;
    assert_eq!(outcome.device, Some(DeviceStatus::Enrolled));

    let row: (String, String, String, String, Vec<u8>) = sqlx::query_as(
        "SELECT account_id, key_id, name, last_addr, public_key FROM trusted_devices",
    )
    .fetch_one(env.db.pool())
    .await
    .unwrap();
    assert_eq!(row.0, marie(&env).await.as_str());
    assert_eq!(row.1, key.key_id());
    assert_eq!(row.2, "poste/1.0", "le nom annoncé par le poste");
    assert_eq!(row.3, "10.7.7.7");
    assert_eq!(
        row.4,
        key.public_key().to_vec(),
        "la clé publique seulement"
    );

    // L'adresse de la connexion est retenue et liée au poste ; la session est rattachée au poste.
    let (address, device): (String, Option<String>) =
        sqlx::query_as("SELECT address, device_id FROM known_addresses")
            .fetch_one(env.db.pool())
            .await
            .unwrap();
    assert_eq!(address, "10.7.7.7");
    let device_id: String = sqlx::query_scalar("SELECT id FROM trusted_devices")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert_eq!(device.as_deref(), Some(device_id.as_str()));
    let session_device: Option<String> = sqlx::query_scalar("SELECT device_id FROM sessions")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert_eq!(session_device.as_deref(), Some(device_id.as_str()));
}

#[tokio::test]
async fn the_same_key_presented_twice_is_one_device_proven_the_second_time() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let key = DeviceKey::new();
    assert_eq!(
        login_with_key(&env, &key, "10.7.7.7").await.device,
        Some(DeviceStatus::Enrolled)
    );
    env.clock.advance(Duration::minutes(10));
    assert_eq!(
        login_with_key(&env, &key, "10.7.7.7").await.device,
        Some(DeviceStatus::Proven)
    );
    assert_eq!(devices(&env).await, 1);
    let list = SqliteDeviceRepo::new(env.db.pool().clone())
        .of_account(&marie(&env).await)
        .await
        .unwrap();
    assert_eq!(list.len(), 1);
    assert!(list[0].last_proved_at > list[0].created_at, "preuve datée");
}

#[tokio::test]
async fn a_session_alone_never_enrolls_a_device_nor_retains_an_address() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    // Une session ouverte SANS clé.
    let outcome = login(&env, "marie", PASSWORD, "10.7.7.7", None)
        .await
        .unwrap();
    let token = outcome.token.encode();
    let key = DeviceKey::new();

    // Présenter la session, seule ou avec une preuve de clé valide mais inscrite nulle part :
    // aucun poste n'apparaît, aucune adresse n'est apprise.
    let proof = key.prove(&env.trust, session_binding(&outcome), "marie", "10.8.8.8");
    env.clock.advance(RENEWAL_INTERVAL + Duration::seconds(1));
    env.sessions
        .authenticate_at(&token, "10.8.8.8")
        .await
        .unwrap();
    env.sessions
        .authenticate_proved(&token, "10.8.8.8", &proof)
        .await
        .expect("la session fonctionne, la preuve ne fait rien");
    assert_eq!(
        devices(&env).await,
        0,
        "jamais d'inscription par une session"
    );
    let known = SqliteKnownAddressRepo::new(env.db.pool().clone())
        .of_username("marie")
        .await
        .unwrap();
    let addresses: Vec<_> = known.iter().map(|k| k.address.as_str()).collect();
    assert_eq!(addresses, vec!["10.7.7.7"], "{addresses:?}");
}

#[tokio::test]
async fn the_ninth_device_is_refused_without_evicting_another_and_the_connection_still_succeeds() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let mut first = None;
    for n in 0..MAX_DEVICES_PER_ACCOUNT {
        let key = DeviceKey::new();
        let from = format!("10.7.7.{}", n + 1);
        assert_eq!(
            login_with_key(&env, &key, &from).await.device,
            Some(DeviceStatus::Enrolled),
            "poste {n}"
        );
        first.get_or_insert(key);
        env.clock.advance(Duration::minutes(1));
    }
    assert_eq!(devices(&env).await, 8);

    let ninth = DeviceKey::new();
    let outcome = login_with_key(&env, &ninth, "10.7.7.99").await;
    assert_eq!(outcome.device, Some(DeviceStatus::Limit));
    assert_eq!(devices(&env).await, 8, "pas de 9e poste, pas d'éviction");
    let keys: Vec<String> = sqlx::query_scalar("SELECT key_id FROM trusted_devices")
        .fetch_all(env.db.pool())
        .await
        .unwrap();
    assert!(
        keys.contains(&first.unwrap().key_id()),
        "le plus ancien est resté"
    );
    assert!(!keys.contains(&ninth.key_id()));
    env.sessions
        .authenticate(&outcome.token.encode())
        .await
        .expect("la connexion a réussi");

    // Un poste retiré laisse la place : le 9e peut alors s'inscrire.
    let one: String =
        sqlx::query_scalar("SELECT id FROM trusted_devices ORDER BY created_at LIMIT 1")
            .fetch_one(env.db.pool())
            .await
            .unwrap();
    let session = login(&env, "marie", PASSWORD, "10.7.7.100", None)
        .await
        .unwrap();
    env.trust
        .remove(&marie(&env).await, &session.session_id, &one, by())
        .await
        .unwrap();
    assert_eq!(
        login_with_key(&env, &ninth, "10.7.7.99").await.device,
        Some(DeviceStatus::Enrolled)
    );
}

#[tokio::test]
async fn a_key_enrolled_for_one_account_is_never_enrolled_for_another() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    env.create("paul", Role::Admin).await;
    let key = DeviceKey::new();
    assert_eq!(
        login_with_key(&env, &key, "10.7.7.7").await.device,
        Some(DeviceStatus::Enrolled)
    );

    let proof = key.login_proof(&env, "paul", "10.7.7.8");
    let outcome = login(&env, "paul", PASSWORD, "10.7.7.8", Some(&proof))
        .await
        .expect("paul se connecte, sa clé n'est pas prise en compte");
    assert_eq!(outcome.device, None);
    let owners: Vec<String> = sqlx::query_scalar("SELECT account_id FROM trusted_devices")
        .fetch_all(env.db.pool())
        .await
        .unwrap();
    assert_eq!(owners, vec![marie(&env).await.as_str().to_owned()]);
    // La clé de marie ne se retire pas depuis le compte de paul.
    let device: String = sqlx::query_scalar("SELECT id FROM trusted_devices")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    let paul = env.service.find("paul").await.unwrap().id;
    let error = env
        .trust
        .remove(&paul, &outcome.session_id, &device, by())
        .await
        .unwrap_err();
    assert!(matches!(error, RemoveError::NotFound), "{error:?}");
    assert_eq!(devices(&env).await, 1);
}

#[tokio::test]
async fn the_enrolment_is_frozen_while_the_attack_mode_is_active() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let key = DeviceKey::new();
    // Le poste est déjà connu par son adresse : en mode attaque (HRT-25), un poste qui n'a aucun critère
    // est bloqué avant même l'inscription, et celui-ci n'a que son adresse (un essai).
    login(&env, "marie", PASSWORD, "10.7.7.7", None)
        .await
        .expect("connexion avant l'activation");
    sqlx::query("UPDATE attack_mode SET active = 1, activation_id = 'A1' WHERE id = 1")
        .execute(env.db.pool())
        .await
        .unwrap();
    let outcome = login_with_key(&env, &key, "10.7.7.7").await;
    assert_eq!(outcome.device, Some(DeviceStatus::Deferred));
    assert_eq!(devices(&env).await, 0);
    env.sessions
        .authenticate_at(&outcome.token.encode(), "10.7.7.7")
        .await
        .expect("le refus n'est jamais dû à la clé : la connexion a réussi");
    sqlx::query("UPDATE attack_mode SET active = 0 WHERE id = 1")
        .execute(env.db.pool())
        .await
        .unwrap();
    assert_eq!(
        login_with_key(&env, &key, "10.7.7.7").await.device,
        Some(DeviceStatus::Enrolled)
    );
}

#[tokio::test]
async fn an_enrolment_is_atomic_with_the_login_nothing_is_left_when_the_login_is_not_granted() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let key = DeviceKey::new();
    let proof = key.login_proof(&env, "marie", "10.7.7.7");
    let error = login(&env, "marie", WRONG, "10.7.7.7", Some(&proof))
        .await
        .unwrap_err();
    assert!(matches!(error, LoginError::InvalidCredentials));
    assert_eq!(devices(&env).await, 0);
    assert_eq!(scalar(&env, "SELECT COUNT(*) FROM sessions").await, 0);
    assert_eq!(
        scalar(&env, "SELECT COUNT(*) FROM known_addresses").await,
        0
    );
}

// ---------------------------------------------------------------------------------------------
// Liste et retrait
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn the_list_shows_only_the_own_devices_and_marks_the_current_one() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    env.create("paul", Role::ReadOnly).await;
    let (a, b) = (DeviceKey::new(), DeviceKey::new());
    let on_a = login_with_key(&env, &a, "10.7.7.1").await;
    env.clock.advance(Duration::minutes(1));
    let on_b = login_with_key(&env, &b, "10.7.7.2").await;
    let proof = DeviceKey::new().login_proof(&env, "paul", "10.7.7.3");
    let paul = login(&env, "paul", PASSWORD, "10.7.7.3", Some(&proof))
        .await
        .unwrap();

    let account = marie(&env).await;
    let from_a = env.trust.list(&account, &on_a.session_id).await.unwrap();
    assert_eq!(from_a.len(), 2, "les postes de marie seulement");
    assert_eq!(from_a.iter().filter(|d| d.current).count(), 1);
    assert!(
        from_a[0].current && !from_a[1].current,
        "le poste de la session est le courant"
    );
    let from_b = env.trust.list(&account, &on_b.session_id).await.unwrap();
    assert!(!from_b[0].current && from_b[1].current);
    // Une session sans poste (ancien client) : aucun poste n'est « courant ».
    let none = login(&env, "marie", PASSWORD, "10.7.7.9", None)
        .await
        .unwrap();
    assert!(
        env.trust
            .list(&account, &none.session_id)
            .await
            .unwrap()
            .iter()
            .all(|d| !d.current)
    );
    let own = env
        .trust
        .list(
            &env.service.find("paul").await.unwrap().id,
            &paul.session_id,
        )
        .await
        .unwrap();
    assert_eq!(own.len(), 1);
}

#[tokio::test]
async fn the_current_device_cannot_be_removed_from_itself_but_another_can() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let (a, b) = (DeviceKey::new(), DeviceKey::new());
    let on_a = login_with_key(&env, &a, "10.7.7.1").await;
    login_with_key(&env, &b, "10.7.7.2").await;
    let account = marie(&env).await;
    let list = env.trust.list(&account, &on_a.session_id).await.unwrap();
    let current = list.iter().find(|d| d.current).unwrap().id.clone();
    let other = list.iter().find(|d| !d.current).unwrap().id.clone();

    let error = env
        .trust
        .remove(&account, &on_a.session_id, current.as_str(), by())
        .await
        .unwrap_err();
    assert!(matches!(error, RemoveError::IsCurrent), "{error:?}");
    assert_eq!(devices(&env).await, 2, "rien n'a été retiré");
    env.trust
        .remove(&account, &on_a.session_id, other.as_str(), by())
        .await
        .expect("un autre poste se retire");
    assert_eq!(devices(&env).await, 1);
    for absurd in ["", "inconnu", &"x".repeat(1_000)] {
        let error = env
            .trust
            .remove(&account, &on_a.session_id, absurd, by())
            .await
            .unwrap_err();
        assert!(matches!(error, RemoveError::NotFound));
    }
}

#[tokio::test]
async fn removing_a_device_closes_its_sessions_forgets_its_address_and_is_journaled() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let (stolen, mine) = (DeviceKey::new(), DeviceKey::new());
    let on_stolen = login_with_key(&env, &stolen, "10.7.7.1").await;
    let on_mine = login_with_key(&env, &mine, "10.7.7.2").await;
    let account = marie(&env).await;
    let stolen_id = env
        .trust
        .list(&account, &on_stolen.session_id)
        .await
        .unwrap()
        .into_iter()
        .find(|d| d.current)
        .unwrap()
        .id;

    env.trust
        .remove(&account, &on_mine.session_id, stolen_id.as_str(), by())
        .await
        .unwrap();

    // La session du poste retiré est fermée : « accès révoqué », pas « expirée ».
    let error = env
        .sessions
        .authenticate(&on_stolen.token.encode())
        .await
        .unwrap_err();
    assert!(
        matches!(error, AuthError::Ended(SessionEnd::Revoked)),
        "{error:?}"
    );
    env.sessions
        .authenticate(&on_mine.token.encode())
        .await
        .expect("l'autre poste garde sa session");
    // Son adresse retenue est partie avec lui ; celle de l'autre poste reste.
    let addresses: Vec<String> = sqlx::query_scalar("SELECT address FROM known_addresses")
        .fetch_all(env.db.pool())
        .await
        .unwrap();
    assert_eq!(addresses, vec!["10.7.7.2".to_owned()]);
    // Le journal : une entrée device.remove réussie, du compte, sans champ libre.
    let entry: (String, String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT action, outcome, account, target FROM audit_events WHERE action = 'device.remove'",
    )
    .fetch_one(env.db.pool())
    .await
    .unwrap();
    assert_eq!(entry.0, "device.remove");
    assert_eq!(entry.1, "ok");
    assert_eq!(entry.2.as_deref(), Some("root"), "l'appelant");
    assert_eq!(entry.3.as_deref(), Some("poste poste/1.0"));
}

// ---------------------------------------------------------------------------------------------
// Adresses retenues
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_device_has_one_address_at_a_time_the_old_one_is_forgotten() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let key = DeviceKey::new();
    login_with_key(&env, &key, "10.7.7.1").await;
    env.clock.advance(Duration::minutes(1));
    login_with_key(&env, &key, "10.7.7.2").await;
    let rows: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT address, device_id FROM known_addresses")
            .fetch_all(env.db.pool())
            .await
            .unwrap();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].0, "10.7.7.2");
    assert!(rows[0].1.is_some());
    let last_addr: String = sqlx::query_scalar("SELECT last_addr FROM trusted_devices")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert_eq!(last_addr, "10.7.7.2");
}

#[tokio::test]
async fn using_a_session_from_a_retained_address_refreshes_it_every_five_minutes_and_learns_nothing()
 {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let outcome = login(&env, "marie", PASSWORD, "10.7.7.7", None)
        .await
        .unwrap();
    let token = outcome.token.encode();
    let known = SqliteKnownAddressRepo::new(env.db.pool().clone());
    let last_used = |env: &Env| {
        let pool = env.db.pool().clone();
        async move {
            sqlx::query_scalar::<_, Option<String>>("SELECT last_used_at FROM known_addresses")
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };
    assert_eq!(
        last_used(&env).await,
        None,
        "jamais utilisée depuis la connexion"
    );

    // Moins de 5 minutes : la session n'est pas renouvelée, rien n'est écrit.
    env.clock.advance(Duration::minutes(1));
    env.sessions
        .authenticate_at(&token, "10.7.7.7")
        .await
        .unwrap();
    assert_eq!(last_used(&env).await, None);

    // 20 jours plus tard (le poste sert tous les jours) : l'usage repousse la durée de l'adresse.
    env.clock.advance(Duration::days(20));
    env.sessions
        .authenticate_at(&token, "10.7.7.7")
        .await
        .unwrap();
    assert!(last_used(&env).await.is_some());
    // 15 jours encore : 35 jours après la connexion, mais 15 après le dernier usage. La purge
    // compte depuis le dernier usage, pas depuis la connexion.
    env.clock.advance(Duration::days(15));
    assert_eq!(env.maintenance.purge().await.unwrap().known_addresses, 0);
    let now = env.clock.now();
    let list = known.of_username("marie").await.unwrap();
    assert!(
        hearth_agent::domain::known_address::is_known(&list, "10.7.7.7", now),
        "l'usage a repoussé la durée de l'adresse"
    );

    // Une adresse non retenue n'est jamais apprise par l'usage d'une session.
    env.sessions
        .authenticate_at(&token, "10.9.9.9")
        .await
        .unwrap();
    let rows = scalar(&env, "SELECT COUNT(*) FROM known_addresses").await;
    assert_eq!(rows, 1);
    let list = known.of_username("marie").await.unwrap();
    assert!(!hearth_agent::domain::known_address::is_known(
        &list, "10.9.9.9", now
    ));
    // Sans nouvel usage, 30 jours après le dernier : oubliée.
    env.clock.advance(Duration::days(16));
    assert_eq!(env.maintenance.purge().await.unwrap().known_addresses, 1);
}

#[tokio::test]
async fn a_session_with_a_valid_key_proof_makes_the_new_address_retained_and_moves_the_device() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let key = DeviceKey::new();
    let outcome = login_with_key(&env, &key, "10.7.7.1").await;
    let token = outcome.token.encode();
    env.clock.advance(Duration::minutes(10));

    // Le poste change d'adresse et prouve sa clé à l'ouverture du flux : session + clé.
    let proof = key.prove(&env.trust, session_binding(&outcome), "marie", "10.7.7.2");
    env.sessions
        .authenticate_proved(&token, "10.7.7.2", &proof)
        .await
        .unwrap();
    let rows: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT address, device_id FROM known_addresses")
            .fetch_all(env.db.pool())
            .await
            .unwrap();
    assert_eq!(rows.len(), 1, "le poste n'a qu'une adresse : {rows:?}");
    assert_eq!(rows[0].0, "10.7.7.2");
    assert!(rows[0].1.is_some(), "l'adresse reste liée au poste");
    let last_addr: String = sqlx::query_scalar("SELECT last_addr FROM trusted_devices")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert_eq!(last_addr, "10.7.7.2");

    // Une preuve faite pour un AUTRE jeton ne fait rien retenir.
    let other = login(&env, "marie", PASSWORD, "10.7.7.3", None)
        .await
        .unwrap();
    let for_other = key.prove(&env.trust, session_binding(&other), "marie", "10.7.7.4");
    env.sessions
        .authenticate_proved(&token, "10.7.7.4", &for_other)
        .await
        .expect("la session fonctionne, la preuve est ignorée");
    let addresses: Vec<String> = sqlx::query_scalar("SELECT address FROM known_addresses")
        .fetch_all(env.db.pool())
        .await
        .unwrap();
    assert!(!addresses.contains(&"10.7.7.4".to_owned()), "{addresses:?}");
}

// ---------------------------------------------------------------------------------------------
// Oublis
// ---------------------------------------------------------------------------------------------

/// Un poste à clé (adresse liée) et une adresse apprise sans clé, pour `marie`.
async fn one_device_and_one_plain_address(env: &Env) -> (AccountId, LoginOutcome) {
    env.create("root2", Role::Admin).await;
    env.create("marie", Role::Admin).await;
    let key = DeviceKey::new();
    let outcome = login_with_key(env, &key, "10.7.7.1").await;
    login(env, "marie", PASSWORD, "10.7.7.2", None)
        .await
        .unwrap();
    assert_eq!(devices(env).await, 1);
    assert_eq!(scalar(env, "SELECT COUNT(*) FROM known_addresses").await, 2);
    (marie(env).await, outcome)
}

#[tokio::test]
async fn changing_your_own_password_keeps_the_devices_and_forgets_the_addresses_without_a_key() {
    let env = env().await;
    let (account, outcome) = one_device_and_one_plain_address(&env).await;
    env.service
        .change_own_password(
            &account,
            secret(PASSWORD),
            secret("Encore-Un-Autre-88"),
            Some(outcome.session_id),
            by(),
        )
        .await
        .unwrap();
    assert_eq!(devices(&env).await, 1, "BR-TRUST-023 : la clé survit");
    let addresses: Vec<String> = sqlx::query_scalar("SELECT address FROM known_addresses")
        .fetch_all(env.db.pool())
        .await
        .unwrap();
    assert_eq!(
        addresses,
        vec!["10.7.7.1".to_owned()],
        "l'adresse du poste à clé reste, l'autre est oubliée"
    );
}

#[tokio::test]
async fn a_password_change_by_an_administrator_forgets_every_device_and_address() {
    let env = env().await;
    let (account, _) = one_device_and_one_plain_address(&env).await;
    env.service
        .set_password(&account, secret("Nouveau-Mot-De-Passe-77"), by())
        .await
        .unwrap();
    assert_eq!(devices(&env).await, 0, "BR-TRUST-024");
    assert_eq!(
        scalar(&env, "SELECT COUNT(*) FROM known_addresses").await,
        0
    );
    // La même clé est réinscrite à la connexion suivante par mot de passe.
}

#[tokio::test]
async fn closing_the_sessions_forgets_the_devices_and_the_addresses_of_the_account() {
    let env = env().await;
    let (account, _) = one_device_and_one_plain_address(&env).await;
    env.service.revoke_sessions(&account, by()).await.unwrap();
    assert_eq!(devices(&env).await, 0);
    assert_eq!(
        scalar(&env, "SELECT COUNT(*) FROM known_addresses").await,
        0
    );
}

#[tokio::test]
async fn deleting_the_account_erases_its_devices_and_addresses_and_only_its_own() {
    let env = env().await;
    let (account, _) = one_device_and_one_plain_address(&env).await;
    let other = env.create("paul", Role::ReadOnly).await;
    env.insert_device(
        &other.id,
        "01JDEVICEOFPAUL0000000000",
        &"cd".repeat(16),
        "poste-de-paul",
    )
    .await;
    env.service
        .delete(&account, None, None, by())
        .await
        .unwrap();
    assert_eq!(devices(&env).await, 1, "celui de paul reste");
    let owner: String = sqlx::query_scalar("SELECT account_id FROM trusted_devices")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert_eq!(owner, other.id.as_str());
    assert_eq!(
        scalar(&env, "SELECT COUNT(*) FROM known_addresses").await,
        0
    );
}

#[tokio::test]
async fn the_purge_forgets_a_device_after_ninety_days_without_proof_and_detaches_its_session() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let key = DeviceKey::new();
    let outcome = login_with_key(&env, &key, "10.7.7.1").await;
    // La session reste ouverte (elle est renouvelée) pendant que le poste ne prouve plus rien.
    for _ in 0..3 {
        env.clock.advance(Duration::days(25));
        env.sessions
            .authenticate_at(&outcome.token.encode(), "10.7.7.1")
            .await
            .unwrap();
    }
    env.clock.advance(Duration::days(14));
    let report = env.maintenance.purge().await.unwrap();
    assert_eq!(report.devices, 0, "89 jours : le poste est gardé");
    assert_eq!(devices(&env).await, 1);

    env.clock.advance(Duration::days(2));
    let report = env.maintenance.purge().await.unwrap();
    assert_eq!(report.devices, 1, "91 jours sans preuve : oublié");
    assert_eq!(devices(&env).await, 0);
    // Sa session, elle, n'a pas été fermée (elle est seulement détachée du poste).
    let detached: Option<String> = sqlx::query_scalar("SELECT device_id FROM sessions")
        .fetch_optional(env.db.pool())
        .await
        .unwrap()
        .flatten();
    assert_eq!(detached, None);
}

// ---------------------------------------------------------------------------------------------
// Journal
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn the_journal_of_the_devices_has_no_key_no_secret_and_says_nothing_of_missing_identifiers() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let key = DeviceKey::new();
    let proof = key.login_proof(&env, "marie", "10.7.7.7");
    let public_key = proof.public_key.clone();
    let challenge = proof.challenge.clone();
    let signature = proof.signature.clone();
    // Un identifiant inexistant avec une preuve valide, puis la vraie connexion.
    let ghost = key.login_proof(&env, "fantome", "10.7.7.8");
    login(&env, "fantome", PASSWORD, "10.7.7.8", Some(&ghost))
        .await
        .unwrap_err();
    login(&env, "marie", PASSWORD, "10.7.7.7", Some(&proof))
        .await
        .unwrap();

    let entries: Vec<AuditRow> =
        sqlx::query_as(
            "SELECT action, outcome, account, target, origin_addr, reason FROM audit_events ORDER BY id",
        )
        .fetch_all(env.db.pool())
        .await
        .unwrap();
    let device_entries: Vec<_> = entries
        .iter()
        .filter(|entry| entry.0.starts_with("device."))
        .collect();
    assert_eq!(device_entries.len(), 1, "{entries:?}");
    let enroll = device_entries[0];
    assert_eq!(enroll.0, "device.enroll");
    assert_eq!(enroll.1, "ok");
    assert_eq!(enroll.2.as_deref(), Some("marie"));
    assert_eq!(enroll.3, None, "aucune cible, aucun champ libre");
    assert_eq!(enroll.4.as_deref(), Some("10.7.7.7"));
    assert_eq!(enroll.5, None);
    // Rien du matériel de la preuve, rien de l'identifiant inexistant, nulle part au journal.
    let everything = format!("{entries:?}");
    for secret in [
        public_key.as_str(),
        challenge.as_str(),
        signature.as_str(),
        key.key_id().as_str(),
        "fantome",
        PASSWORD,
    ] {
        assert!(!everything.contains(secret), "« {secret} » dans le journal");
    }
    // Le refus de l'identifiant inexistant est celui d'un mot de passe faux, sans compte.
    let refused: Vec<_> = entries.iter().filter(|e| e.1 == "denied").collect();
    assert_eq!(refused.len(), 1);
    assert_eq!(
        refused[0].2, None,
        "aucun compte nommé pour un identifiant inconnu"
    );
}

// ---------------------------------------------------------------------------------------------
// Sans identité d'appareil : le service se comporte comme avant
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn without_the_device_identity_a_proof_is_ignored_and_nothing_is_written() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let key = DeviceKey::new();
    let proof = key.login_proof(&env, "marie", "10.7.7.7");
    let plain = support::plain_sessions(&env);
    assert!(plain.trust().is_none());
    let outcome = plain
        .login_with_device(
            "marie",
            secret(PASSWORD),
            &client_at("10.7.7.7"),
            Some(&proof),
        )
        .await
        .unwrap();
    assert_eq!(outcome.device, None);
    assert_eq!(devices(&env).await, 0);
}

#[tokio::test]
async fn a_device_identifier_that_is_not_ours_cannot_be_removed() {
    // Les identifiants de postes sont des textes quelconques : jamais interprétés (pas de SQL
    // construit à la main, pas de chemin).
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let outcome = login(&env, "marie", PASSWORD, "10.7.7.7", None)
        .await
        .unwrap();
    for id in ["' OR '1'='1", "%", "../..", "01J\u{0}X", "*"] {
        let error = env
            .trust
            .remove(&marie(&env).await, &outcome.session_id, id, by())
            .await
            .unwrap_err();
        assert!(matches!(error, RemoveError::NotFound), "{id:?}");
    }
    let _ = DeviceId::new("x");
}

// ---------------------------------------------------------------------------------------------
// Revue Stephen r1 : bloquants 1 et 2
// ---------------------------------------------------------------------------------------------

use hearth_proto::device_proof::{CHALLENGE_LEN, signing_bytes};

/// Bloquant 1 : un appareil sans compte, sans mot de passe et sans clé inscrite signe avec des clés
/// jetables. Aucune de ces tentatives ne doit retenir quoi que ce soit : la preuve d'un poste
/// inscrit vaut toujours.
#[tokio::test]
async fn throwaway_keys_never_mute_the_proof_of_an_enrolled_device() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let key = DeviceKey::new();
    assert_eq!(
        login_with_key(&env, &key, "10.7.7.7").await.device,
        Some(DeviceStatus::Enrolled)
    );
    env.clock.advance(Duration::minutes(1));

    // Largement plus de 4 096 preuves valides sous des clés jetables, par la vérification seule,
    // par des connexions au mauvais mot de passe, et par un flux de session valide.
    let session = login(&env, "marie", PASSWORD, "10.7.7.8", None)
        .await
        .unwrap();
    for n in 0..4_600 {
        let throwaway = DeviceKey::new();
        // Les 4 200 premières par la vérification seule (de quoi remplir 4 096 places), le reste
        // mêlé aux deux autres chemins.
        match if n < 4_200 { 0 } else { n % 3 } {
            0 => {
                let proof = throwaway.login_proof(&env, "marie", "10.7.7.9");
                let _ = env
                    .trust
                    .verify(&proof, Binding::Login, "marie", "10.7.7.9");
            }
            1 => {
                let proof = throwaway.login_proof(&env, "marie", "10.7.7.9");
                let _ = login(&env, "marie", WRONG, "10.7.7.9", Some(&proof)).await;
                env.clock.advance(Duration::minutes(10));
            }
            _ => {
                let proof =
                    throwaway.prove(&env.trust, session_binding(&session), "marie", "10.7.7.8");
                let _ = env
                    .sessions
                    .authenticate_proved(&session.token.encode(), "10.7.7.8", &proof)
                    .await;
            }
        }
    }

    // La preuve du poste inscrit, par la connexion comme par le flux, compte toujours.
    let again = login_with_key(&env, &key, "10.7.7.7").await;
    assert_eq!(again.device, Some(DeviceStatus::Proven));
    let proof = key.prove(&env.trust, session_binding(&session), "marie", "10.7.7.20");
    env.sessions
        .authenticate_proved(&session.token.encode(), "10.7.7.20", &proof)
        .await
        .unwrap();
    let addresses: Vec<String> =
        sqlx::query_scalar("SELECT address FROM known_addresses WHERE device_id IS NOT NULL")
            .fetch_all(env.db.pool())
            .await
            .unwrap();
    assert_eq!(addresses, vec!["10.7.7.20".to_owned()]);
    assert_eq!(
        devices(&env).await,
        1,
        "aucune clé jetable n'a été inscrite"
    );
}

/// Bloquant 2 : les clés de petit ordre (le point neutre en tête) vérifient n'importe quel message
/// sous une signature fixe : jamais inscrites, jamais vérifiées.
#[tokio::test]
async fn a_small_order_public_key_is_never_verified_nor_enrolled() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    // Signature (R = point neutre, S = 0) : valable pour tout message sous la clé « point neutre »
    // (ring ne la refuse pas, voir infrastructure::crypto::tests).
    let mut signature = [0_u8; 64];
    signature[0] = 1;
    let weak_keys: [[u8; 32]; 3] = [
        {
            let mut k = [0_u8; 32];
            k[0] = 1; // y = 1 : le point neutre (ordre 1)
            k
        },
        [0_u8; 32], // y = 0 : ordre 4
        {
            let mut k = [0xff_u8; 32]; // y = p - 1 (ordre 2)
            k[0] = 0xec;
            k[31] = 0x7f;
            k
        },
    ];
    for (n, public) in weak_keys.iter().enumerate() {
        let from = format!("10.7.8.{n}");
        for _ in 0..2 {
            let challenge = env
                .trust
                .issue_challenge("marie", ChallengePurpose::Login, &from)
                .unwrap()
                .challenge;
            let proof = DeviceProof {
                algorithm: "ed25519".into(),
                public_key: STANDARD.encode(public),
                challenge,
                signature: STANDARD.encode(signature),
            };
            assert!(
                env.trust
                    .verify(&proof, Binding::Login, "marie", &from)
                    .is_none(),
                "clé de petit ordre {n} vérifiée"
            );
            let outcome = login(&env, "marie", PASSWORD, &from, Some(&proof))
                .await
                .expect("la connexion ne dépend pas de la clé");
            assert_eq!(outcome.device, None, "clé de petit ordre {n} inscrite");
            env.clock.advance(Duration::minutes(1));
        }
    }
    assert_eq!(devices(&env).await, 0);
    let _ = (CHALLENGE_LEN, signing_bytes as fn(_, _, _, _) -> _);
}

// ---------------------------------------------------------------------------------------------
// Suivis de la revue r1
// ---------------------------------------------------------------------------------------------

/// L'empreinte de 16 octets retrouve le poste ; la clé publique inscrite doit être celle de la preuve.
#[tokio::test]
async fn the_stored_public_key_is_compared_not_only_its_fingerprint() {
    let env = env().await;
    let account = env.create("marie", Role::Admin).await;
    let key = DeviceKey::new();
    // Un poste inscrit sous la même empreinte mais une autre clé publique (impossible à fabriquer par
    // une preuve : on l'écrit directement).
    env.insert_device(
        &account.id,
        "01JDEVICEOTHERKEY00000000",
        &key.key_id(),
        "autre",
    )
    .await;
    let outcome = login_with_key(&env, &key, "10.7.7.7").await;
    assert_eq!(outcome.device, None, "ni prouvé ni inscrit");
    assert_eq!(devices(&env).await, 1);
    let dates: (String, String) =
        sqlx::query_as("SELECT created_at, last_proved_at FROM trusted_devices")
            .fetch_one(env.db.pool())
            .await
            .unwrap();
    assert_eq!(dates.0, dates.1, "aucune preuve datée");
    let linked: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM known_addresses WHERE device_id IS NOT NULL")
            .fetch_one(env.db.pool())
            .await
            .unwrap();
    assert_eq!(linked, 0);
}

#[tokio::test]
async fn removing_a_device_closes_every_session_opened_from_it_even_before_its_enrolment() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    // Une session ouverte SANS clé depuis le poste, puis l'inscription du poste (même adresse).
    let before = login(&env, "marie", PASSWORD, "10.7.7.1", None)
        .await
        .unwrap();
    let key = DeviceKey::new();
    let enrolled = login_with_key(&env, &key, "10.7.7.1").await;
    // Un autre poste, d'une autre adresse, et une session d'une autre adresse sans clé.
    let other = login(&env, "marie", PASSWORD, "10.7.7.5", None)
        .await
        .unwrap();
    let account = marie(&env).await;
    let device = env
        .trust
        .list(&account, &enrolled.session_id)
        .await
        .unwrap()
        .remove(0)
        .id;
    env.trust
        .remove(&account, &other.session_id, device.as_str(), by())
        .await
        .unwrap();
    for gone in [&before, &enrolled] {
        let error = env
            .sessions
            .authenticate(&gone.token.encode())
            .await
            .unwrap_err();
        assert!(
            matches!(error, AuthError::Ended(SessionEnd::Revoked)),
            "{error:?}"
        );
    }
    env.sessions
        .authenticate(&other.token.encode())
        .await
        .expect("la session d'une autre adresse reste");
}

#[tokio::test]
async fn two_devices_behind_the_same_address_the_last_to_prove_holds_it_and_both_stay_enrolled() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let (a, b) = (DeviceKey::new(), DeviceKey::new());
    login_with_key(&env, &a, "10.7.7.1").await;
    env.clock.advance(Duration::minutes(1));
    login_with_key(&env, &b, "10.7.7.1").await;
    assert_eq!(devices(&env).await, 2, "les deux postes restent inscrits");
    let rows: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT address, device_id FROM known_addresses")
            .fetch_all(env.db.pool())
            .await
            .unwrap();
    assert_eq!(rows.len(), 1, "une ligne par adresse : {rows:?}");
    let id_of = |key: &DeviceKey| {
        let pool = env.db.pool().clone();
        let key_id = key.key_id();
        async move {
            sqlx::query_scalar::<_, String>("SELECT id FROM trusted_devices WHERE key_id = ?")
                .bind(key_id)
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };
    assert_eq!(
        rows[0].1.as_deref(),
        Some(id_of(&b).await.as_str()),
        "le dernier qui prouve"
    );
    // Chacun garde sa dernière adresse dans la liste.
    let last: Vec<String> = sqlx::query_scalar("SELECT last_addr FROM trusted_devices")
        .fetch_all(env.db.pool())
        .await
        .unwrap();
    assert_eq!(last, vec!["10.7.7.1".to_owned(), "10.7.7.1".to_owned()]);
    // Le premier prouve de nouveau : l'adresse lui revient, rien n'est perdu.
    env.clock.advance(Duration::minutes(1));
    assert_eq!(
        login_with_key(&env, &a, "10.7.7.1").await.device,
        Some(DeviceStatus::Proven)
    );
    let holder: String = sqlx::query_scalar("SELECT device_id FROM known_addresses")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert_eq!(holder, id_of(&a).await);
}

#[tokio::test]
async fn two_simultaneous_enrolments_of_the_same_key_make_one_device() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let key = DeviceKey::new();
    let (p1, p2) = (
        key.login_proof(&env, "marie", "10.7.7.1"),
        key.login_proof(&env, "marie", "10.7.7.2"),
    );
    let (a, b) = tokio::join!(
        login(&env, "marie", PASSWORD, "10.7.7.1", Some(&p1)),
        login(&env, "marie", PASSWORD, "10.7.7.2", Some(&p2)),
    );
    let mut statuses = [a.unwrap().device, b.unwrap().device];
    statuses.sort_by_key(|s| format!("{s:?}"));
    assert_eq!(
        statuses,
        [Some(DeviceStatus::Enrolled), Some(DeviceStatus::Proven)]
    );
    assert_eq!(devices(&env).await, 1);
    let rows: Vec<Option<String>> = sqlx::query_scalar("SELECT device_id FROM known_addresses")
        .fetch_all(env.db.pool())
        .await
        .unwrap();
    assert_eq!(
        rows.iter().filter(|r| r.is_some()).count(),
        1,
        "une seule adresse liée"
    );
}

/// La vraie course d'inscription : six connexions de la MÊME clé, de six adresses (chacune son défi,
/// donc aucune file commune), sur plusieurs fils. Un seul poste est inscrit, une seule connexion
/// l'inscrit, les cinq autres le prouvent ; la clé n'a qu'une adresse retenue.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn six_simultaneous_logins_of_one_key_from_six_addresses_enrol_exactly_one_device() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let key = DeviceKey::new();
    let tasks: Vec<_> = (1..=6)
        .map(|n| {
            let from = format!("10.7.7.{n}");
            let proof = key.login_proof(&env, "marie", &from);
            spawn_login(&env, from, proof)
        })
        .collect();
    let mut statuses = Vec::new();
    for task in tasks {
        statuses.push(task.await.unwrap().unwrap().device);
    }
    assert_eq!(
        statuses
            .iter()
            .filter(|s| **s == Some(DeviceStatus::Enrolled))
            .count(),
        1,
        "{statuses:?}"
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|s| **s == Some(DeviceStatus::Proven))
            .count(),
        5,
        "{statuses:?}"
    );
    assert_eq!(devices(&env).await, 1);
    let bound: i64 = scalar(
        &env,
        "SELECT COUNT(*) FROM known_addresses WHERE device_id IS NOT NULL",
    )
    .await;
    assert_eq!(bound, 1, "une seule adresse liée au poste");
}

/// Quatre nouveaux postes se disputent la dernière place (huit au plus), sur plusieurs fils : exactement
/// un entre, les trois autres se connectent sans être inscrits, jamais plus de huit postes.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn four_simultaneous_enrolments_for_the_last_place_leave_exactly_eight() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    for n in 0..7 {
        login_with_key(&env, &DeviceKey::new(), &format!("10.7.7.{}", n + 1)).await;
        env.clock.advance(Duration::minutes(1));
    }
    let tasks: Vec<_> = (0..4)
        .map(|n| {
            let from = format!("10.7.8.{n}");
            let proof = DeviceKey::new().login_proof(&env, "marie", &from);
            spawn_login(&env, from, proof)
        })
        .collect();
    let mut statuses = Vec::new();
    for task in tasks {
        statuses.push(task.await.unwrap().unwrap().device);
    }
    assert_eq!(
        statuses
            .iter()
            .filter(|s| **s == Some(DeviceStatus::Enrolled))
            .count(),
        1,
        "{statuses:?}"
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|s| **s == Some(DeviceStatus::Limit))
            .count(),
        3,
        "{statuses:?}"
    );
    assert_eq!(devices(&env).await, 8);
}

#[tokio::test]
async fn the_eighth_and_the_ninth_enrolments_at_the_same_time_leave_exactly_eight() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    for n in 0..7 {
        login_with_key(&env, &DeviceKey::new(), &format!("10.7.7.{}", n + 1)).await;
        env.clock.advance(Duration::minutes(1));
    }
    let (k8, k9) = (DeviceKey::new(), DeviceKey::new());
    let (p8, p9) = (
        k8.login_proof(&env, "marie", "10.7.8.8"),
        k9.login_proof(&env, "marie", "10.7.8.9"),
    );
    let (a, b) = tokio::join!(
        login(&env, "marie", PASSWORD, "10.7.8.8", Some(&p8)),
        login(&env, "marie", PASSWORD, "10.7.8.9", Some(&p9)),
    );
    let mut statuses = [a.unwrap().device, b.unwrap().device];
    statuses.sort_by_key(|s| format!("{s:?}"));
    assert_eq!(
        statuses,
        [Some(DeviceStatus::Enrolled), Some(DeviceStatus::Limit)]
    );
    assert_eq!(devices(&env).await, 8);
}

/// Une preuve valide d'une clé non inscrite n'empêche pas la session d'être renouvelée : l'adresse déjà
/// retenue est repoussée ce tour-là.
#[tokio::test]
async fn a_proof_that_does_not_serve_still_refreshes_the_retained_address_of_the_session() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let outcome = login(&env, "marie", PASSWORD, "10.7.7.7", None)
        .await
        .unwrap();
    env.clock.advance(Duration::minutes(10));
    let stranger = DeviceKey::new();
    let proof = stranger.prove(&env.trust, session_binding(&outcome), "marie", "10.7.7.7");
    env.sessions
        .authenticate_proved(&outcome.token.encode(), "10.7.7.7", &proof)
        .await
        .unwrap();
    let used: Option<String> = sqlx::query_scalar("SELECT last_used_at FROM known_addresses")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert!(used.is_some(), "adresse rafraîchie");
    assert_eq!(devices(&env).await, 0);
}

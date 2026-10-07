//! La règle « 2 critères sur 3 » aux états NORMAL et ALERTE, l'alerte « attaque probable » et ses
//! deux canaux (`GET /security`, message `security` du flux), le journal (HRT-24, ADR-0024,
//! BR-TRUST-001, 002, 006, 008, 034, 035). Vraie base SQLite temporaire, vraies signatures Ed25519,
//! temps contrôlé (horloges de test, aucun `sleep`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use axum::response::IntoResponse;
use hearth_agent::application::sessions::{LoginError, LoginOutcome};
use hearth_agent::domain::accounts::Role;
use hearth_agent::domain::sessions::SessionId;
use hearth_agent::entrypoint::http::ApiError;
use hearth_proto::api::sessions::DeviceProof;
use http_body_util::BodyExt;
use serde_json::json;
use support::api::Api;
use support::device::DeviceKey;
use support::https;
use support::probe::metering;
use support::ws;
use support::{CLIENT_ADDR, Env, PASSWORD, client_at, env, secret};
use time::Duration;

const WRONG: &str = "Wrong-Horse-9999";

/// Une adresse inconnue de tous les comptes, la `n`-ième.
fn stranger(n: u32) -> String {
    format!("10.1.{}.{}", n / 250, n % 250 + 1)
}

async fn wrong(env: &Env, username: &str, from: &str) -> LoginError {
    env.sessions
        .login(username, secret(WRONG), &client_at(from))
        .await
        .expect_err("mauvais mot de passe")
}

async fn wrong_with(
    env: &Env,
    username: &str,
    from: &str,
    proof: Option<&DeviceProof>,
) -> LoginError {
    env.sessions
        .login_with_device(username, secret(WRONG), &client_at(from), proof)
        .await
        .expect_err("mauvais mot de passe")
}

/// Ce que le client voit d'un refus : code HTTP, en-têtes triés, corps.
type Wire = (u16, Vec<(String, String)>, String);

async fn on_the_wire(error: LoginError) -> Wire {
    let response = ApiError::from(error).into_response();
    let status = response.status().as_u16();
    let mut headers: Vec<_> = response
        .headers()
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_str().unwrap().to_owned()))
        .collect();
    headers.sort();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, headers, String::from_utf8(body.to_vec()).unwrap())
}

/// `count` échecs venus d'adresses inconnues, qui ne se font pas attendre (l'horloge ne bouge pas :
/// les dix premiers sont gratuits, le onzième ouvre l'alerte et sa première attente).
async fn attack(env: &Env, username: &str, count: u32) {
    for n in 0..count {
        let _ = wrong(env, username, &stranger(n)).await;
    }
}

async fn scalar(env: &Env, sql: &'static str) -> i64 {
    sqlx::query_scalar(sql)
        .fetch_one(env.db.pool())
        .await
        .unwrap()
}

/// `marie` (administrateur) a une adresse retenue (`CLIENT_ADDR`) et un poste inscrit (la clé rendue),
/// puis son identifiant est visé : onze échecs d'adresses inconnues, donc ALERTE et attente en cours.
async fn marie_under_attack() -> (Env, DeviceKey) {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let key = DeviceKey::new();
    let proof = key.login_proof(&env, "marie", CLIENT_ADDR);
    env.sessions
        .login_with_device(
            "marie",
            secret(PASSWORD),
            &client_at(CLIENT_ADDR),
            Some(&proof),
        )
        .await
        .expect("connexion et inscription de marie");
    attack(&env, "marie", 11).await;
    (env, key)
}

fn waited(error: &LoginError) -> Option<i64> {
    match error {
        LoginError::TooManyAttempts { retry_after } => Some(retry_after.whole_seconds()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------
// La règle : la table de vérité, de bout en bout
// ---------------------------------------------------------------------------------------------

/// Un poste se présente avec ces critères pendant l'alerte, mot de passe juste.
async fn present(
    address: bool,
    key: bool,
    first_try: bool,
) -> (Env, Result<LoginOutcome, LoginError>) {
    let (env, device) = marie_under_attack().await;
    let from = if address { CLIENT_ADDR } else { "10.9.9.9" };
    if !first_try {
        // Une erreur de frappe depuis ce poste : le compteur du couple n'est plus à zéro.
        let _ = wrong(&env, "marie", from).await;
    }
    let proof = key.then(|| device.login_proof(&env, "marie", from));
    let outcome = env
        .sessions
        .login_with_device("marie", secret(PASSWORD), &client_at(from), proof.as_ref())
        .await;
    (env, outcome)
}

/// (adresse retenue, clé prouvée, premier coup, reconnu) : le plan de scénario « Reconnaissance d'un
/// poste en état d'alerte », écrit à la main.
const RECOGNITION: [(bool, bool, bool, bool); 8] = [
    (true, true, true, true),
    (true, true, false, true),
    (true, false, true, true),
    (true, false, false, false),
    (false, true, true, true),
    (false, true, false, false),
    (false, false, true, false),
    (false, false, false, false),
];

#[tokio::test]
async fn in_alert_every_combination_of_criteria_is_recognised_as_the_table_says_and_nobody_is_locked_out()
 {
    for (address, key, first_try, recognised) in RECOGNITION {
        let label = format!("adresse {address}, clé {key}, premier coup {first_try}");
        let (env, outcome) = present(address, key, first_try).await;
        if recognised {
            assert!(
                outcome.is_ok(),
                "{label} : reconnu, pas ralenti : {outcome:?}"
            );
            continue;
        }
        // Non reconnu : ralenti, jamais refusé au-delà du plafond de deux minutes ; le mot de passe
        // juste passe dès la fin de l'attente (jamais d'enfermement).
        let error = outcome.expect_err(&label);
        let wait =
            waited(&error).unwrap_or_else(|| panic!("{label} : ralenti attendu : {error:?}"));
        assert!((1..=120).contains(&wait), "{label} : attente {wait} s");
        env.clock.advance(Duration::seconds(121));
        let from = if address { CLIENT_ADDR } else { "10.9.9.9" };
        let again = env
            .sessions
            .login("marie", secret(PASSWORD), &client_at(from))
            .await;
        assert!(
            again.is_ok(),
            "{label} : le bon mot de passe passe après l'attente : {again:?}"
        );
    }
}

#[tokio::test]
async fn in_the_normal_state_the_rule_changes_nothing_a_stranger_connects_like_a_known_poste() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let key = DeviceKey::new();
    let proof = key.login_proof(&env, "marie", CLIENT_ADDR);
    env.sessions
        .login_with_device(
            "marie",
            secret(PASSWORD),
            &client_at(CLIENT_ADDR),
            Some(&proof),
        )
        .await
        .unwrap();
    // Dix échecs d'adresses inconnues : encore gratuits, aucune alerte (la règle ne joue pas).
    attack(&env, "marie", 10).await;
    for from in ["10.9.9.9", CLIENT_ADDR] {
        let outcome = env
            .sessions
            .login("marie", secret(PASSWORD), &client_at(from))
            .await;
        assert!(outcome.is_ok(), "{from} : {outcome:?}");
    }
    let state = env
        .security
        .state_for(
            &env.service.find("marie").await.unwrap(),
            &SessionId::new("S"),
        )
        .await
        .unwrap();
    assert!(!state.alert.own);
}

#[tokio::test]
async fn a_recognised_poste_is_not_slowed_while_other_devices_pile_up_failures() {
    let (env, key) = marie_under_attack().await;
    // Les attaquants continuent, l'attente de l'identifiant est en cours.
    for n in 100..140 {
        let error = wrong(&env, "marie", &stranger(n)).await;
        assert!(waited(&error).is_some());
    }
    // Le poste de marie : adresse retenue + clé + premier coup, il passe sans attendre.
    let proof = key.login_proof(&env, "marie", CLIENT_ADDR);
    let outcome = env
        .sessions
        .login_with_device(
            "marie",
            secret(PASSWORD),
            &client_at(CLIENT_ADDR),
            Some(&proof),
        )
        .await;
    assert!(outcome.is_ok(), "{outcome:?}");
    // Un poste non reconnu, lui, est ralenti sans être bloqué.
    let stranger_error = env
        .sessions
        .login("marie", secret(PASSWORD), &client_at("10.9.9.9"))
        .await
        .expect_err("ralenti");
    assert!(waited(&stranger_error).is_some(), "{stranger_error:?}");
}

#[tokio::test]
async fn my_pc_changes_address_and_stays_recognised_thanks_to_the_key_and_the_new_address_is_retained()
 {
    let (env, key) = marie_under_attack().await;
    let proof = key.login_proof(&env, "marie", "10.8.8.8");
    let outcome = env
        .sessions
        .login_with_device(
            "marie",
            secret(PASSWORD),
            &client_at("10.8.8.8"),
            Some(&proof),
        )
        .await
        .expect("clé + premier coup : reconnu");
    assert!(outcome.device.is_some());
    // La nouvelle adresse est retenue, la clé a toujours une seule adresse.
    let addresses: Vec<String> = sqlx::query_scalar("SELECT address FROM known_addresses")
        .fetch_all(env.db.pool())
        .await
        .unwrap();
    assert!(addresses.contains(&"10.8.8.8".to_owned()), "{addresses:?}");
}

/// Aucune suite d'actions d'un appareil SANS COMPTE ne fait refuser ni ralentir un poste qui présente
/// ses critères et le bon mot de passe : flot d'échecs, de défis, de preuves de session fausses, de
/// clés jetables.
#[tokio::test]
async fn nothing_a_deviceless_appliance_does_ever_refuses_a_poste_with_two_criteria_and_the_right_password()
 {
    let (env, key) = marie_under_attack().await;
    let token = env
        .sessions
        .login("marie", secret(PASSWORD), &client_at(CLIENT_ADDR))
        .await
        .unwrap()
        .token
        .encode();
    for n in 0..60_u32 {
        let from = stranger(200 + n);
        // Des échecs sur marie et sur d'autres identifiants, des défis pour marie, des clés jetables
        // avec de faux mots de passe, des preuves de session sur un jeton qu'il ne possède pas.
        let _ = wrong(&env, "marie", &from).await;
        let _ = wrong(&env, &format!("autre{n}"), &from).await;
        let _ = env.trust.issue_challenge(
            "marie",
            hearth_proto::api::sessions::ChallengePurpose::Login,
            &from,
        );
        let throwaway = DeviceKey::new();
        let proof = throwaway.login_proof(&env, "marie", &from);
        let _ = wrong_with(&env, "marie", &from, Some(&proof)).await;
        let _ = env
            .sessions
            .authenticate_proved(&token, &from, &proof)
            .await;
    }
    let proof = key.login_proof(&env, "marie", CLIENT_ADDR);
    let outcome = env
        .sessions
        .login_with_device(
            "marie",
            secret(PASSWORD),
            &client_at(CLIENT_ADDR),
            Some(&proof),
        )
        .await;
    assert!(outcome.is_ok(), "{outcome:?}");
}

// ---------------------------------------------------------------------------------------------
// Absence d'oracle
// ---------------------------------------------------------------------------------------------

/// Ce qu'un appareil sans compte peut observer, et ce que l'agent a fait pour le produire.
#[derive(Debug, PartialEq, Eq)]
struct Observation {
    wire: Wire,
    /// Calculs Argon2.
    hashes: u64,
    /// Vérifications de signature.
    signatures: u64,
    /// Lignes écrites ou créées, par table (compteurs, ralentissements, adresses, sessions, postes).
    rows: Vec<i64>,
    /// Entrées de journal ajoutées.
    journal: i64,
}

async fn rows(env: &Env) -> Vec<i64> {
    let mut out = Vec::new();
    for sql in [
        // Les échecs comptés (le nombre de lignes dépend de l'historique du couple, pas du compte).
        "SELECT COALESCE(SUM(failures), 0) FROM login_attempts",
        "SELECT COUNT(*) FROM identifier_slowdowns",
        "SELECT COUNT(*) FROM identifier_slowdowns WHERE alerted_at IS NOT NULL",
        "SELECT COUNT(*) FROM known_addresses",
        "SELECT COUNT(*) FROM sessions",
        "SELECT COUNT(*) FROM trusted_devices",
    ] {
        out.push(scalar(env, sql).await);
    }
    out
}

async fn observe(
    env: &Env,
    username: &str,
    from: &str,
    proof: Option<&DeviceProof>,
) -> Observation {
    let (hashes, signatures, before, journal) = (
        env.hasher.verifications(),
        env.verifier.calls(),
        rows(env).await,
        scalar(env, "SELECT COUNT(*) FROM audit_events").await,
    );
    let error = wrong_with(env, username, from, proof).await;
    let wire = on_the_wire(error).await;
    let after = rows(env).await;
    Observation {
        wire,
        hashes: env.hasher.verifications() - hashes,
        signatures: env.verifier.calls() - signatures,
        rows: after.iter().zip(&before).map(|(a, b)| a - b).collect(),
        journal: scalar(env, "SELECT COUNT(*) FROM audit_events").await - journal,
    }
}

/// `marie` existe, `fantome` non. Les deux sont dans le même état (normal ou alerte). `marie` a une
/// adresse retenue et un poste ; une autre adresse est retenue par `autre` (pour que `fantome` soit
/// aussi approché depuis une adresse « vue »).
async fn oracle_setup(alert: bool) -> (Env, DeviceKey) {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    env.create("autre", Role::Admin).await;
    let key = DeviceKey::new();
    let proof = key.login_proof(&env, "marie", CLIENT_ADDR);
    env.sessions
        .login_with_device(
            "marie",
            secret(PASSWORD),
            &client_at(CLIENT_ADDR),
            Some(&proof),
        )
        .await
        .unwrap();
    env.sessions
        .login("autre", secret(PASSWORD), &client_at("10.0.0.50"))
        .await
        .unwrap();
    let failures = if alert { 11 } else { 3 };
    for name in ["marie", "fantome"] {
        attack(&env, name, failures).await;
    }
    (env, key)
}

#[tokio::test]
async fn a_device_without_account_sees_the_same_answer_whether_the_identifier_exists_or_not_in_every_state()
 {
    for alert in [false, true] {
        for seen in [false, true] {
            for proof_kind in ["none", "enrolled key", "throwaway key"] {
                let label = format!("alerte {alert}, adresse vue {seen}, preuve {proof_kind}");
                let (env, enrolled) = oracle_setup(alert).await;
                let throwaway = DeviceKey::new();
                // `marie` est approchée depuis son adresse retenue (ou une neuve) ; `fantome` depuis
                // une adresse retenue par `autre` (ou une neuve) : « vue » dans les deux cas.
                let (for_marie, for_fantome) = if seen {
                    (CLIENT_ADDR, "10.0.0.50")
                } else {
                    ("10.5.5.5", "10.5.5.6")
                };
                let proof_for = |env: &Env, user: &str, from: &str| match proof_kind {
                    "none" => None,
                    "enrolled key" => Some(enrolled.login_proof(env, user, from)),
                    _ => Some(throwaway.login_proof(env, user, from)),
                };
                let marie_proof = proof_for(&env, "marie", for_marie);
                let fantome_proof = proof_for(&env, "fantome", for_fantome);
                let marie = observe(&env, "marie", for_marie, marie_proof.as_ref()).await;
                let fantome = observe(&env, "fantome", for_fantome, fantome_proof.as_ref()).await;
                assert_eq!(marie, fantome, "{label}");
                assert_eq!(marie.hashes, 1, "{label} : un calcul coûteux, toujours");
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// L'alerte : une fois par épisode, au journal et au flux
// ---------------------------------------------------------------------------------------------

async fn alert_entries(env: &Env) -> Vec<(Option<String>, Option<String>, String)> {
    sqlx::query_as::<_, (Option<String>, Option<String>, String)>(
        "SELECT account, target, outcome FROM audit_events WHERE action = 'security.alert' ORDER BY id",
    )
    .fetch_all(env.db.pool())
    .await
    .unwrap()
}

#[tokio::test]
async fn the_alert_is_signalled_once_when_the_identifier_starts_to_be_slowed_and_never_for_a_missing_one()
 {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    for name in ["marie", "fantome"] {
        attack(&env, name, 10).await;
    }
    assert!(
        alert_entries(&env).await.is_empty(),
        "dix échecs : gratuits"
    );
    let before = rows(&env).await;
    // Le onzième échec ouvre l'épisode ; les suivants ne le signalent plus.
    attack(&env, "marie", 11).await;
    for n in 11..16 {
        let _ = wrong(&env, "marie", &stranger(n)).await;
    }
    let entries = alert_entries(&env).await;
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert_eq!(entries[0].0.as_deref(), Some("marie"));
    assert_eq!(entries[0].1.as_deref(), Some("début de l'alerte"));
    // `fantome` : l'épisode est noté pareil (même écriture), mais rien n'est signalé au journal.
    attack(&env, "fantome", 11).await;
    assert_eq!(alert_entries(&env).await.len(), 1);
    let after = rows(&env).await;
    assert_eq!(
        after[2] - before[2],
        2,
        "alerted_at noté pour les deux identifiants"
    );
}

#[tokio::test]
async fn the_end_of_the_alert_is_signalled_once_by_the_sweep_and_a_new_episode_starts_clean() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    attack(&env, "marie", 11).await;
    assert_eq!(env.security.sweep().await.unwrap(), 0, "l'épisode dure");
    env.clock.advance(Duration::minutes(29));
    assert_eq!(env.security.sweep().await.unwrap(), 0);
    env.clock.advance(Duration::minutes(2));
    assert_eq!(env.security.sweep().await.unwrap(), 1);
    assert_eq!(env.security.sweep().await.unwrap(), 0, "une seule fois");
    let entries = alert_entries(&env).await;
    assert_eq!(entries.len(), 2, "{entries:?}");
    assert_eq!(entries[1].1.as_deref(), Some("fin de l'alerte"));
    assert_eq!(
        scalar(
            &env,
            "SELECT COUNT(*) FROM identifier_slowdowns WHERE alerted_at IS NOT NULL"
        )
        .await,
        0
    );
    // Un nouvel épisode recommence à zéro : 10 échecs gratuits, le 11e le signale de nouveau.
    attack(&env, "marie", 11).await;
    assert_eq!(alert_entries(&env).await.len(), 3);
}

#[tokio::test]
async fn a_counter_that_starts_over_without_a_sweep_still_ends_the_old_episode_once() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    attack(&env, "marie", 11).await;
    env.clock.advance(Duration::minutes(31));
    // La première tentative après 30 minutes remet le compteur à zéro : fin de l'épisode.
    let _ = wrong(&env, "marie", &stranger(500)).await;
    let entries = alert_entries(&env).await;
    assert_eq!(entries.len(), 2, "{entries:?}");
    assert_eq!(entries[1].1.as_deref(), Some("fin de l'alerte"));
    assert_eq!(env.security.sweep().await.unwrap(), 0);
}

#[tokio::test]
async fn my_own_typos_from_my_recognised_poste_never_raise_an_alert() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    env.sessions
        .login("marie", secret(PASSWORD), &client_at(CLIENT_ADDR))
        .await
        .unwrap();
    // Des erreurs de frappe depuis son poste, en plusieurs rafales (le compteur du couple s'en occupe).
    for _ in 0..3 {
        for _ in 0..4 {
            let _ = wrong(&env, "marie", CLIENT_ADDR).await;
        }
        env.clock.advance(Duration::hours(2));
    }
    let state = env
        .security
        .state_for(
            &env.service.find("marie").await.unwrap(),
            &SessionId::new("S"),
        )
        .await
        .unwrap();
    assert!(!state.alert.own);
    assert!(alert_entries(&env).await.is_empty());
}

// ---------------------------------------------------------------------------------------------
// Qui voit quoi
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn the_owner_sees_the_alert_an_administrator_sees_the_count_of_the_others_and_a_readonly_only_its_own()
 {
    let env = env().await;
    let api = Api::new(&env);
    let marie = env.account_with_token(&api, "marie", Role::Admin).await;
    let paul = env.account_with_token(&api, "paul", Role::Admin).await;
    let lucas = env.account_with_token(&api, "lucas", Role::ReadOnly).await;
    let zoe = env.account_with_token(&api, "zoe", Role::ReadOnly).await;
    for name in ["marie", "lucas", "fantome"] {
        attack(&env, name, 11).await;
    }
    let state = |token: String| {
        let api = &api;
        async move { api.get("/security").token(&token).send().await }
    };
    // marie : visée, voit un autre compte existant (lucas) ; `fantome` ne compte pas.
    let reply = state(marie).await;
    assert_eq!(reply.status, 200);
    assert_eq!(reply.body["alert"]["own"], true);
    assert_eq!(reply.body["alert"]["others"], 1);
    assert!(reply.body["alert"]["since"].is_string());
    assert_eq!(reply.body["attack_mode"]["state"], "off");
    // paul : pas visé, administrateur : voit deux comptes existants visés, jamais leurs noms.
    let reply = state(paul).await;
    assert_eq!(reply.body["alert"]["own"], false);
    assert_eq!(reply.body["alert"]["others"], 2);
    assert!(
        !reply.text.contains("marie") && !reply.text.contains("lucas"),
        "{}",
        reply.text
    );
    // lucas : lecture seule, visé : voit SON alerte et rien sur les autres.
    let reply = state(lucas).await;
    assert_eq!(reply.body["alert"]["own"], true);
    assert!(
        reply.body["alert"].get("others").is_none(),
        "{}",
        reply.text
    );
    // zoe : lecture seule, pas visée : rien, et aucun indice que d'autres le sont.
    let reply = state(zoe).await;
    assert_eq!(reply.body["alert"], json!({ "own": false }));
}

#[tokio::test]
async fn the_security_route_needs_a_session() {
    let env = env().await;
    let api = Api::new(&env);
    let reply = api.get("/security").send().await;
    assert_eq!(reply.status, 401);
}

#[tokio::test]
async fn the_stream_always_tells_the_state_after_auth_then_on_every_change_and_only_what_the_role_shows()
 {
    let env = env().await;
    let agent = https::start_metered(&env, metering()).await;
    env.create("marie", Role::Admin).await;
    env.create("lucas", Role::ReadOnly).await;
    // Les jetons viennent d'une autre adresse que celle des attaques (127.0.0.1), qui reste donc
    // inconnue de tous les comptes. L'agent a ses propres services (et son propre canal) : l'attaque
    // passe par lui, comme en production.
    let token = |name: &'static str| {
        let env = &env;
        async move {
            env.sessions
                .login(name, secret(PASSWORD), &client_at("10.0.0.7"))
                .await
                .unwrap()
                .token
                .encode()
        }
    };
    let (marie_token, lucas_token) = (token("marie").await, token("lucas").await);
    let attack_through = |name: &'static str| {
        let (env, agent) = (&env, &agent);
        async move {
            // 11 échecs depuis une seule adresse (127.0.0.1) : le compteur du couple verrouille au
            // 5e (1 minute, doublée, 15 au plus), on laisse donc passer le verrou à chaque fois.
            for n in 0..11 {
                let reply = agent
                    .request("POST", "/sessions")
                    .json(&json!({ "username": name, "password": WRONG }))
                    .send()
                    .await;
                assert!(matches!(reply.status, 401 | 429), "{n} : {:?}", reply.body);
                env.clock.advance(Duration::minutes(16));
            }
        }
    };
    let mut marie = ws::open(&agent).await;
    marie.auth(&marie_token).await;
    let mut lucas = ws::open(&agent).await;
    lucas.auth(&lucas_token).await;
    // Toujours envoyé après l'auth, sans abonnement.
    let first = marie.next_security().await;
    let hearth_proto::stream::SecurityMessage::Security(view) = first;
    assert!(!view.alert.own);
    assert_eq!(view.alert.others, Some(0));
    let hearth_proto::stream::SecurityMessage::Security(view) = lucas.next_security().await;
    assert_eq!(view.alert.others, None);

    // L'identifiant de marie est visé : elle le sait, lucas (lecture seule) n'apprend rien.
    attack_through("marie").await;
    let hearth_proto::stream::SecurityMessage::Security(view) = marie.next_security().await;
    assert!(view.alert.own);
    assert!(view.alert.since.is_some());
    // Puis lucas est visé à son tour : marie (administratrice) voit le nombre, lucas voit le sien.
    attack_through("lucas").await;
    let hearth_proto::stream::SecurityMessage::Security(view) = marie.next_security().await;
    assert_eq!(view.alert.others, Some(1));
    let hearth_proto::stream::SecurityMessage::Security(view) = lucas.next_security().await;
    assert!(view.alert.own);
    assert_eq!(view.alert.others, None);
    agent.shutdown().await;
}

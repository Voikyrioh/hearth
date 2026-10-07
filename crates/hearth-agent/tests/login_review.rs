//! Les bloquants de la review de la PR #23 (HRT-20), chacun par un test écrit AVANT la correction :
//! journal inondable par une attaque qui dure (B1), horloge qui recule ou avance (B4), oracle
//! d'existence d'un identifiant par l'exemption des adresses connues (B5). Temps contrôlé : horloge
//! de test, aucune durée réelle, aucun `sleep`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use hearth_agent::application::sessions::LoginError;
use hearth_agent::domain::accounts::Role;
use hearth_agent::domain::audit::{AuditFilter, RawFilter};
use hearth_agent::entrypoint::http::ApiError;
use support::{CLIENT_ADDR, Env, PASSWORD, client_at, env, secret};
use time::Duration;

const WRONG: &str = "Wrong-Horse-9999";

fn addr(n: u32) -> String {
    format!("10.1.{}.{}", n / 250, n % 250 + 1)
}

async fn wrong(env: &Env, username: &str, from: &str) -> LoginError {
    env.sessions
        .login(username, secret(WRONG), &client_at(from))
        .await
        .expect_err("mauvais mot de passe")
}

fn waited(error: &LoginError) -> Option<i64> {
    match error {
        LoginError::TooManyAttempts { retry_after } => Some(retry_after.whole_seconds()),
        _ => None,
    }
}

type Wire = (u16, Vec<(String, String)>, String);

/// Ce que le client voit d'un refus : code HTTP, en-têtes triés, corps.
async fn on_the_wire(error: LoginError) -> Wire {
    use axum::response::IntoResponse;
    use http_body_util::BodyExt;
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

/// Dix échecs gratuits d'un identifiant, venus de dix adresses différentes.
async fn free_failures(env: &Env, username: &str) {
    for n in 0..10 {
        let error = wrong(env, username, &addr(n)).await;
        assert!(matches!(error, LoginError::InvalidCredentials), "{error:?}");
    }
}

// ---- B1 : une attaque qui dure ne fait pas tourner le journal

#[tokio::test]
async fn an_attack_of_hours_with_a_new_address_each_time_leaves_a_journal_bounded_by_time_with_the_exact_count()
 {
    let env = env().await;
    let minutes: u32 = 180;
    let per_minute: u32 = 4;
    let mut next = 0;
    // Un appareil sans compte tente « fantome » depuis une adresse neuve toutes les 15 secondes,
    // pendant trois heures simulées ; le journal est vidé chaque minute comme en production.
    for _ in 0..minutes {
        for _ in 0..per_minute {
            let _ = wrong(&env, "fantome", &addr(next)).await;
            next += 1;
            env.clock.advance(Duration::seconds(15));
        }
        env.audit_recorder.flush().await;
    }
    env.audit_recorder.flush_all().await;

    // Toutes les pages du journal.
    let mut records = Vec::new();
    let mut before = None;
    loop {
        let filter = AuditFilter::new(RawFilter {
            before,
            ..RawFilter::default()
        })
        .unwrap();
        let page = env.audit.search(Role::Admin, &filter).await.unwrap();
        records.extend(page.records);
        before = page.next_before;
        if before.is_none() {
            break;
        }
    }
    let logins: Vec<_> = records
        .iter()
        .filter(|record| record.action == "login")
        .collect();
    // Chaque tentative est comptée, exactement : une entrée sans répétition vaut une tentative, une
    // synthèse vaut ses répétitions.
    let counted: u32 = logins.iter().map(|record| record.repeat_count.max(1)).sum();
    assert_eq!(counted, minutes * per_minute);
    // BR-AUDIT-007 (Q14, points 9 et 10) : un groupe par adresse, au plus 8 adresses par famille et
    // par fenêtre, une synthèse « N tentatives depuis M adresses » au-delà, la fenêtre de la
    // famille qui s'allonge. Cette attaque change d'adresse à CHAQUE tentative ; le nombre d'entrées
    // est borné par le TEMPS, pas par le nombre de tentatives (le test d'avant : 541 ; sans plafond
    // par famille : 814). Valeur exacte, déterministe (horloge simulée) : 329 entrées, bien sous les 541
    // d'avant (suivi de la revue de la PR #26, rendu exact avec HRT-25).
    assert_eq!(
        records.len(),
        329,
        "{} entrées pour {} tentatives",
        records.len(),
        minutes * per_minute
    );
    eprintln!(
        "MESURE login_review : {} entrées pour 720 tentatives",
        records.len()
    );
}

// ---- B4 : l'horloge du serveur recule ou avance

#[tokio::test]
async fn a_clock_set_back_never_blocks_longer_than_the_cap_and_a_jump_forward_ends_the_wait() {
    for (label, shift) in [
        ("recul d'une heure", Duration::hours(-1)),
        ("recul d'un an", Duration::days(-365)),
        ("avance d'un an", Duration::days(365)),
    ] {
        let env = env().await;
        env.create("marie", Role::Admin).await;
        free_failures(&env, "marie").await;
        let slowed = wrong(&env, "marie", &addr(10)).await;
        assert_eq!(waited(&slowed), Some(2), "{label}");

        env.clock.advance(shift);
        if shift.is_negative() {
            // Une attente n'est jamais prolongée au-delà du plafond de 2 minutes par un recul, ni
            // raccourcie à zéro : elle vaut 120 s depuis la première tentative après le recul.
            let first = wrong(&env, "marie", &addr(11)).await;
            assert_eq!(waited(&first), Some(120), "{label}");
            env.clock.advance(Duration::seconds(119));
            let still = wrong(&env, "marie", &addr(12)).await;
            assert_eq!(waited(&still), Some(1), "{label}");
            env.clock.advance(Duration::seconds(1));
        }
        // Au plus 2 minutes plus tard : une tentative est de nouveau admise.
        let admitted = wrong(&env, "marie", &addr(13)).await;
        assert!(
            matches!(admitted, LoginError::InvalidCredentials),
            "{label} : {admitted:?}"
        );
    }
}

// ---- B5 : aucun oracle d'existence d'un identifiant, même depuis une adresse connue

/// Un compte `marie` connu de `CLIENT_ADDR` (et un identifiant `fantome` qui n'existe pas) ; les
/// deux ont reçu les mêmes échecs d'adresses inconnues.
async fn oracle_setup() -> Env {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    env.sessions
        .login("marie", secret(PASSWORD), &client_at(CLIENT_ADDR))
        .await
        .expect("connexion de marie, son poste devient connu");
    env
}

#[tokio::test]
async fn twenty_requests_from_a_known_address_do_not_tell_an_existing_identifier_from_a_missing_one()
 {
    // La suite du reviewer : depuis l'adresse connue de marie, 20 échecs sur des identifiants
    // inventés, puis « marie » et « fantome » avec un mauvais mot de passe.
    let env = oracle_setup().await;
    for n in 0..20 {
        let _ = wrong(&env, &format!("inconnu{n}"), CLIENT_ADDR).await;
    }
    let marie = on_the_wire(wrong(&env, "marie", CLIENT_ADDR).await).await;
    let fantome = on_the_wire(wrong(&env, "fantome", CLIENT_ADDR).await).await;
    assert_eq!(marie, fantome);
}

#[tokio::test]
async fn a_slowed_identifier_answers_a_known_address_like_a_missing_one_unless_the_password_is_right()
 {
    let env = oracle_setup().await;
    // Les deux identifiants sont ralentis par des adresses inconnues.
    for name in ["marie", "fantome"] {
        free_failures(&env, name).await;
        let slowed = wrong(&env, name, &addr(10)).await;
        assert_eq!(waited(&slowed), Some(2), "{name}");
    }
    // Depuis l'adresse connue de marie, mauvais mot de passe : même réponse sur le fil, même
    // nombre de vérifications.
    let before = env.hasher.verifications();
    let marie = on_the_wire(wrong(&env, "marie", CLIENT_ADDR).await).await;
    let marie_checks = env.hasher.verifications() - before;
    let before = env.hasher.verifications();
    let fantome = on_the_wire(wrong(&env, "fantome", CLIENT_ADDR).await).await;
    let fantome_checks = env.hasher.verifications() - before;
    assert_eq!(marie, fantome);
    assert_eq!(marie_checks, fantome_checks);
    assert_eq!(marie_checks, 1, "même chemin : une vérification chacun");
    // Marie vient de se tromper depuis son poste connu : ce n'est plus « du premier coup » et son
    // adresse retenue seule ne suffit plus pendant l'alerte (ADR-0024, règle « 2 critères sur 3 »,
    // BR-TRUST-001 b). Elle est ralentie, jamais bloquée : le bon mot de passe passe dès la fin de
    // l'attente.
    let attempt = env
        .sessions
        .login("marie", secret(PASSWORD), &client_at(CLIENT_ADDR))
        .await;
    assert!(
        matches!(attempt, Err(LoginError::TooManyAttempts { .. })),
        "{attempt:?}"
    );
    env.clock.advance(Duration::seconds(121));
    assert!(
        env.sessions
            .login("marie", secret(PASSWORD), &client_at(CLIENT_ADDR))
            .await
            .is_ok()
    );
}

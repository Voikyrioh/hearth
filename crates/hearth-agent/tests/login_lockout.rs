//! Le verrouillage de connexion résiste au changement d'adresse (HRT-20, ADR-0022), de bout en
//! bout sur une vraie base SQLite temporaire : ralentissement par identifiant, adresses connues,
//! origine IPv6 en /64, plafond des connexions en cours. Temps contrôlé (horloge de test, aucune
//! durée réelle, aucun `sleep`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;

use async_trait::async_trait;
use hearth_agent::application::ports::{Clock, HashError, KnownAddressRepo, PasswordHasher, Store};
use hearth_agent::application::sessions::{LoginError, LoginOutcome, SessionService};
use hearth_agent::domain::accounts::{PlainPassword, Role};
use hearth_agent::domain::identifier_slowdown::{self, FREE_FAILURES, MAX_DELAY};
use hearth_agent::domain::known_address::MAX_PER_ACCOUNT;
use hearth_agent::domain::lockout::{AttemptKey, MAX_TRACKED_ATTEMPTS};
use hearth_agent::domain::login_policy::{MAX_LOGINS_IN_FLIGHT, RESERVED_FOR_KNOWN};
use hearth_agent::domain::secret::Secret;
use hearth_agent::infrastructure::random::OsTokenGen;
use hearth_agent::infrastructure::sqlite::{
    SqliteAccountRepo, SqliteKnownAddressRepo, SqliteLoginAttemptRepo, SqliteSessionRepo,
    SqliteStore,
};
use support::{CLIENT_ADDR, Env, PASSWORD, by, client_at, env, secret};
use time::Duration;
use tokio::sync::{mpsc, watch};

const WRONG: &str = "Wrong-Horse-9999";

/// Une adresse IPv4 neuve par numéro.
fn addr(n: u32) -> String {
    format!("10.1.{}.{}", n / 250, n % 250 + 1)
}

async fn wrong(env: &Env, username: &str, from: &str) -> LoginError {
    env.sessions
        .login(username, secret(WRONG), &client_at(from))
        .await
        .expect_err("mauvais mot de passe")
}

async fn right(env: &Env, username: &str, from: &str) -> Result<LoginOutcome, LoginError> {
    env.sessions
        .login(username, secret(PASSWORD), &client_at(from))
        .await
}

/// Attente annoncée en secondes, si le refus en annonce une.
fn waited(error: &LoginError) -> Option<i64> {
    match error {
        LoginError::TooManyAttempts { retry_after } => Some(retry_after.whole_seconds()),
        _ => None,
    }
}

async fn count(env: &Env, table: &str) -> i64 {
    let sql = match table {
        "known_addresses" => "SELECT COUNT(*) FROM known_addresses",
        "identifier_slowdowns" => "SELECT COUNT(*) FROM identifier_slowdowns",
        "login_attempts" => "SELECT COUNT(*) FROM login_attempts",
        other => panic!("table inattendue {other}"),
    };
    sqlx::query_scalar(sql)
        .fetch_one(env.db.pool())
        .await
        .unwrap()
}

/// Les dix échecs gratuits d'un identifiant, venus de dix adresses différentes.
async fn free_failures(env: &Env, username: &str) {
    for n in 0..FREE_FAILURES {
        let error = wrong(env, username, &addr(n)).await;
        assert!(matches!(error, LoginError::InvalidCredentials), "{error:?}");
    }
}

// ---- critère 1 : l'identifiant attaqué depuis de nombreuses adresses est ralenti

#[tokio::test]
async fn an_identifier_attacked_from_many_addresses_is_slowed_with_a_growing_delay_capped_at_two_minutes()
 {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    free_failures(&env, "marie").await;

    let mut previous = Duration::ZERO;
    for (index, expected) in [2, 4, 8, 16, 32, 64, 120, 120, 120].into_iter().enumerate() {
        env.clock.advance(previous);
        let index = u32::try_from(index).unwrap();
        let error = wrong(&env, "marie", &addr(FREE_FAILURES + index)).await;
        assert_eq!(waited(&error), Some(expected), "échec {}", 11 + index);

        // Pendant l'attente, une adresse encore jamais vue est refusée APRÈS la vérification
        // (même chemin et même durée que l'identifiant existe ou non, BR-CONN-013).
        let verifications = env.hasher.verifications();
        let during = wrong(&env, "marie", &addr(500 + index)).await;
        assert_eq!(waited(&during), Some(expected));
        assert_eq!(env.hasher.verifications(), verifications + 1);
        previous = Duration::seconds(expected);
    }
    assert_eq!(
        previous, MAX_DELAY,
        "le plafond est atteint, jamais dépassé"
    );
}

#[tokio::test]
async fn the_wait_is_never_a_block_a_try_is_always_possible_when_it_ends() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    free_failures(&env, "marie").await;
    // Cent attaques de plus, chacune à la fin de l'attente précédente : jamais plus de 2 minutes.
    let mut wait = Duration::ZERO;
    for n in 0..100 {
        env.clock.advance(wait);
        let error = wrong(&env, "marie", &addr(FREE_FAILURES + n)).await;
        let seconds = waited(&error).expect("une attente");
        assert!(seconds <= MAX_DELAY.whole_seconds(), "{seconds} s");
        wait = Duration::seconds(seconds);
    }
    // Et le bon mot de passe passe à la fin de l'attente, depuis une adresse jamais vue.
    env.clock.advance(wait);
    assert!(right(&env, "marie", "10.9.9.9").await.is_ok());
}

#[tokio::test]
async fn thirty_minutes_without_a_failure_give_the_counter_back() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    free_failures(&env, "marie").await;
    let slowed = wrong(&env, "marie", &addr(10)).await;
    assert!(waited(&slowed).is_some());
    env.clock.advance(identifier_slowdown::RESET_AFTER);
    let error = wrong(&env, "marie", &addr(11)).await;
    assert!(matches!(error, LoginError::InvalidCredentials), "{error:?}");
}

#[tokio::test]
async fn the_slowdown_applies_to_an_identifier_that_does_not_exist_too() {
    let env = env().await;
    free_failures(&env, "fantome").await;
    let error = wrong(&env, "fantome", &addr(10)).await;
    assert_eq!(waited(&error), Some(2));
}

#[tokio::test]
async fn the_slowdown_is_per_identifier_another_account_is_not_slowed() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    env.create("paul", Role::Admin).await;
    free_failures(&env, "marie").await;
    assert!(waited(&wrong(&env, "marie", &addr(10)).await).is_some());
    assert!(right(&env, "paul", &addr(11)).await.is_ok());
}

// ---- critère 2 : l'administrateur sur son poste habituel peut toujours se connecter

#[tokio::test]
async fn the_admin_on_a_known_address_can_always_log_in_during_the_attack() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    right(&env, "marie", CLIENT_ADDR)
        .await
        .expect("première connexion");

    free_failures(&env, "marie").await;
    let mut wait = Duration::ZERO;
    for n in 0..9 {
        env.clock.advance(wait);
        let error = wrong(&env, "marie", &addr(FREE_FAILURES + n)).await;
        wait = Duration::seconds(waited(&error).expect("l'attaque est ralentie"));

        // Au plus fort de l'attente, marie passe depuis son poste habituel.
        assert!(
            right(&env, "marie", CLIENT_ADDR).await.is_ok(),
            "marie est bloquée à l'attente {wait}"
        );
        // Depuis une adresse nouvelle, la même attente que l'attaquant (jamais un blocage).
        let from_new = right(&env, "marie", "10.9.9.9").await.unwrap_err();
        assert_eq!(waited(&from_new), Some(wait.whole_seconds()));
    }
    // Après l'attente, la nouvelle adresse passe aussi, et devient connue.
    env.clock.advance(wait);
    assert!(right(&env, "marie", "10.9.9.9").await.is_ok());
    let fresh = wrong(&env, "marie", &addr(900)).await;
    assert!(waited(&fresh).is_some(), "l'attaque reste ralentie");
    assert!(right(&env, "marie", "10.9.9.9").await.is_ok());
}

#[tokio::test]
async fn ipv6_addresses_of_one_prefix_are_counted_one_by_one_never_as_a_prefix() {
    // Décision du détenteur (HRT-20) : pas de regroupement en /64. Vingt échecs depuis vingt
    // adresses d'un même préfixe ne bloquent aucune adresse du préfixe.
    let env = env().await;
    env.create("marie", Role::Admin).await;
    for n in 0..20 {
        let from = format!("2001:db8:0:1:aaaa::{:x}", n + 1);
        let error = wrong(&env, &format!("inconnu{n}"), &from).await;
        assert!(
            matches!(error, LoginError::InvalidCredentials),
            "{n} : {error:?}"
        );
    }
    assert!(right(&env, "marie", "2001:db8:0:1::6").await.is_ok());
    // Chaque adresse a son propre compteur : vingt adresses, vingt lignes, plus celle de marie.
    let rows: Vec<String> =
        sqlx::query_scalar("SELECT key FROM login_attempts WHERE key LIKE 'addr:%'")
            .fetch_all(env.db.pool())
            .await
            .unwrap();
    assert_eq!(rows.len(), 20, "{rows:?}");
    assert!(rows.iter().all(|key| !key.contains("/64")));
}

#[tokio::test]
async fn failures_from_a_known_address_count_in_the_pair_and_the_address_but_not_in_the_identifier()
{
    // Le couple et l'adresse comptent tout échec (aucune exemption : sinon une adresse connue
    // serait un oracle d'existence, BR-CONN-013). Le ralentissement par identifiant ne compte que
    // les adresses inconnues de tous les comptes (même règle pour un identifiant inexistant).
    let env = env().await;
    env.create("marie", Role::Admin).await;
    right(&env, "marie", CLIENT_ADDR)
        .await
        .expect("première connexion");
    for _ in 0..4 {
        let error = wrong(&env, "marie", CLIENT_ADDR).await;
        assert!(matches!(error, LoginError::InvalidCredentials));
    }
    assert_eq!(count(&env, "identifier_slowdowns").await, 0);
    let rows: Vec<String> = sqlx::query_scalar("SELECT key FROM login_attempts ORDER BY key")
        .fetch_all(env.db.pool())
        .await
        .unwrap();
    assert_eq!(rows.len(), 2, "le couple et l'adresse : {rows:?}");
    assert!(rows.iter().any(|key| key.starts_with("addr:")));
}

// ---- adresse connue usurpée : aucun droit

#[tokio::test]
async fn a_spoofed_known_address_with_a_wrong_password_stays_locked_per_pair() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    right(&env, "marie", CLIENT_ADDR)
        .await
        .expect("première connexion");

    for _ in 0..4 {
        let error = wrong(&env, "marie", CLIENT_ADDR).await;
        assert!(matches!(error, LoginError::InvalidCredentials), "{error:?}");
    }
    let fifth = wrong(&env, "marie", CLIENT_ADDR).await;
    assert_eq!(waited(&fifth), Some(60), "le couple verrouille toujours");
    // Pendant l'attente, même le bon mot de passe est refusé pour ce couple : la connaître ne
    // donne aucun droit.
    let verifications = env.hasher.verifications();
    let blocked = right(&env, "marie", CLIENT_ADDR).await.unwrap_err();
    assert_eq!(waited(&blocked), Some(60));
    assert_eq!(env.hasher.verifications(), verifications);
    // Et personne d'autre n'est gêné : une adresse neuve n'est pas ralentie par ces échecs.
    assert!(right(&env, "marie", "10.9.9.9").await.is_ok());
}

#[tokio::test]
async fn a_known_address_with_a_wrong_password_never_opens_a_session() {
    let env = env().await;
    let marie = env.create("marie", Role::Admin).await;
    right(&env, "marie", CLIENT_ADDR)
        .await
        .expect("première connexion");
    let sessions = env.session_ids(&marie.id).await.len();
    let error = wrong(&env, "marie", CLIENT_ADDR).await;
    assert!(matches!(error, LoginError::InvalidCredentials));
    assert_eq!(env.session_ids(&marie.id).await.len(), sessions);
}

// ---- comment une adresse devient connue, combien, combien de temps

#[tokio::test]
async fn an_address_becomes_known_only_by_a_successful_login_of_that_account() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    env.create("paul", Role::Admin).await;

    // Des échecs n'apprennent rien.
    wrong(&env, "marie", "10.9.9.9").await;
    assert_eq!(count(&env, "known_addresses").await, 0);

    // paul se connecte depuis 10.9.9.9 : l'adresse est connue de paul, pas de marie.
    right(&env, "paul", "10.9.9.9").await.expect("paul");
    assert_eq!(count(&env, "known_addresses").await, 1);
    // Dix échecs de plus depuis des adresses inconnues (le premier était déjà compté) : le
    // ralentissement de « marie » est enclenché.
    for n in 0..FREE_FAILURES {
        wrong(&env, "marie", &addr(n)).await;
    }
    let from_pauls_address = wrong(&env, "marie", "10.9.9.9").await;
    assert!(
        waited(&from_pauls_address).is_some(),
        "connue de paul, pas de marie : elle subit le ralentissement"
    );

    // Une connexion réussie de marie l'apprend, sans doublon.
    env.clock.advance(Duration::seconds(5));
    right(&env, "marie", "10.9.9.9").await.expect("marie");
    right(&env, "marie", "10.9.9.9")
        .await
        .expect("marie encore");
    assert_eq!(count(&env, "known_addresses").await, 2);
}

#[tokio::test]
async fn at_most_eight_known_addresses_per_account_the_oldest_is_forgotten() {
    let env = env().await;
    let marie = env.create("marie", Role::Admin).await;
    for n in 0..=u32::try_from(MAX_PER_ACCOUNT).unwrap() {
        right(&env, "marie", &addr(n)).await.expect("connexion");
        env.clock.advance(Duration::minutes(1));
    }
    let rows: Vec<String> =
        sqlx::query_scalar("SELECT address FROM known_addresses WHERE account_id = ?")
            .bind(marie.id.as_str())
            .fetch_all(env.db.pool())
            .await
            .unwrap();
    assert_eq!(rows.len(), MAX_PER_ACCOUNT);
    assert!(!rows.contains(&addr(0)), "la plus ancienne est oubliée");
    assert!(rows.contains(&addr(1)));
}

#[tokio::test]
async fn a_known_address_is_forgotten_thirty_days_after_its_last_success() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    right(&env, "marie", CLIENT_ADDR).await.expect("connexion");
    // Trente et un jours plus tard (sans nouvelle connexion de marie), l'adresse n'est plus
    // exemptée du ralentissement.
    env.clock.advance(Duration::days(31));
    free_failures(&env, "marie").await;
    let slowed = wrong(&env, "marie", &addr(10)).await;
    assert!(waited(&slowed).is_some());
    let from_old_known = right(&env, "marie", CLIENT_ADDR).await.unwrap_err();
    assert!(
        waited(&from_old_known).is_some(),
        "une adresse non rafraîchie depuis 31 jours n'est plus connue"
    );
    // La purge périodique oublie la ligne.
    let report = env.maintenance.purge().await.unwrap();
    assert_eq!(report.known_addresses, 1);
    assert_eq!(count(&env, "known_addresses").await, 0);
}

#[tokio::test]
async fn password_change_revocation_and_deletion_forget_the_known_addresses() {
    let env = env().await;
    env.create("root2", Role::Admin).await;
    let marie = env.create("marie", Role::Admin).await;
    let known = || count(&env, "known_addresses");

    right(&env, "marie", CLIENT_ADDR).await.expect("connexion");
    assert_eq!(known().await, 1);
    env.service
        .set_password(&marie.id, secret("Nouveau-Mot-De-Passe-77"), by())
        .await
        .unwrap();
    assert_eq!(
        known().await,
        0,
        "mot de passe changé par un administrateur"
    );

    env.service
        .set_password(&marie.id, secret(PASSWORD), by())
        .await
        .unwrap();
    let outcome = right(&env, "marie", CLIENT_ADDR).await.expect("connexion");
    assert_eq!(known().await, 1);
    env.service
        .change_own_password(
            &marie.id,
            secret(PASSWORD),
            secret("Encore-Un-Autre-88"),
            Some(outcome.session_id),
            by(),
        )
        .await
        .unwrap();
    assert_eq!(known().await, 0, "mot de passe changé par son titulaire");

    env.service
        .set_password(&marie.id, secret(PASSWORD), by())
        .await
        .unwrap();
    right(&env, "marie", CLIENT_ADDR).await.expect("connexion");
    assert_eq!(known().await, 1);
    env.service.revoke_sessions(&marie.id, by()).await.unwrap();
    assert_eq!(known().await, 0, "sessions fermées par l'administration");

    right(&env, "marie", CLIENT_ADDR).await.expect("connexion");
    assert_eq!(known().await, 1);
    env.service
        .delete(&marie.id, None, None, by())
        .await
        .unwrap();
    assert_eq!(known().await, 0, "compte supprimé");
}

#[tokio::test]
async fn a_plain_logout_keeps_the_known_address() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    let outcome = right(&env, "marie", CLIENT_ADDR).await.expect("connexion");
    env.sessions
        .logout(&outcome.session_id, by())
        .await
        .unwrap();
    assert_eq!(count(&env, "known_addresses").await, 1);
}

// ---- le flux : adresse déjà connue

#[tokio::test]
async fn an_address_is_known_to_the_stream_with_a_valid_session_or_a_recent_login() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    assert!(!env.sessions.address_is_known("10.0.0.7").await.unwrap());

    let outcome = right(&env, "marie", "10.0.0.7").await.expect("connexion");
    assert!(env.sessions.address_is_known("10.0.0.7").await.unwrap());
    assert!(
        !env.sessions.address_is_known("10.0.0.8").await.unwrap(),
        "une autre adresse"
    );

    // Session fermée : la connexion réussie récente suffit encore, trente jours.
    env.sessions
        .logout(&outcome.session_id, by())
        .await
        .unwrap();
    assert!(env.sessions.address_is_known("10.0.0.7").await.unwrap());
    env.clock.advance(Duration::days(31));
    assert!(!env.sessions.address_is_known("10.0.0.7").await.unwrap());
}

// ---- persistance : le redémarrage n'efface rien

#[tokio::test]
async fn the_slowdown_and_the_known_addresses_survive_a_restart_of_the_agent() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    right(&env, "marie", CLIENT_ADDR).await.expect("connexion");
    free_failures(&env, "marie").await;
    let slowed = wrong(&env, "marie", &addr(10)).await;
    assert_eq!(waited(&slowed), Some(2));

    // Un second service sur la même base : c'est un agent redémarré.
    let pool = env.db.pool().clone();
    let restarted = SessionService::new(
        Arc::new(SqliteAccountRepo::new(pool.clone())),
        Arc::new(SqliteSessionRepo::new(pool.clone())),
        Arc::new(SqliteLoginAttemptRepo::new(pool.clone())),
        Arc::new(SqliteKnownAddressRepo::new(pool.clone())),
        Arc::new(SqliteStore::new(pool)),
        env.hasher.clone(),
        env.clock.clone(),
        Arc::new(support::SequentialIds::starting_at(9_000)),
        Arc::new(OsTokenGen),
        env.trail.clone(),
        env.audit_sink.clone(),
    );
    let still = restarted
        .login("marie", secret(WRONG), &client_at(&addr(11)))
        .await
        .unwrap_err();
    assert_eq!(waited(&still), Some(2), "le ralentissement est gardé");
    assert!(
        restarted
            .login("marie", secret(PASSWORD), &client_at(CLIENT_ADDR))
            .await
            .is_ok(),
        "l'adresse connue est gardée"
    );
}

// ---- BR-CONN-013 : même réponse, même forme, existant ou non

/// Ce que le client voit d'un refus : code HTTP, en-têtes triés, corps, tels que l'agent les écrit.
type Wire = (u16, Vec<(String, String)>, String);
type TraceLine = (String, String, String, u32, String);

async fn on_the_wire(error: LoginError) -> Wire {
    use axum::response::IntoResponse;
    use http_body_util::BodyExt;
    let response = hearth_agent::entrypoint::http::ApiError::from(error).into_response();
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

/// La même suite de tentatives (mêmes adresses, même horloge) contre `username`, qui existe ou non.
struct Run {
    wire: Vec<Wire>,
    displays: Vec<String>,
    verifications: u64,
    against_decoy: u64,
    rows: (i64, i64, i64),
    /// Trace du journal : tout sauf le compte visé (qui n'est renseigné que pour un compte
    /// existant, par BR-AUDIT-006, et lisible de l'administrateur seul).
    trace: Vec<TraceLine>,
    accounts: Vec<Option<String>>,
}

async fn run(username: &str, exists: bool) -> Run {
    let env = env().await;
    if exists {
        env.create(username, Role::Admin).await;
    }
    let mut wire = Vec::new();
    let mut displays = Vec::new();
    let mut wait = Duration::ZERO;
    // 14 adresses : 10 échecs gratuits, l'échec qui ralentit, puis des tentatives refusées sans
    // vérification, puis, l'attente écoulée, la reprise.
    for n in 0..14_u32 {
        if n == 13 {
            env.clock.advance(wait);
        }
        let error = wrong(&env, username, &addr(n)).await;
        if let LoginError::TooManyAttempts { retry_after } = &error {
            wait = *retry_after;
        }
        displays.push(format!("{error:?} | {error}"));
        wire.push(on_the_wire(error).await);
    }
    env.clock.advance(Duration::seconds(61));
    env.audit_recorder.flush().await;
    let filter = hearth_agent::domain::audit::AuditFilter::new(
        hearth_agent::domain::audit::RawFilter::default(),
    )
    .unwrap();
    let records = env
        .audit
        .search(Role::Admin, &filter)
        .await
        .unwrap()
        .records
        .into_iter()
        // La création du compte d'essai n'est pas une tentative de connexion.
        .filter(|record| record.action.starts_with("login"))
        .collect::<Vec<_>>();
    let accounts = records
        .iter()
        .map(|record| record.account.clone())
        .collect();
    let mut trace: Vec<TraceLine> = records
        .into_iter()
        .map(|record| {
            (
                record.action,
                record.outcome.code().to_owned(),
                record.reason.unwrap_or_default(),
                record.repeat_count,
                record.origin_addr.unwrap_or_default(),
            )
        })
        .collect();
    trace.sort();
    Run {
        wire,
        displays,
        verifications: env.hasher.verifications(),
        against_decoy: env.hasher.against_decoy(),
        rows: (
            count(&env, "login_attempts").await,
            count(&env, "identifier_slowdowns").await,
            count(&env, "known_addresses").await,
        ),
        trace,
        accounts,
    }
}

#[tokio::test]
async fn an_existing_and_a_missing_identifier_get_the_same_answers_the_same_path_and_the_same_trace()
 {
    let existing = run("marie", true).await;
    let missing = run("marie", false).await;

    // Mêmes réponses sur le fil : codes, en-têtes, corps, attente annoncée.
    assert_eq!(existing.wire, missing.wire);
    assert_eq!(existing.displays, missing.displays);
    // Le ralentissement s'applique : la suite contient bien des attentes, et une reprise.
    assert!(existing.wire.iter().any(|(status, ..)| *status == 429));
    assert!(existing.wire.iter().any(|(status, ..)| *status == 401));

    // Même chemin : autant de vérifications, toutes contre le haché factice pour l'inconnu, aucune
    // pour l'existant ; mêmes écritures (compteurs, ralentissement, adresses connues).
    assert_eq!(existing.verifications, missing.verifications);
    assert_eq!(existing.against_decoy, 0);
    assert_eq!(missing.against_decoy, missing.verifications);
    assert_eq!(existing.rows, missing.rows);
    assert_eq!(
        existing.rows.1, 1,
        "une ligne de ralentissement dans les deux cas"
    );

    // Même forme de trace : mêmes actions, résultats, raisons, comptes de répétition, origines.
    assert_eq!(existing.trace, missing.trace);
    // Seule différence, voulue (BR-AUDIT-006) : le compte visé n'est nommé que s'il existe.
    assert!(
        existing
            .accounts
            .iter()
            .all(|account| account.as_deref() == Some("marie"))
    );
    assert!(missing.accounts.iter().all(Option::is_none));
}

#[tokio::test]
async fn an_unknown_identifier_is_looked_up_with_the_same_query_as_a_known_one() {
    // La lecture des adresses connues d'un identifiant inexistant rend une liste vide, par la
    // même requête (jointure) : aucun chemin ne dépend de l'existence du compte.
    let env = env().await;
    env.create("marie", Role::Admin).await;
    right(&env, "marie", CLIENT_ADDR).await.expect("connexion");
    let repo = SqliteKnownAddressRepo::new(env.db.pool().clone());
    assert_eq!(repo.of_username("marie").await.unwrap().len(), 1);
    assert_eq!(repo.of_username("MARIE").await.unwrap().len(), 1);
    assert!(repo.of_username("fantome").await.unwrap().is_empty());
}

// ---- plafonds et tables bornées

/// Hacheur dont la vérification attend qu'on l'ouvre : aucun `sleep`, la file se remplit puis se
/// libère à la demande du test.
struct GatedHasher {
    inner: Arc<support::CountingHasher>,
    open: watch::Receiver<bool>,
    entered: mpsc::UnboundedSender<()>,
}

#[async_trait]
impl PasswordHasher for GatedHasher {
    async fn hash(&self, password: &PlainPassword) -> Result<Secret, HashError> {
        self.inner.hash(password).await
    }

    async fn verify(&self, password: &Secret, hash: &Secret) -> Result<bool, HashError> {
        self.entered.send(()).ok();
        let mut open = self.open.clone();
        open.wait_for(|opened| *opened).await.ok();
        self.inner.verify(password, hash).await
    }

    fn decoy_hash(&self) -> &Secret {
        self.inner.decoy_hash()
    }
}

#[tokio::test]
async fn unknown_addresses_never_take_the_places_reserved_for_known_ones() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    // Huit postes habituels de marie, connus.
    for n in 0..8 {
        right(&env, "marie", &addr(1_000 + n))
            .await
            .expect("connexion");
    }
    let (open_tx, open_rx) = watch::channel(false);
    let (entered_tx, mut entered) = mpsc::unbounded_channel();
    let pool = env.db.pool().clone();
    let gated = Arc::new(SessionService::new(
        Arc::new(SqliteAccountRepo::new(pool.clone())),
        Arc::new(SqliteSessionRepo::new(pool.clone())),
        Arc::new(SqliteLoginAttemptRepo::new(pool.clone())),
        Arc::new(SqliteKnownAddressRepo::new(pool.clone())),
        Arc::new(SqliteStore::new(pool)),
        Arc::new(GatedHasher {
            inner: env.hasher.clone(),
            open: open_rx,
            entered: entered_tx,
        }),
        env.clock.clone(),
        Arc::new(support::SequentialIds::starting_at(50_000)),
        Arc::new(OsTokenGen),
        env.trail.clone(),
        env.audit_sink.clone(),
    ));

    let unknown_ceiling = MAX_LOGINS_IN_FLIGHT - RESERVED_FOR_KNOWN;
    let mut unknown = Vec::new();
    for n in 0..unknown_ceiling {
        let service = gated.clone();
        unknown.push(tokio::spawn(async move {
            service
                .login(
                    &format!("inconnu{n}"),
                    secret(WRONG),
                    &client_at(&addr(2_000 + u32::try_from(n).unwrap())),
                )
                .await
        }));
    }
    for _ in 0..unknown_ceiling {
        entered.recv().await.expect("une vérification commence");
    }

    // Les places qui restent sont celles des adresses connues : une inconnue de plus est refusée
    // tout de suite (le plafond des inconnues est atteint), sans mot de passe gardé.
    for n in 0..RESERVED_FOR_KNOWN {
        let error = gated
            .login(
                &format!("encore{n}"),
                secret(WRONG),
                &client_at(&addr(3_000 + u32::try_from(n).unwrap())),
            )
            .await
            .unwrap_err();
        assert!(matches!(error, LoginError::Busy), "{error:?}");
    }

    // Huit postes habituels de marie prennent les huit places réservées.
    let mut known = Vec::new();
    for n in 0..RESERVED_FOR_KNOWN {
        let service = gated.clone();
        known.push(tokio::spawn(async move {
            service
                .login(
                    "marie",
                    secret(PASSWORD),
                    &client_at(&addr(1_000 + u32::try_from(n).unwrap())),
                )
                .await
        }));
    }
    for _ in 0..RESERVED_FOR_KNOWN {
        entered.recv().await.expect("une vérification commence");
    }
    // Le plafond total est atteint : même un poste habituel de plus est refusé.
    let full = gated
        .login("marie", secret(PASSWORD), &client_at(&addr(1_000)))
        .await
        .unwrap_err();
    assert!(matches!(full, LoginError::Busy), "{full:?}");

    // On libère le hacheur : tout le monde est traité, les postes habituels sont connectés.
    open_tx.send(true).unwrap();
    for task in unknown {
        let error = task.await.unwrap().unwrap_err();
        assert!(matches!(error, LoginError::InvalidCredentials), "{error:?}");
    }
    for task in known {
        assert!(
            task.await.unwrap().is_ok(),
            "un poste habituel est connecté"
        );
    }
}

#[tokio::test]
async fn the_identifier_table_is_bounded_and_keeps_the_identifier_really_attacked() {
    let env = env().await;
    // 10 000 identifiants inventés (un échec chacun), puis l'identifiant réellement attaqué.
    sqlx::query(
        "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 10000)
         INSERT INTO identifier_slowdowns (key, failures, wait_until, last_failure_at)
         SELECT 'ident:flood' || i, 1, NULL, '2020-01-01T00:00:00.000Z' FROM n",
    )
    .execute(env.db.pool())
    .await
    .unwrap();
    let store = SqliteStore::new(env.db.pool().clone());
    let attacked = AttemptKey::identifier("marie");
    let now = env.clock.now();
    let state = identifier_slowdown::Slowdown {
        failures: 50,
        wait_until: Some(now + Duration::seconds(30)),
        last_failure_at: Some(now),
    };
    let mut tx = store.begin().await.unwrap();
    tx.login_attempts()
        .save_identifier(&attacked, &state)
        .await
        .unwrap();
    tx.login_attempts().trim_identifiers(now).await.unwrap();
    // Un flot de nouveaux identifiants évince ses propres lignes.
    for n in 0..5 {
        let key = AttemptKey::identifier(&format!("nouveau{n}"));
        let one = identifier_slowdown::Slowdown {
            failures: 1,
            wait_until: None,
            last_failure_at: Some(now),
        };
        tx.login_attempts()
            .save_identifier(&key, &one)
            .await
            .unwrap();
        tx.login_attempts().trim_identifiers(now).await.unwrap();
    }
    let kept = tx.login_attempts().identifier(&attacked).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        kept.failures, 50,
        "l'identifiant attaqué n'est jamais évincé"
    );
    assert_eq!(
        count(&env, "identifier_slowdowns").await,
        i64::try_from(identifier_slowdown::MAX_TRACKED).unwrap()
    );
}

#[tokio::test]
async fn the_attempts_table_is_bounded_and_keeps_the_waits_in_progress() {
    let env = env().await;
    let max = i64::try_from(MAX_TRACKED_ATTEMPTS).unwrap();
    sqlx::query(
        "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 50005)
         INSERT INTO login_attempts (key, failures, locked_until, updated_at)
         SELECT 'flood' || i, 1, NULL, '2020-01-01T00:00:00.000Z' FROM n",
    )
    .execute(env.db.pool())
    .await
    .unwrap();
    sqlx::query(
        "UPDATE login_attempts SET failures = 9, locked_until = '2999-01-01T00:00:00.000Z'
         WHERE key = 'flood1'",
    )
    .execute(env.db.pool())
    .await
    .unwrap();
    let store = SqliteStore::new(env.db.pool().clone());
    let mut tx = store.begin().await.unwrap();
    let removed = tx.login_attempts().trim(env.clock.now()).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(removed, 5);
    assert_eq!(count(&env, "login_attempts").await, max);
    let still: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM login_attempts WHERE key = 'flood1'")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert_eq!(
        still, 1,
        "une attente en cours n'est pas évincée en premier"
    );
}

// ---- migration : une base déjà installée

#[tokio::test]
async fn the_migration_keeps_the_addresses_of_the_open_sessions_of_an_installed_database() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    for script in [
        include_str!("../migrations/0001_accounts_sessions_meta.sql"),
        include_str!("../migrations/0002_login_attempts_operations.sql"),
        include_str!("../migrations/0003_audit_events.sql"),
    ] {
        sqlx::raw_sql(script).execute(&pool).await.unwrap();
    }
    sqlx::query(
        "INSERT INTO accounts (id, username, password_hash, role, created_at, password_changed_at)
         VALUES ('A1', 'marie', 'x', 'admin', '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    // Dix sessions ouvertes de dix adresses (de plus en plus récentes), une expirée.
    for n in 0..10 {
        sqlx::query(
            "INSERT INTO sessions (id, account_id, token_hash, client_name, client_addr, created_at,
                                   last_seen_at, expires_at)
             VALUES (?, 'A1', ?, 'poste', ?, ?, ?, '2999-01-01T00:00:00.000Z')",
        )
        .bind(format!("S{n}"))
        .bind(format!("hash{n}"))
        .bind(format!("10.0.0.{}", n + 1))
        .bind(format!("2026-10-0{}T10:00:00.000Z", n % 9 + 1))
        .bind("2026-10-06T10:00:00.000Z")
        .execute(&pool)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO sessions (id, account_id, token_hash, client_name, client_addr, created_at,
                               last_seen_at, expires_at)
         VALUES ('SX', 'A1', 'hashx', 'poste', '10.0.0.99', '2026-10-06T10:00:00.000Z',
                 '2026-10-06T10:00:00.000Z', '2000-01-01T00:00:00.000Z')",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::raw_sql(include_str!(
        "../migrations/0004_known_addresses_identifier_slowdowns.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();

    let known: Vec<String> =
        sqlx::query_scalar("SELECT address FROM known_addresses ORDER BY address")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(known.len(), 8, "au plus 8 par compte : {known:?}");
    assert!(!known.contains(&"10.0.0.99".to_owned()), "session expirée");
    let slowdowns: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM identifier_slowdowns")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(slowdowns, 0);
}

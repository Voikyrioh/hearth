//! Le mode attaque côté serveur, au niveau des cas d'usage (HRT-25, ADR-0025, BR-TRUST-011 à 021, 027,
//! 030 à 032) : la règle de reconnaissance en mode attaque, l'essai unique, l'anti-enfermement, la
//! sortie automatique, la fenêtre de redémarrage, l'absence d'oracle. Vraie base SQLite temporaire,
//! vraies signatures Ed25519, temps contrôlé (horloge murale, horloge monotone et temps écoulé depuis
//! le démarrage sont trois horloges de test, aucun `sleep`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use axum::response::IntoResponse;
use hearth_agent::application::attack_mode::{AttackModeService, AttackStatus};
use hearth_agent::application::sessions::{AuthError, LoginError, LoginOutcome};
use hearth_agent::domain::accounts::Role;
use hearth_agent::domain::trust::attack_mode::{Effective, EndHow};
use hearth_agent::entrypoint::http::ApiError;
use http_body_util::BodyExt;
use support::device::DeviceKey;
use support::{CLIENT_ADDR, Env, PASSWORD, by, client_at, env, secret};
use time::Duration;

const WRONG: &str = "Wrong-Horse-9999";

/// Une adresse inconnue de tous les comptes, la `n`-ième.
fn stranger(n: u32) -> String {
    format!("10.1.{}.{}", n / 250, n % 250 + 1)
}

async fn plain(
    env: &Env,
    user: &str,
    password: &str,
    from: &str,
) -> Result<LoginOutcome, LoginError> {
    env.sessions
        .login(user, secret(password), &client_at(from))
        .await
}

async fn keyed(
    env: &Env,
    key: &DeviceKey,
    user: &str,
    password: &str,
    from: &str,
) -> Result<LoginOutcome, LoginError> {
    let proof = key.login_proof(env, user, from);
    env.sessions
        .login_with_device(user, secret(password), &client_at(from), Some(&proof))
        .await
}

async fn scalar(env: &Env, sql: &'static str) -> i64 {
    sqlx::query_scalar(sql)
        .fetch_one(env.db.pool())
        .await
        .unwrap()
}

async fn enable(env: &Env) -> AttackStatus {
    env.attack
        .change(true, by(), EndHow::Manual)
        .await
        .expect("activation")
}

async fn disable(env: &Env) -> AttackStatus {
    env.attack
        .change(false, by(), EndHow::Manual)
        .await
        .expect("désactivation")
}

async fn state(env: &Env) -> Effective {
    env.attack.effective().await.unwrap()
}

async fn activation_id(env: &Env) -> Option<String> {
    sqlx::query_scalar("SELECT activation_id FROM attack_mode WHERE id = 1")
        .fetch_one(env.db.pool())
        .await
        .unwrap()
}

/// Le temps passe : l'horloge murale, l'horloge monotone de l'agent et le temps écoulé depuis le
/// démarrage du noyau avancent ensemble.
fn pass(env: &Env, by: Duration) {
    env.clock.advance(by);
    env.monotonic.advance(by);
    env.boot.set_uptime(env.boot.uptime() + by);
}

/// Un environnement dont le service a déjà noté le démarrage du noyau (`boot-1`, dix heures
/// d'activité) : comme un agent qui tourne depuis un moment.
async fn booted() -> Env {
    let env = env().await;
    assert!(!env.attack.on_start().await.unwrap());
    env
}

/// Le « redémarrage de la machine » : un autre démarrage du noyau, juste commencé, puis le service
/// repart (c'est lui qui lit l'identifiant au lancement).
async fn reboot(env: &Env, boot_id: &str, uptime: Duration) -> bool {
    env.boot.set_id(Some(boot_id));
    env.boot.set_uptime(uptime);
    env.attack.on_start().await.unwrap()
}

/// Les entrées du journal : (action, résultat, origine, raison).
async fn journal(env: &Env) -> Vec<(String, String, String, Option<String>)> {
    env.audit_recorder.flush_all().await;
    sqlx::query_as(
        "SELECT action, outcome, origin_kind, reason FROM audit_events
         WHERE action NOT IN ('account.create', 'login', 'device.enroll') ORDER BY id",
    )
    .fetch_all(env.db.pool())
    .await
    .unwrap()
}

async fn count_action(env: &Env, action: &str) -> usize {
    journal(env)
        .await
        .iter()
        .filter(|entry| entry.0 == action)
        .count()
}

/// `marie` (administratrice) a un poste inscrit (la clé rendue) et une adresse retenue (`CLIENT_ADDR`).
async fn marie_with_key(env: &Env) -> DeviceKey {
    env.create("marie", Role::Admin).await;
    let key = DeviceKey::new();
    keyed(env, &key, "marie", PASSWORD, CLIENT_ADDR)
        .await
        .expect("connexion et inscription de marie");
    key
}

/// `paul` n'a QUE son adresse retenue (`CLIENT_ADDR`), pas de clé.
async fn paul_with_address(env: &Env) {
    env.create("paul", Role::Admin).await;
    plain(env, "paul", PASSWORD, CLIENT_ADDR)
        .await
        .expect("connexion de paul");
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

fn refused(result: &Result<LoginOutcome, LoginError>) -> bool {
    matches!(result, Err(LoginError::InvalidCredentials))
}

/// Refusé, par un mot de passe faux ou par l'attente qu'il a déclenchée : jamais connecté.
fn denied(result: &Result<LoginOutcome, LoginError>) -> bool {
    matches!(
        result,
        Err(LoginError::InvalidCredentials | LoginError::TooManyAttempts { .. })
    )
}

// ---------------------------------------------------------------------------------------------
// Enfermement : ce qui passe TOUJOURS
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn an_administrator_with_key_and_address_always_passes_even_under_attack_and_after_a_typo() {
    let env = booted().await;
    let key = marie_with_key(&env).await;
    enable(&env).await;
    // L'identifiant de marie est en plus visé par 11 échecs d'adresses inconnues (alerte).
    for n in 0..11 {
        let _ = plain(&env, "marie", WRONG, &stranger(n)).await;
    }
    for round in 0..3 {
        let ok = keyed(&env, &key, "marie", PASSWORD, CLIENT_ADDR).await;
        assert!(ok.is_ok(), "tour {round} : {:?}", ok.err());
    }
    // Une faute de frappe depuis son poste n'ôte rien : deux critères, aucun essai consommé.
    let typo = keyed(&env, &key, "marie", WRONG, CLIENT_ADDR).await;
    assert!(refused(&typo) || matches!(typo, Err(LoginError::TooManyAttempts { .. })));
    let ok = keyed(&env, &key, "marie", PASSWORD, CLIENT_ADDR).await;
    assert!(ok.is_ok(), "{:?}", ok.err());
    assert_eq!(
        scalar(&env, "SELECT COUNT(*) FROM attack_trials").await,
        0,
        "deux critères : aucun essai n'est consommé"
    );
}

#[tokio::test]
async fn a_key_alone_after_an_address_change_has_exactly_one_trial() {
    let env = booted().await;
    let key = marie_with_key(&env).await;
    enable(&env).await;
    let first = keyed(&env, &key, "marie", PASSWORD, "10.5.5.5").await;
    assert!(first.is_ok(), "{:?}", first.err());
    let trial: (String, String) = sqlx::query_as("SELECT kind, outcome FROM attack_trials")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert_eq!(trial, ("key".into(), "succeeded".into()));
    // La nouvelle adresse est apprise : le poste a de nouveau deux critères.
    assert!(
        scalar(
            &env,
            "SELECT COUNT(*) FROM known_addresses WHERE address = '10.5.5.5'"
        )
        .await
            == 1
    );
    let again = keyed(&env, &key, "marie", PASSWORD, "10.5.5.5").await;
    assert!(again.is_ok(), "adresse retenue + clé : deux critères");
    // Une autre adresse encore, la clé seule : un essai RÉUSSI ne consomme rien (BR-TRUST-014), le
    // poste est encore reconnu à sa prochaine connexion.
    let third = keyed(&env, &key, "marie", PASSWORD, "10.6.6.6").await;
    assert!(third.is_ok(), "{:?}", third.err());
    // Seul un mot de passe faux, ensuite, depuis une clé seule marque l'essai raté (le succès d'avant devient
    // un échec) : la clé est alors bloquée jusqu'à la fin du mode.
    let miss = keyed(&env, &key, "marie", WRONG, "10.7.7.7").await;
    assert!(refused(&miss));
    let blocked = keyed(&env, &key, "marie", PASSWORD, "10.8.8.8").await;
    assert!(
        refused(&blocked),
        "un seul essai raté par critère et par activation"
    );
}

#[tokio::test]
async fn a_key_alone_that_misses_its_trial_is_blocked_until_the_end_of_the_mode_even_with_the_right_password()
 {
    let env = booted().await;
    let key = marie_with_key(&env).await;
    enable(&env).await;
    let miss = keyed(&env, &key, "marie", WRONG, "10.5.5.5").await;
    assert!(refused(&miss));
    for from in ["10.5.5.5", "10.6.6.6"] {
        let right = keyed(&env, &key, "marie", PASSWORD, from).await;
        assert!(refused(&right), "{from} : bloqué jusqu'à la fin du mode");
    }
    disable(&env).await;
    assert!(
        keyed(&env, &key, "marie", PASSWORD, "10.6.6.6")
            .await
            .is_ok(),
        "à la fin du mode, le chemin ordinaire"
    );
}

#[tokio::test]
async fn a_corrupt_active_row_without_an_activation_gives_no_trial_never_one_for_free() {
    let env = booted().await;
    paul_with_address(&env).await;
    // Un mode actif sans identifiant d'activation n'existe pas : si la ligne l'était, l'essai ne serait
    // jamais donné (fermé, jamais ouvert).
    sqlx::query("UPDATE attack_mode SET active = 1 WHERE id = 1")
        .execute(env.db.pool())
        .await
        .unwrap();
    assert!(refused(&plain(&env, "paul", PASSWORD, CLIENT_ADDR).await));
    assert_eq!(scalar(&env, "SELECT COUNT(*) FROM attack_trials").await, 0);
}

#[tokio::test]
async fn an_address_alone_without_key_has_exactly_one_trial() {
    let env = booted().await;
    paul_with_address(&env).await;
    enable(&env).await;
    let first = plain(&env, "paul", PASSWORD, CLIENT_ADDR).await;
    assert!(first.is_ok(), "{:?}", first.err());
    let trial: (String, String) = sqlx::query_as("SELECT kind, outcome FROM attack_trials")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert_eq!(trial, ("address".into(), "succeeded".into()));
    // Un essai réussi ne consomme rien (BR-TRUST-014) : le titulaire qui se reconnecte pendant la même
    // activation (session perdue) entre encore.
    let second = plain(&env, "paul", PASSWORD, CLIENT_ADDR).await;
    assert!(second.is_ok(), "{:?}", second.err());
    let still: (String,) = sqlx::query_as("SELECT outcome FROM attack_trials")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert_eq!(still.0, "succeeded");
    // La session ouverte par l'essai fonctionne (session + adresse retenue).
    let token = first.unwrap().token.encode();
    assert!(
        env.sessions
            .authenticate_at(&token, CLIENT_ADDR)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn a_wrong_password_on_the_single_trial_blocks_the_post_and_the_right_one_no_longer_counts() {
    let env = booted().await;
    paul_with_address(&env).await;
    enable(&env).await;
    assert!(refused(&plain(&env, "paul", WRONG, CLIENT_ADDR).await));
    let trial: (String,) = sqlx::query_as("SELECT outcome FROM attack_trials")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert_eq!(trial.0, "failed");
    assert!(refused(&plain(&env, "paul", PASSWORD, CLIENT_ADDR).await));
}

#[tokio::test]
async fn a_post_without_any_criterion_is_blocked_without_a_trial_even_with_the_right_password() {
    let env = booted().await;
    paul_with_address(&env).await;
    enable(&env).await;
    for n in 0..4 {
        assert!(refused(&plain(&env, "paul", PASSWORD, &stranger(n)).await));
    }
    assert_eq!(scalar(&env, "SELECT COUNT(*) FROM attack_trials").await, 0);
    // Un identifiant qui n'existe pas : le même refus, aucune écriture d'essai.
    assert!(refused(&plain(&env, "ghost", PASSWORD, &stranger(9)).await));
    assert_eq!(scalar(&env, "SELECT COUNT(*) FROM attack_trials").await, 0);
}

// ---------------------------------------------------------------------------------------------
// Enfermement : les trois sorties quand plus aucun poste n'est reconnu
// ---------------------------------------------------------------------------------------------

/// `marie` (administratrice, SANS clé) n'a que son adresse retenue ; le mode attaque est actif et un
/// attaquant qui usurpe cette adresse a brûlé son essai : plus aucun poste n'est reconnu.
async fn locked_out() -> Env {
    let env = booted().await;
    env.create("marie", Role::Admin).await;
    plain(&env, "marie", PASSWORD, CLIENT_ADDR)
        .await
        .expect("connexion de marie");
    enable(&env).await;
    assert!(
        refused(&plain(&env, "marie", WRONG, CLIENT_ADDR).await),
        "l'essai est brûlé"
    );
    assert!(
        refused(&plain(&env, "marie", PASSWORD, CLIENT_ADDR).await),
        "marie est dehors : son poste n'a plus d'essai"
    );
    env
}

#[tokio::test]
async fn lockout_exit_1_the_mode_ends_by_itself_after_thirty_quiet_minutes() {
    let env = locked_out().await;
    // Marie n'insiste pas (chacune de ses tentatives refusées repousserait la sortie).
    pass(&env, Duration::minutes(29) + Duration::seconds(59));
    assert!(!env.attack.sweep().await.unwrap().auto_disabled);
    assert_eq!(state(&env).await, Effective::Active);
    pass(&env, Duration::seconds(2));
    assert!(env.attack.sweep().await.unwrap().auto_disabled);
    assert_eq!(state(&env).await, Effective::Off);
    assert!(plain(&env, "marie", PASSWORD, CLIENT_ADDR).await.is_ok());
    let entries = journal(&env).await;
    let auto: Vec<_> = entries
        .iter()
        .filter(|entry| entry.0 == "attack_mode.auto_disable")
        .collect();
    assert_eq!(auto.len(), 1);
    assert_eq!((auto[0].1.as_str(), auto[0].2.as_str()), ("ok", "system"));
    let status = env.attack.status().await.unwrap();
    assert_eq!(status.last_end, Some(EndHow::Auto));
}

#[tokio::test]
async fn lockout_exit_2_the_local_command_ends_the_mode_without_the_network() {
    use hearth_agent::entrypoint::attack_mode::{MSG_OFF, execute};
    use hearth_agent::entrypoint::cli::AttackModeAction;
    let env = locked_out().await;
    let mut out = Vec::new();
    execute(&AttackModeAction::Off, &env.attack, &mut out)
        .await
        .unwrap();
    assert_eq!(String::from_utf8(out).unwrap().trim(), MSG_OFF);
    assert_eq!(state(&env).await, Effective::Off);
    assert!(plain(&env, "marie", PASSWORD, CLIENT_ADDR).await.is_ok());
    let entries = journal(&env).await;
    let off: Vec<_> = entries
        .iter()
        .filter(|entry| entry.0 == "attack_mode.disable")
        .collect();
    assert_eq!(off.len(), 1);
    assert_eq!((off[0].1.as_str(), off[0].2.as_str()), ("ok", "cli"));
    assert_eq!(
        env.attack.status().await.unwrap().last_end,
        Some(EndHow::Cli)
    );
    // Une deuxième fois : rien n'était actif, rien n'est écrit.
    let mut out = Vec::new();
    execute(&AttackModeAction::Off, &env.attack, &mut out)
        .await
        .unwrap();
    assert!(
        String::from_utf8(out)
            .unwrap()
            .contains("n'était pas actif")
    );
    assert_eq!(count_action(&env, "attack_mode.disable").await, 1);
}

#[tokio::test]
async fn lockout_exit_3_a_physical_reboot_suspends_the_mode_and_the_right_password_passes() {
    let env = locked_out().await;
    assert!(reboot(&env, "boot-2", Duration::minutes(1)).await);
    assert!(matches!(state(&env).await, Effective::Suspended { .. }));
    // Régime d'alerte : personne n'est bloqué, le titulaire du mot de passe passe.
    assert!(plain(&env, "marie", PASSWORD, CLIENT_ADDR).await.is_ok());
    assert!(plain(&env, "marie", PASSWORD, &stranger(3)).await.is_ok());
    let entries = journal(&env).await;
    assert_eq!(
        entries
            .iter()
            .filter(|entry| entry.0 == "attack_mode.suspend")
            .map(|entry| entry.2.as_str())
            .collect::<Vec<_>>(),
        vec!["system"]
    );
}

// ---------------------------------------------------------------------------------------------
// Enfermement : service redémarré, horloge, identifiant de démarrage, base restaurée
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_service_restarted_in_a_loop_gives_no_trial_back_and_opens_no_window() {
    let env = locked_out().await;
    let id = activation_id(&env).await;
    // Le service redémarre en boucle (un attaquant qui le plante, ou une mise à jour de l'agent) : le
    // démarrage du noyau n'a pas changé. Un autre processus (une autre instance du service sur la même
    // base) fait de même.
    let other = AttackModeService::new(
        std::sync::Arc::new(
            hearth_agent::infrastructure::sqlite::SqliteAttackModeRepo::new(env.db.pool().clone()),
        ),
        std::sync::Arc::new(hearth_agent::infrastructure::sqlite::SqliteStore::new(
            env.db.pool().clone(),
        )),
        env.clock.clone(),
        env.monotonic.clone(),
        env.boot.clone(),
        std::sync::Arc::new(support::SequentialIds::starting_at(50_000)),
        env.trail.clone(),
        std::sync::Arc::new(
            hearth_agent::infrastructure::security_feed::BroadcastSecurityFeed::new(),
        ),
    );
    for _ in 0..6 {
        assert!(!env.attack.on_start().await.unwrap());
        assert!(!other.on_start().await.unwrap());
        pass(&env, Duration::seconds(5));
    }
    assert_eq!(state(&env).await, Effective::Active);
    assert_eq!(activation_id(&env).await, id);
    assert_eq!(scalar(&env, "SELECT COUNT(*) FROM attack_trials").await, 1);
    assert_eq!(count_action(&env, "attack_mode.suspend").await, 0);
    assert!(refused(&plain(&env, "marie", PASSWORD, CLIENT_ADDR).await));
}

#[tokio::test]
async fn the_wall_clock_set_back_or_forward_never_moves_the_thirty_minutes() {
    let env = booted().await;
    paul_with_address(&env).await;
    enable(&env).await;
    // Horloge murale avancée d'un an : rien ne sort.
    env.clock.advance(Duration::days(365));
    env.monotonic.advance(Duration::minutes(10));
    assert!(!env.attack.sweep().await.unwrap().auto_disabled);
    // Reculée de deux ans : la sortie automatique reste décidée par l'horloge monotone seule.
    env.clock.advance(Duration::days(-730));
    env.monotonic.advance(Duration::minutes(19));
    assert!(!env.attack.sweep().await.unwrap().auto_disabled);
    env.monotonic.advance(Duration::minutes(2));
    assert!(env.attack.sweep().await.unwrap().auto_disabled);
    assert_eq!(state(&env).await, Effective::Off);
}

#[tokio::test]
async fn the_window_is_measured_on_the_uptime_not_on_the_wall_clock() {
    let env = booted().await;
    enable(&env).await;
    assert!(reboot(&env, "boot-2", Duration::minutes(1)).await);
    let Effective::Suspended { remaining } = state(&env).await else {
        panic!("suspendu attendu")
    };
    assert_eq!(remaining, Duration::minutes(29));
    for skew in [
        Duration::days(-365),
        Duration::days(730),
        Duration::days(-365),
    ] {
        env.clock.advance(skew);
        assert_eq!(
            state(&env).await,
            Effective::Suspended { remaining },
            "horloge murale décalée de {skew}"
        );
    }
    env.boot.set_uptime(Duration::minutes(30));
    assert_eq!(state(&env).await, Effective::Active);
}

#[tokio::test]
async fn an_unreadable_boot_identifier_never_opens_a_window_and_never_closes_the_mode() {
    let env = booted().await;
    enable(&env).await;
    env.boot.set_id(None);
    env.boot.set_uptime(Duration::minutes(1));
    assert!(!env.attack.on_start().await.unwrap());
    assert_eq!(state(&env).await, Effective::Active);
    // Il redevient lisible, c'est un autre démarrage : la fenêtre s'ouvre alors normalement.
    assert!(reboot(&env, "boot-9", Duration::minutes(1)).await);
}

#[tokio::test]
async fn a_backup_restored_while_the_mode_was_active_neither_locks_nor_silently_ends_the_mode() {
    let env = env().await;
    env.create("marie", Role::Admin).await;
    // Base restaurée : le mode était actif, le démarrage noté est celui d'avant.
    sqlx::query(
        "UPDATE attack_mode SET active = 1, activation_id = 'OLD', activated_at = '2026-10-01T10:00:00.000Z',
                activated_by = 'marie', last_boot_id = 'boot-before'
         WHERE id = 1",
    )
    .execute(env.db.pool())
    .await
    .unwrap();
    // La machine tourne depuis dix heures : pas de fenêtre, le mode reste actif, la sortie existe.
    assert!(!env.attack.on_start().await.unwrap());
    assert_eq!(state(&env).await, Effective::Active);
    // Un démarrage tout juste fait : la fenêtre (le régime d'alerte, qui n'enferme personne) s'ouvre.
    sqlx::query("UPDATE attack_mode SET last_boot_id = 'boot-before' WHERE id = 1")
        .execute(env.db.pool())
        .await
        .unwrap();
    env.boot.set_uptime(Duration::minutes(2));
    assert!(env.attack.on_start().await.unwrap());
    assert!(matches!(state(&env).await, Effective::Suspended { .. }));
    // Dans tous les cas, la voie de secours existe.
    assert!(env.attack.disable_from_cli().await.unwrap());
    assert_eq!(state(&env).await, Effective::Off);
}

// ---------------------------------------------------------------------------------------------
// Essais : le compte exact
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn an_attacker_spoofing_the_eight_retained_addresses_obtains_exactly_eight_trials_and_not_one_more()
 {
    let env = booted().await;
    env.create("marie", Role::Admin).await;
    // Huit adresses retenues, la limite d'un compte.
    let retained: Vec<String> = (0..8).map(|n| format!("10.20.0.{}", n + 1)).collect();
    for from in &retained {
        plain(&env, "marie", PASSWORD, from)
            .await
            .expect("connexion");
    }
    assert_eq!(
        scalar(&env, "SELECT COUNT(*) FROM known_addresses").await,
        8
    );
    enable(&env).await;
    // L'attaquant usurpe chaque adresse retenue et devine : trois tentatives par adresse.
    for from in &retained {
        for _ in 0..3 {
            assert!(refused(&plain(&env, "marie", WRONG, from).await));
        }
    }
    assert_eq!(
        scalar(
            &env,
            "SELECT COUNT(*) FROM attack_trials WHERE outcome = 'failed'"
        )
        .await,
        8,
        "un essai par adresse retenue, pas un de plus"
    );
    // Puis avec le BON mot de passe, depuis les mêmes adresses et depuis d'autres : aucun ne compte.
    for from in &retained {
        assert!(
            refused(&plain(&env, "marie", PASSWORD, from).await),
            "{from}"
        );
    }
    for n in 0..5 {
        assert!(refused(&plain(&env, "marie", PASSWORD, &stranger(n)).await));
    }
    assert_eq!(scalar(&env, "SELECT COUNT(*) FROM attack_trials").await, 8);
    assert_eq!(
        scalar(&env, "SELECT COUNT(*) FROM sessions").await,
        8,
        "aucune session de plus que les huit d'avant l'activation"
    );
}

#[tokio::test]
async fn without_spoofing_an_attacker_obtains_zero_trial() {
    let env = booted().await;
    marie_with_key(&env).await;
    enable(&env).await;
    for n in 0..20 {
        assert!(denied(&plain(&env, "marie", PASSWORD, &stranger(n)).await));
        assert!(denied(&plain(&env, "marie", WRONG, &stranger(n)).await));
    }
    assert_eq!(scalar(&env, "SELECT COUNT(*) FROM attack_trials").await, 0);
}

#[tokio::test]
async fn rapid_activation_cycles_give_no_trial_back_until_thirty_minutes_after_the_end() {
    let env = booted().await;
    paul_with_address(&env).await;
    enable(&env).await;
    let first_id = activation_id(&env).await;
    assert!(refused(&plain(&env, "paul", WRONG, CLIENT_ADDR).await));
    for cycle in 0..5 {
        disable(&env).await;
        pass(&env, Duration::minutes(2));
        enable(&env).await;
        assert_eq!(
            activation_id(&env).await,
            first_id,
            "cycle {cycle} : même activation"
        );
        assert!(
            denied(&plain(&env, "paul", PASSWORD, CLIENT_ADDR).await),
            "cycle {cycle} : l'essai n'est pas rendu"
        );
        assert_eq!(scalar(&env, "SELECT COUNT(*) FROM attack_trials").await, 1);
    }
    // 29 minutes 59 après la fin : encore la même activation. 30 minutes : une neuve.
    disable(&env).await;
    pass(&env, Duration::minutes(29) + Duration::seconds(59));
    enable(&env).await;
    assert_eq!(activation_id(&env).await, first_id);
    disable(&env).await;
    pass(&env, Duration::minutes(30));
    enable(&env).await;
    let second_id = activation_id(&env).await;
    assert_ne!(
        second_id, first_id,
        "une activation distincte rend les essais"
    );
    assert_eq!(
        scalar(&env, "SELECT COUNT(*) FROM attack_trials").await,
        0,
        "les essais de l'ancienne activation sont supprimés"
    );
    assert!(plain(&env, "paul", PASSWORD, CLIENT_ADDR).await.is_ok());
}

#[tokio::test]
async fn the_reactivation_guard_does_not_depend_on_the_wall_clock_on_the_same_boot() {
    let env = booted().await;
    enable(&env).await;
    let id = activation_id(&env).await;
    disable(&env).await;
    // Cinq minutes de temps réel, mais l'horloge murale saute d'un an dans les deux sens.
    env.monotonic.advance(Duration::minutes(5));
    env.boot
        .set_uptime(env.boot.uptime() + Duration::minutes(5));
    env.clock.advance(Duration::days(365));
    enable(&env).await;
    assert_eq!(activation_id(&env).await, id);
    disable(&env).await;
    env.boot
        .set_uptime(env.boot.uptime() + Duration::minutes(40));
    env.clock.advance(Duration::days(-365));
    enable(&env).await;
    assert_ne!(
        activation_id(&env).await,
        id,
        "quarante minutes de temps écoulé"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_simultaneous_attempts_on_the_same_trial_never_give_two_free_failures() {
    let env = std::sync::Arc::new(booted().await);
    enable(&env).await;
    // Douze comptes, chacun avec sa clé et SANS adresse retenue depuis les adresses d'attaque : la clé
    // seule a un essai. Deux tentatives simultanées (deux adresses, donc deux tours de parole distincts)
    // visent le même essai. Tours pairs : deux mots de passe FAUX ; tours impairs : deux mots de passe
    // justes.
    for round in 0..12_u32 {
        let name = format!("user-{round}");
        // Compte et poste inscrits AVANT l'activation : on les inscrit en désactivant un instant.
        disable(&env).await;
        env.create(&name, Role::ReadOnly).await;
        let key = std::sync::Arc::new(DeviceKey::new());
        keyed(&env, &key, &name, PASSWORD, "10.30.0.1")
            .await
            .expect("inscription");
        // Une activation distincte à chaque tour : trente minutes passent.
        pass(&env, Duration::minutes(31));
        enable(&env).await;
        let password = if round % 2 == 0 { WRONG } else { PASSWORD };
        let (a, b) = (stranger(2 * round), stranger(2 * round + 1));
        let first = {
            let (env, key, name) = (env.clone(), key.clone(), name.clone());
            tokio::spawn(async move { keyed(&env, &key, &name, password, &a).await.is_ok() })
        };
        let second = {
            let (env, key, name) = (env.clone(), key.clone(), name.clone());
            tokio::spawn(async move { keyed(&env, &key, &name, password, &b).await.is_ok() })
        };
        let (first, second) = (first.await.unwrap(), second.await.unwrap());
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT outcome FROM attack_trials WHERE account_id IN (SELECT id FROM accounts WHERE username = ?)",
        )
        .bind(&name)
        .fetch_all(env.db.pool())
        .await
        .unwrap();
        env.audit_recorder.flush_all().await;
        let entries: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_events WHERE action = 'attack_mode.trial' AND account = ?",
        )
        .bind(&name)
        .fetch_one(env.db.pool())
        .await
        .unwrap();
        assert_eq!(rows.len(), 1, "tour {round} : une seule ligne d'essai");
        assert_eq!(entries, 1, "tour {round} : un seul essai consigné");
        if round % 2 == 0 {
            assert!(!first && !second, "tour {round}");
            assert_eq!(rows[0].0, "failed", "tour {round}");
        } else {
            // Un essai réussi ne consomme rien : les deux entrent.
            assert!(first && second, "tour {round}");
            assert_eq!(rows[0].0, "succeeded", "tour {round}");
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_right_and_a_wrong_password_at_the_same_time_on_one_trial_end_failed_never_twice_never_back()
 {
    let env = std::sync::Arc::new(booted().await);
    enable(&env).await;
    for round in 0..12_u32 {
        let name = format!("race-{round}");
        disable(&env).await;
        env.create(&name, Role::ReadOnly).await;
        let key = std::sync::Arc::new(DeviceKey::new());
        keyed(&env, &key, &name, PASSWORD, "10.30.0.1")
            .await
            .expect("inscription");
        pass(&env, Duration::minutes(31));
        enable(&env).await;
        let (a, b) = (stranger(2 * round), stranger(2 * round + 1));
        let right = {
            let (env, key, name) = (env.clone(), key.clone(), name.clone());
            tokio::spawn(async move { keyed(&env, &key, &name, PASSWORD, &a).await.is_ok() })
        };
        let wrong = {
            let (env, key, name) = (env.clone(), key.clone(), name.clone());
            tokio::spawn(async move { keyed(&env, &key, &name, WRONG, &b).await.is_ok() })
        };
        let (_right, wrong) = (right.await.unwrap(), wrong.await.unwrap());
        assert!(!wrong, "un mot de passe faux n'entre jamais");
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT outcome FROM attack_trials WHERE account_id IN (SELECT id FROM accounts WHERE username = ?)",
        )
        .bind(&name)
        .fetch_all(env.db.pool())
        .await
        .unwrap();
        // Quel que soit l'ordre : une seule ligne, ratée (un succès n'est jamais écrit après un échec).
        assert_eq!(rows.len(), 1, "tour {round}");
        assert_eq!(rows[0].0, "failed", "tour {round}");
        env.audit_recorder.flush_all().await;
        let denied: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_events WHERE action = 'attack_mode.trial' AND outcome = 'denied' AND account = ?",
        )
        .bind(&name)
        .fetch_one(env.db.pool())
        .await
        .unwrap();
        assert_eq!(denied, 1, "tour {round} : jamais deux échecs gratuits");
    }
}

#[tokio::test]
async fn an_address_alone_succeeds_then_misses_then_is_blocked() {
    let env = booted().await;
    paul_with_address(&env).await;
    enable(&env).await;
    assert!(plain(&env, "paul", PASSWORD, CLIENT_ADDR).await.is_ok());
    assert!(
        refused(&plain(&env, "paul", WRONG, CLIENT_ADDR).await),
        "raté"
    );
    let row: (String,) = sqlx::query_as("SELECT outcome FROM attack_trials")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert_eq!(row.0, "failed", "le succès d'avant devient un échec");
    assert!(
        denied(&plain(&env, "paul", PASSWORD, CLIENT_ADDR).await),
        "bloqué"
    );
    assert_eq!(scalar(&env, "SELECT COUNT(*) FROM attack_trials").await, 1);
    disable(&env).await;
    assert!(plain(&env, "paul", PASSWORD, CLIENT_ADDR).await.is_ok());
}

// ---------------------------------------------------------------------------------------------
// Sessions
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_session_alone_is_refused_without_trial_and_without_being_destroyed_and_works_again_at_the_end()
 {
    let env = booted().await;
    env.create("marie", Role::Admin).await;
    let token = plain(&env, "marie", PASSWORD, CLIENT_ADDR)
        .await
        .unwrap()
        .token
        .encode();
    enable(&env).await;
    // Depuis une autre adresse : la session seule.
    for _ in 0..3 {
        let error = env
            .sessions
            .authenticate_at(&token, "10.9.9.9")
            .await
            .unwrap_err();
        assert!(matches!(error, AuthError::NotRecognized), "{error:?}");
    }
    assert_eq!(
        scalar(&env, "SELECT COUNT(*) FROM attack_trials").await,
        0,
        "pas d'essai"
    );
    assert_eq!(
        scalar(&env, "SELECT COUNT(*) FROM sessions").await,
        1,
        "non détruite"
    );
    // Depuis l'adresse retenue : session + adresse, elle passe.
    assert!(
        env.sessions
            .authenticate_at(&token, CLIENT_ADDR)
            .await
            .is_ok()
    );
    // À la fin du mode, la session refonctionne sans reconnexion, de partout.
    disable(&env).await;
    assert!(
        env.sessions
            .authenticate_at(&token, "10.9.9.9")
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn a_session_with_a_key_proof_passes_and_makes_the_address_retained() {
    let env = booted().await;
    let key = marie_with_key(&env).await;
    let outcome = keyed(&env, &key, "marie", PASSWORD, CLIENT_ADDR)
        .await
        .unwrap();
    let token = outcome.token.encode();
    enable(&env).await;
    assert!(matches!(
        env.sessions.authenticate_at(&token, "10.9.9.9").await,
        Err(AuthError::NotRecognized)
    ));
    // La preuve d'usage « session », liée à ce jeton, depuis la nouvelle adresse.
    let proof = key.prove(
        &env.trust,
        hearth_proto::device_proof::Binding::Session {
            token_hash: outcome.token.hash().as_bytes(),
        },
        "marie",
        "10.9.9.9",
    );
    assert!(
        env.sessions
            .authenticate_proved(&token, "10.9.9.9", &proof)
            .await
            .is_ok()
    );
    assert_eq!(
        scalar(
            &env,
            "SELECT COUNT(*) FROM known_addresses WHERE address = '10.9.9.9'"
        )
        .await,
        1
    );
    // L'adresse est retenue : la session seule, désormais, passe depuis elle.
    assert!(
        env.sessions
            .authenticate_at(&token, "10.9.9.9")
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn a_proof_of_another_account_or_a_replayed_proof_does_not_make_a_second_criterion() {
    let env = booted().await;
    let marie_key = marie_with_key(&env).await;
    env.create("paul", Role::ReadOnly).await;
    let outcome = plain(&env, "paul", PASSWORD, "10.7.7.7").await.unwrap();
    let token = outcome.token.encode();
    enable(&env).await;
    // La clé de marie, signée pour paul : la clé n'est pas inscrite pour son compte.
    let proof = marie_key.prove(
        &env.trust,
        hearth_proto::device_proof::Binding::Session {
            token_hash: outcome.token.hash().as_bytes(),
        },
        "paul",
        "10.9.9.9",
    );
    assert!(matches!(
        env.sessions
            .authenticate_proved(&token, "10.9.9.9", &proof)
            .await,
        Err(AuthError::NotRecognized)
    ));
}

#[tokio::test]
async fn the_refusals_of_sessions_are_journaled_in_a_bounded_way_and_push_the_automatic_exit_back()
{
    let env = booted().await;
    env.create("marie", Role::Admin).await;
    let token = plain(&env, "marie", PASSWORD, CLIENT_ADDR)
        .await
        .unwrap()
        .token
        .encode();
    enable(&env).await;
    for _ in 0..500 {
        assert!(
            env.sessions
                .authenticate_at(&token, "10.9.9.9")
                .await
                .is_err()
        );
    }
    let entries = journal(&env).await;
    let refusals: Vec<_> = entries
        .iter()
        .filter(|entry| entry.0 == "session.refused")
        .collect();
    assert!(
        (1..=3).contains(&refusals.len()),
        "500 refus, {} entrées",
        refusals.len()
    );
    assert!(refusals.iter().all(|entry| entry.1 == "denied"));
    // Les refus repoussent la sortie : 29 minutes, un refus, 29 minutes, un refus : toujours actif.
    pass(&env, Duration::minutes(29));
    assert!(
        env.sessions
            .authenticate_at(&token, "10.9.9.9")
            .await
            .is_err()
    );
    pass(&env, Duration::minutes(29));
    assert!(!env.attack.sweep().await.unwrap().auto_disabled);
    pass(&env, Duration::minutes(2));
    assert!(env.attack.sweep().await.unwrap().auto_disabled);
}

#[tokio::test]
async fn during_the_window_the_alert_regime_applies_a_session_alone_passes_then_the_mode_resumes() {
    let env = booted().await;
    env.create("marie", Role::Admin).await;
    let token = plain(&env, "marie", PASSWORD, CLIENT_ADDR)
        .await
        .unwrap()
        .token
        .encode();
    enable(&env).await;
    assert!(reboot(&env, "boot-2", Duration::minutes(1)).await);
    assert!(
        env.sessions
            .authenticate_at(&token, "10.9.9.9")
            .await
            .is_ok()
    );
    pass(&env, Duration::minutes(30));
    assert!(
        env.sessions
            .authenticate_at(&token, "10.9.9.9")
            .await
            .is_err()
    );
}

// ---------------------------------------------------------------------------------------------
// Fenêtre de redémarrage, reprise, sortie, marqueur de redémarrage demandé
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn the_window_lasts_thirty_minutes_of_uptime_then_the_mode_resumes_with_the_same_activation_and_no_trial_back()
 {
    let env = booted().await;
    paul_with_address(&env).await;
    enable(&env).await;
    let id = activation_id(&env).await;
    assert!(refused(&plain(&env, "paul", WRONG, CLIENT_ADDR).await));
    assert!(reboot(&env, "boot-2", Duration::minutes(2)).await);
    assert!(matches!(state(&env).await, Effective::Suspended { .. }));
    assert_eq!(count_action(&env, "attack_mode.suspend").await, 1);
    pass(&env, Duration::minutes(10));
    let Effective::Suspended { remaining } = state(&env).await else {
        panic!("suspendu")
    };
    assert_eq!(remaining, Duration::minutes(18));
    assert!(!env.attack.sweep().await.unwrap().resumed);
    pass(&env, Duration::minutes(18));
    assert_eq!(state(&env).await, Effective::Active);
    let report = env.attack.sweep().await.unwrap();
    assert!(report.resumed);
    assert!(
        !env.attack.sweep().await.unwrap().resumed,
        "une seule reprise"
    );
    assert_eq!(count_action(&env, "attack_mode.resume").await, 1);
    assert_eq!(activation_id(&env).await, id);
    // Les essais ne sont pas rendus par la reprise.
    assert_eq!(scalar(&env, "SELECT COUNT(*) FROM attack_trials").await, 1);
    assert!(refused(&plain(&env, "paul", PASSWORD, CLIENT_ADDR).await));
}

#[tokio::test]
async fn the_suspension_does_not_count_as_a_quiet_period_for_the_automatic_exit() {
    let env = booted().await;
    enable(&env).await;
    assert!(reboot(&env, "boot-2", Duration::minutes(1)).await);
    env.attack.sweep().await.unwrap();
    pass(&env, Duration::minutes(29));
    // Trente minutes de temps écoulé depuis le démarrage : la fenêtre est finie, le mode reprend.
    assert!(env.attack.sweep().await.unwrap().resumed);
    // Au moment de la reprise, la période calme repart de zéro : les trente minutes de la fenêtre ne
    // comptent pas.
    pass(&env, Duration::minutes(29));
    assert!(!env.attack.sweep().await.unwrap().auto_disabled);
    pass(&env, Duration::minutes(2));
    assert!(env.attack.sweep().await.unwrap().auto_disabled);
}

#[tokio::test]
async fn a_disable_during_the_window_is_not_reopened_after_it() {
    let env = booted().await;
    enable(&env).await;
    assert!(reboot(&env, "boot-2", Duration::minutes(1)).await);
    disable(&env).await;
    assert_eq!(state(&env).await, Effective::Off);
    pass(&env, Duration::minutes(45));
    let report = env.attack.sweep().await.unwrap();
    assert!(!report.resumed && !report.auto_disabled);
    assert!(!env.attack.on_start().await.unwrap());
    assert_eq!(state(&env).await, Effective::Off);
    assert_eq!(count_action(&env, "attack_mode.resume").await, 0);
}

#[tokio::test]
async fn a_service_restart_during_the_window_finds_the_exact_remaining_time_and_never_extends_it() {
    let env = booted().await;
    enable(&env).await;
    assert!(reboot(&env, "boot-2", Duration::minutes(1)).await);
    pass(&env, Duration::minutes(9));
    // Le service redémarre : même démarrage du noyau.
    assert!(!env.attack.on_start().await.unwrap());
    assert_eq!(
        state(&env).await,
        Effective::Suspended {
            remaining: Duration::minutes(20)
        }
    );
    // Un service qui démarre APRÈS la fenêtre ne la rouvre pas.
    pass(&env, Duration::minutes(21));
    assert!(!env.attack.on_start().await.unwrap());
    assert_eq!(state(&env).await, Effective::Active);
    assert_eq!(count_action(&env, "attack_mode.suspend").await, 1);
}

#[tokio::test]
async fn a_reboot_asked_by_hearth_opens_no_window() {
    let env = booted().await;
    enable(&env).await;
    // Ce que l'épic « machine » écrira AVANT d'appeler le système : le démarrage courant.
    sqlx::query("UPDATE attack_mode SET remote_reboot_boot_id = 'boot-1' WHERE id = 1")
        .execute(env.db.pool())
        .await
        .unwrap();
    assert!(!reboot(&env, "boot-2", Duration::minutes(1)).await);
    assert_eq!(state(&env).await, Effective::Active);
    let marker: Option<String> =
        sqlx::query_scalar("SELECT remote_reboot_boot_id FROM attack_mode WHERE id = 1")
            .fetch_one(env.db.pool())
            .await
            .unwrap();
    assert_eq!(marker, None, "la marque est consommée");
    // Un redémarrage suivant, que Hearth n'a pas demandé, ouvre la fenêtre.
    assert!(reboot(&env, "boot-3", Duration::minutes(1)).await);
}

#[tokio::test]
async fn a_machine_reboot_with_the_mode_off_opens_nothing() {
    let env = booted().await;
    assert!(!reboot(&env, "boot-2", Duration::minutes(1)).await);
    assert_eq!(state(&env).await, Effective::Off);
    assert_eq!(count_action(&env, "attack_mode.suspend").await, 0);
    let noted: Option<String> =
        sqlx::query_scalar("SELECT last_boot_id FROM attack_mode WHERE id = 1")
            .fetch_one(env.db.pool())
            .await
            .unwrap();
    assert_eq!(noted.as_deref(), Some("boot-2"));
}

// ---------------------------------------------------------------------------------------------
// Activation : idempotence, journal, état
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn enabling_and_disabling_are_idempotent_and_journaled_once_with_the_administrator() {
    let env = booted().await;
    let first = enable(&env).await;
    assert_eq!(first.state, Effective::Active);
    assert!(first.since.is_some());
    let id = activation_id(&env).await;
    enable(&env).await;
    assert_eq!(activation_id(&env).await, id);
    assert_eq!(count_action(&env, "attack_mode.enable").await, 1);
    let off = disable(&env).await;
    assert_eq!(off.state, Effective::Off);
    assert_eq!(off.last_end, Some(EndHow::Manual));
    disable(&env).await;
    assert_eq!(count_action(&env, "attack_mode.disable").await, 1);
    let entries = journal(&env).await;
    assert!(entries.iter().all(|entry| entry.1 == "ok"));
    let activated_by: Option<String> =
        sqlx::query_scalar("SELECT activated_by FROM attack_mode WHERE id = 1")
            .fetch_one(env.db.pool())
            .await
            .unwrap();
    assert_eq!(activated_by.as_deref(), Some("root"));
}

#[tokio::test]
async fn the_enrolment_is_frozen_in_attack_mode_but_the_login_that_the_trial_admits_still_works() {
    let env = booted().await;
    paul_with_address(&env).await;
    enable(&env).await;
    let key = DeviceKey::new();
    // Un poste dont l'adresse est retenue présente SA clé neuve : un critère, l'essai passe, le poste
    // n'est PAS inscrit pendant le mode (BR-TRUST-004).
    let outcome = keyed(&env, &key, "paul", PASSWORD, CLIENT_ADDR)
        .await
        .unwrap();
    assert_eq!(
        outcome.device,
        Some(hearth_proto::api::sessions::DeviceStatus::Deferred)
    );
    assert_eq!(
        scalar(&env, "SELECT COUNT(*) FROM trusted_devices").await,
        0
    );
}

// ---------------------------------------------------------------------------------------------
// Absence d'oracle
// ---------------------------------------------------------------------------------------------

/// Tout ce qu'un appareil du réseau, ou la base, peut constater d'une tentative.
#[derive(Debug, PartialEq, Eq)]
struct Observed {
    wire: Wire,
    /// Les compteurs écrits : couples, adresses (échecs), puis identifiants.
    counters: (Vec<i64>, Vec<i64>),
    /// Vérifications du mot de passe faites, dont celles contre le haché factice.
    hashed: (u64, u64),
    /// Les entrées du journal, sans le compte (le journal ne nomme un compte que s'il existe).
    journal: Vec<(String, String, Option<String>)>,
    sessions: i64,
}

#[derive(Clone, Copy, Debug)]
enum Situation {
    Normal,
    Attack,
    Suspended,
}

async fn observe(situation: Situation, identifier: &str, password: &str) -> Observed {
    let env = booted().await;
    env.create("marie", Role::Admin).await;
    plain(&env, "marie", PASSWORD, CLIENT_ADDR).await.unwrap();
    let (verified_before, decoys_before) = (
        env.hasher
            .verifications
            .load(std::sync::atomic::Ordering::SeqCst),
        env.hasher
            .against_decoy
            .load(std::sync::atomic::Ordering::SeqCst),
    );
    let journal_before = journal(&env).await.len();
    match situation {
        Situation::Normal => {}
        Situation::Attack => {
            enable(&env).await;
        }
        Situation::Suspended => {
            enable(&env).await;
            assert!(reboot(&env, "boot-2", Duration::minutes(1)).await);
        }
    }
    let journal_mid = journal(&env).await.len();
    let error = plain(&env, identifier, password, "10.9.9.9")
        .await
        .expect_err("l'adresse n'est pas reconnue et le mot de passe est faux ou ignoré");
    let wire = on_the_wire(error).await;
    let column = |sql: &'static str| {
        let pool = env.db.pool().clone();
        async move {
            sqlx::query_scalar::<_, i64>(sql)
                .fetch_all(&pool)
                .await
                .unwrap()
        }
    };
    let counters = (
        column("SELECT failures FROM login_attempts ORDER BY failures, key").await,
        column("SELECT failures FROM identifier_slowdowns ORDER BY failures, key").await,
    );
    let journal_all = journal(&env).await;
    Observed {
        wire,
        counters,
        hashed: (
            env.hasher
                .verifications
                .load(std::sync::atomic::Ordering::SeqCst)
                - verified_before,
            env.hasher
                .against_decoy
                .load(std::sync::atomic::Ordering::SeqCst)
                - decoys_before,
        ),
        journal: journal_all[journal_mid.max(journal_before)..]
            .iter()
            .map(|entry| (entry.0.clone(), entry.1.clone(), entry.3.clone()))
            .collect(),
        sessions: scalar(&env, "SELECT COUNT(*) FROM sessions").await,
    }
}

#[tokio::test]
async fn an_unrecognised_device_cannot_tell_the_mode_the_existence_of_the_identifier_or_the_password()
 {
    // Pour chaque situation, un identifiant qui existe et un qui n'existe pas, un mot de passe faux et le
    // bon : la même réponse sur le fil, les mêmes compteurs, les mêmes calculs, les mêmes écritures.
    let reference = observe(Situation::Normal, "marie", WRONG).await;
    assert_eq!(reference.wire.0, 401, "{:?}", reference.wire);
    assert_eq!(reference.hashed, (1, 0));
    for situation in [Situation::Normal, Situation::Attack, Situation::Suspended] {
        for identifier in ["marie", "ghost"] {
            for password in [WRONG, PASSWORD] {
                // Dans l'état normal, le bon mot de passe d'un poste inconnu passe : seule la
                // situation qui le refuse est comparée.
                if matches!(situation, Situation::Normal | Situation::Suspended)
                    && password == PASSWORD
                    && identifier == "marie"
                {
                    continue;
                }
                let observed = observe(situation, identifier, password).await;
                let decoy = u64::from(identifier == "ghost");
                // Le journal (lisible des seuls administrateurs) dit « poste non reconnu » pour une
                // connexion bloquée sans essai en mode attaque ; tout le reste est identique.
                let journal = if matches!(situation, Situation::Attack) {
                    reference
                        .journal
                        .iter()
                        .map(|(action, outcome, _)| {
                            (
                                action.clone(),
                                outcome.clone(),
                                Some("mode attaque : poste non reconnu".to_owned()),
                            )
                        })
                        .collect()
                } else {
                    reference.journal.clone()
                };
                assert_eq!(
                    observed.hashed,
                    (1, decoy),
                    "{situation:?} {identifier} {password}"
                );
                assert_eq!(
                    Observed {
                        hashed: reference.hashed,
                        ..observed
                    },
                    Observed {
                        hashed: reference.hashed,
                        wire: reference.wire.clone(),
                        counters: (reference.counters.0.clone(), reference.counters.1.clone()),
                        journal,
                        sessions: reference.sessions,
                    },
                    "{situation:?} {identifier} {password}"
                );
            }
        }
    }
}

#[tokio::test]
async fn a_trial_a_used_trial_and_no_criterion_give_the_same_answer_on_the_wire() {
    let env = booted().await;
    paul_with_address(&env).await;
    enable(&env).await;
    // (a) aucun critère, mot de passe faux ; (b) l'essai, raté ; (c) l'essai déjà utilisé, faux ;
    // (d) l'essai déjà utilisé, mot de passe juste ; (e) aucun critère, mot de passe juste.
    let mut wires = Vec::new();
    for (from, password) in [
        ("10.9.9.9", WRONG),
        (CLIENT_ADDR, WRONG),
        (CLIENT_ADDR, WRONG),
        (CLIENT_ADDR, PASSWORD),
        ("10.9.9.8", PASSWORD),
    ] {
        let verified = env
            .hasher
            .verifications
            .load(std::sync::atomic::Ordering::SeqCst);
        let error = plain(&env, "paul", password, from).await.unwrap_err();
        wires.push(on_the_wire(error).await);
        assert_eq!(
            env.hasher
                .verifications
                .load(std::sync::atomic::Ordering::SeqCst)
                - verified,
            1,
            "le calcul du mot de passe est fait dans tous les cas"
        );
    }
    for wire in &wires[1..] {
        assert_eq!(wire, &wires[0]);
    }
    assert_eq!(wires[0].0, 401);
}

// ---------------------------------------------------------------------------------------------
// Journal
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn the_trial_is_journaled_with_its_outcome_and_never_a_secret() {
    let env = booted().await;
    paul_with_address(&env).await;
    enable(&env).await;
    assert!(refused(&plain(&env, "paul", WRONG, CLIENT_ADDR).await));
    let rows: Vec<(String, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT action, outcome, target, reason FROM audit_events WHERE action = 'attack_mode.trial'",
    )
    .fetch_all(env.db.pool())
    .await
    .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].1, "denied");
    assert_eq!(rows[0].2.as_deref(), Some("essai sur l'adresse retenue"));
    assert_eq!(rows[0].3.as_deref(), Some("identifiants incorrects"));
    let everything: String = sqlx::query_scalar(
        "SELECT group_concat(coalesce(account,'') || coalesce(origin_addr,'') || coalesce(target,'') || coalesce(reason,''), '|')
         FROM audit_events",
    )
    .fetch_one(env.db.pool())
    .await
    .unwrap();
    assert!(!everything.contains(WRONG) && !everything.contains(PASSWORD));
}

#[tokio::test]
async fn the_attack_mode_status_reports_state_since_remaining_time_and_last_end() {
    use hearth_agent::entrypoint::attack_mode::write_status;
    let env = booted().await;
    let text = |status: AttackStatus| {
        let mut out = Vec::new();
        write_status(&mut out, &status).unwrap();
        String::from_utf8(out).unwrap()
    };
    let off = text(env.attack.status().await.unwrap());
    assert!(off.contains("mode : off"), "{off}");
    enable(&env).await;
    let id = activation_id(&env).await.unwrap();
    let active = text(env.attack.status().await.unwrap());
    assert!(
        active.contains("mode : active") && active.contains(&id),
        "{active}"
    );
    reboot(&env, "boot-2", Duration::minutes(10)).await;
    let suspended = text(env.attack.status().await.unwrap());
    assert!(suspended.contains("mode : suspended"), "{suspended}");
    assert!(suspended.contains("resumes_in_s : 1200"), "{suspended}");
}

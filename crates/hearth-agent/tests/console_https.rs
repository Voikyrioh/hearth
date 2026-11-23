//! La console d'un serveur de jeu et les joueurs connectés, de bout en bout (HRT-268, ADR-0067, BR-CONSOLE-002, 004 à 009,
//! 011) : vrai agent en HTTPS, vraie base, faux moteur, faux jeu qui parle par la console. Un serveur Minecraft est posé
//! sans passer par l'installation (description durable avec son secret `rcon`, conteneur en marche).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::net::SocketAddr;
use std::sync::Arc;

use hearth_agent::application::ports::ConsoleError;
use hearth_agent::domain::accounts::Role;
use hearth_agent::domain::ops::OpKind;
use hearth_agent::domain::services::{RunState, ServiceKey};
use serde_json::{Value, json};
use support::device::Actor;
use support::engine::{fast_timings, parts_supervised};
use support::fake_engine::FakeEngine;
use support::fake_game_console::FakeGameConsole;
use support::fake_supervisor::FakeSupervisor;
use support::game::{RCON_PASSWORD, minecraft_console_service};
use support::https::{self, Agent, Wire};
use support::{Env, env};

struct Bench {
    env: Env,
    engine: Arc<FakeEngine>,
    game: Arc<FakeGameConsole>,
    agent: Agent,
    ada: Actor,
    lucas: Actor,
    key: ServiceKey,
    container: String,
}

async fn bench() -> Bench {
    let env = env().await;
    let engine = Arc::new(FakeEngine::new());
    let game = FakeGameConsole::new();
    let mut parts = parts_supervised(
        engine.clone(),
        Arc::new(FakeSupervisor::new()),
        fast_timings(),
    );
    parts.catalog.console = Some(game.clone());
    parts.catalog.include_preview = true;
    let agent = https::start_engined(&env, parts).await;
    let ada = agent.actor(&env, "ada", Role::Admin).await;
    let lucas = agent.actor(&env, "lucas", Role::ReadOnly).await;
    env.grant_services("ada").await;
    let key = minecraft_console_service(&env, &engine, &agent.services(), 1).await;
    let container = engine
        .container_ids()
        .into_iter()
        .last()
        .expect("conteneur");
    Bench {
        env,
        engine,
        game,
        agent,
        ada,
        lucas,
        key,
        container,
    }
}

fn code(wire: &Wire) -> String {
    wire.body["error"]["code"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

impl Bench {
    fn path(&self) -> String {
        format!("/services/{}/console", self.key)
    }

    /// Envoie une commande, avec ou sans mot de passe de confirmation.
    async fn send_as(&self, who: &Actor, command: &str, class: u8, password: &str) -> Wire {
        let body = json!({ "command": command, "class": class });
        let body = self
            .agent
            .confirm_with(who, "POST", &self.path(), body, password)
            .await;
        self.agent
            .request("POST", &self.path())
            .token(&who.token)
            .json(&body)
            .send()
            .await
    }

    /// Une commande libre en NORMAL : sans mot de passe.
    async fn free(&self, command: &str) -> Wire {
        self.send_as(&self.ada, command, 1, "").await
    }

    async fn players(&self) -> Value {
        let reply = self
            .agent
            .request("GET", &format!("/services/{}/players", self.key))
            .token(&self.lucas.token)
            .send()
            .await;
        assert_eq!(reply.status, 200, "{}", reply.text);
        reply.body
    }

    async fn journal(&self) -> Vec<(String, Option<String>)> {
        self.agent.flush_journal().await;
        sqlx::query_as(
            "SELECT outcome, target FROM audit_events WHERE action = 'service.console' ORDER BY id",
        )
        .fetch_all(self.env.db.pool())
        .await
        .unwrap()
    }

    async fn stop_container(&self) {
        self.engine
            .update(&self.container, |o| o.run_state = RunState::Exited);
        self.agent.services().resync().await.expect("liste");
    }
}

// -------------------------------------------------------------------------------------------------------------
// Décrire
// -------------------------------------------------------------------------------------------------------------

#[tokio::test]
async fn the_description_gives_the_policy_of_the_fiche_to_any_account_and_the_right_to_write_to_the_right_ones()
 {
    let b = bench().await;
    let read = |who: &Actor| {
        let path = b.path();
        let agent = &b.agent;
        let token = who.token.clone();
        async move { agent.request("GET", &path).token(&token).send().await }
    };
    let reader = read(&b.lucas).await;
    assert_eq!(reader.status, 200, "{}", reader.text);
    assert_eq!(reader.body["available"], true);
    assert_eq!(reader.body["state"], "ready");
    assert_eq!(reader.body["can_write"], false);
    let verbs: Vec<&str> = reader.body["policy"]["sensitive"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["verb"].as_str().unwrap())
        .collect();
    assert!(verbs.contains(&"op") && verbs.contains(&"execute"));
    assert_eq!(reader.body["policy"]["refused"][0]["verb"], "stop");
    assert_eq!(reader.body["limits"]["max_chars"], 256);
    assert_eq!(reader.body["limits"]["per_minute"], 20);
    assert!(reader.body["quick"].as_array().unwrap().len() >= 3);
    let writer = read(&b.ada).await;
    assert_eq!(writer.body["can_write"], true);
    // Arrêté : l'état le dit.
    b.stop_container().await;
    let stopped = read(&b.ada).await;
    assert_eq!(stopped.body["state"], "stopped");
    // Rien n'a été envoyé au jeu par la description.
    assert!(b.game.lines().is_empty());
}

#[tokio::test]
async fn an_unknown_service_is_not_found_and_a_service_without_a_console_is_not_available() {
    let b = bench().await;
    let reply = b
        .agent
        .request("GET", "/services/h_01JZ0000000000000000000099/console")
        .token(&b.lucas.token)
        .send()
        .await;
    assert_eq!(reply.status, 404);
    assert_eq!(code(&reply), "SERVICE_NOT_FOUND");
    let players = b
        .agent
        .request("GET", "/services/h_01JZ0000000000000000000099/players")
        .token(&b.lucas.token)
        .send()
        .await;
    assert_eq!(players.status, 404);
}

// -------------------------------------------------------------------------------------------------------------
// Envoyer : mot de passe selon la classe
// -------------------------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_free_command_goes_without_password_to_the_private_address_with_the_secret_of_the_fiche()
{
    let b = bench().await;
    b.game.answer("time set day", "Set the time to 1000");
    let reply = b.free("time set day").await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["outcome"], "answered");
    assert_eq!(reply.body["reply"], "Set the time to 1000");
    assert_eq!(reply.body["truncated"], false);
    let received = b.game.received();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].line, "time set day");
    assert_eq!(received[0].password, RCON_PASSWORD);
    // L'adresse privée du conteneur et le port de la fiche, jamais autre chose.
    assert_eq!(
        received[0].address,
        "172.17.0.9:25575".parse::<SocketAddr>().unwrap()
    );
}

#[tokio::test]
async fn a_slash_and_spaces_are_removed_before_sending() {
    let b = bench().await;
    let reply = b.free("  /time set day ").await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(b.game.lines(), ["time set day"]);
}

#[tokio::test]
async fn a_sensitive_command_asks_for_the_password_every_time_even_under_the_elevation() {
    let b = bench().await;
    // Sans mot de passe : refusé, rien n'est parti.
    let refused = b.send_as(&b.ada, "op Bram", 2, "").await;
    assert_eq!(refused.status, 409, "{}", refused.text);
    assert_eq!(
        refused.body["error"]["details"]["reason"], "password_required",
        "{}",
        refused.text
    );
    assert!(b.game.lines().is_empty());
    // Avec le mot de passe : envoyée ; le mot de passe ouvre l'élévation de cinq minutes...
    b.game.answer("op Bram", "Made Bram a server operator");
    let sent = b.send_as(&b.ada, "op Bram", 2, support::PASSWORD).await;
    assert_eq!(sent.status, 200, "{}", sent.text);
    assert_eq!(sent.body["reply"], "Made Bram a server operator");
    // ... qui ne couvre pas une commande sensible : un nouvel envoi sans mot de passe est refusé.
    let again = b.send_as(&b.ada, "op Lia", 2, "").await;
    assert_eq!(again.status, 409, "{}", again.text);
    assert_eq!(
        again.body["error"]["details"]["reason"], "password_required",
        "{}",
        again.text
    );
    assert_eq!(b.game.lines(), ["op Bram"]);
}

#[tokio::test]
async fn an_account_without_the_right_is_refused_before_anything_else() {
    let b = bench().await;
    let reply = b.send_as(&b.lucas, "time set day", 1, "").await;
    assert_eq!(reply.status, 403, "{}", reply.text);
    assert!(b.game.lines().is_empty());
    // Une commande sans confirmation du tout n'est pas un acte.
    let bare = b
        .agent
        .request("POST", &b.path())
        .token(&b.ada.token)
        .json(&json!({ "command": "list", "class": 1 }))
        .send()
        .await;
    assert!(bare.status >= 400, "{}", bare.text);
    assert!(b.game.lines().is_empty());
}

// -------------------------------------------------------------------------------------------------------------
// Envoyer : les refus, dans l'ordre, rien n'est envoyé
// -------------------------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_hostile_form_is_refused_with_its_reason_and_nothing_is_sent() {
    let b = bench().await;
    for (command, reason) in [
        ("   ", "empty"),
        ("say a\nstop", "control_char"),
        (&"a".repeat(257), "too_long"),
        ("say a\u{202e}b", "control_char"),
    ] {
        let reply = b.free(command).await;
        assert_eq!(reply.status, 422, "{command:?} {}", reply.text);
        assert_eq!(code(&reply), "CONSOLE_COMMAND_INVALID");
        assert_eq!(
            reply.body["error"]["details"]["reason"], reason,
            "{command:?}"
        );
    }
    assert!(b.game.lines().is_empty());
}

#[tokio::test]
async fn a_refused_verb_is_refused_even_with_a_prefix_or_capitals_and_is_journaled() {
    let b = bench().await;
    for (command, reason) in [
        ("stop", "use_hearth_stop"),
        ("/STOP", "use_hearth_stop"),
        ("minecraft:restart", "use_hearth_restart"),
        ("reload", "breaks_server"),
    ] {
        let reply = b.free(command).await;
        assert_eq!(reply.status, 422, "{command} {}", reply.text);
        assert_eq!(code(&reply), "CONSOLE_COMMAND_REFUSED");
        assert_eq!(reply.body["error"]["details"]["reason"], reason);
    }
    assert!(b.game.lines().is_empty());
    let journal = b.journal().await;
    assert_eq!(journal.len(), 4, "{journal:?}");
    assert!(journal.iter().all(|(_, target)| {
        let text = target.clone().unwrap_or_default();
        text.contains("refused")
    }));
}

#[tokio::test]
async fn a_wrong_signed_class_is_refused_and_nothing_is_sent() {
    let b = bench().await;
    // « op » signé libre : l'agent le juge sensible.
    let reply = b.send_as(&b.ada, "op Bram", 1, "").await;
    assert_eq!(reply.status, 422, "{}", reply.text);
    assert_eq!(code(&reply), "CONSOLE_CLASS_MISMATCH");
    // Une commande libre signée sensible : refusée aussi (même avec le mot de passe).
    let other = b
        .send_as(&b.ada, "time set day", 2, support::PASSWORD)
        .await;
    assert_eq!(other.status, 422, "{}", other.text);
    assert_eq!(code(&other), "CONSOLE_CLASS_MISMATCH");
    assert!(b.game.lines().is_empty());
}

#[tokio::test]
async fn a_stopped_server_refuses_and_nothing_is_sent() {
    let b = bench().await;
    b.stop_container().await;
    let reply = b.free("time set day").await;
    assert_eq!(reply.status, 409, "{}", reply.text);
    assert_eq!(code(&reply), "SERVICE_NOT_RUNNING");
    assert!(b.game.lines().is_empty());
}

#[tokio::test]
async fn an_exclusive_operation_on_the_service_refuses_the_command() {
    let b = bench().await;
    b.agent
        .ops()
        .locks()
        .with(|table| table.acquire_exclusive(b.key.as_str(), "op-test", OpKind::Mods))
        .expect("verrou");
    let reply = b.free("time set day").await;
    assert_eq!(reply.status, 409, "{}", reply.text);
    assert_eq!(code(&reply), "OPERATION_IN_PROGRESS");
    assert!(b.game.lines().is_empty());
    // Le verrou rendu, la commande part.
    b.agent
        .ops()
        .locks()
        .with(|table| table.release_exclusive(b.key.as_str(), "op-test"));
    assert_eq!(b.free("time set day").await.status, 200);
}

#[tokio::test]
async fn the_commands_are_limited_per_minute_and_the_wait_is_told() {
    let b = bench().await;
    // La rafale est de 20 commandes immédiates (spec D7) ; la vingt et unième est refusée sans partir.
    for _ in 0..20 {
        assert_eq!(b.free("list").await.status, 200);
    }
    let limited = b.free("list").await;
    assert_eq!(limited.status, 429, "{}", limited.text);
    assert_eq!(code(&limited), "CONSOLE_RATE_LIMITED");
    assert!(
        limited.body["error"]["details"]["retry_after_s"]
            .as_u64()
            .unwrap()
            >= 1
    );
    assert_eq!(
        b.game.lines().len(),
        20,
        "la vingt et unième n'est pas partie"
    );
}

#[tokio::test]
async fn an_unreachable_console_is_a_bad_gateway_and_is_journaled() {
    let b = bench().await;
    b.game.fail("say bonjour", ConsoleError::Unreachable);
    let reply = b.free("say bonjour").await;
    assert_eq!(reply.status, 502, "{}", reply.text);
    assert_eq!(code(&reply), "CONSOLE_UNREACHABLE");
    let journal = b.journal().await;
    assert_eq!(journal.len(), 1, "{journal:?}");
    assert!(journal[0].1.clone().unwrap().contains("unreachable"));
    // Un mot de passe refusé par le jeu est la même réponse.
    b.game.fail("say deux", ConsoleError::AuthRefused);
    assert_eq!(b.free("say deux").await.status, 502);
    // Pas d'adresse privée : la même.
    b.engine.set_private_address(&b.container, None);
    assert_eq!(b.free("say trois").await.status, 502);
}

#[tokio::test]
async fn a_late_answer_is_a_success_without_text_and_the_command_is_never_sent_again() {
    let b = bench().await;
    b.game.fail("say lent", ConsoleError::NoAnswer);
    let reply = b.free("say lent").await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["outcome"], "no_answer");
    assert!(reply.body.get("reply").is_none());
    assert_eq!(b.game.lines(), ["say lent"], "une seule fois");
}

#[tokio::test]
async fn an_empty_answer_and_a_cut_answer_are_told() {
    let b = bench().await;
    b.game.answer("say muet", "");
    let empty = b.free("say muet").await;
    assert_eq!(empty.body["outcome"], "empty");
    assert!(empty.body.get("reply").is_none());
    b.game.truncated("list", &"x".repeat(100));
    let cut = b.free("list").await;
    assert_eq!(cut.body["outcome"], "answered");
    assert_eq!(cut.body["truncated"], true);
}

// -------------------------------------------------------------------------------------------------------------
// Le journal
// -------------------------------------------------------------------------------------------------------------

#[tokio::test]
async fn the_journal_keeps_the_verb_the_class_the_player_the_length_and_the_issue_never_the_text_or_the_secrets()
 {
    let b = bench().await;
    b.game
        .answer("op Bram", "SENTINELLE-REPONSE made Bram an operator");
    assert_eq!(
        b.send_as(&b.ada, "op Bram", 2, support::PASSWORD)
            .await
            .status,
        200
    );
    assert_eq!(
        b.free("say SENTINELLE-TEXTE-DU-MESSAGE-123").await.status,
        200
    );
    let journal = b.journal().await;
    assert_eq!(journal.len(), 2, "{journal:?}");
    let sensitive = journal[0].1.clone().unwrap();
    assert!(sensitive.contains("op"), "{sensitive}");
    assert!(sensitive.contains("sensible"), "{sensitive}");
    assert!(sensitive.contains("Bram"), "{sensitive}");
    assert!(sensitive.contains("7 caractères"), "{sensitive}");
    assert!(sensitive.contains("answered"), "{sensitive}");
    let free = journal[1].1.clone().unwrap();
    assert!(free.contains("say"), "{free}");
    assert!(free.contains("libre"), "{free}");
    assert!(free.contains("answered"), "{free}");
    // Sentinelles : ni le texte d'un message, ni la réponse, ni le mot de passe du jeu, ni celui du compte.
    let everything = format!("{journal:?}");
    for secret in [
        "SENTINELLE-TEXTE",
        "SENTINELLE-REPONSE",
        RCON_PASSWORD,
        support::PASSWORD,
    ] {
        assert!(!everything.contains(secret), "{secret} dans le journal");
    }
    // Ni dans les entrées de l'agent (toutes actions confondues).
    let all: Vec<(Option<String>,)> = sqlx::query_as("SELECT target FROM audit_events")
        .fetch_all(b.env.db.pool())
        .await
        .unwrap();
    let all = format!("{all:?}");
    for secret in ["SENTINELLE-TEXTE", "SENTINELLE-REPONSE", RCON_PASSWORD] {
        assert!(!all.contains(secret), "{secret} dans le journal");
    }
}

#[tokio::test]
async fn a_player_name_that_is_not_a_pseudo_never_reaches_the_journal() {
    let b = bench().await;
    assert_eq!(
        b.send_as(
            &b.ada,
            "op nom-hors-forme-très-long-et-faux-1234567890",
            2,
            support::PASSWORD
        )
        .await
        .status,
        200
    );
    let journal = b.journal().await;
    assert!(
        !journal[0].1.clone().unwrap().contains("nom-hors-forme"),
        "{journal:?}"
    );
}

// -------------------------------------------------------------------------------------------------------------
// Les joueurs
// -------------------------------------------------------------------------------------------------------------

#[tokio::test]
async fn the_players_are_read_by_the_command_of_the_fiche_and_cached_ten_seconds() {
    let b = bench().await;
    b.game.answer(
        "list",
        "There are 2 of a max of 20 players online: Alice_42, Bram",
    );
    let first = b.players().await;
    assert_eq!(first["state"], "ok");
    assert_eq!(first["online"], 2);
    assert_eq!(first["max"], 20);
    assert_eq!(first["names"], json!(["Alice_42", "Bram"]));
    assert!(first["read_at"].is_string());
    let second = b.players().await;
    assert_eq!(second, first);
    assert_eq!(
        b.game.lines(),
        ["list"],
        "une seule commande pour deux lectures"
    );
    // Aucun journal pour une lecture.
    assert!(b.journal().await.is_empty());
}

#[tokio::test]
async fn an_unreadable_answer_is_unavailable_never_a_guessed_figure() {
    let b = bench().await;
    b.game.answer("list", "Unknown command. Try /help");
    let view = b.players().await;
    assert_eq!(view["state"], "unavailable");
    for field in ["online", "max", "names"] {
        assert!(view.get(field).is_none(), "{field}");
    }
}

#[tokio::test]
async fn a_hostile_player_name_is_counted_but_not_returned() {
    let b = bench().await;
    b.game.answer(
        "list",
        "There are 2 of a max of 20 players online: Bram, <script>alert(1)</script>",
    );
    let view = b.players().await;
    assert_eq!(view["names"], json!(["Bram"]));
    assert_eq!(view["hidden_names"], 1);
    assert!(!view.to_string().contains("script"));
}

#[tokio::test]
async fn the_players_of_a_stopped_server_have_no_figure_and_nothing_is_asked() {
    let b = bench().await;
    b.stop_container().await;
    let view = b.players().await;
    assert_eq!(view["state"], "stopped");
    assert!(view.get("online").is_none());
    assert!(b.game.lines().is_empty());
}

#[tokio::test]
async fn an_unreachable_console_makes_the_players_unavailable() {
    let b = bench().await;
    b.game.fail("list", ConsoleError::Unreachable);
    let view = b.players().await;
    assert_eq!(view["state"], "unavailable");
}

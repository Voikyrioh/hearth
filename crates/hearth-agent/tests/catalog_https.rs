//! Le catalogue des fiches de serveurs de jeu, de bout en bout (HRT-101, ADR-0050) : vrai agent en HTTPS, vraie
//! base, faux moteur. Règles : BR-FICHE-001 à 004, 006, 007, 008, 061.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;

use hearth_agent::application::catalog::{CatalogService, Refused};
use hearth_agent::domain::accounts::Role;
use hearth_proto::api::catalog::{CatalogView, EvidenceView, FicheDetail, FicheStatus};
use support::engine::{fast_timings, parts_supervised};
use support::fake_engine::FakeEngine;
use support::fake_supervisor::FakeSupervisor;
use support::https::{self, Agent};
use support::{Env, env};

/// Une ligne du journal : résultat, cible, compte, origine.
type AuditRow = (String, Option<String>, Option<String>, Option<String>);

struct Bench {
    env: Env,
    agent: Agent,
    token: String,
}

async fn bench(include_preview: bool) -> Bench {
    let env = env().await;
    let mut parts = parts_supervised(
        Arc::new(FakeEngine::new()),
        Arc::new(FakeSupervisor::new()),
        fast_timings(),
    );
    parts.catalog.include_preview = include_preview;
    let agent = https::start_engined(&env, parts).await;
    let marie = agent.actor(&env, "marie", Role::ReadOnly).await;
    Bench {
        env,
        agent,
        token: marie.token,
    }
}

#[tokio::test]
async fn a_published_agent_lists_previews_with_their_reason_not_installable_and_refuses_their_detail()
 {
    let published = bench(false).await;
    let reply = published
        .agent
        .request("GET", "/catalog")
        .token(&published.token)
        .send()
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let view: CatalogView = serde_json::from_value(reply.body).unwrap();
    // Minecraft (HRT-116), Terraria (HRT-226), Valheim (HRT-228) et Enshrouded (HRT-235) sont publiées ; les huit
    // fiches `preview` sont LISTÉES avec leur raison (HRT-236) mais jamais proposées par un agent publié.
    let published_ids: Vec<&str> = view
        .fiches
        .iter()
        .filter(|f| f.status == FicheStatus::Published)
        .map(|f| f.id.as_str())
        .collect();
    assert_eq!(
        published_ids,
        ["enshrouded", "minecraft", "terraria", "valheim"],
        "les fiches publiées"
    );
    let previews: Vec<&str> = view
        .fiches
        .iter()
        .filter(|f| f.status == FicheStatus::Preview)
        .map(|f| f.id.as_str())
        .collect();
    assert_eq!(
        previews,
        [
            "7dtd",
            "corekeeper",
            "factorio",
            "palworld",
            "rust",
            "satisfactory",
            "vrising",
            "zomboid"
        ],
        "les fiches en cours de preuve"
    );
    for card in &view.fiches {
        match card.status {
            FicheStatus::Preview => {
                assert!(!card.installable, "{} n'est pas installable", card.id);
                let reason = card.preview_reason.as_deref().unwrap_or_default();
                assert!(reason.ends_with('.') && reason.len() >= 20, "{}", card.id);
            }
            _ => {
                assert!(card.installable, "{}", card.id);
                assert!(card.preview_reason.is_none(), "{}", card.id);
            }
        }
    }
    assert!(view.refused.is_empty());
    // Une fiche d'essai n'a ni détail ni installation pour un agent publié : même réponse qu'une fiche inconnue.
    for id in ["7dtd", "zomboid", "nope"] {
        let reply = published
            .agent
            .request("GET", &format!("/catalog/{id}"))
            .token(&published.token)
            .send()
            .await;
        assert_eq!(reply.status, 404, "{id}");
        assert_eq!(reply.code(), "FICHE_UNKNOWN", "{id}");
    }
    published.agent.shutdown().await;

    let trial = bench(true).await;
    let reply = trial
        .agent
        .request("GET", "/catalog")
        .token(&trial.token)
        .send()
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let view: CatalogView = serde_json::from_value(reply.body).unwrap();
    let preview = view.fiches.iter().find(|f| f.id == "7dtd").unwrap();
    assert_eq!(preview.status, FicheStatus::Preview);
    assert!(preview.installable);
    let reply = trial
        .agent
        .request("GET", "/catalog/terraria")
        .token(&trial.token)
        .send()
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let detail: FicheDetail = serde_json::from_value(reply.body).unwrap();
    assert_eq!(detail.digest.len(), 64);
    assert!(detail.digest.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_eq!(detail.image.repository, "ryshe/terraria");
    // Ressources : la limite de mémoire essayée par le banc d'essai réel (HRT-226), mesurée.
    assert_eq!(detail.resources.memory.evidence, EvidenceView::Measured);
    assert_eq!(detail.resources.memory.min_bytes, Some(2_147_483_648));
    assert_eq!(
        detail.resources.memory.recommended_bytes,
        Some(2_147_483_648)
    );
    // Ce que la fiche impose : un port de jeu, des dossiers gérés (jamais un chemin du serveur).
    assert!(
        detail
            .ports
            .iter()
            .any(|p| p.id == "game" && p.container == 7777)
    );
    assert!(detail.folders.iter().all(|f| f.target.starts_with('/')));
    trial.agent.shutdown().await;
}

#[tokio::test]
async fn the_list_carries_each_recommended_memory_with_its_evidence() {
    let b = bench(false).await;
    let reply = b
        .agent
        .request("GET", "/catalog")
        .token(&b.token)
        .send()
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let view: CatalogView = serde_json::from_value(reply.body.clone()).unwrap();
    let terraria = view.fiches.iter().find(|f| f.id == "terraria").unwrap();
    let hint = terraria
        .memory
        .as_ref()
        .expect("Terraria recommande une mémoire");
    assert_eq!(hint.recommended_bytes, 2_147_483_648);
    assert_eq!(hint.evidence, EvidenceView::Measured);
    // Une fiche sans chiffre n'a pas le membre (client ancien : ignoré, jamais requis).
    let none = view.fiches.iter().find(|f| f.id == "7dtd").unwrap();
    assert!(none.memory.is_none());
    let raw = reply.body["fiches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"] == "7dtd")
        .unwrap();
    assert!(raw.get("memory").is_none(), "le membre est absent du fil");
    b.agent.shutdown().await;
}

/// Le nombre de lignes du journal d'activité.
async fn audit_rows(b: &Bench) -> i64 {
    b.agent.flush_journal().await;
    sqlx::query_scalar("SELECT COUNT(*) FROM audit_events")
        .fetch_one(b.env.db.pool())
        .await
        .unwrap()
}

/// La carte de Minecraft, telle que la liste la rend (identifiant Steam absent, illustration présente).
async fn minecraft_card(b: &Bench) -> hearth_proto::api::catalog::FicheCard {
    let reply = b
        .agent
        .request("GET", "/catalog")
        .token(&b.token)
        .send()
        .await;
    let view: CatalogView = serde_json::from_value(reply.body).unwrap();
    view.fiches
        .into_iter()
        .find(|f| f.id == "minecraft")
        .unwrap()
}

#[tokio::test]
async fn the_card_carries_the_steam_app_id() {
    let b = bench(false).await;
    let reply = b
        .agent
        .request("GET", "/catalog")
        .token(&b.token)
        .send()
        .await;
    let view: CatalogView = serde_json::from_value(reply.body.clone()).unwrap();
    let id = |name: &str| {
        view.fiches
            .iter()
            .find(|f| f.id == name)
            .unwrap()
            .steam_app_id
    };
    assert_eq!(id("terraria"), Some(105_600));
    assert_eq!(id("valheim"), Some(892_970));
    assert_eq!(id("rust"), Some(252_490));
    assert_eq!(id("minecraft"), None);
    // Un jeu hors Steam n'a pas le membre sur le fil ; un jeu Steam n'a pas d'illustration.
    let raw = |name: &str| {
        reply.body["fiches"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["id"] == name)
            .unwrap()
            .clone()
    };
    assert!(raw("minecraft").get("steam_app_id").is_none());
    assert!(raw("terraria").get("illustration").is_none());
    b.agent.shutdown().await;
}

#[tokio::test]
async fn the_illustration_is_served_with_its_type_length_etag_and_nosniff() {
    let b = bench(false).await;
    let card = minecraft_card(&b).await;
    let illustration = card.illustration.expect("Minecraft porte son illustration");
    let reply = b
        .agent
        .request("GET", "/catalog/minecraft/illustration")
        .token(&b.token)
        .send()
        .await;
    assert_eq!(reply.status, 200);
    assert_eq!(reply.header("content-type"), Some("image/jpeg"));
    assert_eq!(
        reply.header("content-length").map(str::to_owned),
        Some(illustration.bytes.to_string())
    );
    assert_eq!(
        reply.header("etag").map(str::to_owned),
        Some(format!("\"{}\"", illustration.digest))
    );
    assert_eq!(reply.header("x-content-type-options"), Some("nosniff"));
    assert_eq!(
        reply.header("content-security-policy"),
        Some("default-src 'none'")
    );
    assert_eq!(
        reply.header("cache-control"),
        Some("private, max-age=31536000, immutable")
    );
    b.agent.shutdown().await;
}

#[tokio::test]
async fn if_none_match_gives_304() {
    let b = bench(false).await;
    let digest = minecraft_card(&b).await.illustration.unwrap().digest;
    let reply = b
        .agent
        .request("GET", "/catalog/minecraft/illustration")
        .token(&b.token)
        .header("if-none-match", &format!("\"{digest}\""))
        .send()
        .await;
    assert_eq!(reply.status, 304);
    assert!(reply.text.is_empty());
    let other = b
        .agent
        .request("GET", "/catalog/minecraft/illustration")
        .token(&b.token)
        .header("if-none-match", "\"sha256:other\"")
        .send()
        .await;
    assert_eq!(other.status, 200);
    b.agent.shutdown().await;
}

#[tokio::test]
async fn an_unknown_or_non_stable_id_is_404_without_echo() {
    let b = bench(false).await;
    for id in ["nope", "Minecraft", "x".repeat(65).as_str()] {
        let reply = b
            .agent
            .request("GET", &format!("/catalog/{id}/illustration"))
            .token(&b.token)
            .send()
            .await;
        assert_eq!(reply.status, 404, "{id}");
        assert_eq!(reply.code(), "FICHE_UNKNOWN", "{id}");
        assert!(
            !reply.text.contains(id),
            "l'identifiant reçu n'est jamais répété : {id}"
        );
    }
    b.agent.shutdown().await;
}

#[tokio::test]
async fn a_steam_fiche_has_no_illustration_404_illustration_absent() {
    let b = bench(false).await;
    // Terraria (publiée) et Zomboid (listée, en cours de preuve) : listées, sans illustration.
    for id in ["terraria", "zomboid"] {
        let reply = b
            .agent
            .request("GET", &format!("/catalog/{id}/illustration"))
            .token(&b.token)
            .send()
            .await;
        assert_eq!(reply.status, 404, "{id}");
        assert_eq!(reply.code(), "ILLUSTRATION_ABSENT", "{id}");
    }
    b.agent.shutdown().await;
}

#[tokio::test]
async fn an_anonymous_request_is_401() {
    let b = bench(false).await;
    let reply = b
        .agent
        .request("GET", "/catalog/minecraft/illustration")
        .send()
        .await;
    assert_eq!(reply.status, 401);
    b.agent.shutdown().await;
}

#[tokio::test]
async fn the_route_is_open_read_and_never_journaled() {
    let b = bench(false).await;
    let before = audit_rows(&b).await;
    for _ in 0..3 {
        b.agent
            .request("GET", "/catalog/minecraft/illustration")
            .token(&b.token)
            .send()
            .await;
    }
    assert_eq!(
        audit_rows(&b).await,
        before,
        "une lecture ne s'écrit pas au journal"
    );
    b.agent.shutdown().await;
}

#[tokio::test]
async fn the_catalog_needs_a_session_and_says_what_the_agent_can_do() {
    let b = bench(true).await;
    let reply = b.agent.request("GET", "/catalog").send().await;
    assert_eq!(reply.status, 401);
    let reply = b.agent.request("GET", "/catalog/terraria").send().await;
    assert_eq!(reply.status, 401);
    let hello = b.agent.request("GET", "/hello").send().await;
    let capabilities = hello.body["capabilities"].as_array().unwrap();
    assert!(capabilities.iter().any(|c| c == "catalog"));
    b.agent.shutdown().await;
}

#[tokio::test]
async fn the_detail_carries_no_secret_a_refused_fiche_is_counted_and_journaled_once_by_the_system()
{
    let b = bench(true).await;
    let reply = b
        .agent
        .request("GET", "/catalog/terraria")
        .token(&b.token)
        .send()
        .await;
    // Une fiche ne contient pas de secret, mais ses questions secrètes ne montrent jamais de valeur proposée.
    let detail: FicheDetail = serde_json::from_value(reply.body).unwrap();
    for question in detail.questions.iter().filter(|q| q.secret) {
        assert!(question.proposed.is_none());
    }
    // Une fiche écartée : comptée avec son invariant, consignée UNE fois, sous le compte système.
    let catalog = CatalogService::from_parts(
        Vec::new(),
        vec![Refused {
            dir: "autre-jeu".into(),
            invariant: "I2",
        }],
        true,
    );
    assert_eq!(catalog.list().refused[0].invariant, "I2");
    catalog.record_refused(b.env.audit_sink.as_ref()).await;
    b.agent.flush_journal().await;
    let rows: Vec<AuditRow> = sqlx::query_as(
        "SELECT outcome, target, account, origin_kind FROM audit_events
         WHERE action = 'catalog.fiche.refused' ORDER BY id",
    )
    .fetch_all(b.env.db.pool())
    .await
    .unwrap();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].0, "failed");
    assert_eq!(rows[0].1.as_deref(), Some("fiche autre-jeu (I2)"));
    assert_eq!(rows[0].2, None, "aucun compte : l'agent lui-même");
    assert_eq!(rows[0].3.as_deref(), Some("system"));
    b.agent.shutdown().await;
}

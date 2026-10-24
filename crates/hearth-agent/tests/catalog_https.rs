//! Le catalogue des fiches de serveurs de jeu, de bout en bout (HRT-101, ADR-0050) : vrai agent en HTTPS, vraie
//! base, faux moteur. Règles : BR-FICHE-001 à 004, 006, 007, 008.

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
async fn a_published_agent_lists_no_preview_fiche_and_a_trial_agent_lists_terraria_with_its_digest()
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
    assert!(view.fiches.is_empty(), "aucune fiche publiée aujourd'hui");
    assert!(view.refused.is_empty());
    // Une fiche d'essai n'existe pas pour un agent publié : même réponse qu'une fiche inconnue.
    for id in ["terraria", "nope"] {
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
    let terraria = view.fiches.iter().find(|f| f.id == "terraria").unwrap();
    assert_eq!(terraria.status, FicheStatus::Preview);
    assert!(terraria.installable);
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
    // Ressources : aucune preuve, aucun chiffre inventé.
    assert_eq!(detail.resources.memory.evidence, EvidenceView::Unmeasured);
    assert_eq!(detail.resources.memory.min_bytes, None);
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

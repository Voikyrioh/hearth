//! Les routes `/agent/update` et `/agent/update/last` de bout en bout en processus : réservées aux
//! administrateurs pour lancer (BR-UPDATE-011), une seule mise à jour à la fois (BR-UPDATE-012),
//! refus consignés (BR-UPDATE-024), installation gérée refusée avec un code clair, lectures
//! ouvertes à tout compte.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use axum::http::{Method, StatusCode};
use hearth_agent::domain::accounts::Role;
use hearth_agent::domain::audit::{AuditFilter, AuditRecord, RawFilter};
use serde_json::{Value, json};
use support::api::{Api, state_with};
use support::update::Rig;
use support::{Env, env};

const BINARY: &[u8] = b"nouvel agent";
const KEY: &str = "01J9ZY0G3Q8M2K6W4T7V5N1B9D";

struct Bench {
    env: Env,
    api: Api,
    rig: Rig,
    admin: String,
    readonly: String,
}

async fn bench(allowed: bool, gated: bool) -> Bench {
    let env = env().await;
    let rig = Rig::new(&env, allowed, gated);
    let api = Api::from_state(state_with(&env, rig.service.clone()));
    let admin = env.account_with_token(&api, "marie", Role::Admin).await;
    let readonly = env.account_with_token(&api, "lucas", Role::ReadOnly).await;
    Bench {
        env,
        api,
        rig,
        admin,
        readonly,
    }
}

impl Bench {
    async fn post(&self, token: &str, body: &Value) -> support::api::Reply {
        self.api
            .call(Method::POST, "/agent/update")
            .token(token)
            .json(body)
            .send()
            .await
    }

    async fn journal(&self) -> Vec<AuditRecord> {
        self.env.audit_recorder.flush_all().await;
        let filter = AuditFilter::new(RawFilter {
            actions: vec!["agent.update".to_owned()],
            ..RawFilter::default()
        })
        .unwrap();
        let mut records = self
            .env
            .audit
            .search(Role::Admin, &filter)
            .await
            .unwrap()
            .records;
        records.reverse();
        records
    }
}

#[tokio::test]
async fn an_administrator_starts_an_update_and_gets_202_with_the_first_step() {
    let bench = bench(true, true).await;
    let body = bench.rig.request("0.2.0", BINARY);
    let reply = bench.post(&bench.admin, &body).await;
    assert_eq!(reply.status, StatusCode::ACCEPTED, "{:?}", reply.body);
    assert_eq!(
        reply.body,
        json!({ "version": "0.2.0", "step": "download" })
    );

    // Pendant le téléchargement : en cours, lisible par tous.
    let status = bench
        .api
        .call(Method::GET, "/agent/update")
        .token(&bench.readonly)
        .send()
        .await;
    assert_eq!(status.status, StatusCode::OK);
    assert_eq!(status.body["in_progress"], true);
    assert_eq!(status.body["managed"], false);
    assert_eq!(status.body["current"], "0.1.0");
    assert_eq!(status.body["progress"]["step"], "download");
    bench.rig.release_gate();
}

#[tokio::test]
async fn a_read_only_account_is_refused_and_the_refusal_is_journaled() {
    let bench = bench(true, false).await;
    let body = bench.rig.request("0.2.0", BINARY);
    let reply = bench.post(&bench.readonly, &body).await;
    assert_eq!(
        (reply.status, reply.code()),
        (StatusCode::FORBIDDEN, "FORBIDDEN_ROLE")
    );
    assert_eq!(
        reply.body["error"]["message"],
        "Seul un administrateur peut mettre à jour l'agent"
    );
    assert!(bench.rig.downloader.fetched.lock().unwrap().is_empty());
    let entries = bench.journal().await;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].outcome.code(), "denied");
    assert_eq!(entries[0].account.as_deref(), Some("lucas"));
    assert_eq!(entries[0].action_label, "Mise à jour de l'agent");
}

#[tokio::test]
async fn without_a_session_the_route_answers_401() {
    let bench = bench(true, false).await;
    let reply = bench
        .api
        .call(Method::POST, "/agent/update")
        .json(&bench.rig.request("0.2.0", BINARY))
        .send()
        .await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_second_request_during_an_update_is_refused_with_the_spec_message_and_journaled_once() {
    let bench = bench(true, true).await;
    let body = bench.rig.request("0.2.0", BINARY);
    assert_eq!(
        bench.post(&bench.admin, &body).await.status,
        StatusCode::ACCEPTED
    );

    let reply = bench.post(&bench.admin, &body).await;
    assert_eq!(
        (reply.status, reply.code()),
        (StatusCode::CONFLICT, "OPERATION_IN_PROGRESS")
    );
    assert_eq!(
        reply.body["error"]["message"],
        "Une mise à jour de l'agent est déjà en cours. Réessaye plus tard."
    );
    let entries = bench.journal().await;
    assert_eq!(entries.len(), 1, "une seule entrée : {entries:?}");
    assert_eq!(entries[0].outcome.code(), "failed");
    assert_eq!(bench.rig.downloader.fetched.lock().unwrap().len(), 1);
    bench.rig.release_gate();
}

#[tokio::test]
async fn replaying_the_same_operation_key_after_a_cut_does_not_start_a_second_update() {
    // Le client a perdu la réponse (coupure) : il rejoue avec la même clé et reçoit la même
    // réponse ; la mise à jour, elle, n'est partie qu'une fois (BR-UPDATE-017).
    let bench = bench(true, true).await;
    let body = bench.rig.request("0.2.0", BINARY);
    let first = bench
        .api
        .call(Method::POST, "/agent/update")
        .token(&bench.admin)
        .key(KEY)
        .json(&body)
        .send()
        .await;
    assert_eq!(first.status, StatusCode::ACCEPTED);
    let again = bench
        .api
        .call(Method::POST, "/agent/update")
        .token(&bench.admin)
        .key(KEY)
        .json(&body)
        .send()
        .await;
    assert_eq!(again.status, StatusCode::ACCEPTED);
    assert_eq!(again.body, first.body);
    assert_eq!(bench.rig.downloader.fetched.lock().unwrap().len(), 1);
    bench.rig.release_gate();
}

#[tokio::test]
async fn a_managed_installation_answers_a_clear_code_and_nothing_is_downloaded() {
    let bench = bench(false, false).await;
    let reply = bench
        .post(&bench.admin, &bench.rig.request("0.2.0", BINARY))
        .await;
    assert_eq!(
        (reply.status, reply.code()),
        (StatusCode::CONFLICT, "MANAGED_INSTALL")
    );
    assert!(bench.rig.downloader.fetched.lock().unwrap().is_empty());
    let entries = bench.journal().await;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].outcome.code(), "failed");

    let status = bench
        .api
        .call(Method::GET, "/agent/update")
        .token(&bench.readonly)
        .send()
        .await;
    assert_eq!(status.body["managed"], true);
    assert_eq!(status.body["in_progress"], false);
}

#[tokio::test]
async fn a_signature_by_another_key_is_refused_with_bad_signature_and_nothing_runs() {
    let bench = bench(true, false).await;
    let stranger = support::update::Keys::generate();
    let mut body = bench.rig.request("0.2.0", BINARY);
    body["signature"] = json!(stranger.sign(BINARY));
    let reply = bench.post(&bench.admin, &body).await;
    assert_eq!(
        (reply.status, reply.code()),
        (StatusCode::UNPROCESSABLE_ENTITY, "BAD_SIGNATURE")
    );
    assert!(bench.rig.downloader.fetched.lock().unwrap().is_empty());
    bench.rig.host.with(|s| {
        assert!(s.staged.is_none() && s.job.is_none() && s.launched.is_empty());
    });
    let entries = bench.journal().await;
    assert_eq!(entries.len(), 1);
    assert!(
        entries[0]
            .reason
            .as_deref()
            .unwrap()
            .contains("signature invalide")
    );
}

#[tokio::test]
async fn invalid_bodies_are_validation_errors_naming_the_field() {
    let bench = bench(true, false).await;
    let good = bench.rig.request("0.2.0", BINARY);
    for (field, value) in [
        ("version", json!("0.1.0")),
        ("version", json!("bientôt")),
        ("url", json!("http://exemple.org/x")),
        ("sha256", json!("abc")),
        ("signature", json!("")),
    ] {
        let mut body = good.clone();
        body[field] = value.clone();
        let reply = bench.post(&bench.admin, &body).await;
        assert_eq!(
            (reply.status, reply.code()),
            (StatusCode::UNPROCESSABLE_ENTITY, "VALIDATION_ERROR"),
            "{field} = {value}"
        );
        assert_eq!(reply.body["error"]["details"]["field"], field, "{value}");
    }
    let reply = bench
        .api
        .call(Method::POST, "/agent/update")
        .token(&bench.admin)
        .raw_body("pas du json")
        .send()
        .await;
    assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(bench.rig.downloader.fetched.lock().unwrap().is_empty());
}

#[tokio::test]
async fn the_last_result_is_readable_by_any_account_and_empty_before_the_first_update() {
    let bench = bench(true, false).await;
    let empty = bench
        .api
        .call(Method::GET, "/agent/update/last")
        .token(&bench.readonly)
        .send()
        .await;
    assert_eq!(empty.status, StatusCode::OK);
    assert_eq!(empty.body, json!({ "last": null }));

    // Une mise à jour qui échoue (somme fausse) : le résultat reste lisible.
    let mut body = bench.rig.request("0.2.0", BINARY);
    body["sha256"] = json!("00".repeat(32));
    assert_eq!(
        bench.post(&bench.admin, &body).await.status,
        StatusCode::ACCEPTED
    );
    // Le résultat est écrit, puis l'état « en cours » tombe : on attend la fin.
    for _ in 0..200 {
        let status = bench
            .api
            .call(Method::GET, "/agent/update")
            .token(&bench.readonly)
            .send()
            .await;
        if status.body["in_progress"] == false && !status.body["last"].is_null() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let last = bench
        .api
        .call(Method::GET, "/agent/update/last")
        .token(&bench.readonly)
        .send()
        .await
        .body["last"]
        .clone();
    assert_eq!(last["outcome"], "failed");
    assert_eq!(last["reason"], "bad_checksum");
    assert_eq!(last["version"], "0.2.0");
    assert_eq!(last["previous"], "0.1.0");
    // Le même résultat par l'état (comme `last`), et plus rien en cours.
    let status = bench
        .api
        .call(Method::GET, "/agent/update")
        .token(&bench.readonly)
        .send()
        .await;
    assert_eq!(status.body["last"], last);
    assert_eq!(status.body["in_progress"], false);
}

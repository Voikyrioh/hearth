//! BR-COMPAT-001 point 8 : un code d'erreur inconnu (agent plus récent) retombe sur le sens du statut HTTP
//! dans chacun des interpréteurs de la coquille, jamais sur un succès ni sur un refus passe-partout qui
//! cacherait un refus de rôle ou une session finie.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use hearth_desktop_lib::accounts::wire as accounts;
use hearth_desktop_lib::agent_update::wire as agent_update;
use hearth_desktop_lib::devices::service as devices;
use hearth_desktop_lib::reauth::service as reauth;
use hearth_desktop_lib::security::service as security;
use serde_json::{Value, json};

fn future(retry: Option<u64>) -> Value {
    let details = retry.map_or(json!({}), |seconds| json!({ "retry_after_s": seconds }));
    json!({ "error": { "code": "CODE_DU_FUTUR", "message": "m", "details": details } })
}

fn said<T: std::fmt::Debug>(outcome: &Result<T, impl std::fmt::Debug>) -> String {
    format!("{outcome:?}")
}

#[test]
fn accounts_read_the_status_of_a_future_code() {
    let refusal = |status| said(&accounts::refusal_from_error(status, &future(None)));
    assert!(refusal(403).contains("Forbidden"), "{}", refusal(403));
    assert!(refusal(404).contains("NotFound"));
    assert!(refusal(409).contains("Conflict"));
    assert!(refusal(401).contains("SessionEnded"));
    assert!(refusal(503).contains("Busy"));
    assert!(refusal(500).contains("Other"));
    assert!(refusal(422).contains("Other"));
}

#[test]
fn the_agent_update_reads_the_status_of_a_future_code() {
    let outcome = |status| said(&agent_update::interpret(status, &future(None)));
    assert!(outcome(403).contains("Forbidden"), "{}", outcome(403));
    assert!(outcome(409).contains("Other"));
    assert!(outcome(422).contains("InvalidTarget"));
    assert!(!outcome(500).contains("Accepted"));
}

#[test]
fn the_reauth_setting_reads_the_status_of_a_future_code() {
    let outcome = |status| said(&reauth::interpret(status, &future(None)));
    assert!(outcome(403).contains("Forbidden"), "{}", outcome(403));
    assert!(outcome(404).contains("Unsupported"));
    assert!(outcome(401).contains("SessionEnded"));
    assert!(outcome(500).contains("Other"));
}

#[test]
fn the_attack_mode_reads_the_status_of_a_future_code() {
    let outcome = |status| said(&security::interpret(status, &future(None)));
    assert!(outcome(403).contains("Forbidden"), "{}", outcome(403));
    assert!(outcome(404).contains("Unsupported"));
    assert!(outcome(401).contains("SessionEnded"));
    assert!(outcome(500).contains("Other"));
    assert!(!outcome(500).contains("Done"));
}

#[test]
fn the_device_removal_reads_the_status_of_a_future_code() {
    let outcome = |status, retry| said(&devices::interpret(status, &future(retry)));
    assert!(
        outcome(403, None).contains("Forbidden"),
        "{}",
        outcome(403, None)
    );
    assert!(outcome(404, None).contains("NotFound"));
    assert!(outcome(401, None).contains("SessionEnded"));
    assert!(
        outcome(429, Some(30)).contains("30"),
        "{}",
        outcome(429, Some(30))
    );
    assert!(outcome(500, None).contains("Other"));
    assert!(!outcome(500, None).contains("Done"));
}

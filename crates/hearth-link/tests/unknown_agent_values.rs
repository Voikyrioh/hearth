//! Un agent PLUS RÉCENT que le client (FIX:01M4VR6Y4Q4AVQVT9FCME7Z2SB, BR-COMPAT-001) : il envoie, pendant le
//! suivi d'une mise à jour, une étape et une issue que ce client ne connaît pas, un type de message nouveau, un
//! code d'erreur nouveau, une fin de session au motif nouveau et un champ en trop. Le client reçoit l'événement
//! avec la valeur de repli (`Unknown`), jamais pris pour une valeur connue, et le flux CONTINUE (la trame
//! suivante est lue). Le vrai agent ne sait émettre que ce qu'il connaît : on utilise `FutureAgent`, un faux
//! agent TLS 1.3 + WebSocket qui rejoue des trames brutes contre le vrai `HttpTransport`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "support/future_agent.rs"]
mod future_agent;

use std::time::Duration;

use future_agent::FutureAgent;
use hearth_link::adapters::{HttpTransport, HttpTransportConfig};
use hearth_link::ports::transport::{Frame, Pin, StreamConn, Target, Transport};
use hearth_proto::api::update::{UpdateOutcome, UpdateReason, UpdateStep};
use hearth_proto::error::ErrorCode;
use hearth_proto::stream::{ServerMessage, SessionNotice};
use serde_json::json;
use tokio::time::timeout;

fn frames() -> Vec<String> {
    [
        // Une trame connue en tête : le flux est vivant.
        json!({ "type": "pong", "n": 1 }),
        // L'étape inconnue pendant le suivi, avec un champ en trop.
        json!({ "type": "update", "version": "9.9.9", "step": "etape_du_futur", "percent": null,
                "outcome": null, "reason": null, "champ_du_futur": { "a": 1 } }),
        // L'étape finale connue, mais une issue et une raison inconnues.
        json!({ "type": "update", "version": "9.9.9", "step": "done", "percent": null,
                "outcome": "issue_du_futur", "reason": "raison_du_futur" }),
        // Un type de message que ce client ne connaît pas.
        json!({ "type": "message_du_futur", "x": 1 }),
        // Un code d'erreur nouveau (le message et les détails restent lus).
        json!({ "type": "error", "code": "CODE_DU_FUTUR", "message": "m", "details": {} }),
        // Une fin de session au motif nouveau.
        json!({ "type": "session", "kind": "motif_du_futur" }),
        // Le flux continue : une dernière trame connue.
        json!({ "type": "pong", "n": 2 }),
    ]
    .into_iter()
    .map(|frame| frame.to_string())
    .collect()
}

async fn next_frame(stream: &mut Box<dyn StreamConn>) -> Frame {
    timeout(Duration::from_secs(10), stream.recv())
        .await
        .expect("trame attendue")
        .expect("flux vivant")
}

#[tokio::test]
async fn a_newer_agent_update_stream_is_read_with_fallback_values_and_keeps_flowing() {
    let agent = FutureAgent::start(frames()).await;
    let transport = HttpTransport::new(HttpTransportConfig::default());
    let target = Target {
        host: "127.0.0.1".into(),
        port: agent.port,
        pin: Pin::Pinned(agent.fingerprint),
    };
    let mut stream = transport.open_stream(&target).await.expect("flux ouvert");

    assert!(matches!(
        next_frame(&mut stream).await,
        Frame::Message(message) if matches!(*message, ServerMessage::Pong { n: 1 })
    ));

    // Étape inconnue : l'événement arrive, avec la valeur de repli.
    let Frame::Message(message) = next_frame(&mut stream).await else {
        panic!("l'étape inconnue doit arriver comme un message, pas être ignorée");
    };
    let ServerMessage::Update(progress) = *message else {
        panic!("message inattendu");
    };
    assert_eq!(progress.version, "9.9.9");
    assert_eq!(progress.step, UpdateStep::Unknown);

    // Issue et raison inconnues : ni réussite ni échec affirmés.
    let Frame::Message(message) = next_frame(&mut stream).await else {
        panic!("l'issue inconnue doit arriver comme un message");
    };
    let ServerMessage::Update(progress) = *message else {
        panic!("message inattendu");
    };
    assert_eq!(progress.step, UpdateStep::Done);
    assert_eq!(progress.outcome, Some(UpdateOutcome::Unknown));
    assert_ne!(progress.outcome, Some(UpdateOutcome::Succeeded));
    assert_ne!(progress.outcome, Some(UpdateOutcome::Failed));
    assert_eq!(progress.reason, Some(UpdateReason::Unknown));

    // Un type de message nouveau : ignoré (Other), le flux continue.
    assert!(matches!(next_frame(&mut stream).await, Frame::Other));

    // Un code d'erreur nouveau : le message d'erreur est lu avec le code de repli.
    let Frame::Message(message) = next_frame(&mut stream).await else {
        panic!("l'erreur au code inconnu doit arriver comme un message");
    };
    let ServerMessage::Error(detail) = *message else {
        panic!("message inattendu");
    };
    assert_eq!(detail.code, ErrorCode::Unknown);
    assert_eq!(detail.message, "m");

    // Une fin de session au motif nouveau : lue `Unknown`, ni expirée ni révoquée.
    let Frame::Message(message) = next_frame(&mut stream).await else {
        panic!("la fin de session au motif inconnu doit arriver comme un message");
    };
    assert!(matches!(
        *message,
        ServerMessage::Session {
            kind: SessionNotice::Unknown
        }
    ));

    // Et la trame suivante est lue : rien n'a cassé le flux.
    assert!(matches!(
        next_frame(&mut stream).await,
        Frame::Message(message) if matches!(*message, ServerMessage::Pong { n: 2 })
    ));
}

//! Messages du flux temps réel `GET /api/v1/stream` (WebSocket, un message JSON par trame texte).
//!
//! Chaque message est un objet à champ `type`. Déroulement :
//!
//! 1. le client ouvre le WebSocket (en-tête `X-Hearth-Api` sur la requête d'ouverture) et envoie
//!    `auth` **en premier message**, dans les [`AUTH_TIMEOUT_S`] secondes ; sinon, ou si le jeton
//!    est refusé, l'agent répond `error` puis ferme ;
//! 2. le client envoie `subscribe` avec les sujets voulus ; pour `metrics`, l'agent répond par un
//!    `snapshot` (identité + 5 minutes d'historique) puis par un `metrics` à chaque échantillon ;
//! 3. le client envoie `ping` (toutes les 2 s), l'agent répond `pong` avec le même `n` ;
//! 4. si la session prend fin pendant le flux, l'agent envoie `session` puis ferme.
//!
//! Aucun type de framework ici : le jeton de `auth` n'apparaît jamais dans un `Debug`.

use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::machine::MachineResponse;
use crate::api::metrics::Sample;
use crate::error::ErrorDetail;

/// Délai laissé au client pour envoyer `auth` après l'ouverture.
pub const AUTH_TIMEOUT_S: u64 = 5;

/// Taille maximale d'un message du client, en octets. Au-delà, l'agent ferme le flux.
pub const MAX_CLIENT_MESSAGE_BYTES: usize = 4096;

/// Un sujet auquel s'abonner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Topic {
    /// Identité, historique, puis un échantillon par seconde.
    Metrics,
    /// Événements du journal d'activité (administrateurs seulement).
    Audit,
    /// Fin de session : toujours reçue, même sans abonnement.
    Session,
}

/// Messages du client vers l'agent.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Jeton de session (`Authorization: Bearer` ne sert pas : l'ouverture d'un WebSocket depuis
    /// un client web ne peut pas porter d'en-tête, le jeton voyage dans ce premier message).
    Auth {
        token: String,
    },
    Subscribe {
        topics: Vec<Topic>,
    },
    Ping {
        n: u64,
    },
}

impl fmt::Debug for ClientMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Auth { .. } => f.debug_struct("Auth").field("token", &"***").finish(),
            Self::Subscribe { topics } => {
                f.debug_struct("Subscribe").field("topics", topics).finish()
            }
            Self::Ping { n } => f.debug_struct("Ping").field("n", n).finish(),
        }
    }
}

/// Pourquoi la session du flux a pris fin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionNotice {
    Revoked,
    Expired,
}

/// Messages de l'agent vers le client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Premier message après l'abonnement à `metrics` : de quoi afficher sans attendre.
    Snapshot {
        machine: MachineResponse,
        /// Les 5 dernières minutes, un échantillon par seconde, du plus ancien au plus récent.
        history: Vec<Sample>,
    },
    /// Un échantillon, chaque seconde (les champs de [`Sample`] sont à plat à côté de `type`).
    Metrics(Sample),
    /// Un événement du journal d'activité, dans la forme que le journal lui donne.
    Audit {
        event: Value,
    },
    Session {
        kind: SessionNotice,
    },
    Pong {
        n: u64,
    },
    /// Erreur du protocole (mêmes codes que l'API HTTP) ; l'agent ferme ensuite le flux quand
    /// l'erreur est fatale (authentification), pas quand il s'agit d'un message mal formé.
    Error(ErrorDetail),
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::api::metrics::MemorySample;
    use crate::error::ErrorCode;

    fn sample() -> Sample {
        Sample {
            at: "2026-10-04T10:30:15.250Z".into(),
            uptime_s: 1,
            cpu: 1.5,
            cores: vec![1.5],
            mem: MemorySample {
                used_bytes: 1,
                total_bytes: 2,
            },
            disks: vec![],
            net: None,
            gpus: vec![],
            temps: vec![],
        }
    }

    #[test]
    fn client_messages_carry_their_type() {
        let auth: ClientMessage =
            serde_json::from_value(json!({ "type": "auth", "token": "abc" })).expect("auth");
        assert_eq!(
            auth,
            ClientMessage::Auth {
                token: "abc".into()
            }
        );
        let subscribe: ClientMessage =
            serde_json::from_value(json!({ "type": "subscribe", "topics": ["metrics", "audit"] }))
                .expect("subscribe");
        assert_eq!(
            subscribe,
            ClientMessage::Subscribe {
                topics: vec![Topic::Metrics, Topic::Audit]
            }
        );
        let ping: ClientMessage =
            serde_json::from_value(json!({ "type": "ping", "n": 7 })).expect("ping");
        assert_eq!(ping, ClientMessage::Ping { n: 7 });
    }

    #[test]
    fn an_unknown_type_or_topic_is_refused() {
        assert!(serde_json::from_value::<ClientMessage>(json!({ "type": "nope" })).is_err());
        assert!(
            serde_json::from_value::<ClientMessage>(
                json!({ "type": "subscribe", "topics": ["nope"] })
            )
            .is_err()
        );
        assert!(serde_json::from_value::<ClientMessage>(json!({ "token": "abc" })).is_err());
    }

    #[test]
    fn debug_hides_the_token() {
        let message = ClientMessage::Auth {
            token: "secret-token".into(),
        };
        assert!(!format!("{message:?}").contains("secret-token"));
    }

    #[test]
    fn metrics_fields_sit_next_to_the_type() {
        let json = serde_json::to_value(ServerMessage::Metrics(sample())).expect("json");
        assert_eq!(json["type"], "metrics");
        assert_eq!(json["at"], "2026-10-04T10:30:15.250Z");
        assert_eq!(json["cpu"], 1.5);
        assert!(json["cores"].is_array());
    }

    #[test]
    fn server_messages_round_trip() {
        let messages = [
            ServerMessage::Metrics(sample()),
            ServerMessage::Pong { n: 3 },
            ServerMessage::Session {
                kind: SessionNotice::Revoked,
            },
            ServerMessage::Audit {
                event: json!({ "id": 1 }),
            },
            ServerMessage::Error(ErrorDetail {
                code: ErrorCode::Unauthenticated,
                message: "non".into(),
                details: json!({}),
            }),
        ];
        for message in messages {
            let text = serde_json::to_string(&message).expect("serialization");
            let back: ServerMessage = serde_json::from_str(&text).expect("deserialization");
            assert_eq!(back, message, "{text}");
        }
    }

    #[test]
    fn the_error_message_carries_the_http_error_fields() {
        let json = serde_json::to_value(ServerMessage::Error(ErrorDetail {
            code: ErrorCode::SessionRevoked,
            message: "x".into(),
            details: json!({}),
        }))
        .expect("json");
        assert_eq!(json["type"], "error");
        assert_eq!(json["code"], "SESSION_REVOKED");
    }
}

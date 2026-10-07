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

use crate::api::audit::AuditEventItem;
use crate::api::machine::MachineResponse;
use crate::api::metrics::Sample;
use crate::api::security::SecurityView;
use crate::api::sessions::DeviceProof;
use crate::api::update::UpdateProgress;
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
    /// Progression de la mise à jour de l'agent (tout compte authentifié) : l'état courant à
    /// l'abonnement, puis un message par changement d'étape ou de pourcentage entier.
    Update,
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

/// Le premier message `auth` avec, en option, la preuve de la clé d'appareil (HRT-22) : sur le
/// fil, le même objet que `ClientMessage::Auth` plus `device`. Un client sans clé envoie `auth`
/// avec le jeton seul et l'agent le lit comme avant. L'agent lit le premier message par ce type ;
/// `ClientMessage::Auth` reste celui des clients existants.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SignedAuth {
    Auth {
        token: String,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "crate::api::sessions::lenient_proof"
        )]
        device: Option<DeviceProof>,
    },
}

impl fmt::Debug for SignedAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Auth { device, .. } => f
                .debug_struct("Auth")
                .field("token", &"***")
                .field("device", device)
                .finish(),
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

/// Message du sujet `update` tel que l'AGENT l'envoie. Sur le fil, il est identique à
/// [`ServerMessage::Update`] (même `type`, mêmes champs à plat, voir le test
/// `the_two_shapes_of_an_update_message_are_the_same_on_the_wire`) : l'agent garde ce type tant
/// qu'il n'est pas touché pour autre chose, le CLIENT lit `ServerMessage::Update` (ADR-0021). Un
/// client plus ancien, qui ne connaît pas la variante, lit la trame comme inconnue et l'ignore.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UpdateMessage {
    Update(UpdateProgress),
}

/// Message `security` du flux (HRT-24, BR-TRUST-008) : l'état de sécurité du compte connecté, envoyé
/// toujours (sans abonnement) une fois après l'`auth`, puis à chaque changement. Comme
/// [`UpdateMessage`], il n'est pas un [`ServerMessage`] : un client qui ne le connaît pas l'ignore
/// (trame inconnue).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SecurityMessage {
    Security(SecurityView),
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
    /// Un événement du journal d'activité (administrateurs seulement), dans la même forme que
    /// `GET /audit`.
    Audit {
        event: AuditEventItem,
    },
    Session {
        kind: SessionNotice,
    },
    Pong {
        n: u64,
    },
    /// Progression de la mise à jour de l'agent (sujet `update`) : l'état courant à l'abonnement,
    /// puis un message par changement d'étape ou de pourcentage entier. Les champs de
    /// [`UpdateProgress`] sont à plat à côté de `type`, comme ceux de `Metrics`.
    Update(UpdateProgress),
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
    fn the_two_shapes_of_an_update_message_are_the_same_on_the_wire() {
        let message = UpdateMessage::Update(UpdateProgress {
            version: "0.2.0".into(),
            step: crate::api::update::UpdateStep::Download,
            percent: Some(35),
            outcome: None,
            reason: None,
        });
        let value = serde_json::to_value(&message).expect("json");
        assert_eq!(value["type"], "update");
        assert_eq!(value["step"], "download");
        assert_eq!(value["percent"], 35);
        // `ServerMessage` le connaît aussi (ADR-0021) : même forme sur le fil, le client lit l'une
        // ou l'autre ; un client plus ancien le lit comme une trame inconnue.
        let ServerMessage::Update(progress) =
            serde_json::from_value::<ServerMessage>(value.clone()).expect("server message")
        else {
            panic!("variante inattendue");
        };
        assert_eq!(UpdateMessage::Update(progress.clone()), message);
        assert_eq!(
            serde_json::to_value(ServerMessage::Update(progress)).expect("json"),
            value
        );
        // Un type que ni l'un ni l'autre ne connaît (un sujet futur) reste une erreur de lecture,
        // que la bibliothèque de liaison traite comme une trame sans intérêt.
        assert!(
            serde_json::from_value::<ServerMessage>(json!({ "type": "something_new", "x": 1 }))
                .is_err()
        );
        assert_eq!(
            serde_json::from_value::<UpdateMessage>(value).expect("update"),
            message
        );
        let subscribe: ClientMessage =
            serde_json::from_value(json!({ "type": "subscribe", "topics": ["update"] }))
                .expect("subscribe");
        assert_eq!(
            subscribe,
            ClientMessage::Subscribe {
                topics: vec![Topic::Update]
            }
        );
    }

    #[test]
    fn the_security_message_is_flat_on_the_wire_and_unknown_to_the_server_messages() {
        use crate::api::security::{AlertInfo, AttackModeInfo};
        let message = SecurityMessage::Security(SecurityView {
            alert: AlertInfo {
                own: true,
                since: Some("2026-10-07T01:00:00Z".into()),
                others: None,
            },
            attack_mode: AttackModeInfo::off(),
        });
        let value = serde_json::to_value(&message).expect("json");
        assert_eq!(value["type"], "security");
        assert_eq!(value["alert"]["own"], true);
        assert_eq!(value["attack_mode"]["state"], "off");
        assert_eq!(
            serde_json::from_value::<SecurityMessage>(value.clone()).expect("security"),
            message
        );
        // Un client qui ne connaît que `ServerMessage` lit la trame comme inconnue : il l'ignore.
        assert!(serde_json::from_value::<ServerMessage>(value).is_err());
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
    fn the_first_message_reads_with_or_without_a_device_proof() {
        let plain: SignedAuth =
            serde_json::from_value(json!({ "type": "auth", "token": "abc" })).expect("auth");
        assert_eq!(
            plain,
            SignedAuth::Auth {
                token: "abc".into(),
                device: None
            }
        );
        let signed: SignedAuth = serde_json::from_value(json!({
            "type": "auth", "token": "abc",
            "device": { "algorithm": "ed25519", "public_key": "k", "challenge": "c", "signature": "s" }
        }))
        .expect("auth signé");
        let SignedAuth::Auth { device, .. } = &signed;
        assert_eq!(
            device.as_ref().map(|d| d.algorithm.as_str()),
            Some("ed25519")
        );
        // Le message avec preuve se lit encore comme l'ancien : le champ en plus est ignoré.
        let old: ClientMessage = serde_json::from_value(json!({
            "type": "auth", "token": "abc", "device": { "algorithm": "ed25519" }
        }))
        .expect("ancien type");
        assert_eq!(
            old,
            ClientMessage::Auth {
                token: "abc".into()
            }
        );
        // Une preuve de la mauvaise forme est une preuve absente, jamais un auth refusé.
        let bad: SignedAuth =
            serde_json::from_value(json!({ "type": "auth", "token": "abc", "device": 5 }))
                .expect("auth tolérant");
        let SignedAuth::Auth { device, .. } = &bad;
        assert!(device.is_none());
        // Un autre type de message n'est jamais pris pour un auth.
        assert!(
            serde_json::from_value::<SignedAuth>(json!({ "type": "subscribe", "topics": [] }))
                .is_err()
        );
        assert!(!format!("{signed:?}").contains("abc"));
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
                event: crate::api::audit::AuditEventItem {
                    id: 1,
                    at: "2026-10-04T10:30:15.250Z".into(),
                    account: Some("marie".into()),
                    origin: crate::api::audit::AuditOrigin {
                        kind: crate::api::audit::OriginKindName::Cli,
                        name: None,
                        addr: None,
                        text: "ligne de commande du serveur".into(),
                    },
                    action: "login".into(),
                    action_label: "Connexion".into(),
                    target: None,
                    outcome: crate::api::audit::OutcomeName::Ok,
                    reason: None,
                    repeat_count: 0,
                },
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

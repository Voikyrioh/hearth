//! Socket des journaux d'un service, `GET /api/v1/services/{key}/logs` (HRT-66, ADR-0038, BR-SLOG-*).
//!
//! UN WebSocket par lecture (jamais le flux commun `/stream` : à 1 000 lignes par seconde un lecteur
//! lent ferait fermer tout le flux, donc le lien). Déroulement :
//!
//! 1. le client ouvre le WebSocket (en-tête `X-Hearth-Api` sur l'ouverture) et envoie `auth` en premier
//!    message, dans les [`crate::stream::AUTH_TIMEOUT_S`] secondes (même forme que `/stream` :
//!    [`crate::stream::SignedAuth`]) ;
//! 2. il envoie `open { tail, since? }` ; l'agent répond par `status` puis, au fil de l'eau, `lines`,
//!    `gap`, `marker` ;
//! 3. `ping { n }` toutes les 2 s, `pong { n }` en réponse ; 30 s de silence ferment le socket ;
//! 4. une erreur terminale (`error { code }`) est suivie de la fermeture ; fermer le socket arrête la
//!    lecture côté serveur et libère la place (dix lectures au plus pour tout l'agent).
//!
//! Évolution : un client ignore un message de type inconnu et lit toute valeur inconnue d'une énumération
//! comme sa variante `Unknown` (un code d'erreur inconnu est une erreur terminale générique), comme
//! `api::services` (ADR-0038).
//!
//! Les lignes sont déjà nettoyées ([`crate::log_text::clean`]) et bornées ([`crate::log_text::MAX_LINE_CHARS`]).
//! Les heures sont des millisecondes depuis l'époque Unix (un entier que tout client lit sans perte,
//! contrairement à des nanosecondes). Le contenu d'une ligne ne figure dans aucun `Debug`.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Capacité annoncée par `GET /hello` (`capabilities`) quand l'agent sert le socket des journaux d'un
/// service. Un client n'ouvre l'onglet Journaux qu'à un agent qui l'annonce.
pub const CAPABILITY_SERVICE_LOGS: &str = "service-logs";

/// Nombre de lignes de la première page (BR-SLOG-001).
pub const TAIL_LINES: u32 = 1_000;
/// Lectures simultanées au plus, pour tout l'agent et tous les comptes (BR-SLOG-020).
pub const MAX_READS: usize = 10;
/// Un message `lines` part au plus toutes les 100 ms (BR-SLOG-007)...
pub const BATCH_WINDOW_MS: u64 = 100;
/// ... dès 500 lignes...
pub const BATCH_LINES: usize = 500;
/// ... ou dès 256 Kio de texte.
pub const BATCH_BYTES: usize = 256 * 1024;
/// Anneau d'une lecture : 5 000 lignes...
pub const RING_LINES: usize = 5_000;
/// ... et 2 Mio de texte.
pub const RING_BYTES: usize = 2 * 1024 * 1024;

/// Le chemin de la route, avec la clé du service.
pub fn path(key: &str) -> String {
    format!("/services/{key}/logs")
}

/// D'où vient une ligne : la sortie normale ou la sortie d'erreur du service (BR-SLOG-012). Un
/// service lancé avec un terminal n'a qu'un flux : tout est `Out` et `status.mixed` est vrai.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogSource {
    #[serde(rename = "o")]
    Out,
    #[serde(rename = "e")]
    Err,
    /// Une sortie que ce client ne connaît pas (ajoutée par un agent plus récent). L'agent n'en produit
    /// jamais ; un client la traite comme la sortie normale.
    #[serde(other)]
    Unknown,
}

/// Le curseur de reprise : l'heure de la dernière ligne reçue et son rang parmi les lignes de la même
/// milliseconde, dans l'ordre du moteur (BR-SLOG-014). L'agent relit à partir de l'heure et saute
/// jusqu'à ce rang : ni doublon ni perte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogCursor {
    pub ts: i64,
    pub rank: u32,
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn default_tail() -> u32 {
    TAIL_LINES
}

/// Messages du client après `auth`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LogsClientMessage {
    /// Ouvre la lecture. `tail` : nombre de dernières lignes (plafonné à [`TAIL_LINES`]) ; `since` :
    /// reprise après une coupure, à la place des dernières lignes.
    Open {
        #[serde(default = "default_tail")]
        tail: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        since: Option<LogCursor>,
    },
    Ping {
        n: u64,
    },
}

/// L'état de la lecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogState {
    /// Le service tourne : les nouvelles lignes arrivent.
    Live,
    /// Le service est arrêté : les dernières lignes que le moteur a gardées, rien de nouveau tant
    /// qu'il ne redémarre pas (BR-SLOG-008).
    Ended,
    /// Le service n'a pas de conteneur pour l'instant ; la lecture repart quand il revient.
    Waiting,
    /// Un état que ce client ne connaît pas (ajouté par un agent plus récent).
    #[serde(other)]
    Unknown,
}

/// Envoyé à l'ouverture puis à chaque changement d'état.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogsStatus {
    pub state: LogState,
    /// Les douze premiers caractères de l'identifiant du conteneur lu (absent : aucun conteneur).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    /// Le service mélange ses deux sorties (terminal) : `LogSource::Err` n'existe pas (BR-SLOG-012).
    #[serde(default)]
    pub mixed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i64>,
    /// Heure de l'arrêt, millisecondes depuis l'époque.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<i64>,
    /// Le conteneur d'un service géré par le système est supprimé à chaque arrêt : le moteur n'a
    /// gardé aucune ligne (BR-SLOG-008).
    #[serde(default, skip_serializing_if = "is_false")]
    pub not_kept: bool,
}

/// Une ligne.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogItem {
    /// Heure du moteur, millisecondes depuis l'époque.
    pub t: i64,
    pub s: LogSource,
    /// Le texte, déjà nettoyé et coupé à `MAX_LINE_CHARS` caractères.
    pub x: String,
    /// Si la ligne a été coupée : son nombre total de caractères.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cut: Option<u32>,
}

impl fmt::Debug for LogItem {
    /// Le texte d'une ligne de journal est du contenu sensible : jamais dans un `Debug`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LogItem")
            .field("t", &self.t)
            .field("s", &self.s)
            .field("chars", &self.x.chars().count())
            .field("cut", &self.cut)
            .finish()
    }
}

/// Un lot de lignes, dans l'ordre du moteur.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogLines {
    /// Numéro du lot dans cette lecture, de 1 en 1.
    pub seq: u64,
    pub items: Vec<LogItem>,
    /// Le curseur de reprise de la DERNIÈRE ligne de ce lot.
    pub cursor: LogCursor,
}

/// Pourquoi des lignes manquent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GapReason {
    /// Le lecteur était trop lent : l'agent a gardé les lignes les plus récentes et compté les autres.
    Slow,
    /// Le moteur a fait tourner ses fichiers (réservé : aucune lecture ne le produit encore).
    Rotated,
    /// La ligne de reprise n'existe plus, ou la reprise demandée n'est pas acceptée (curseur trop ancien,
    /// dans le futur ou illisible, BR-SLOG-014) : les 1 000 dernières lignes suivent.
    ResumeLost,
    /// La première page n'a pas pu être livrée en entier : le client n'a rien lu pendant que l'agent la
    /// lisait (ou le moteur l'a rendue trop lentement), les lignes les plus anciennes de la page manquent.
    /// Ce n'est PAS un lecteur lent du direct (`Slow`) : le client ne doit pas le dire (BR-SLOG-001).
    FirstPage,
    /// Une raison que ce client ne connaît pas (ajoutée par un agent plus récent) : un trou, sans plus.
    #[serde(other)]
    Unknown,
}

/// Des lignes manquent ici, avant le lot suivant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogGap {
    /// Combien, quand on le sait (`resume_lost` : on ne le sait pas).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u64>,
    pub reason: GapReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkerKind {
    /// Le service a redémarré : ce qui suit est l'exécution suivante (BR-SLOG-009).
    Restarted,
    /// Un repère que ce client ne connaît pas : un séparateur sans texte particulier.
    #[serde(other)]
    Unknown,
}

/// Un repère entre deux exécutions du service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogMarker {
    pub kind: MarkerKind,
    /// Millisecondes depuis l'époque.
    pub at: i64,
}

/// Les erreurs du socket. Un code, jamais un texte du moteur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LogsErrorCode {
    /// Dix lectures sont déjà ouvertes (BR-SLOG-020).
    LogsLimit,
    /// Le moteur ne permet pas de relire les journaux de ce service (BR-SLOG-017).
    LogsUnreadable,
    /// Aucun service de cette clé, ou il a disparu pendant la lecture (BR-SLOG-015).
    ServiceNotFound,
    /// Le moteur ne répond pas ou n'est pas prêt (BR-SLOG-016).
    EngineUnavailable,
    /// Jeton refusé, ou premier message qui n'est pas `auth`.
    Unauthenticated,
    /// Message illisible, ou `open` attendu et absent.
    BadRequest,
    /// Un code que ce client ne connaît pas (ajouté par un agent plus récent) : une erreur terminale
    /// générique, la lecture est finie.
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogsError {
    pub code: LogsErrorCode,
}

/// Messages de l'agent vers le client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LogsServerMessage {
    Status(LogsStatus),
    Lines(LogLines),
    Gap(LogGap),
    Marker(LogMarker),
    Error(LogsError),
    Pong {
        n: u64,
    },
    /// Un message que ce client ne connaît pas (ajouté par un agent plus récent) : à ignorer.
    #[serde(other)]
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn open_reads_with_its_defaults_and_a_resume_cursor() {
        let open: LogsClientMessage = serde_json::from_value(json!({ "type": "open" })).unwrap();
        assert_eq!(
            open,
            LogsClientMessage::Open {
                tail: 1_000,
                since: None
            }
        );
        let open: LogsClientMessage = serde_json::from_value(
            json!({ "type": "open", "tail": 50, "since": { "ts": 1_791_849_600_123i64, "rank": 2 } }),
        )
        .unwrap();
        assert_eq!(
            open,
            LogsClientMessage::Open {
                tail: 50,
                since: Some(LogCursor {
                    ts: 1_791_849_600_123,
                    rank: 2
                })
            }
        );
        assert!(
            serde_json::from_value::<LogsClientMessage>(json!({ "type": "subscribe" })).is_err()
        );
    }

    #[test]
    fn messages_have_the_documented_shape() {
        let lines = LogsServerMessage::Lines(LogLines {
            seq: 3,
            items: vec![
                LogItem {
                    t: 10,
                    s: LogSource::Out,
                    x: "ok".into(),
                    cut: None,
                },
                LogItem {
                    t: 11,
                    s: LogSource::Err,
                    x: "boum".into(),
                    cut: Some(20_000),
                },
            ],
            cursor: LogCursor { ts: 11, rank: 0 },
        });
        assert_eq!(
            serde_json::to_value(&lines).unwrap(),
            json!({
                "type": "lines", "seq": 3,
                "items": [
                    { "t": 10, "s": "o", "x": "ok" },
                    { "t": 11, "s": "e", "x": "boum", "cut": 20_000 }
                ],
                "cursor": { "ts": 11, "rank": 0 }
            })
        );
        let gap = LogsServerMessage::Gap(LogGap {
            count: Some(40),
            reason: GapReason::Slow,
        });
        assert_eq!(
            serde_json::to_value(&gap).unwrap(),
            json!({ "type": "gap", "count": 40, "reason": "slow" })
        );
        let lost = LogsServerMessage::Gap(LogGap {
            count: None,
            reason: GapReason::ResumeLost,
        });
        assert_eq!(
            serde_json::to_value(&lost).unwrap(),
            json!({ "type": "gap", "reason": "resume_lost" })
        );
        let marker = LogsServerMessage::Marker(LogMarker {
            kind: MarkerKind::Restarted,
            at: 5,
        });
        assert_eq!(
            serde_json::to_value(&marker).unwrap(),
            json!({ "type": "marker", "kind": "restarted", "at": 5 })
        );
        let status = LogsServerMessage::Status(LogsStatus {
            state: LogState::Waiting,
            container: None,
            mixed: false,
            exit_code: None,
            ended_at: None,
            not_kept: true,
        });
        assert_eq!(
            serde_json::to_value(&status).unwrap(),
            json!({ "type": "status", "state": "waiting", "mixed": false, "not_kept": true })
        );
        let error = LogsServerMessage::Error(LogsError {
            code: LogsErrorCode::LogsLimit,
        });
        assert_eq!(
            serde_json::to_value(&error).unwrap(),
            json!({ "type": "error", "code": "LOGS_LIMIT" })
        );
        for (code, text) in [
            (LogsErrorCode::LogsUnreadable, "LOGS_UNREADABLE"),
            (LogsErrorCode::ServiceNotFound, "SERVICE_NOT_FOUND"),
            (LogsErrorCode::EngineUnavailable, "ENGINE_UNAVAILABLE"),
        ] {
            assert_eq!(serde_json::to_value(code).unwrap(), json!(text));
        }
    }

    #[test]
    fn every_server_message_survives_a_round_trip() {
        let messages = vec![
            LogsServerMessage::Pong { n: 7 },
            LogsServerMessage::Status(LogsStatus {
                state: LogState::Ended,
                container: Some("0123456789ab".into()),
                mixed: true,
                exit_code: Some(137),
                ended_at: Some(1),
                not_kept: false,
            }),
            LogsServerMessage::Lines(LogLines {
                seq: 1,
                items: vec![LogItem {
                    t: 1,
                    s: LogSource::Out,
                    x: "x".into(),
                    cut: None,
                }],
                cursor: LogCursor { ts: 1, rank: 0 },
            }),
        ];
        for message in messages {
            let text = serde_json::to_string(&message).unwrap();
            assert_eq!(
                serde_json::from_str::<LogsServerMessage>(&text).unwrap(),
                message
            );
        }
    }

    #[test]
    fn every_enumeration_reads_a_value_it_does_not_know_instead_of_failing() {
        assert_eq!(
            serde_json::from_value::<LogState>(json!("paused")).unwrap(),
            LogState::Unknown
        );
        assert_eq!(
            serde_json::from_value::<GapReason>(json!("evicted")).unwrap(),
            GapReason::Unknown
        );
        assert_eq!(
            serde_json::from_value::<MarkerKind>(json!("moved")).unwrap(),
            MarkerKind::Unknown
        );
        assert_eq!(
            serde_json::from_value::<LogsErrorCode>(json!("FORBIDDEN_SERVICE")).unwrap(),
            LogsErrorCode::Unknown
        );
        assert_eq!(
            serde_json::from_value::<LogSource>(json!("x")).unwrap(),
            LogSource::Unknown
        );
        // Un message de type inconnu, et un message connu qui porte une valeur inconnue.
        assert_eq!(
            serde_json::from_value::<LogsServerMessage>(json!({ "type": "progress", "n": 1 }))
                .unwrap(),
            LogsServerMessage::Unknown
        );
        let message: LogsServerMessage =
            serde_json::from_value(json!({ "type": "error", "code": "SOMETHING_NEW" })).unwrap();
        assert_eq!(
            message,
            LogsServerMessage::Error(LogsError {
                code: LogsErrorCode::Unknown
            })
        );
        let message: LogsServerMessage =
            serde_json::from_value(json!({ "type": "gap", "count": 3, "reason": "something_new" }))
                .unwrap();
        assert_eq!(
            message,
            LogsServerMessage::Gap(LogGap {
                count: Some(3),
                reason: GapReason::Unknown
            })
        );
        // La raison de la première page est son propre mot, jamais « lent ».
        assert_eq!(
            serde_json::to_value(GapReason::FirstPage).unwrap(),
            json!("first_page")
        );
    }

    #[test]
    fn the_text_of_a_line_never_appears_in_debug() {
        let item = LogItem {
            t: 1,
            s: LogSource::Out,
            x: "SENTINEL-LINE-TEXT".into(),
            cut: None,
        };
        let shown = format!(
            "{item:?} {:?}",
            LogLines {
                seq: 1,
                items: vec![item.clone()],
                cursor: LogCursor { ts: 1, rank: 0 },
            }
        );
        assert!(!shown.contains("SENTINEL"), "{shown}");
        assert!(shown.contains("chars"));
    }

    #[test]
    fn the_route_path_carries_the_key() {
        assert_eq!(
            path("u_docker-x.service"),
            "/services/u_docker-x.service/logs"
        );
    }
}

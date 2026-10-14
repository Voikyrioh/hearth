//! Client WebSocket de test : vrai TLS 1.3 vers un vrai agent (certificat accepté sans
//! vérification), puis `tokio-tungstenite` sur la connexion. Les messages sont lus avec les types
//! de `hearth-proto` : le contrat est vérifié à chaque lecture.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use futures_util::StreamExt as _;
use hearth_proto::stream::{ClientMessage, SecurityMessage, ServerMessage, Topic};
use rustls::pki_types::ServerName;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{WebSocketStream, client_async};

use super::https::{Agent, client_config};

/// Attente maximale d'un message dans un test.
const WAIT: Duration = Duration::from_secs(5);

pub struct WsClient {
    socket: WebSocketStream<TlsStream<TcpStream>>,
    /// Les messages `security` (toujours envoyés, HRT-24) reçus pendant que le test lisait autre
    /// chose : `next` les met de côté, `next_security` les rend.
    security: std::collections::VecDeque<SecurityMessage>,
}

/// Le serveur a refusé la mise à niveau : statut HTTP et corps.
#[derive(Debug)]
pub struct Refused {
    pub status: u16,
}

/// Ce qui a terminé la lecture.
#[derive(Debug, PartialEq, Eq)]
pub enum End {
    /// Trame de fermeture avec ce code.
    Closed(u16),
    /// Connexion coupée sans trame de fermeture.
    Dropped,
}

pub async fn connect(agent: &Agent, api_version: Option<&str>) -> Result<WsClient, Refused> {
    connect_with(agent, api_version, None).await
}

/// Comme `connect`, avec un tampon de réception minuscule côté client : s'il ne lit pas, la
/// fenêtre TCP se ferme vite et l'agent ne peut plus rien lui envoyer.
pub async fn connect_with(
    agent: &Agent,
    api_version: Option<&str>,
    recv_buffer: Option<u32>,
) -> Result<WsClient, Refused> {
    connect_path(agent, "/api/v1/stream", api_version, recv_buffer).await
}

/// Ouvre le socket des journaux d'un service (HRT-66) avec la version d'interface courante.
pub async fn open_logs(agent: &Agent, key: &str) -> Result<WsClient, Refused> {
    connect_path(
        agent,
        &format!("/api/v1/services/{key}/logs"),
        Some("1"),
        None,
    )
    .await
}

/// Comme `open_logs`, avec un tampon de réception minuscule : un client qui ne lit pas bloque vite
/// l'envoi de l'agent.
pub async fn open_logs_small_buffer(agent: &Agent, key: &str) -> Result<WsClient, Refused> {
    connect_path(
        agent,
        &format!("/api/v1/services/{key}/logs"),
        Some("1"),
        Some(4096),
    )
    .await
}

pub async fn connect_path(
    agent: &Agent,
    path: &str,
    api_version: Option<&str>,
    recv_buffer: Option<u32>,
) -> Result<WsClient, Refused> {
    let tcp = match recv_buffer {
        None => TcpStream::connect(agent.addr).await.expect("connexion"),
        Some(size) => {
            let socket = tokio::net::TcpSocket::new_v4().expect("socket");
            socket
                .set_recv_buffer_size(size)
                .expect("tampon de réception");
            socket.connect(agent.addr).await.expect("connexion")
        }
    };
    let name = ServerName::try_from("localhost").expect("nom");
    let tls = TlsConnector::from(client_config())
        .connect(name, tcp)
        .await
        .expect("poignée de main TLS 1.3");
    let mut request = format!("wss://localhost:{}{path}", agent.addr.port())
        .into_client_request()
        .expect("requête");
    if let Some(version) = api_version {
        request
            .headers_mut()
            .insert("x-hearth-api", version.parse().expect("en-tête"));
    }
    match client_async(request, tls).await {
        Ok((socket, _response)) => Ok(WsClient {
            socket,
            security: Default::default(),
        }),
        Err(tokio_tungstenite::tungstenite::Error::Http(response)) => Err(Refused {
            status: response.status().as_u16(),
        }),
        Err(other) => panic!("ouverture du WebSocket : {other}"),
    }
}

/// Ouvre le flux avec la version d'interface courante.
pub async fn open(agent: &Agent) -> WsClient {
    connect(agent, Some("1")).await.expect("flux ouvert")
}

impl WsClient {
    pub async fn send(&mut self, message: &ClientMessage) {
        let text = serde_json::to_string(message).expect("json");
        self.send_text(&text).await;
    }

    pub async fn send_text(&mut self, text: &str) {
        use futures_util::SinkExt as _;
        self.socket
            .send(Message::Text(text.into()))
            .await
            .expect("envoi");
    }

    /// Ferme proprement : trame de fermeture envoyée par le client.
    pub async fn close(&mut self) {
        use futures_util::SinkExt as _;
        self.socket.send(Message::Close(None)).await.expect("envoi");
    }

    /// Envoie une trame binaire (que les sockets de ce protocole refusent).
    pub async fn send_binary(&mut self, bytes: &[u8]) {
        use futures_util::SinkExt as _;
        self.socket
            .send(Message::Binary(bytes.to_vec().into()))
            .await
            .expect("envoi");
    }

    pub async fn auth(&mut self, token: &str) {
        self.send(&ClientMessage::Auth {
            token: token.to_owned(),
        })
        .await;
    }

    pub async fn subscribe(&mut self, topics: &[Topic]) {
        self.send(&ClientMessage::Subscribe {
            topics: topics.to_vec(),
        })
        .await;
    }

    /// Envoie un message JSON quelconque.
    pub async fn send_value(&mut self, value: &impl serde::Serialize) {
        let text = serde_json::to_string(value).expect("json");
        self.send_text(&text).await;
    }

    /// Prochain message du socket des journaux, ou la fin de la connexion.
    pub async fn next_logs(
        &mut self,
    ) -> Result<hearth_proto::api::service_logs::LogsServerMessage, End> {
        self.try_next_logs(WAIT)
            .await
            .expect("aucun message des journaux dans le délai")
    }

    /// Comme `next_logs`, mais `None` si rien ne vient dans ce délai.
    pub async fn try_next_logs(
        &mut self,
        wait: Duration,
    ) -> Option<Result<hearth_proto::api::service_logs::LogsServerMessage, End>> {
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            let frame = match tokio::time::timeout_at(deadline, self.socket.next()).await {
                Err(_) => return None,
                Ok(frame) => frame.transpose().unwrap_or(None),
            };
            match frame {
                Some(Message::Text(text)) => {
                    return Some(Ok(serde_json::from_str(text.as_str()).unwrap_or_else(
                        |error| panic!("message des journaux illisible ({error}) : {text}"),
                    )));
                }
                Some(Message::Close(frame)) => {
                    return Some(Err(End::Closed(frame.map_or(1005, |f| u16::from(f.code)))));
                }
                Some(_) => {}
                None => return Some(Err(End::Dropped)),
            }
        }
    }

    /// Coupe sans trame de fermeture (le client disparaît).
    pub fn drop_connection(self) {
        drop(self);
    }

    /// Prochain message du serveur, ou la fin de la connexion.
    pub async fn next(&mut self) -> Result<ServerMessage, End> {
        loop {
            let frame = timeout(WAIT, self.socket.next())
                .await
                .expect("aucun message dans le délai")
                .transpose()
                .unwrap_or(None);
            match frame {
                Some(Message::Text(text)) => {
                    // L'état de sécurité part toujours, sans abonnement : il n'est pas un
                    // `ServerMessage`, les tests des autres sujets le laissent de côté.
                    if let Ok(security) = serde_json::from_str::<SecurityMessage>(text.as_str()) {
                        self.security.push_back(security);
                        continue;
                    }
                    return Ok(serde_json::from_str(text.as_str())
                        .unwrap_or_else(|error| panic!("message illisible ({error}) : {text}")));
                }
                Some(Message::Close(frame)) => {
                    return Err(End::Closed(frame.map_or(1005, |f| u16::from(f.code))));
                }
                Some(_) => {}
                None => return Err(End::Dropped),
            }
        }
    }

    /// Prochain message `security` (l'état de sécurité du compte connecté) ; les autres messages
    /// lus en attendant sont ignorés.
    pub async fn next_security(&mut self) -> SecurityMessage {
        if let Some(message) = self.security.pop_front() {
            return message;
        }
        loop {
            let frame = timeout(WAIT, self.socket.next())
                .await
                .expect("aucun message security dans le délai")
                .transpose()
                .unwrap_or(None);
            match frame {
                Some(Message::Text(text)) => {
                    if let Ok(message) = serde_json::from_str::<SecurityMessage>(text.as_str()) {
                        return message;
                    }
                }
                Some(Message::Close(_)) | None => panic!("flux fermé avant le message security"),
                Some(_) => {}
            }
        }
    }

    /// Prochain message du sujet `services` (les autres messages sont ignorés).
    pub async fn next_services(&mut self) -> hearth_proto::stream::ServicesMessage {
        self.try_next_services(WAIT)
            .await
            .expect("aucun message services dans le délai")
    }

    /// Comme `next_services`, mais `None` si rien ne vient dans ce délai.
    pub async fn try_next_services(
        &mut self,
        wait: Duration,
    ) -> Option<hearth_proto::stream::ServicesMessage> {
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            let frame = match tokio::time::timeout_at(deadline, self.socket.next()).await {
                Err(_) => return None,
                Ok(frame) => frame.transpose().unwrap_or(None),
            };
            match frame {
                Some(Message::Text(text)) => {
                    // `Unknown` est la variante de repli : toute trame d'un autre sujet (mesures, sécurité…) s'y lit.
                    if let Ok(message) =
                        serde_json::from_str::<hearth_proto::stream::ServicesMessage>(text.as_str())
                        && message != hearth_proto::stream::ServicesMessage::Unknown
                    {
                        return Some(message);
                    }
                }
                Some(Message::Close(_)) | None => panic!("flux fermé avant le message services"),
                Some(_) => {}
            }
        }
    }

    /// Prochaine progression de mise à jour (les autres messages sont ignorés).
    pub async fn next_update(&mut self) -> hearth_proto::stream::UpdateMessage {
        loop {
            let frame = timeout(WAIT, self.socket.next())
                .await
                .expect("aucun message dans le délai")
                .transpose()
                .unwrap_or(None);
            match frame {
                Some(Message::Text(text)) => {
                    if let Ok(message) =
                        serde_json::from_str::<hearth_proto::stream::UpdateMessage>(text.as_str())
                    {
                        return message;
                    }
                }
                Some(Message::Close(_)) | None => panic!("flux fermé avant la progression"),
                Some(_) => {}
            }
        }
    }

    /// Prochain message, qui doit exister.
    pub async fn expect(&mut self) -> ServerMessage {
        self.next().await.expect("un message")
    }

    /// Lit jusqu'à la fin de la connexion (ignore ce qui reste).
    pub async fn until_end(&mut self) -> End {
        loop {
            if let Err(end) = self.next().await {
                return end;
            }
        }
    }
}

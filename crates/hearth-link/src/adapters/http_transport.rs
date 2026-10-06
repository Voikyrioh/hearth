//! Transport réel : `reqwest` (rustls, fournisseur `ring`) pour les requêtes, `tokio-tungstenite`
//! sur une connexion `tokio-rustls` pour le flux. Dans les deux cas, le certificat est vérifié
//! par empreinte (voir `tls.rs`) et la poignée de main est TLS 1.3 seul.
//!
//! Chaque appel ouvre sa propre connexion (pas de réutilisation) : une coupure se voit tout de
//! suite à la requête suivante, jamais sur une connexion morte restée au chaud.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::{SinkExt as _, StreamExt as _};
use hearth_proto::api::hello::HelloResponse;
use hearth_proto::api::operations::OperationResponse;
use hearth_proto::api::sessions::{LoginRequest, LoginResponse};
use hearth_proto::error::ErrorBody;
use hearth_proto::headers;
use hearth_proto::stream::{ClientMessage, ServerMessage};
use hearth_proto::version::API_VERSION;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use rustls::pki_types::ServerName;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::{self, Message};
use tokio_tungstenite::{WebSocketStream, client_async_with_config};

use super::tls::{PinState, client_config};
use crate::domain::agent_identity;
use crate::domain::pending_ops::OperationId;
use crate::domain::secret::Secret;
use crate::ports::transport::{
    ApiError, ApiRequest, ApiResponse, AuditExport, Frame, Method, Pin, Probed, StreamConn, Target,
    Transport, TransportError,
};

/// Taille maximale d'une réponse HTTP lue (un `GET /metrics/history` en tient largement).
const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;
/// Taille maximale d'un export du journal (10 000 entrées au plus, quelques centaines d'octets chacune).
const MAX_EXPORT_BYTES: usize = 16 * 1024 * 1024;
/// Taille maximale d'un message du flux (le `snapshot` porte cinq minutes d'historique).
const MAX_STREAM_MESSAGE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct HttpTransportConfig {
    /// Délai pour établir la connexion TCP, puis la poignée de main TLS (chacune).
    pub connect_timeout: Duration,
    /// Délai total d'une requête (connexion, envoi, réponse).
    pub request_timeout: Duration,
    /// Délai d'envoi d'un message sur le flux.
    pub send_timeout: Duration,
    /// `X-Hearth-Client` : `poste/version`.
    pub client_name: String,
}

impl Default for HttpTransportConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(3),
            request_timeout: Duration::from_secs(10),
            send_timeout: Duration::from_secs(5),
            client_name: format!("poste/{}", env!("CARGO_PKG_VERSION")),
        }
    }
}

/// Les éléments d'une requête à envoyer.
struct Call<'a> {
    method: Method,
    path: &'a str,
    token: Option<&'a Secret>,
    idempotency_key: Option<&'a str>,
    body: Option<&'a Value>,
}

pub struct HttpTransport {
    config: HttpTransportConfig,
}

impl HttpTransport {
    pub fn new(config: HttpTransportConfig) -> Self {
        Self { config }
    }

    fn client(&self, pin: Pin) -> Result<(reqwest::Client, Arc<PinState>), TransportError> {
        let (tls, state) =
            client_config(pin).map_err(|e| TransportError::Protocol(e.to_string()))?;
        let client = reqwest::Client::builder()
            .use_preconfigured_tls((*tls).clone())
            .no_proxy()
            .http1_only()
            .pool_max_idle_per_host(0)
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(self.config.connect_timeout)
            .timeout(self.config.request_timeout)
            .build()
            .map_err(|e| TransportError::Protocol(chain(&e)))?;
        Ok((client, state))
    }

    /// Envoie une requête et lit la réponse (statut, en-têtes, corps borné).
    async fn send(
        &self,
        target: &Target,
        call: Call<'_>,
    ) -> Result<(u16, HeaderMap, Vec<u8>), TransportError> {
        self.send_limited(target, call, MAX_BODY_BYTES).await
    }

    async fn send_limited(
        &self,
        target: &Target,
        call: Call<'_>,
        max_body: usize,
    ) -> Result<(u16, HeaderMap, Vec<u8>), TransportError> {
        let Call {
            method,
            path,
            token,
            idempotency_key,
            body,
        } = call;
        require_pinned(target)?;
        if !path.starts_with('/') {
            return Err(TransportError::Protocol("chemin invalide".into()));
        }
        let (client, state) = self.client(target.pin)?;
        let url = format!(
            "https://{}:{}/api/v1{path}",
            url_host(&target.host),
            target.port
        );
        let mut request = client.request(reqwest_method(method), url);
        let mut headers = HeaderMap::new();
        headers.insert(headers::API_VERSION, HeaderValue::from(API_VERSION));
        if let Ok(value) = HeaderValue::from_str(&self.config.client_name) {
            headers.insert(headers::CLIENT, value);
        }
        if let Some(token) = token {
            let mut value = HeaderValue::from_str(&format!("Bearer {}", token.expose()))
                .map_err(|_| TransportError::Protocol("jeton illisible".into()))?;
            value.set_sensitive(true);
            headers.insert(AUTHORIZATION, value);
        }
        if let Some(key) = idempotency_key {
            let value = HeaderValue::from_str(key)
                .map_err(|_| TransportError::Protocol("clé d'opération invalide".into()))?;
            headers.insert(headers::IDEMPOTENCY_KEY, value);
        }
        if let Some(body) = body {
            headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
            let bytes =
                serde_json::to_vec(body).map_err(|e| TransportError::Protocol(e.to_string()))?;
            request = request.body(bytes);
        }
        let mut response = request
            .headers(headers)
            .send()
            .await
            .map_err(|e| map_error(e, &state))?;
        let status = response.status().as_u16();
        let response_headers = response.headers().clone();
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|e| map_error(e, &state))? {
            if bytes.len().saturating_add(chunk.len()) > max_body {
                return Err(TransportError::Protocol("réponse trop volumineuse".into()));
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok((status, response_headers, bytes))
    }

    /// Réponse typée : un statut hors 2xx est une erreur de l'agent.
    async fn typed<T: DeserializeOwned>(
        &self,
        target: &Target,
        method: Method,
        path: &str,
        token: Option<&Secret>,
        body: Option<&Value>,
    ) -> Result<T, TransportError> {
        let call = Call {
            method,
            path,
            token,
            idempotency_key: None,
            body,
        };
        let (status, headers, bytes) = self.send(target, call).await?;
        if !(200..300).contains(&status) {
            return Err(TransportError::Api(api_error(status, &headers, &bytes)));
        }
        serde_json::from_slice(&bytes)
            .map_err(|e| TransportError::Protocol(format!("réponse illisible : {e}")))
    }
}

#[async_trait]
impl Transport for HttpTransport {
    async fn hello(&self, target: &Target) -> Result<Probed, TransportError> {
        // `/hello` ne demande pas `X-Hearth-Api` : il sert aussi à découvrir la plage.
        let (client, state) = self.client(target.pin)?;
        let url = format!(
            "https://{}:{}/api/v1/hello",
            url_host(&target.host),
            target.port
        );
        let response = client
            .get(url)
            .send()
            .await
            .map_err(|e| map_error(e, &state))?;
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let bytes = read_body(response, &state).await?;
        let Some(fingerprint) = state.presented() else {
            return Err(TransportError::Protocol("aucun certificat présenté".into()));
        };
        if status != 200 {
            return Err(TransportError::Api(api_error(status, &headers, &bytes)));
        }
        let hello: HelloResponse = serde_json::from_slice(&bytes)
            .map_err(|e| TransportError::Protocol(format!("réponse illisible : {e}")))?;
        // BR-CONN-012 : la règle est dans le domaine.
        agent_identity::check_product(&hello.product)
            .map_err(|e| TransportError::Protocol(e.to_string()))?;
        Ok(Probed { fingerprint, hello })
    }

    async fn login(
        &self,
        target: &Target,
        request: &LoginRequest,
    ) -> Result<LoginResponse, TransportError> {
        let body =
            serde_json::to_value(request).map_err(|e| TransportError::Protocol(e.to_string()))?;
        self.typed(target, Method::Post, "/sessions", None, Some(&body))
            .await
    }

    async fn logout(&self, target: &Target, token: &Secret) -> Result<(), TransportError> {
        let call = Call {
            method: Method::Delete,
            path: "/sessions/current",
            token: Some(token),
            idempotency_key: None,
            body: None,
        };
        let (status, headers, bytes) = self.send(target, call).await?;
        if status == 204 || (200..300).contains(&status) {
            Ok(())
        } else {
            Err(TransportError::Api(api_error(status, &headers, &bytes)))
        }
    }

    async fn request(
        &self,
        target: &Target,
        token: &Secret,
        request: &ApiRequest,
    ) -> Result<ApiResponse, TransportError> {
        let call = Call {
            method: request.method,
            path: &request.path,
            token: Some(token),
            idempotency_key: request.idempotency_key.as_deref(),
            body: request.body.as_ref(),
        };
        let (status, headers, bytes) = self.send(target, call).await?;
        let replayed = headers
            .get(headers::IDEMPOTENT_REPLAYED)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.eq_ignore_ascii_case("true"));
        // Un corps illisible n'est pas une erreur de transport : la réponse est arrivée.
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::Null)
        };
        Ok(ApiResponse {
            status,
            body,
            replayed,
        })
    }

    async fn export_audit(
        &self,
        target: &Target,
        token: &Secret,
        path: &str,
    ) -> Result<AuditExport, TransportError> {
        let call = Call {
            method: Method::Get,
            path,
            token: Some(token),
            idempotency_key: None,
            body: None,
        };
        let (status, headers, bytes) = self.send_limited(target, call, MAX_EXPORT_BYTES).await?;
        if !(200..300).contains(&status) {
            return Err(TransportError::Api(api_error(status, &headers, &bytes)));
        }
        let truncated = headers
            .get(hearth_proto::api::audit::EXPORT_TRUNCATED_HEADER)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.eq_ignore_ascii_case("true"));
        Ok(AuditExport {
            body: bytes,
            truncated,
        })
    }

    async fn operation(
        &self,
        target: &Target,
        token: &Secret,
        id: &OperationId,
    ) -> Result<OperationResponse, TransportError> {
        let path = format!("/operations/{}", id.as_str());
        self.typed(target, Method::Get, &path, Some(token), None)
            .await
    }

    async fn open_stream(&self, target: &Target) -> Result<Box<dyn StreamConn>, TransportError> {
        require_pinned(target)?;
        let (tls_config, state) =
            client_config(target.pin).map_err(|e| TransportError::Protocol(e.to_string()))?;
        let connect_timeout = self.config.connect_timeout;
        let tcp = timeout(
            connect_timeout,
            TcpStream::connect((target.host.as_str(), target.port)),
        )
        .await
        .map_err(|_| TransportError::Timeout)?
        .map_err(|e| TransportError::Connect(e.kind().to_string()))?;
        // Les petits messages (battement) partent tout de suite.
        let _ = tcp.set_nodelay(true);
        let name = ServerName::try_from(target.host.clone())
            .map_err(|_| TransportError::Protocol("nom de serveur invalide".into()))?;
        let tls = timeout(
            connect_timeout,
            TlsConnector::from(tls_config).connect(name, tcp),
        )
        .await
        .map_err(|_| TransportError::Timeout)?
        .map_err(|e| {
            if state.mismatched()
                && let Some(presented) = state.presented()
            {
                TransportError::FingerprintMismatch { presented }
            } else {
                TransportError::Io(e.kind().to_string())
            }
        })?;

        let mut request = format!(
            "wss://{}:{}/api/v1/stream",
            url_host(&target.host),
            target.port
        )
        .into_client_request()
        .map_err(|e| TransportError::Protocol(e.to_string()))?;
        request
            .headers_mut()
            .insert(headers::API_VERSION, HeaderValue::from(API_VERSION));
        if let Ok(value) = HeaderValue::from_str(&self.config.client_name) {
            request.headers_mut().insert(headers::CLIENT, value);
        }
        let ws_config = WebSocketConfig::default()
            .max_message_size(Some(MAX_STREAM_MESSAGE_BYTES))
            .max_frame_size(Some(MAX_STREAM_MESSAGE_BYTES));
        let (socket, _response) = timeout(
            connect_timeout,
            client_async_with_config(request, tls, Some(ws_config)),
        )
        .await
        .map_err(|_| TransportError::Timeout)?
        .map_err(map_ws_error)?;
        Ok(Box::new(WsConn {
            socket,
            send_timeout: self.config.send_timeout,
        }))
    }
}

struct WsConn {
    socket: WebSocketStream<TlsStream<TcpStream>>,
    send_timeout: Duration,
}

#[async_trait]
impl StreamConn for WsConn {
    async fn send(&mut self, message: &ClientMessage) -> Result<(), TransportError> {
        let text =
            serde_json::to_string(message).map_err(|e| TransportError::Protocol(e.to_string()))?;
        match timeout(
            self.send_timeout,
            self.socket.send(Message::Text(text.into())),
        )
        .await
        {
            Err(_) => Err(TransportError::Timeout),
            Ok(Err(error)) => Err(map_ws_error(error)),
            Ok(Ok(())) => Ok(()),
        }
    }

    async fn recv(&mut self) -> Result<Frame, TransportError> {
        match self.socket.next().await {
            None => Err(TransportError::Closed(None)),
            Some(Err(error)) => Err(map_ws_error(error)),
            Some(Ok(Message::Close(frame))) => {
                Err(TransportError::Closed(frame.map(|f| u16::from(f.code))))
            }
            Some(Ok(Message::Text(text))) => {
                Ok(match serde_json::from_str::<ServerMessage>(text.as_str()) {
                    Ok(message) => Frame::Message(Box::new(message)),
                    // Message mal formé ou d'un type inconnu : on l'ignore, le lien vit.
                    Err(_) => Frame::Other,
                })
            }
            Some(Ok(_)) => Ok(Frame::Other),
        }
    }
}

/// Tout sauf `hello` exige l'empreinte confirmée : le mode « sonde » n'envoie jamais ni
/// identifiant ni jeton (BR-CONN-011).
fn require_pinned(target: &Target) -> Result<(), TransportError> {
    match target.pin {
        Pin::Pinned(_) => Ok(()),
        Pin::Probe => Err(TransportError::Protocol(
            "empreinte du serveur non confirmée".into(),
        )),
    }
}

fn reqwest_method(method: Method) -> reqwest::Method {
    match method {
        Method::Get => reqwest::Method::GET,
        Method::Post => reqwest::Method::POST,
        Method::Put => reqwest::Method::PUT,
        Method::Patch => reqwest::Method::PATCH,
        Method::Delete => reqwest::Method::DELETE,
    }
}

/// Hôte tel qu'il s'écrit dans une URL (IPv6 entre crochets).
fn url_host(host: &str) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_owned()
    }
}

async fn read_body(
    mut response: reqwest::Response,
    state: &PinState,
) -> Result<Vec<u8>, TransportError> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| map_error(e, state))? {
        if bytes.len().saturating_add(chunk.len()) > MAX_BODY_BYTES {
            return Err(TransportError::Protocol("réponse trop volumineuse".into()));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

/// La chaîne des causes d'une erreur, sans l'URL.
fn chain(error: &dyn std::error::Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(" : ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}

fn map_error(error: reqwest::Error, state: &PinState) -> TransportError {
    if state.mismatched()
        && let Some(presented) = state.presented()
    {
        return TransportError::FingerprintMismatch { presented };
    }
    let error = error.without_url();
    if error.is_timeout() {
        TransportError::Timeout
    } else if error.is_connect() {
        TransportError::Connect(chain(&error))
    } else {
        TransportError::Io(chain(&error))
    }
}

fn map_ws_error(error: tungstenite::Error) -> TransportError {
    match error {
        tungstenite::Error::Http(response) => {
            let status = response.status().as_u16();
            let body = response.body().as_deref().unwrap_or_default();
            TransportError::Api(api_error(status, response.headers(), body))
        }
        tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed => {
            TransportError::Closed(None)
        }
        tungstenite::Error::Io(error) => TransportError::Io(error.kind().to_string()),
        other => TransportError::Protocol(other.to_string()),
    }
}

/// Analyse un corps d'erreur de l'agent ; tolère tout autre corps.
fn api_error(status: u16, headers: &HeaderMap, body: &[u8]) -> ApiError {
    let parsed = serde_json::from_slice::<ErrorBody>(body).ok();
    let retry_header = headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok());
    let (code, details) = match parsed {
        Some(body) => (Some(body.error.code), body.error.details),
        None => (None, Value::Null),
    };
    let retry_after_s = details
        .get("retry_after_s")
        .and_then(Value::as_u64)
        .or(retry_header);
    ApiError {
        status,
        code,
        details,
        retry_after_s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_probe_mode_is_refused_for_everything_but_hello() {
        use hearth_proto::api::sessions::LoginRequest;
        let transport = HttpTransport::new(HttpTransportConfig::default());
        let target = Target {
            host: "127.0.0.1".into(),
            port: 1,
            pin: Pin::Probe,
        };
        let refused = |e: TransportError| {
            assert_eq!(
                e,
                TransportError::Protocol("empreinte du serveur non confirmée".into())
            );
        };
        let login = LoginRequest {
            username: "u".into(),
            password: "p".into(),
        };
        refused(transport.login(&target, &login).await.unwrap_err());
        refused(transport.open_stream(&target).await.err().unwrap());
        let token = Secret::from("t");
        refused(transport.logout(&target, &token).await.unwrap_err());
        let request = ApiRequest {
            method: Method::Get,
            path: "/me".into(),
            body: None,
            idempotency_key: None,
        };
        refused(
            transport
                .request(&target, &token, &request)
                .await
                .unwrap_err(),
        );
        let id = OperationId::parse("A").unwrap();
        refused(transport.operation(&target, &token, &id).await.unwrap_err());
    }

    #[test]
    fn ipv6_hosts_are_bracketed_in_urls() {
        assert_eq!(url_host("::1"), "[::1]");
        assert_eq!(url_host("[::1]"), "[::1]");
        assert_eq!(url_host("forge.lan"), "forge.lan");
        assert_eq!(url_host("192.168.1.9"), "192.168.1.9");
    }

    #[test]
    fn error_bodies_are_parsed_and_anything_else_is_tolerated() {
        let body = br#"{"error":{"code":"TOO_MANY_ATTEMPTS","message":"x","details":{"retry_after_s":60}}}"#;
        let error = api_error(429, &HeaderMap::new(), body);
        assert_eq!(
            error.code,
            Some(hearth_proto::error::ErrorCode::TooManyAttempts)
        );
        assert_eq!(error.retry_after_s, Some(60));
        let junk = api_error(502, &HeaderMap::new(), b"<html>bad gateway</html>");
        assert_eq!(junk.code, None);
        assert_eq!(junk.status, 502);
        let empty = api_error(404, &HeaderMap::new(), b"");
        assert_eq!(empty.code, None);
    }

    #[test]
    fn the_retry_after_header_is_used_when_the_body_has_none() {
        let mut headers = HeaderMap::new();
        headers.insert(reqwest::header::RETRY_AFTER, HeaderValue::from_static("1"));
        let error = api_error(503, &headers, br#"{"error":{"code":"BUSY","message":"x"}}"#);
        assert_eq!(error.retry_after_s, Some(1));
    }
}

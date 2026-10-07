//! Transport vers l'agent : HTTPS pour les requêtes, WebSocket pour le flux. Tout passe par une
//! connexion TLS 1.3 dont le certificat est vérifié par empreinte (ADR-0005).

use async_trait::async_trait;
use hearth_proto::api::hello::HelloResponse;
use hearth_proto::api::operations::OperationResponse;
use hearth_proto::api::sessions::{
    ChallengeRequest, ChallengeResponse, DeviceLoginRequest, DeviceLoginResponse, LoginRequest,
    LoginResponse,
};
use hearth_proto::error::ErrorCode;
use hearth_proto::fingerprint::Fingerprint;
use hearth_proto::stream::{ClientMessage, ServerMessage, SignedAuth};
use serde_json::Value;
use thiserror::Error;

use crate::domain::pending_ops::OperationId;
use crate::domain::secret::Secret;

/// Comment le certificat du serveur est vérifié.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pin {
    /// Accepte tout certificat et rend son empreinte. **Uniquement** pour `hello` à la première
    /// prise de contact : l'adaptateur refuse ce mode pour tout le reste (connexion, requêtes,
    /// flux), BR-CONN-011.
    Probe,
    /// Refuse tout certificat dont l'empreinte SHA-256 diffère, quelle que soit la chaîne ou le
    /// nom.
    Pinned(Fingerprint),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub host: String,
    pub port: u16,
    pub pin: Pin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
        }
    }
}

/// Une requête authentifiée générique, chemin relatif à `/api/v1` (par exemple `/accounts`).
/// Le corps peut porter un mot de passe : `Debug` écrit à la main, sans le corps.
#[derive(Clone, PartialEq)]
pub struct ApiRequest {
    pub method: Method,
    pub path: String,
    pub body: Option<Value>,
    /// Clé d'opération (`Idempotency-Key`) pour les requêtes qui modifient.
    pub idempotency_key: Option<String>,
}

impl std::fmt::Debug for ApiRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiRequest")
            .field("method", &self.method)
            .field("path", &self.path)
            .field("body", &self.body.as_ref().map(|_| "***"))
            .field("idempotency_key", &self.idempotency_key)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ApiResponse {
    pub status: u16,
    /// `Value::Null` si la réponse n'a pas de corps.
    pub body: Value,
    /// `Idempotent-Replayed: true` : premier résultat rendu sans ré-exécution.
    pub replayed: bool,
}

/// Le fichier de l'export du journal tel que l'agent l'a produit (déjà neutralisé).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditExport {
    pub body: Vec<u8>,
    /// Le plafond de l'agent est atteint : seules les entrées les plus récentes y sont.
    pub truncated: bool,
}

/// Réponse d'erreur de l'agent (`{ "error": { code, message, details } }`).
#[derive(Debug, Clone, PartialEq)]
pub struct ApiError {
    pub status: u16,
    /// `None` si le corps n'est pas au format d'erreur de l'agent.
    pub code: Option<ErrorCode>,
    pub details: Value,
    pub retry_after_s: Option<u64>,
}

/// Résultat de `hello` : l'identité de l'agent et l'empreinte du certificat présenté.
#[derive(Debug, Clone, PartialEq)]
pub struct Probed {
    pub fingerprint: Fingerprint,
    pub hello: HelloResponse,
}

/// Pourquoi un échange a échoué. Les messages ne contiennent ni secret ni jeton.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum TransportError {
    #[error("connexion impossible : {0}")]
    Connect(String),
    #[error("délai dépassé")]
    Timeout,
    #[error("lien coupé : {0}")]
    Io(String),
    /// Le flux a été fermé (code de fermeture WebSocket s'il y en a un).
    #[error("flux fermé")]
    Closed(Option<u16>),
    /// Le certificat présenté n'a pas l'empreinte attendue : rien n'a été envoyé.
    #[error("l'empreinte du serveur a changé")]
    FingerprintMismatch { presented: Fingerprint },
    #[error("réponse inattendue : {0}")]
    Protocol(String),
    #[error("le serveur a répondu {}", .0.status)]
    Api(ApiError),
}

impl TransportError {
    pub fn api_code(&self) -> Option<ErrorCode> {
        match self {
            Self::Api(error) => error.code,
            _ => None,
        }
    }
}

/// Ce qui se lit sur le flux.
#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    Message(Box<ServerMessage>),
    /// Trame sans intérêt : contrôle, binaire, message mal formé ou inconnu. Compte comme signe
    /// de vie, jamais comme erreur.
    Other,
}

/// Un flux WebSocket ouvert. `recv` peut être abandonné et repris sans perdre de message.
#[async_trait]
pub trait StreamConn: Send {
    async fn send(&mut self, message: &ClientMessage) -> Result<(), TransportError>;
    async fn recv(&mut self) -> Result<Frame, TransportError>;

    /// Premier message `auth` du flux, avec en option la preuve de la clé d'appareil (HRT-23,
    /// usage `session`). Par défaut le jeton seul, comme avant : les flux simulés qui ne connaissent
    /// pas la preuve restent valides ; le flux réel écrit le message signé.
    async fn send_auth(&mut self, auth: &SignedAuth) -> Result<(), TransportError> {
        let SignedAuth::Auth { token, device } = auth;
        // Jamais une preuve retirée en silence : un flux qui ne sait pas la porter échoue, il ne se
        // fait pas passer pour un client sans clé (le repli des types la porterait, voir ADR-0023).
        if device.is_some() {
            return Err(TransportError::Protocol(
                "ce flux ne sait pas porter la preuve de la clé".into(),
            ));
        }
        let mut message = ClientMessage::Auth {
            token: token.clone(),
        };
        let sent = self.send(&message).await;
        if let ClientMessage::Auth { token } = &mut message {
            zeroize::Zeroize::zeroize(token);
        }
        sent
    }
}

#[async_trait]
pub trait Transport: Send + Sync {
    /// `GET /hello` : identité de l'agent et empreinte du certificat présenté.
    async fn hello(&self, target: &Target) -> Result<Probed, TransportError>;

    /// `POST /sessions`. Un refus (`401`, `429`, …) est une `TransportError::Api`.
    async fn login(
        &self,
        target: &Target,
        request: &LoginRequest,
    ) -> Result<LoginResponse, TransportError>;

    /// `POST /sessions/challenge` : un défi à signer avec la clé d'appareil (route publique, sans
    /// effet). Un agent d'avant la fonction répond `404` (`TransportError::Api`) : la clé n'est pas
    /// prise en charge. Par défaut, c'est cette réponse : les transports simulés qui ne connaissent
    /// pas la clé se comportent comme un agent ancien.
    async fn challenge(
        &self,
        _target: &Target,
        _request: &ChallengeRequest,
    ) -> Result<ChallengeResponse, TransportError> {
        Err(TransportError::Api(ApiError {
            status: 404,
            code: Some(ErrorCode::NotFound),
            details: Value::Null,
            retry_after_s: None,
        }))
    }

    /// `POST /sessions` avec la preuve de la clé d'appareil en option. Par défaut : la connexion
    /// ordinaire (`login`), sans clé.
    async fn login_with_device(
        &self,
        target: &Target,
        request: &DeviceLoginRequest,
    ) -> Result<DeviceLoginResponse, TransportError> {
        let login = self.login(target, &request.login).await?;
        Ok(DeviceLoginResponse {
            login,
            device: None,
        })
    }

    /// `DELETE /sessions/current`.
    async fn logout(&self, target: &Target, token: &Secret) -> Result<(), TransportError>;

    /// Requête authentifiée quelconque. Toute réponse de l'agent, même un refus (`4xx`), est un
    /// `Ok` : c'est une réponse reçue. Seul un échec de transport est une erreur.
    async fn request(
        &self,
        target: &Target,
        token: &Secret,
        request: &ApiRequest,
    ) -> Result<ApiResponse, TransportError>;

    /// `GET /audit/export…` : le corps brut (CSV), `path` construit par la bibliothèque. Un refus
    /// est une `TransportError::Api`. Par défaut non pris en charge (transports simulés).
    async fn export_audit(
        &self,
        _target: &Target,
        _token: &Secret,
        _path: &str,
    ) -> Result<AuditExport, TransportError> {
        Err(TransportError::Protocol("export non pris en charge".into()))
    }

    /// `GET /operations/{id}`. Un `404` est une `TransportError::Api` (« jamais reçue »).
    async fn operation(
        &self,
        target: &Target,
        token: &Secret,
        id: &OperationId,
    ) -> Result<OperationResponse, TransportError>;

    /// Ouvre le flux `/stream` (la requête d'ouverture porte `X-Hearth-Api`). L'authentification
    /// (`auth`) et l'abonnement (`subscribe`) sont envoyés par l'appelant.
    async fn open_stream(&self, target: &Target) -> Result<Box<dyn StreamConn>, TransportError>;
}

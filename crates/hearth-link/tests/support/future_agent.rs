//! Un agent « du futur » pour les tests de lecture : un serveur TLS 1.3 + WebSocket minimal qui joue, sur
//! `/api/v1/stream`, les trames EXACTES qu'on lui donne (texte brut, valeurs inventées comprises). Le vrai
//! agent ne sait émettre que ce qu'il connaît ; celui-ci sert à prouver que le client lit un agent plus
//! récent que lui (FIX:01M4VR6Y4Q4AVQVT9FCME7Z2SB). Il n'implémente aucune autre route : la liaison réelle
//! (`HttpTransport`) s'y connecte, épingle son empreinte et lit ce qu'il envoie.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use futures_util::{SinkExt as _, StreamExt as _};
use hearth_proto::fingerprint::Fingerprint;
use rustls::ServerConfig;
use rustls::pki_types::PrivateKeyDer;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_rustls::TlsAcceptor;
use tokio_tungstenite::tungstenite::Message;

pub struct FutureAgent {
    pub port: u16,
    pub fingerprint: Fingerprint,
    task: JoinHandle<()>,
}

impl FutureAgent {
    /// Démarre l'agent ; chaque connexion au flux reçoit `frames`, une par trame texte, dans l'ordre, puis
    /// reste ouverte (le client ferme).
    pub async fn start(frames: Vec<String>) -> Self {
        let certified = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).unwrap();
        let der = certified.cert.der().clone();
        let fingerprint = Fingerprint::of_certificate_der(der.as_ref());
        let key = PrivateKeyDer::try_from(certified.signing_key.serialize_der()).unwrap();
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = ServerConfig::builder_with_provider(provider)
            .with_protocol_versions(&[&rustls::version::TLS13])
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![der], key)
            .unwrap();
        let acceptor = TlsAcceptor::from(Arc::new(config));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let task = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                let acceptor = acceptor.clone();
                let frames = frames.clone();
                tokio::spawn(async move {
                    let Ok(tls) = acceptor.accept(stream).await else {
                        return;
                    };
                    let Ok(mut socket) = tokio_tungstenite::accept_async(tls).await else {
                        return;
                    };
                    for frame in frames {
                        if socket.send(Message::Text(frame.into())).await.is_err() {
                            return;
                        }
                    }
                    // Reste ouvert jusqu'à la fermeture par le client.
                    while let Some(Ok(message)) = socket.next().await {
                        if message.is_close() {
                            return;
                        }
                    }
                });
            }
        });
        Self {
            port,
            fingerprint,
            task,
        }
    }
}

impl Drop for FutureAgent {
    fn drop(&mut self) {
        self.task.abort();
    }
}

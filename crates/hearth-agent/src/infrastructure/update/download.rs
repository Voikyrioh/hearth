//! Téléchargement de la mise à jour : HTTPS seulement, en mémoire, plafonné, redirections
//! suivies une à une et toutes en HTTPS. TLS 1.3 par `rustls` (fournisseur `ring`, jamais aws-lc ni
//! OpenSSL), autorités de certification du système (`rustls-native-certs`) : l'adresse du flux de
//! versions est celle de l'administrateur, elle se valide comme n'importe quel site.

use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use async_trait::async_trait;
use reqwest::redirect::Policy;
use rustls::pki_types::CertificateDer;
use rustls::{ClientConfig, RootCertStore};

use crate::application::ports::{Downloader, FetchError};
use crate::domain::update::{host_is_local, is_local_address};

const MAX_REDIRECTS: usize = 5;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// Silence maximal entre deux morceaux reçus.
const READ_TIMEOUT: Duration = Duration::from_secs(60);
/// Durée maximale d'un téléchargement.
const TOTAL_TIMEOUT: Duration = Duration::from_secs(900);

/// D'où viennent les autorités de certification.
enum Roots {
    /// Celles du système (`rustls-native-certs`).
    System,
    /// Celles-ci et elles seules : les tests y mettent la leur.
    Given(Vec<CertificateDer<'static>>),
}

/// Résolveur qui ne rend jamais une adresse locale ou privée (BR-UPDATE-027) : le contrôle porte
/// sur ce que le nom **devient**, au moment de se connecter (redirections comprises), pas sur le
/// texte de l'adresse (un nom public peut pointer vers le réseau local).
struct PublicOnlyResolver;

impl reqwest::dns::Resolve for PublicOnlyResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let found: Vec<SocketAddr> =
                tokio::net::lookup_host((host.as_str(), 0)).await?.collect();
            let public: Vec<SocketAddr> = found
                .into_iter()
                .filter(|addr| !is_local_address(addr.ip()))
                .collect();
            if public.is_empty() {
                return Err("adresse locale ou privée refusée".into());
            }
            Ok(Box::new(public.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

pub struct HttpsDownloader {
    allow_local: bool,
    roots: Roots,
    /// Construit au premier téléchargement : lire le magasin de certificats du système à chaque
    /// démarrage de l'agent coûterait, pour une mise à jour qui n'arrive presque jamais.
    client: OnceLock<Result<reqwest::Client, String>>,
}

impl HttpsDownloader {
    /// Avec les autorités de certification du système.
    pub fn new(allow_local: bool) -> Self {
        Self {
            allow_local,
            roots: Roots::System,
            client: OnceLock::new(),
        }
    }

    /// Avec ces autorités (et elles seules).
    pub fn with_roots(certs: Vec<CertificateDer<'static>>, allow_local: bool) -> Self {
        Self {
            allow_local,
            roots: Roots::Given(certs),
            client: OnceLock::new(),
        }
    }

    fn client(&self) -> Result<&reqwest::Client, FetchError> {
        self.client
            .get_or_init(|| build_client(&self.roots, self.allow_local))
            .as_ref()
            .map_err(|error| FetchError::Failed(error.clone()))
    }
}

fn build_client(source: &Roots, allow_local: bool) -> Result<reqwest::Client, String> {
    let certs = match source {
        Roots::System => {
            let result = rustls_native_certs::load_native_certs();
            for error in &result.errors {
                tracing::warn!(%error, "certificat système illisible, ignoré");
            }
            result.certs
        }
        Roots::Given(certs) => certs.clone(),
    };
    let mut roots = RootCertStore::empty();
    let (added, ignored) = roots.add_parsable_certificates(certs);
    if added == 0 {
        tracing::warn!(
            ignored,
            "aucune autorité de certification : tout téléchargement échouera"
        );
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|error| format!("configuration TLS impossible : {error}"))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    let mut builder = reqwest::Client::builder()
        // FIX:01M47XJXQ0GHV77FN4J6R1NXPZ (docs/bugs/FIX-01M47XJXQ0GHV77FN4J6R1NXPZ.md)
        // Aucun proxy d'environnement (HTTPS_PROXY, ALL_PROXY...) : avec un proxy, le nom de l'hôte
        // partirait au proxy, qui le résoudrait lui-même, et le filtre d'adresses appliqué après
        // résolution (BR-UPDATE-027) ne verrait plus rien. Un serveur derrière un proxy sortant ne
        // peut pas se mettre à jour à distance : c'est assumé.
        .no_proxy()
        .use_preconfigured_tls(config)
        .https_only(true)
        .redirect(Policy::custom(move |attempt| {
            // Le même filtre et la même lecture de l'adresse que la demande (crate `url`).
            let literal_local = attempt.url().host().is_some_and(host_is_local);
            if attempt.previous().len() >= MAX_REDIRECTS {
                attempt.error("trop de redirections")
            } else if attempt.url().scheme() != "https" {
                attempt.error("redirection hors HTTPS refusée")
            } else if !allow_local && literal_local {
                attempt.error("redirection vers une adresse locale refusée")
            } else {
                attempt.follow()
            }
        }));
    if !allow_local {
        builder = builder.dns_resolver(Arc::new(PublicOnlyResolver));
    }
    builder
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(READ_TIMEOUT)
        .timeout(TOTAL_TIMEOUT)
        .user_agent(concat!("hearth-agent/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| format!("client HTTPS impossible : {error}"))
}

#[async_trait]
impl Downloader for HttpsDownloader {
    async fn fetch(
        &self,
        url: &str,
        max_bytes: u64,
        progress: &(dyn Fn(u64, Option<u64>) + Send + Sync),
    ) -> Result<Vec<u8>, FetchError> {
        if !url
            .get(..8)
            .is_some_and(|s| s.eq_ignore_ascii_case("https://"))
        {
            return Err(FetchError::Failed("adresse hors HTTPS".to_owned()));
        }
        // La même lecture de l'adresse que celle de `reqwest` (un seul parseur), le même filtre que
        // la demande : un littéral ne passe jamais par le résolveur.
        let parsed = reqwest::Url::parse(url)
            .map_err(|_| FetchError::Failed("adresse illisible".to_owned()))?;
        if !self.allow_local && parsed.host().is_none_or(host_is_local) {
            return Err(FetchError::Failed(
                "adresse locale ou privée refusée".to_owned(),
            ));
        }
        let mut response = self.client()?.get(parsed).send().await.map_err(map_error)?;
        if !response.status().is_success() {
            return Err(FetchError::Failed(format!("statut {}", response.status())));
        }
        let total = response.content_length();
        if total.is_some_and(|total| total > max_bytes) {
            return Err(FetchError::TooLarge);
        }
        let mut data =
            Vec::with_capacity(usize::try_from(total.unwrap_or(0).min(max_bytes)).unwrap_or(0));
        progress(0, total);
        while let Some(chunk) = response.chunk().await.map_err(map_error)? {
            data.extend_from_slice(&chunk);
            if data.len() as u64 > max_bytes {
                return Err(FetchError::TooLarge);
            }
            progress(data.len() as u64, total);
        }
        if total.is_some_and(|total| total != data.len() as u64) {
            return Err(FetchError::Failed("fichier incomplet".to_owned()));
        }
        Ok(data)
    }
}

/// Une erreur de connexion ou de résolution : le serveur n'a pas accès à cette adresse.
fn map_error(error: reqwest::Error) -> FetchError {
    if error.is_connect() {
        FetchError::Unreachable(error.without_url().to_string())
    } else {
        FetchError::Failed(error.without_url().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_clear_text_address_is_refused_without_any_connection() {
        let downloader = HttpsDownloader::with_roots(Vec::new(), true);
        let result = downloader
            .fetch("http://127.0.0.1:1/x", 10, &|_, _| {})
            .await;
        assert!(matches!(result, Err(FetchError::Failed(_))));
    }

    #[tokio::test]
    async fn an_unreachable_address_is_reported_as_such() {
        let downloader = HttpsDownloader::with_roots(Vec::new(), true);
        // Un port fermé sur la machine : la connexion est refusée.
        let result = downloader
            .fetch("https://127.0.0.1:9/x", 10, &|_, _| {})
            .await;
        assert!(
            matches!(result, Err(FetchError::Unreachable(_))),
            "{result:?}"
        );
    }
}

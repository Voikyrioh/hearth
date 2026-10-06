//! Adaptateur du port `Feed` sur le greffon officiel `tauri-plugin-updater` (ADR-0017).
//!
//! Le greffon fait le réseau (HTTPS, rustls + ring), lit le manifeste statique `latest.json` des
//! GitHub Releases, télécharge l'installateur EN MÉMOIRE, vérifie la signature minisign contre la
//! clé publique embarquée (`update-key.pub`), et lance l'installateur NSIS (`/UPDATE`, relance).
//! Rien ici n'est exposé à l'interface : le greffon est branché sans aucune permission côté web
//! (`capabilities/default.json` n'en accorde aucune), l'adresse du flux et la clé sont des
//! constantes de la compilation.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Runtime};
use tauri_plugin_updater::{Update, UpdaterExt as _};
use url::Url;

use super::domain::{Candidate, DownloadPolicy, FEED_URL};
use super::ports::{DownloadError, Feed, FeedError, VerifiedInstaller};
use crate::agent_update::domain::{
    AGENT_FEED_URL, AgentCandidate, MANIFEST_MAX_BYTES, parse_manifest,
};

/// La clé publique de signature (fichier `.pub` de minisign), embarquée à la compilation. Aucune
/// clé ne se lit sur le disque de la machine : qui peut écrire un fichier sur le PC ne peut pas
/// faire accepter son installateur.
pub const EMBEDDED_PUBLIC_KEY: &str = include_str!("../../update-key.pub");

/// Clé de développement du dépôt : sans clé secrète, aucune signature ne la satisfait, donc aucune
/// mise à jour n'est possible tant que Voiky n'a pas mis la sienne (runbook
/// `publier-une-version-du-client`).
pub fn embedded_key_is_development() -> bool {
    is_development_key(EMBEDDED_PUBLIC_KEY)
}

/// Le fichier de clé porte-t-il la marque de la clé de développement dans son commentaire ? Le
/// flux de publication refuse un fichier qui la porte (runbook).
pub fn is_development_key(public_key_file: &str) -> bool {
    public_key_text(public_key_file)
        .unwrap_or_default()
        .lines()
        .next()
        .is_some_and(|comment| comment.contains(DEV_KEY_MARK))
}

/// Le fichier de clé sous sa forme de texte minisign (`untrusted comment: …` puis la clé) : tel quel
/// s'il l'est déjà, ou décodé s'il vient de `tauri signer generate` (le même texte en base64, sur
/// une ligne). Les deux formes sont acceptées pour qu'aucune conversion manuelle ne soit à faire.
pub fn public_key_text(file: &str) -> Option<String> {
    let file = file.trim();
    if file.starts_with("untrusted comment:") {
        return Some(file.to_owned());
    }
    let bytes = STANDARD.decode(file).ok()?;
    let text = String::from_utf8(bytes).ok()?;
    text.trim()
        .starts_with("untrusted comment:")
        .then(|| text.trim().to_owned())
}

/// Marque du commentaire de la clé de développement du dépôt (`update-key.pub`).
pub const DEV_KEY_MARK: &str = "DEV public key";

/// Cible du manifeste (`platforms` de `latest.json`) : le client n'existe que pour Windows 64 bits.
pub const TARGET: &str = "windows-x86_64";

const CHECK_TIMEOUT: Duration = Duration::from_secs(20);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// Le greffon, avec la clé de ce dépôt.
pub fn plugin<R: Runtime>() -> TauriPlugin<R, tauri_plugin_updater::Config> {
    plugin_with_key(EMBEDDED_PUBLIC_KEY)
}

/// Le greffon avec une autre clé (tests seulement, avec une paire jetable).
pub fn plugin_with_key<R: Runtime>(
    public_key_file: &str,
) -> TauriPlugin<R, tauri_plugin_updater::Config> {
    tauri_plugin_updater::Builder::new()
        .target(TARGET)
        // Illisible : une valeur qui ne vérifie rien (le greffon refusera toute signature).
        .pubkey(STANDARD.encode(public_key_text(public_key_file).unwrap_or_default()))
        .build()
}

pub struct TauriFeed<R: Runtime> {
    app: AppHandle<R>,
    endpoint: Url,
    /// Le fichier de cibles de l'agent ; absent : aucune lecture (tests du greffon seul).
    agent_endpoint: Option<Url>,
    policy: DownloadPolicy,
    check_timeout: Duration,
    /// La dernière annonce dont la source est permise : celle que `download` utilise.
    staged: Mutex<Option<Update>>,
}

impl<R: Runtime> TauriFeed<R> {
    /// Production : le flux des GitHub Releases du dépôt public, fixé à la compilation.
    pub fn production(app: AppHandle<R>, policy: DownloadPolicy) -> Result<Self, url::ParseError> {
        Ok(Self::with_endpoint(app, Url::parse(FEED_URL)?, policy)
            .with_agent_endpoint(Url::parse(AGENT_FEED_URL)?))
    }

    /// Adresse du fichier de cibles de l'agent (production : constante ; tests : serveur local).
    #[doc(hidden)]
    pub fn with_agent_endpoint(mut self, endpoint: Url) -> Self {
        self.agent_endpoint = Some(endpoint);
        self
    }

    /// Flux et source choisis (tests contre un serveur de versions local).
    #[doc(hidden)]
    pub fn with_endpoint(app: AppHandle<R>, endpoint: Url, policy: DownloadPolicy) -> Self {
        Self {
            app,
            endpoint,
            agent_endpoint: None,
            policy,
            check_timeout: CHECK_TIMEOUT,
            staged: Mutex::new(None),
        }
    }

    /// Délai de la vérification (tests : un serveur qui ne répond jamais).
    #[doc(hidden)]
    pub fn with_check_timeout(mut self, timeout: Duration) -> Self {
        self.check_timeout = timeout;
        self
    }

    fn staged_for(&self, version: &str) -> Option<Update> {
        self.staged
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .filter(|update| same_version(&update.version, version))
    }

    fn stage(&self, update: Option<Update>) {
        *self.staged.lock().unwrap_or_else(PoisonError::into_inner) = update;
    }
}

/// `reqwest` (fonction `rustls-no-provider`) n'installe aucun fournisseur de cryptographie : celui
/// du processus est `ring`, le seul compilé (ni `aws-lc` ni OpenSSL, ADR-0011). Déjà installé par un
/// autre composant : rien à faire.
fn install_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

fn same_version(a: &str, b: &str) -> bool {
    match (
        semver::Version::parse(a.trim_start_matches('v')),
        semver::Version::parse(b.trim_start_matches('v')),
    ) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// Rend le quota de 24 h seulement quand RIEN n'a eu lieu avec le PREMIER hôte : échec de résolution
/// du nom ou de connexion TCP à l'adresse du flux, et rien d'autre. Échec TLS, échec à un saut
/// suivant, délai dépassé, réponse invalide : le quota est consommé. Dans le doute : consommé.
fn feed_error(error: tauri_plugin_updater::Error, redirected: bool) -> FeedError {
    if !redirected && is_no_connection(&error) {
        FeedError::offline(error.to_string())
    } else {
        FeedError::failed(error.to_string())
    }
}

/// `redirected` : le premier hôte a répondu par une redirection (reqwest garde l'adresse de départ dans
/// l'erreur d'un saut suivant : seul ce drapeau dit que l'échec est celui d'un second saut).
fn is_no_connection(error: &tauri_plugin_updater::Error) -> bool {
    let tauri_plugin_updater::Error::Reqwest(e) = error else {
        return false;
    };
    if !e.is_connect() || e.is_timeout() {
        return false;
    }
    // La chaîne des causes : une erreur d'E/S de connexion (refusée, réseau injoignable…) ou de
    // résolution du nom. Une erreur TLS ou toute autre cause ne passe pas.
    let mut source: Option<&(dyn std::error::Error + 'static)> = std::error::Error::source(e);
    while let Some(cause) = source {
        if let Some(io) = cause.downcast_ref::<std::io::Error>() {
            use std::io::ErrorKind::*;
            if matches!(
                io.kind(),
                ConnectionRefused | NetworkUnreachable | HostUnreachable | AddrNotAvailable
            ) {
                return true;
            }
        }
        let text = cause.to_string().to_lowercase();
        if text.contains("dns error")
            || text.contains("failed to lookup")
            || text.contains("no such host")
            || text.contains("name or service not known")
        {
            return true;
        }
        source = cause.source();
    }
    false
}

/// Range une erreur du greffon : signature, contenu ou format refusés (corrompu), coupure
/// (interrompu), autre.
pub fn classify(error: &tauri_plugin_updater::Error) -> DownloadError {
    use tauri_plugin_updater::Error as E;
    let text = error.to_string();
    match error {
        E::Minisign(_)
        | E::Base64(_)
        | E::SignatureUtf8(_)
        | E::SignedVersionMismatch { .. }
        | E::MissingSignedVersion
        | E::InvalidUpdaterFormat
        | E::BinaryNotFoundInArchive => DownloadError::Corrupted(text),
        #[cfg(windows)]
        E::Extract(_) => DownloadError::Corrupted(text),
        E::Reqwest(_) | E::Network(_) => DownloadError::Interrupted(text),
        _ => DownloadError::Failed(text),
    }
}

/// Durcit le client HTTP du greffon (vérification ET téléchargement) : HTTPS partout quand la
/// source l'est, et, à chaque redirection, HTTPS et au plus `MAX_REDIRECTS` sauts
/// (`DownloadPolicy::allows_redirect`). Sans cela reqwest suivrait 10 redirections, en clair compris.
fn harden(
    policy: &DownloadPolicy,
    redirected: Arc<AtomicBool>,
) -> impl Fn(reqwest::ClientBuilder) -> reqwest::ClientBuilder + Send + Sync + 'static {
    let policy = policy.clone();
    move |builder| {
        let rules = policy.clone();
        let redirected = redirected.clone();
        let builder = builder.redirect(reqwest::redirect::Policy::custom(move |attempt| {
            redirected.store(true, Ordering::SeqCst);
            if rules.allows_redirect(attempt.url(), attempt.previous().len()) {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }));
        if policy.https_only() {
            builder.https_only(true)
        } else {
            builder
        }
    }
}

#[async_trait]
impl<R: Runtime> Feed for TauriFeed<R> {
    async fn check(&self) -> Result<Option<Candidate>, FeedError> {
        let redirected = Arc::new(AtomicBool::new(false));
        let updater = self
            .app
            .updater_builder()
            .endpoints(vec![self.endpoint.clone()])
            .map_err(|error| feed_error(error, false))?
            .timeout(self.check_timeout)
            .configure_client(harden(&self.policy, redirected.clone()))
            .build()
            .map_err(|error| feed_error(error, false))?;
        let Some(mut update) = updater
            .check()
            .await
            .map_err(|error| feed_error(error, redirected.load(Ordering::SeqCst)))?
        else {
            self.stage(None);
            return Ok(None);
        };
        update.timeout = Some(DOWNLOAD_TIMEOUT);
        let candidate = Candidate {
            version: update.version.clone(),
            notes: update.body.clone(),
            download_url: update.download_url.to_string(),
            signature: update.signature.clone(),
        };
        // Une annonce dont la source n'est pas permise ne sera jamais téléchargée.
        self.stage(self.policy.allows(&update.download_url).then_some(update));
        Ok(Some(candidate))
    }

    async fn check_agent(&self) -> Result<Option<AgentCandidate>, FeedError> {
        let Some(endpoint) = self.agent_endpoint.clone() else {
            return Ok(None);
        };
        install_crypto_provider();
        // Mêmes règles que le flux du client : HTTPS partout, redirections bornées et en HTTPS.
        let client =
            harden(&self.policy, Arc::new(AtomicBool::new(false)))(reqwest::Client::builder())
                .timeout(self.check_timeout)
                .build()
                .map_err(|error| FeedError::failed(error.to_string()))?;
        let mut response = client
            .get(endpoint)
            .send()
            .await
            .map_err(|error| FeedError::failed(error.to_string()))?;
        // Aucun fichier de cibles dans cette release : rien à proposer, pas une panne.
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(FeedError::failed(format!(
                "fichier de cibles : réponse {}",
                response.status()
            )));
        }
        // Lu par morceaux, borné : un serveur qui n'arrête pas d'envoyer ne remplit pas la mémoire.
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| FeedError::failed(error.to_string()))?
        {
            if body.len() + chunk.len() > MANIFEST_MAX_BYTES {
                return Err(FeedError::failed("fichier de cibles trop volumineux"));
            }
            body.extend_from_slice(&chunk);
        }
        parse_manifest(&body).map_err(|error| FeedError::failed(error.to_string()))
    }

    async fn download(
        &self,
        version: &str,
        progress: &mut (dyn FnMut(u64, Option<u64>) + Send),
    ) -> Result<VerifiedInstaller, DownloadError> {
        let update = self.staged_for(version).ok_or(DownloadError::NotStaged)?;
        let mut received = 0u64;
        update
            .download(
                |chunk, total| {
                    received = received.saturating_add(chunk as u64);
                    progress(received, total);
                },
                || {},
            )
            .await
            .map(VerifiedInstaller::new)
            .map_err(|error| classify(&error))
    }

    fn install(&self, version: &str, installer: VerifiedInstaller) -> Result<(), DownloadError> {
        let update = self.staged_for(version).ok_or(DownloadError::NotStaged)?;
        update
            .install(installer.into_bytes())
            .map_err(|error| classify(&error))
    }
}

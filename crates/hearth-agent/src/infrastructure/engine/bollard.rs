//! Adaptateur du port `ContainerEngine` sur l'API HTTP du moteur (socket Unix) par `bollard`
//! (ADR-0036). Le seul endroit où `bollard` apparaît.
//!
//! - **La sonde** ne passe pas par `bollard` : elle ouvre le socket elle-même et lit l'en-tête
//!   `Api-Version` de `GET /_ping` (sans préfixe de version, comme le fait la ligne de commande
//!   `docker`). C'est ce qui distingue socket absent, connexion refusée, `EACCES` et silence, et ce
//!   qui évite de demander à un moteur ancien une version d'API qu'il ne connaît pas.
//! - **Le reste** (`/version`, liste, inspection, événements) passe par `bollard`, avec une version
//!   d'API ramenée à celle du moteur (au plus celle de `bollard`).
//! - **Les journaux** (HRT-66) passent par `logs.rs`, écrit à la main sur le même socket : le décodeur de
//!   `bollard::Docker::logs` accumule sans borne une ligne de terminal sans fin de ligne et écrit les octets
//!   restants dans une trace `debug` (voir l'en-tête de `logs.rs`).
//! - **Écrire** (HRT-64) : `start`, `stop` et `restart` seulement. Un ordre d'arrêt dure jusqu'au délai de
//!   grâce du conteneur (120 s pour un serveur de jeu qui sauvegarde) : ces appels ont un délai de requête
//!   propre, celui de l'attente de l'action (`wait_budget`), pas les 10 s des lectures. Un conteneur déjà
//!   dans l'état visé (le moteur répond « non modifié », 304) n'est pas une erreur.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use bollard::models::{
    ContainerCreateBody, CreateImageInfo, EventMessage, EventMessageTypeEnum, HostConfig, Mount,
    MountType, PortBinding, PortMap, RestartPolicy, RestartPolicyNameEnum, SystemVersion,
};
use bollard::query_parameters::{
    CreateContainerOptionsBuilder, CreateImageOptionsBuilder, EventsOptionsBuilder,
    InspectContainerOptionsBuilder, ListContainersOptionsBuilder, RemoveContainerOptionsBuilder,
    RestartContainerOptionsBuilder, StopContainerOptionsBuilder,
};
use bollard::{API_DEFAULT_VERSION, ClientVersion, Docker};
use futures_util::{StreamExt, stream};
use hearth_proto::api::deploy::DEPLOY_TIMEOUT_S;
use hearth_proto::api::service_spec::split_image;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use super::errors::{translate, translate_create, translate_pull};
use super::installed::InstallMarkers;
use super::translate::{env_entries_of, observation_from};
use crate::application::ports::{
    ContainerEngine, EngineError, EngineEvent, EngineEvents, EngineInfo, EnvEntry, Listing,
    LogQuery, LogStream, PullStream, Unreadable,
};
use crate::domain::ops::wait_budget;
use crate::domain::services::plan::ContainerPlan;
use crate::domain::services::pull::{LayerEvent, PullTracker};
use crate::domain::services::spec::{SpecProtocol, SpecRestart};
use crate::domain::services::{
    Announced, ApiVersion, EngineProbe, MAX_ENGINE_TEXT_CHARS, MAX_NAME_CHARS, MIN_API_VERSION,
    Observation, ProbeOutcome, bounded,
};

/// Délai de la sonde ENTIÈRE (`/_ping` puis `/version`, `BR-ENGINE-009`) : sans réponse complète en
/// 5 secondes, le moteur « ne répond pas ».
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
/// Délai de la relecture complète (liste et toutes les inspections) : au-delà, le moteur est
/// considéré comme bloqué et la tâche du registre reprend la main (sonde, anti-clignotement).
const LIST_TIMEOUT: Duration = Duration::from_secs(30);
/// Délai d'une lecture (liste, inspection).
const REQUEST_TIMEOUT_S: u64 = 10;
/// Inspections simultanées au plus pendant une relecture complète.
const INSPECT_CONCURRENCY: usize = 8;
/// Taille maximale de la réponse de `/_ping` lue (en-têtes et corps).
const MAX_PING_BYTES: usize = 16 * 1024;

/// Les événements qui changent ce que la liste montre. Les autres (`exec_*`, `health_status`,
/// `attach`, `top`, `resize`…) sont du bruit pour une liste de services.
const RELEVANT_ACTIONS: &[&str] = &[
    "create", "start", "restart", "die", "stop", "kill", "destroy", "rename", "pause", "unpause",
    "oom", "update",
];

/// Où parle l'adaptateur.
#[derive(Debug, Clone)]
pub enum Target {
    /// Le socket Unix du moteur (`/var/run/docker.sock`).
    Unix(PathBuf),
    /// Une adresse `hôte:port` en HTTP clair : un faux démon local en test (jamais en production,
    /// où un moteur distant n'est pas pris en charge, ADR-0036). Aucune configuration ne le produit : seul
    /// `BollardEngine::unix` est câblé (`app.rs`). Il reste compilé faute de fonctionnalité de test permise
    /// (garde `confirmation_guard`) ; le faux démon HTTP des tests en a besoin sous Windows.
    Http(String),
}

struct Inner {
    target: Target,
    markers: InstallMarkers,
    /// Délai de la sonde (5 s, `BR-ENGINE-009`) ; les tests le raccourcissent.
    probe_timeout: Duration,
    /// Client négocié, gardé tant que les sondes réussissent.
    client: Mutex<Option<Docker>>,
    /// Ce que `/version` a dit, gardé tant que les sondes réussissent (un `/_ping` toutes les 2 s,
    /// pas de `/version` à chaque fois).
    announced: Mutex<Option<(Option<ApiVersion>, Announced)>>,
}

#[derive(Clone)]
pub struct BollardEngine {
    inner: Arc<Inner>,
}

/// Ce que `/_ping` a répondu.
#[derive(Debug)]
struct PingReply {
    api_version: Option<ApiVersion>,
}

/// Pourquoi `/_ping` n'a pas abouti.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PingFailure {
    Missing,
    Refused,
    Denied,
    NoAnswer,
}

impl BollardEngine {
    pub fn new(target: Target, markers: InstallMarkers) -> Self {
        Self {
            inner: Arc::new(Inner {
                target,
                markers,
                probe_timeout: PROBE_TIMEOUT,
                client: Mutex::new(None),
                announced: Mutex::new(None),
            }),
        }
    }

    /// Raccourcit le délai de la sonde (tests : un faux démon muet ne fait pas attendre 5 s).
    pub fn with_probe_timeout(self, probe_timeout: Duration) -> Self {
        let inner = &self.inner;
        Self {
            inner: Arc::new(Inner {
                target: inner.target.clone(),
                markers: inner.markers.clone(),
                probe_timeout,
                client: Mutex::new(None),
                announced: Mutex::new(None),
            }),
        }
    }

    /// Le moteur du socket Unix réglé par la configuration.
    pub fn unix(socket: impl Into<PathBuf>) -> Self {
        Self::new(Target::Unix(socket.into()), InstallMarkers::linux())
    }

    /// La version d'API que le client négocié parle, `None` avant la première sonde réussie. Les
    /// tests la comparent à ce que le moteur annonce.
    pub fn spoken_api_version(&self) -> Option<ApiVersion> {
        let client = self
            .inner
            .client
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()?;
        let version = client.client_version();
        Some(ApiVersion {
            major: u32::try_from(version.major_version).ok()?,
            minor: u32::try_from(version.minor_version).ok()?,
        })
    }

    /// Liste puis inspecte chaque conteneur. Un conteneur disparu entre les deux est omis ; un
    /// conteneur dont l'inspection échoue (délai, réponse illisible) est rendu à part ; seule la
    /// perte du moteur (injoignable, accès refusé) interrompt la relecture.
    async fn list_all(&self) -> Result<Listing, EngineError> {
        let docker = self.inner.docker().await?;
        let options = ListContainersOptionsBuilder::default().all(true).build();
        let summaries = docker
            .list_containers(Some(options))
            .await
            .map_err(|error| translate(&error))?;
        let named: Vec<(String, String)> = summaries
            .into_iter()
            .filter_map(|summary| {
                let name = summary
                    .names
                    .iter()
                    .flatten()
                    .next()
                    .map(|name| bounded(name.trim_start_matches('/'), MAX_NAME_CHARS))
                    .unwrap_or_default();
                Some((summary.id?, name))
            })
            .collect();
        let mut inspections = stream::iter(named)
            .map(|(id, name)| {
                let docker = docker.clone();
                async move {
                    let result = inspect_with(&docker, &id).await;
                    (id, name, result)
                }
            })
            .buffer_unordered(INSPECT_CONCURRENCY);
        let mut listing = Listing::default();
        while let Some((id, name, result)) = inspections.next().await {
            match result {
                Ok(Some(observation)) => listing.observations.push(observation),
                // Supprimé entre la liste et son inspection : il n'existe plus.
                Ok(None) => {}
                // Le moteur est perdu : abandonner (les inspections en cours sont annulées).
                Err(error) if error.engine_gone() => return Err(error),
                Err(_) => listing.unreadable.push(Unreadable { id, name }),
            }
        }
        Ok(listing)
    }
}

impl Inner {
    fn forget(&self) {
        *self.client.lock().unwrap_or_else(PoisonError::into_inner) = None;
        *self
            .announced
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = None;
    }

    /// Un client prêt : celui qu'on garde, sinon un neuf après un `/_ping`.
    async fn docker(&self) -> Result<Docker, EngineError> {
        if let Some(docker) = self
            .client
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
        {
            return Ok(docker);
        }
        let reply = tokio::time::timeout(self.probe_timeout, ping(&self.target))
            .await
            .map_err(|_| EngineError::Timeout)?
            .map_err(|failure| match failure {
                PingFailure::NoAnswer => EngineError::Timeout,
                PingFailure::Denied => EngineError::Denied,
                PingFailure::Missing | PingFailure::Refused => EngineError::Unreachable,
            })?;
        let docker = self.build(reply.api_version)?;
        *self.client.lock().unwrap_or_else(PoisonError::into_inner) = Some(docker.clone());
        Ok(docker)
    }

    /// Un client pour un ordre (démarrer, arrêter, redémarrer) : son délai de requête est l'attente de l'action
    /// (le délai de grâce du conteneur plus la marge, au moins le plancher), pas les 10 s des lectures ; et le
    /// délai de grâce en secondes pour la requête. Démarrer passe un délai nul : l'attente est le plancher.
    async fn docker_for_order(&self, grace: Duration) -> Result<(Docker, i32), EngineError> {
        let docker = self.docker().await?;
        let seconds = i32::try_from(grace.as_secs()).unwrap_or(i32::MAX);
        let wait = wait_budget(u32::try_from(grace.as_secs()).unwrap_or(u32::MAX));
        Ok((docker.with_timeout(wait), seconds))
    }

    /// Un client à la version d'API du moteur, au plus celle de `bollard`.
    fn build(&self, engine_api: Option<ApiVersion>) -> Result<Docker, EngineError> {
        let version = client_version(engine_api);
        let built = match &self.target {
            #[cfg(unix)]
            Target::Unix(path) => {
                Docker::connect_with_unix(&path.to_string_lossy(), REQUEST_TIMEOUT_S, &version)
            }
            // Pas de socket Unix hors Unix : « injoignable ».
            #[cfg(not(unix))]
            Target::Unix(path) => Err(bollard::errors::Error::SocketNotFoundError(
                path.to_string_lossy().into_owned(),
            )),
            Target::Http(address) => {
                Docker::connect_with_http(address, REQUEST_TIMEOUT_S, &version)
            }
        };
        built.map_err(|error| translate(&error))
    }
}

/// La version d'API à parler : celle du moteur, plafonnée à celle de `bollard`. Sans réponse
/// claire, le plancher pris en charge (toute version de Docker acceptée le comprend).
fn client_version(engine_api: Option<ApiVersion>) -> ClientVersion {
    let ceiling = ClientVersion {
        major_version: API_DEFAULT_VERSION.major_version,
        minor_version: API_DEFAULT_VERSION.minor_version,
    };
    let wanted = engine_api.unwrap_or(MIN_API_VERSION);
    let wanted = ClientVersion {
        major_version: wanted.major as usize,
        minor_version: wanted.minor as usize,
    };
    if wanted < ceiling { wanted } else { ceiling }
}

/// `GET /_ping` à la main : connexion, requête, lecture des en-têtes. Aucune version dans le chemin.
async fn ping(target: &Target) -> Result<PingReply, PingFailure> {
    match target {
        #[cfg(unix)]
        Target::Unix(path) => {
            let stream = tokio::net::UnixStream::connect(path)
                .await
                .map_err(connect_failure)?;
            ping_over(stream).await
        }
        #[cfg(not(unix))]
        Target::Unix(_) => Err(PingFailure::Missing),
        Target::Http(address) => {
            let stream = tokio::net::TcpStream::connect(address)
                .await
                .map_err(connect_failure)?;
            ping_over(stream).await
        }
    }
}

fn connect_failure(error: std::io::Error) -> PingFailure {
    use std::io::ErrorKind;
    match error.kind() {
        ErrorKind::NotFound => PingFailure::Missing,
        ErrorKind::PermissionDenied => PingFailure::Denied,
        ErrorKind::ConnectionRefused => PingFailure::Refused,
        _ => PingFailure::NoAnswer,
    }
}

async fn ping_over<S: AsyncRead + AsyncWrite + Unpin>(
    mut stream: S,
) -> Result<PingReply, PingFailure> {
    stream
        .write_all(b"GET /_ping HTTP/1.1\r\nHost: docker\r\nConnection: close\r\n\r\n")
        .await
        .map_err(|_| PingFailure::NoAnswer)?;
    let mut bytes = Vec::new();
    // Le serveur ferme la connexion après la réponse (`Connection: close`) ; on s'arrête à la fin des
    // en-têtes, le corps (« OK ») ne sert à rien.
    let mut chunk = [0u8; 1024];
    loop {
        let read = stream
            .read(&mut chunk)
            .await
            .map_err(|_| PingFailure::NoAnswer)?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..read]);
        if bytes.windows(4).any(|window| window == b"\r\n\r\n") || bytes.len() > MAX_PING_BYTES {
            break;
        }
    }
    parse_ping(&bytes)
}

fn parse_ping(bytes: &[u8]) -> Result<PingReply, PingFailure> {
    let text = String::from_utf8_lossy(bytes);
    let mut lines = text.split("\r\n");
    let status_line = lines.next().ok_or(PingFailure::NoAnswer)?;
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .ok_or(PingFailure::NoAnswer)?;
    match status {
        200..=299 => {}
        // Un proxy de socket ou un plugin d'autorisation qui refuse.
        401 | 403 => return Err(PingFailure::Denied),
        _ => return Err(PingFailure::NoAnswer),
    }
    let mut api_version = None;
    for line in lines {
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.trim().eq_ignore_ascii_case("api-version")
        {
            api_version = ApiVersion::parse(value);
        }
    }
    Ok(PingReply { api_version })
}

/// Le nom que le moteur annonce : un composant « Podman… » s'il y en a un, sinon le nom de la
/// plateforme, sinon « Docker Engine » (Debian laisse le nom de plateforme vide).
fn announced_name(version: &SystemVersion) -> String {
    if let Some(component) = version
        .components
        .iter()
        .flatten()
        .find(|component| component.name.to_lowercase().contains("podman"))
    {
        return bounded(&component.name, MAX_ENGINE_TEXT_CHARS);
    }
    match version
        .platform
        .as_ref()
        .map(|platform| platform.name.trim())
    {
        Some(name) if !bounded(name, MAX_ENGINE_TEXT_CHARS).is_empty() => {
            bounded(name, MAX_ENGINE_TEXT_CHARS)
        }
        _ => "Docker Engine".to_owned(),
    }
}

/// Les échanges de la sonde, sans délai : l'appelant les borne ensemble.
async fn probe_steps(inner: &Inner) -> EngineProbe {
    let installed = |present: bool| EngineProbe {
        outcome: ProbeOutcome::SocketMissing,
        installed: present,
    };
    let reply = match ping(&inner.target).await {
        Err(failure) => {
            inner.forget();
            return match failure {
                PingFailure::Missing => installed(inner.markers.any_present()),
                PingFailure::Refused => EngineProbe {
                    outcome: ProbeOutcome::Refused,
                    installed: inner.markers.any_present(),
                },
                PingFailure::Denied => EngineProbe {
                    outcome: ProbeOutcome::PermissionDenied,
                    installed: true,
                },
                PingFailure::NoAnswer => EngineProbe {
                    outcome: ProbeOutcome::NoAnswer,
                    installed: true,
                },
            };
        }
        Ok(reply) => reply,
    };
    // Le moteur répond : `/version` une fois, puis seulement si son API change ou après une panne.
    let cached = inner
        .announced
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    if let Some((api, announced)) = cached
        && api == reply.api_version
    {
        return EngineProbe {
            outcome: ProbeOutcome::Answered(announced),
            installed: true,
        };
    }
    let no_answer = EngineProbe {
        outcome: ProbeOutcome::NoAnswer,
        installed: true,
    };
    let Ok(docker) = inner.build(reply.api_version) else {
        return no_answer;
    };
    let version = match docker.version().await {
        Ok(version) => version,
        Err(_) => {
            inner.forget();
            return no_answer;
        }
    };
    let api_version = version
        .api_version
        .as_deref()
        .and_then(ApiVersion::parse)
        .or(reply.api_version);
    let Some(api_version) = api_version else {
        inner.forget();
        return no_answer;
    };
    let announced = Announced {
        name: announced_name(&version),
        version: bounded(
            version.version.as_deref().unwrap_or_default(),
            MAX_ENGINE_TEXT_CHARS,
        ),
        api_version,
    };
    *inner.client.lock().unwrap_or_else(PoisonError::into_inner) = Some(docker);
    *inner
        .announced
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = Some((reply.api_version, announced.clone()));
    EngineProbe {
        outcome: ProbeOutcome::Answered(announced),
        installed: true,
    }
}

#[async_trait]
impl ContainerEngine for BollardEngine {
    async fn probe(&self) -> EngineProbe {
        // Un seul délai pour toute la sonde (`/_ping` puis `/version`) : `BR-ENGINE-009`.
        match tokio::time::timeout(self.inner.probe_timeout, probe_steps(&self.inner)).await {
            Ok(probe) => probe,
            Err(_) => {
                self.inner.forget();
                EngineProbe {
                    outcome: ProbeOutcome::NoAnswer,
                    installed: true,
                }
            }
        }
    }

    async fn list(&self) -> Result<Listing, EngineError> {
        tokio::time::timeout(LIST_TIMEOUT, self.list_all())
            .await
            .map_err(|_| EngineError::Timeout)?
    }

    async fn inspect(&self, id: &str) -> Result<Option<Observation>, EngineError> {
        let docker = self.inner.docker().await?;
        inspect_with(&docker, id).await
    }

    async fn env(&self, id: &str) -> Result<Option<Vec<EnvEntry>>, EngineError> {
        let docker = self.inner.docker().await?;
        let options = InspectContainerOptionsBuilder::default()
            .size(false)
            .build();
        match docker.inspect_container(id, Some(options)).await {
            Ok(response) => Ok(Some(env_entries_of(&response))),
            Err(error) => match translate(&error) {
                EngineError::NotFound => Ok(None),
                other => Err(other),
            },
        }
    }

    fn logs(&self, id: &str, query: LogQuery) -> LogStream {
        super::logs::open(self.inner.target.clone(), id.to_owned(), query)
    }

    async fn start(&self, id: &str) -> Result<(), EngineError> {
        let (docker, _) = self.inner.docker_for_order(Duration::ZERO).await?;
        docker
            .start_container(id, None)
            .await
            .map_err(|error| translate(&error))
    }

    async fn stop(&self, id: &str, grace: Duration) -> Result<(), EngineError> {
        let (docker, seconds) = self.inner.docker_for_order(grace).await?;
        let options = StopContainerOptionsBuilder::default().t(seconds).build();
        docker
            .stop_container(id, Some(options))
            .await
            .map_err(|error| translate(&error))
    }

    async fn restart(&self, id: &str, grace: Duration) -> Result<(), EngineError> {
        let (docker, seconds) = self.inner.docker_for_order(grace).await?;
        let options = RestartContainerOptionsBuilder::default().t(seconds).build();
        docker
            .restart_container(id, Some(options))
            .await
            .map_err(|error| translate(&error))
    }

    async fn image_present(&self, image: &str) -> Result<bool, EngineError> {
        let docker = self.inner.docker().await?;
        match docker.inspect_image(image).await {
            Ok(_) => Ok(true),
            Err(error) => match translate(&error) {
                EngineError::NotFound => Ok(false),
                other => Err(other),
            },
        }
    }

    fn pull(&self, image: &str) -> PullStream {
        let inner = self.inner.clone();
        let (repository, tag) = split_image(image, None);
        stream::once(async move { inner.docker().await })
            .flat_map(move |docker| match docker {
                Ok(docker) => {
                    // Un téléchargement dure des minutes : la borne est celle du déploiement (30 minutes),
                    // pas les 10 s des lectures. L'appelant l'arrête avant, ou l'abandonne (annulation).
                    let docker = docker.with_timeout(Duration::from_secs(DEPLOY_TIMEOUT_S));
                    let options = CreateImageOptionsBuilder::default()
                        .from_image(&repository)
                        .tag(&tag)
                        .build();
                    docker
                        .create_image(Some(options), None, None)
                        .scan(PullTracker::new(), |tracker, item| {
                            let out = match item {
                                Ok(info) => {
                                    layer_event(&info).map(|event| Ok(tracker.apply(event)))
                                }
                                Err(error) => Some(Err(translate_pull(&error))),
                            };
                            futures_util::future::ready(Some(out))
                        })
                        .filter_map(futures_util::future::ready)
                        .boxed()
                }
                Err(error) => stream::once(async move { Err(error) }).boxed(),
            })
            .boxed()
    }

    async fn create(&self, plan: &ContainerPlan) -> Result<String, EngineError> {
        let docker = self.inner.docker().await?;
        let options = CreateContainerOptionsBuilder::default()
            .name(&plan.name)
            .build();
        docker
            .create_container(Some(options), create_body(plan))
            .await
            .map(|created| created.id)
            .map_err(|error| translate_create(&error))
    }

    async fn remove(&self, id: &str) -> Result<(), EngineError> {
        let docker = self.inner.docker().await?;
        let options = RemoveContainerOptionsBuilder::default()
            .v(true)
            .force(true)
            .build();
        match docker.remove_container(id, Some(options)).await {
            Ok(()) => Ok(()),
            Err(error) => match translate(&error) {
                // Déjà parti : le but est atteint.
                EngineError::NotFound => Ok(()),
                other => Err(other),
            },
        }
    }

    async fn info(&self) -> Result<EngineInfo, EngineError> {
        let docker = self.inner.docker().await?;
        let info = docker.info().await.map_err(|error| translate(&error))?;
        Ok(EngineInfo {
            cpus: info
                .ncpu
                .and_then(|n| u32::try_from(n).ok())
                .unwrap_or_default(),
            memory_bytes: info
                .mem_total
                .and_then(|n| u64::try_from(n).ok())
                .unwrap_or_default(),
            storage_dir: info
                .docker_root_dir
                .filter(|dir| dir.starts_with('/') && !dir.contains('\0')),
        })
    }

    fn events(&self) -> EngineEvents {
        let inner = self.inner.clone();
        stream::once(async move { inner.docker().await })
            .flat_map(|docker| match docker {
                Ok(docker) => {
                    let filters =
                        HashMap::from([("type".to_owned(), vec!["container".to_owned()])]);
                    let options = EventsOptionsBuilder::default().filters(&filters).build();
                    docker
                        .events(Some(options))
                        .filter_map(|message| async move {
                            match message {
                                Ok(message) => relevant(&message).map(Ok),
                                Err(error) => Some(Err(translate(&error))),
                            }
                        })
                        .boxed()
                }
                Err(error) => stream::once(async move { Err(error) }).boxed(),
            })
            .boxed()
    }
}

/// L'événement d'une couche que la progression suit, sinon `None`. Le texte du moteur est lu pour être
/// reconnu, jamais gardé ni affiché.
fn layer_event(info: &CreateImageInfo) -> Option<LayerEvent> {
    let layer = info.id.clone().filter(|id| !id.is_empty())?;
    let status = info.status.as_deref()?;
    if status.starts_with("Downloading") {
        let detail = info.progress_detail.as_ref()?;
        return Some(LayerEvent::Downloading {
            layer,
            current: detail
                .current
                .and_then(|n| u64::try_from(n).ok())
                .unwrap_or(0),
            total: detail
                .total
                .and_then(|n| u64::try_from(n).ok())
                .unwrap_or(0),
        });
    }
    matches!(
        status,
        "Download complete" | "Pull complete" | "Already exists"
    )
    .then_some(LayerEvent::Done { layer })
}

/// Le corps de création d'un conteneur, depuis le plan fermé : rien d'autre que ce que [`ContainerPlan`] porte.
/// Les dossiers montés sont des montages de type `bind` structurés (jamais une chaîne `source:cible` à
/// découper), les ports n'ont pas d'adresse (tout le réseau, BR-DEPLOY-035), le redémarrage est
/// `unless-stopped`, les limites sont posées, le swap est égal à la mémoire (la limite est réelle).
fn create_body(plan: &ContainerPlan) -> ContainerCreateBody {
    let mut port_bindings: PortMap = HashMap::new();
    let mut exposed = Vec::new();
    for port in &plan.ports {
        let protocol = match port.protocol {
            SpecProtocol::Tcp => "tcp",
            SpecProtocol::Udp => "udp",
            SpecProtocol::Sctp => "sctp",
        };
        let key = format!("{}/{protocol}", port.container_port);
        if !exposed.contains(&key) {
            exposed.push(key.clone());
        }
        port_bindings
            .entry(key)
            .or_insert_with(|| Some(Vec::new()))
            .get_or_insert_with(Vec::new)
            .push(PortBinding {
                host_ip: None,
                host_port: Some(port.host_port.to_string()),
            });
    }
    let memory = i64::try_from(plan.memory_bytes).unwrap_or(i64::MAX);
    let host_config = HostConfig {
        port_bindings: (!port_bindings.is_empty()).then_some(port_bindings),
        mounts: (!plan.binds.is_empty()).then(|| {
            plan.binds
                .iter()
                .map(|bind| Mount {
                    target: Some(bind.target.clone()),
                    source: Some(bind.host_path.clone()),
                    typ: Some(MountType::BIND),
                    read_only: Some(bind.read_only),
                    ..Mount::default()
                })
                .collect()
        }),
        restart_policy: Some(RestartPolicy {
            name: Some(match plan.restart {
                SpecRestart::No => RestartPolicyNameEnum::NO,
                SpecRestart::Always => RestartPolicyNameEnum::ALWAYS,
                SpecRestart::UnlessStopped => RestartPolicyNameEnum::UNLESS_STOPPED,
                SpecRestart::OnFailure => RestartPolicyNameEnum::ON_FAILURE,
            }),
            maximum_retry_count: None,
        }),
        memory: Some(memory),
        memory_swap: Some(memory),
        nano_cpus: Some(i64::from(plan.cpu_millis) * 1_000_000),
        ..HostConfig::default()
    };
    ContainerCreateBody {
        image: Some(plan.image.clone()),
        labels: Some(plan.labels.clone().into_iter().collect()),
        env: (!plan.env.is_empty()).then(|| {
            plan.env
                .iter()
                .map(|(name, value)| format!("{}={}", name.expose(), value.expose()))
                .collect()
        }),
        cmd: plan
            .command
            .as_ref()
            .map(|command| command.iter().map(|arg| arg.expose().to_owned()).collect()),
        exposed_ports: (!exposed.is_empty()).then_some(exposed),
        open_stdin: plan.stdin_open.then_some(true),
        stop_timeout: plan.stop_grace_s.map(i64::from),
        host_config: Some(host_config),
        ..ContainerCreateBody::default()
    }
}

async fn inspect_with(docker: &Docker, id: &str) -> Result<Option<Observation>, EngineError> {
    let options = InspectContainerOptionsBuilder::default()
        .size(false)
        .build();
    match docker.inspect_container(id, Some(options)).await {
        Ok(response) => Ok(observation_from(&response)),
        Err(error) => match translate(&error) {
            EngineError::NotFound => Ok(None),
            other => Err(other),
        },
    }
}

/// L'événement d'un conteneur dont l'action change la liste, sinon `None`.
fn relevant(message: &EventMessage) -> Option<EngineEvent> {
    if message.typ != Some(EventMessageTypeEnum::CONTAINER) {
        return None;
    }
    let action = message.action.as_deref()?;
    // Certains moteurs suffixent l'action (« health_status: healthy », « exec_start: sh »).
    let verb = action.split(':').next().unwrap_or(action).trim();
    if !RELEVANT_ACTIONS.contains(&verb) {
        return None;
    }
    let id = message
        .actor
        .as_ref()?
        .id
        .clone()
        .filter(|id| !id.is_empty())?;
    Some(EngineEvent {
        container_id: id,
        action: verb.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use bollard::models::EventActor;

    use super::*;

    fn message(typ: EventMessageTypeEnum, action: &str, id: Option<&str>) -> EventMessage {
        EventMessage {
            typ: Some(typ),
            action: Some(action.into()),
            actor: Some(EventActor {
                id: id.map(str::to_owned),
                attributes: None,
            }),
            ..Default::default()
        }
    }

    #[test]
    fn only_container_events_that_change_the_list_are_forwarded() {
        let forwarded = relevant(&message(
            EventMessageTypeEnum::CONTAINER,
            "die",
            Some("abc"),
        ));
        assert_eq!(
            forwarded.map(|event| event.container_id).as_deref(),
            Some("abc")
        );
        for action in [
            "exec_start: sh",
            "health_status: healthy",
            "attach",
            "top",
            "resize",
        ] {
            assert!(
                relevant(&message(
                    EventMessageTypeEnum::CONTAINER,
                    action,
                    Some("abc")
                ))
                .is_none(),
                "{action}"
            );
        }
        assert!(relevant(&message(EventMessageTypeEnum::IMAGE, "pull", Some("abc"))).is_none());
        assert!(relevant(&message(EventMessageTypeEnum::CONTAINER, "start", None)).is_none());
    }

    #[test]
    fn the_ping_reply_gives_the_api_version_in_any_case() {
        let reply = parse_ping(
            b"HTTP/1.1 200 OK\r\nApi-Version: 1.43\r\nServer: Docker/24.0.7 (linux)\r\n\r\nOK",
        )
        .expect("réponse");
        assert_eq!(reply.api_version, ApiVersion::parse("1.43"));
        let reply = parse_ping(b"HTTP/1.1 200 OK\r\napi-version: 1.41\r\n\r\n").expect("réponse");
        assert_eq!(reply.api_version, ApiVersion::parse("1.41"));
        let reply = parse_ping(b"HTTP/1.1 200 OK\r\n\r\nOK").expect("réponse");
        assert_eq!(reply.api_version, None);
    }

    #[test]
    fn an_authorization_refusal_is_a_denial_and_anything_else_is_silence() {
        for status in [401, 403] {
            let reply = parse_ping(format!("HTTP/1.1 {status} X\r\n\r\n").as_bytes());
            assert_eq!(reply.err(), Some(PingFailure::Denied));
        }
        assert_eq!(
            parse_ping(b"HTTP/1.1 500 X\r\n\r\n").err(),
            Some(PingFailure::NoAnswer)
        );
        assert_eq!(parse_ping(b"").err(), Some(PingFailure::NoAnswer));
        assert_eq!(
            parse_ping(b"pas du http").err(),
            Some(PingFailure::NoAnswer)
        );
    }

    #[test]
    fn the_client_speaks_the_engine_version_capped_by_the_library() {
        let engine = |text: &str| client_version(ApiVersion::parse(text));
        assert_eq!(engine("1.41").minor_version, 41);
        assert_eq!(engine("1.45").minor_version, 45);
        // Un moteur plus récent que la bibliothèque : on plafonne.
        assert_eq!(
            engine("1.999").minor_version,
            API_DEFAULT_VERSION.minor_version
        );
        // Sans réponse claire : le plancher pris en charge.
        assert_eq!(client_version(None).minor_version, 41);
    }

    #[test]
    fn the_engine_name_is_what_the_engine_announces() {
        let version: SystemVersion = serde_json::from_value(serde_json::json!({
            "Platform": { "Name": "Docker Engine - Community" },
            "Components": [{ "Name": "Engine", "Version": "24.0.7" }],
            "Version": "24.0.7", "ApiVersion": "1.43"
        }))
        .expect("version");
        assert_eq!(announced_name(&version), "Docker Engine - Community");
        // Debian laisse le nom de plateforme vide.
        let version: SystemVersion = serde_json::from_value(serde_json::json!({
            "Platform": { "Name": "" },
            "Components": [{ "Name": "Engine", "Version": "20.10.24" }],
            "Version": "20.10.24", "ApiVersion": "1.41"
        }))
        .expect("version");
        assert_eq!(announced_name(&version), "Docker Engine");
        let version: SystemVersion = serde_json::from_value(serde_json::json!({
            "Platform": { "Name": "linux/amd64/fedora-40" },
            "Components": [{ "Name": "Podman Engine", "Version": "5.2.0" }],
            "Version": "5.2.0", "ApiVersion": "1.41"
        }))
        .expect("version");
        assert_eq!(announced_name(&version), "Podman Engine");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_missing_socket_is_missing_or_stopped_never_an_error() {
        let engine = BollardEngine::new(
            Target::Unix(PathBuf::from("/nonexistent/hearth-test/docker.sock")),
            InstallMarkers::at(Vec::<PathBuf>::new()),
        );
        let probe = engine.probe().await;
        assert_eq!(probe.outcome, ProbeOutcome::SocketMissing);
        assert!(!probe.installed);
        // Aucune relance interne : une sonde sans moteur est un résultat, pas une panne.
        let engine = BollardEngine::new(
            Target::Unix(PathBuf::from("/nonexistent/hearth-test/docker.sock")),
            InstallMarkers::at([std::env::current_exe().expect("exe")]),
        );
        assert!(engine.probe().await.installed);
    }
}

//! Faux démon de conteneurs : un serveur HTTP/1.1 local qui rejoue des réponses de la forme
//! documentée de l'API du moteur Docker (`/_ping`, `/version`, `/containers/json`,
//! `/containers/{id}/json`, `/containers/{id}/logs`, `/events`), pour éprouver l'adaptateur `bollard` sans moteur.
//!
//! IMPORTANT : ces réponses sont FABRIQUÉES d'après la documentation de l'API du moteur et la forme
//! connue des réponses de Docker 24 ; elles ne viennent pas d'un vrai moteur. Leur fidélité n'est
//! prouvée que par la suite de contrat exécutée contre un vrai moteur en CI (`engine_real.rs`).
//!
//! Comme un vrai démon, il refuse une version d'API plus récente que la sienne (`400 client version
//! … is too new`) : c'est ce qui éprouve la négociation de version de l'adaptateur.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

#[derive(Debug, Clone)]
pub struct DaemonContainer {
    pub id: String,
    pub name: String,
    /// `created`, `running`, `paused`, `restarting`, `exited`, `dead`.
    pub status: String,
    pub exit_code: i64,
    /// Un port publié : (port du serveur, port du conteneur), en TCP.
    pub publish: Option<(u16, u16)>,
    /// `NOM=valeur`, exactement comme le moteur les rend.
    pub env: Vec<String>,
    /// `Config.Cmd`, exactement comme le moteur le rend.
    pub cmd: Vec<String>,
    pub image: String,
    pub restart: String,
    pub started_at: String,
    pub finished_at: String,
    /// `Config.Tty` : les journaux sont un flux brut, sans trames.
    pub tty: bool,
    /// `HostConfig.LogConfig.Type` : `none` ne se relit pas (501).
    pub log_driver: String,
    /// Ce que le conteneur a écrit (HRT-66).
    pub logs: Vec<DaemonLog>,
    /// `Config.Labels` (HRT-68).
    pub labels: BTreeMap<String, String>,
    /// `HostConfig.Memory`, en octets ; 0 : aucune limite.
    pub memory: i64,
    /// `HostConfig.NanoCpus` ; 0 : aucune limite.
    pub nano_cpus: i64,
    /// `HostConfig.Mounts` de type `bind` : (source, cible, lecture seule).
    pub mounts: Vec<(String, String, bool)>,
}

/// Un message de journal du conteneur, comme le moteur le garde : une sortie, une heure, des octets.
#[derive(Debug, Clone)]
pub struct DaemonLog {
    /// 1 : sortie normale, 2 : sortie d'erreur.
    pub stream: u8,
    pub at_ms: i64,
    pub text: Vec<u8>,
}

/// L'horodatage que le moteur écrit devant chaque message : neuf chiffres fixes, UTC.
pub fn stamp(at_ms: i64) -> String {
    let at = time::OffsetDateTime::from_unix_timestamp_nanos(i128::from(at_ms) * 1_000_000)
        .expect("heure valide");
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:09}Z",
        at.year(),
        u8::from(at.month()),
        at.day(),
        at.hour(),
        at.minute(),
        at.second(),
        at.nanosecond()
    )
}

impl DaemonContainer {
    pub fn new(name: &str, status: &str) -> Self {
        use sha2::{Digest, Sha256};
        let id: String = Sha256::digest(name.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let ran = matches!(
            status,
            "running" | "paused" | "restarting" | "exited" | "dead"
        );
        Self {
            id,
            name: name.to_owned(),
            status: status.to_owned(),
            exit_code: 0,
            publish: None,
            env: vec![],
            cmd: vec!["sleep".to_owned(), "300".to_owned()],
            image: "busybox:1.36".to_owned(),
            restart: "no".to_owned(),
            started_at: if ran {
                "2026-10-13T10:00:00.123456789Z".to_owned()
            } else {
                "0001-01-01T00:00:00Z".to_owned()
            },
            finished_at: if matches!(status, "exited" | "dead") {
                "2026-10-13T11:00:00.5Z".to_owned()
            } else {
                "0001-01-01T00:00:00Z".to_owned()
            },
            tty: false,
            log_driver: "json-file".to_owned(),
            logs: Vec::new(),
            labels: BTreeMap::new(),
            memory: 0,
            nano_cpus: 0,
            mounts: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DaemonConfig {
    /// La version d'API la plus récente que le démon parle (`1.43` : Docker 24).
    pub api_version: (u32, u32),
    pub platform_name: String,
    /// Les composants de `/version` (le premier nomme le moteur).
    pub component: String,
    pub engine_version: String,
    /// Statut de `/_ping` (200, ou 403 comme un proxy de socket qui refuse).
    pub ping_status: u16,
    /// Ne répond jamais (le moteur « ne répond pas »).
    pub silent: bool,
    /// Attend ce temps avant de répondre à `/_ping` (un moteur lent à décider).
    pub ping_delay: std::time::Duration,
    /// Répond à `/_ping` mais jamais à `/version`.
    pub version_silent: bool,
    /// Statut de `GET /containers/json` (200, ou 403 comme un proxy qui refuse la liste).
    pub list_status: u16,
    /// `GET /info` : processeurs, mémoire totale et dossier des images (HRT-68).
    pub ncpu: i64,
    pub mem_total: i64,
    pub root_dir: String,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            api_version: (1, 43),
            platform_name: "Docker Engine - Community".into(),
            component: "Engine".into(),
            engine_version: "24.0.7".into(),
            ping_status: 200,
            silent: false,
            ping_delay: std::time::Duration::ZERO,
            version_silent: false,
            list_status: 200,
            ncpu: 8,
            mem_total: 16 * 1024 * 1024 * 1024,
            root_dir: "/var/lib/docker".into(),
        }
    }
}

struct State {
    config: DaemonConfig,
    containers: BTreeMap<String, DaemonContainer>,
    /// Conteneurs dont l'inspection répond n'importe quoi (une réponse illisible).
    broken_inspection: std::collections::BTreeSet<String>,
    /// Conteneurs encore dans la liste mais déjà supprimés quand on les inspecte (404).
    vanished: std::collections::BTreeSet<String>,
    /// Les chemins demandés, dans l'ordre (pour vérifier qu'aucune écriture n'est jamais émise).
    requests: Vec<String>,
    /// Combien de messages de journal le démon a écrits sur le socket, par conteneur (HRT-66) : un
    /// lecteur qui ne vide pas son socket arrête ce compte.
    served_logs: BTreeMap<String, usize>,
    /// Les images présentes sur le serveur (HRT-68).
    images: BTreeSet<String>,
    /// Ce que le registre rend pour `dépôt:version`.
    registry: BTreeMap<String, DaemonPull>,
    /// Les dépôts connus du registre : une version absente d'un dépôt connu est `manifest unknown`.
    known_repositories: BTreeSet<String>,
    /// Combien de téléchargements le client a abandonnés avant la fin.
    pull_aborts: usize,
    /// Les corps de création reçus, tels quels.
    created_bodies: Vec<Value>,
}

/// Ce que le registre du faux démon rend pour une image : des lignes de progression au format du moteur.
#[derive(Debug, Clone)]
pub struct DaemonPull {
    pub lines: Vec<Value>,
    /// Attente entre deux lignes, en millisecondes.
    pub delay_ms: u64,
}

impl DaemonPull {
    /// Une image à une couche, avec la progression que Docker écrit.
    pub fn one_layer(delay_ms: u64) -> Self {
        let layer = "5b1f0f0e2a7c";
        Self {
            lines: vec![
                json!({ "status": "Pulling from library/busybox", "id": "1.36" }),
                json!({ "status": "Pulling fs layer", "progressDetail": {}, "id": layer }),
                json!({ "status": "Downloading", "progressDetail": { "current": 1000, "total": 4000 }, "progress": "[=>   ]", "id": layer }),
                json!({ "status": "Downloading", "progressDetail": { "current": 4000, "total": 4000 }, "progress": "[====>]", "id": layer }),
                json!({ "status": "Download complete", "progressDetail": {}, "id": layer }),
                json!({ "status": "Extracting", "progressDetail": { "current": 4000, "total": 4000 }, "id": layer }),
                json!({ "status": "Pull complete", "progressDetail": {}, "id": layer }),
                json!({ "status": "Status: Downloaded newer image for busybox:1.36" }),
            ],
            delay_ms,
        }
    }
}

pub struct FakeDaemon {
    pub addr: SocketAddr,
    state: Arc<Mutex<State>>,
    events: broadcast::Sender<String>,
    task: JoinHandle<()>,
    /// Les connexions en cours : abandonnées avec le démon, comme si le moteur tombait.
    connections: Arc<Mutex<Vec<JoinHandle<()>>>>,
}

impl Drop for FakeDaemon {
    fn drop(&mut self) {
        self.task.abort();
        for connection in self
            .connections
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain(..)
        {
            connection.abort();
        }
    }
}

impl FakeDaemon {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("port local");
        let addr = listener.local_addr().expect("adresse");
        let state = Arc::new(Mutex::new(State {
            config: DaemonConfig::default(),
            containers: BTreeMap::new(),
            broken_inspection: Default::default(),
            vanished: Default::default(),
            requests: Vec::new(),
            served_logs: BTreeMap::new(),
            images: BTreeSet::new(),
            registry: BTreeMap::new(),
            known_repositories: BTreeSet::new(),
            pull_aborts: 0,
            created_bodies: Vec::new(),
        }));
        let (events, _) = broadcast::channel(256);
        let connections: Arc<Mutex<Vec<JoinHandle<()>>>> = Arc::default();
        let task = tokio::spawn({
            let state = state.clone();
            let events = events.clone();
            let connections = connections.clone();
            async move {
                loop {
                    let Ok((stream, _)) = listener.accept().await else {
                        return;
                    };
                    let handle = tokio::spawn(handle(stream, state.clone(), events.clone()));
                    connections
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push(handle);
                }
            }
        });
        Self {
            addr,
            state,
            events,
            task,
            connections,
        }
    }

    /// `hôte:port`, la forme que `Target::Http` attend.
    pub fn address(&self) -> String {
        self.addr.to_string()
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn configure(&self, change: impl FnOnce(&mut DaemonConfig)) {
        change(&mut self.lock().config);
    }

    pub fn put(&self, container: DaemonContainer) {
        self.lock()
            .containers
            .insert(container.id.clone(), container);
    }

    pub fn update(&self, id: &str, change: impl FnOnce(&mut DaemonContainer)) {
        if let Some(container) = self.lock().containers.get_mut(id) {
            change(container);
        }
    }

    /// L'inspection de ce conteneur répond une réponse illisible.
    pub fn break_inspection(&self, id: &str) {
        let mut state = self.lock();
        state.broken_inspection.insert(id.to_owned());
    }

    /// Ce conteneur reste dans la liste mais n'existe plus à l'inspection (404), et son inspection
    /// n'est plus illisible.
    pub fn vanish_after_listing(&self, id: &str) {
        let mut state = self.lock();
        state.broken_inspection.remove(id);
        state.vanished.insert(id.to_owned());
    }

    pub fn remove(&self, id: &str) {
        self.lock().containers.remove(id);
    }

    /// Le conteneur écrit une ligne sur cette sortie (1 : normale, 2 : erreur), à cette heure.
    pub fn push_log(&self, id: &str, stream: u8, at_ms: i64, text: &str) {
        let mut bytes = text.as_bytes().to_vec();
        bytes.push(b'\n');
        self.push_log_raw(id, stream, at_ms, &bytes);
    }

    /// Le conteneur écrit ces octets tels quels.
    pub fn push_log_raw(&self, id: &str, stream: u8, at_ms: i64, bytes: &[u8]) {
        if let Some(container) = self.lock().containers.get_mut(id) {
            container.logs.push(DaemonLog {
                stream,
                at_ms,
                text: bytes.to_vec(),
            });
        }
    }

    pub fn emit(&self, id: &str, action: &str) {
        let line = json!({
            "Type": "container", "Action": action, "status": action, "id": id,
            "Actor": { "ID": id, "Attributes": { "image": "busybox:1.36", "name": "x" } },
            "scope": "local", "time": 1_791_885_600, "timeNano": 1_791_885_600_000_000_000u64
        });
        // Sans abonné, `send` rend une erreur : personne n'écoute, ce n'est pas un échec.
        let _ = self.events.send(format!("{line}\n"));
    }

    /// Combien de messages de journal de ce conteneur le démon a pu écrire sur le socket de ses lecteurs.
    pub fn served_logs(&self, id: &str) -> usize {
        self.lock().served_logs.get(id).copied().unwrap_or(0)
    }

    /// Tout ce que les clients ont demandé, `MÉTHODE chemin`.
    pub fn requests(&self) -> Vec<String> {
        self.lock().requests.clone()
    }

    /// L'image `dépôt:version` est déjà sur le serveur.
    pub fn have_image(&self, image: &str) {
        self.lock().images.insert(image.to_owned());
    }

    pub fn forget_image(&self, image: &str) {
        self.lock().images.remove(image);
    }

    pub fn has_image(&self, image: &str) -> bool {
        self.lock().images.contains(image)
    }

    /// Le registre rend cette image quand on la demande ; son dépôt devient connu.
    pub fn publish_image(&self, image: &str, pull: DaemonPull) {
        let mut state = self.lock();
        if let Some((repository, _)) = image.rsplit_once(':') {
            state.known_repositories.insert(repository.to_owned());
        }
        state.registry.insert(image.to_owned(), pull);
    }

    /// Combien de téléchargements le client a abandonnés avant leur fin.
    pub fn pull_aborts(&self) -> usize {
        self.lock().pull_aborts
    }

    /// Les corps de création reçus, tels que le moteur les a lus.
    pub fn created_bodies(&self) -> Vec<Value> {
        self.lock().created_bodies.clone()
    }

    /// Ce conteneur, tel que le démon le tient.
    pub fn container(&self, id: &str) -> Option<DaemonContainer> {
        self.lock().containers.get(id).cloned()
    }
}

/// Une réponse : statut, type de contenu, corps, en-têtes en plus.
struct Reply {
    status: u16,
    content_type: &'static str,
    body: String,
    extra: Vec<(&'static str, String)>,
}

fn json_reply(status: u16, body: Value) -> Reply {
    Reply {
        status,
        content_type: "application/json",
        body: body.to_string(),
        extra: vec![],
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        304 => "Not Modified",
        409 => "Conflict",
        500 => "Internal Server Error",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        501 => "Not Implemented",
        _ => "Status",
    }
}

async fn handle(
    mut stream: TcpStream,
    state: Arc<Mutex<State>>,
    events: broadcast::Sender<String>,
) {
    let mut head = Vec::new();
    let mut chunk = [0u8; 1024];
    while !head.windows(4).any(|window| window == b"\r\n\r\n") {
        match stream.read(&mut chunk).await {
            Ok(0) | Err(_) => return,
            Ok(read) => head.extend_from_slice(&chunk[..read]),
        }
        if head.len() > 16 * 1024 {
            return;
        }
    }
    let header_end = head
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map_or(head.len(), |at| at + 4);
    let text = String::from_utf8_lossy(&head[..header_end]).into_owned();
    // Le corps de la requête (création d'un conteneur) : `Content-Length` octets après les en-têtes.
    let length = text
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.trim()
                .eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    let mut body_bytes = head[header_end..].to_vec();
    while body_bytes.len() < length && body_bytes.len() < 1024 * 1024 {
        match stream.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(read) => body_bytes.extend_from_slice(&chunk[..read]),
        }
    }
    let request_line = text.lines().next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let (method, target) = (
        parts.next().unwrap_or_default().to_owned(),
        parts.next().unwrap_or_default().to_owned(),
    );
    let (path, query) = target.split_once('?').unwrap_or((target.as_str(), ""));
    let path = path.to_owned();
    let query = query.to_owned();
    let (config, silent) = {
        let mut guard = state.lock().unwrap_or_else(PoisonError::into_inner);
        guard.requests.push(format!("{method} {path}"));
        (guard.config.clone(), guard.config.silent)
    };
    if silent {
        // Connecté, mais le démon ne dit rien : la connexion reste ouverte.
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        return;
    }
    // Le préfixe de version : refusé s'il est plus récent que le démon, comme le fait Docker.
    let (versioned, route) = split_version(&path);
    if let Some((major, minor)) = versioned
        && (major, minor) > config.api_version
    {
        let message = format!(
            "client version {major}.{minor} is too new. Maximum supported API version is {}.{}",
            config.api_version.0, config.api_version.1
        );
        write_reply(&mut stream, json_reply(400, json!({ "message": message }))).await;
        return;
    }
    // Les trois seuls ordres d'écriture du port (HRT-64) : démarrer, arrêter, redémarrer un conteneur.
    if method == "POST"
        && let Some((id, verb)) = route
            .strip_prefix("/containers/")
            .and_then(|rest| rest.split_once('/'))
        && matches!(verb, "start" | "stop" | "restart")
    {
        let reply = order(&state, &events, id, verb, &query).await;
        write_reply(&mut stream, reply).await;
        return;
    }
    // Créer, supprimer, récupérer (HRT-68).
    if method == "POST" && route == "/containers/create" {
        let body: Value = serde_json::from_slice(&body_bytes).unwrap_or(Value::Null);
        let reply = create_container(&state, &events, &query, &body);
        write_reply(&mut stream, reply).await;
        return;
    }
    if method == "DELETE"
        && let Some(id) = route.strip_prefix("/containers/")
        && !id.contains('/')
    {
        let removed = state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .containers
            .remove(id)
            .is_some();
        let reply = if removed {
            announce(&events, id, "destroy");
            empty_reply(204)
        } else {
            json_reply(
                404,
                json!({ "message": format!("No such container: {id}") }),
            )
        };
        write_reply(&mut stream, reply).await;
        return;
    }
    if method == "POST" && route == "/images/create" {
        pull_image(stream, state.clone(), query).await;
        return;
    }
    if method != "GET" {
        write_reply(
            &mut stream,
            json_reply(404, json!({ "message": "page not found" })),
        )
        .await;
        return;
    }
    if route == "/events" {
        stream_events(stream, events).await;
        return;
    }
    if let Some(id) = route
        .strip_prefix("/containers/")
        .and_then(|rest| rest.strip_suffix("/logs"))
    {
        stream_logs(stream, state.clone(), id.to_owned(), query).await;
        return;
    }
    if route == "/_ping" && !config.ping_delay.is_zero() {
        tokio::time::sleep(config.ping_delay).await;
    }
    if route == "/version" && config.version_silent {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        return;
    }
    let reply = {
        let guard = state.lock().unwrap_or_else(PoisonError::into_inner);
        route_reply(&guard, &config, route)
    };
    write_reply(&mut stream, reply).await;
}

fn empty_reply(status: u16) -> Reply {
    Reply {
        status,
        content_type: "text/plain; charset=utf-8",
        body: String::new(),
        extra: vec![],
    }
}

fn announce(events: &broadcast::Sender<String>, id: &str, action: &str) {
    let line = json!({
        "Type": "container", "Action": action, "status": action, "id": id,
        "Actor": { "ID": id, "Attributes": { "image": "busybox:1.36", "name": "x" } },
        "scope": "local", "time": 1_791_885_600, "timeNano": 1_791_885_600_000_000_000u64
    });
    // Sans abonné, `send` rend une erreur : personne n'écoute, ce n'est pas un échec.
    let _ = events.send(format!("{line}\n"));
}

/// L'heure de maintenant, au format du moteur (nanosecondes).
fn now_stamp() -> String {
    let now = time::OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:09}Z",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
        now.nanosecond()
    )
}

/// `POST /containers/{id}/(start|stop|restart)` : comme le moteur. Démarrer un conteneur qui tourne ou
/// arrêter un conteneur arrêté répond `304` ; un conteneur inconnu `404` ; un port déjà tenu par un autre
/// conteneur en marche `500`. Un programme qui s'appelle `sleep` ignore l'ordre d'arrêt : le démon attend le
/// délai de grâce (`t`), puis le tue (code 137) ; les autres sortent aussitôt, code 0.
async fn order(
    state: &Arc<Mutex<State>>,
    events: &broadcast::Sender<String>,
    id: &str,
    verb: &str,
    query: &str,
) -> Reply {
    let grace: u64 = query
        .split('&')
        .find_map(|pair| pair.strip_prefix("t="))
        .and_then(|value| value.parse().ok())
        .unwrap_or(10);
    let missing = || {
        json_reply(
            404,
            json!({ "message": format!("No such container: {id}") }),
        )
    };
    if verb != "start" {
        // Arrêter (ou la première moitié d'un redémarrage).
        let deaf = {
            let guard = state.lock().unwrap_or_else(PoisonError::into_inner);
            let Some(container) = guard.containers.get(id) else {
                return missing();
            };
            if container.status != "running" {
                if verb == "stop" {
                    return empty_reply(304);
                }
                None
            } else {
                Some(container.cmd.first().map(String::as_str) == Some("sleep"))
            }
        };
        if let Some(deaf) = deaf {
            if deaf {
                tokio::time::sleep(std::time::Duration::from_secs(grace)).await;
            }
            {
                let mut guard = state.lock().unwrap_or_else(PoisonError::into_inner);
                if let Some(container) = guard.containers.get_mut(id) {
                    container.status = "exited".into();
                    container.exit_code = if deaf { 137 } else { 0 };
                    container.finished_at = now_stamp();
                }
            }
            announce(events, id, "die");
            announce(events, id, "stop");
        }
        if verb == "stop" {
            return empty_reply(204);
        }
    }
    // Démarrer (ou la seconde moitié d'un redémarrage).
    let mut guard = state.lock().unwrap_or_else(PoisonError::into_inner);
    let Some(container) = guard.containers.get(id) else {
        return missing();
    };
    if container.status == "running" {
        return empty_reply(304);
    }
    if let Some((host, _)) = container.publish {
        let taken = guard.containers.values().any(|other| {
            other.id != id && other.status == "running" && other.publish.map(|p| p.0) == Some(host)
        });
        if taken {
            return json_reply(
                500,
                json!({ "message": format!(
                    "driver failed programming external connectivity on endpoint {} ({id}): Bind for 0.0.0.0:{host} failed: port is already allocated",
                    container.name
                ) }),
            );
        }
    }
    if let Some(container) = guard.containers.get_mut(id) {
        container.status = "running".into();
        container.exit_code = 0;
        container.started_at = now_stamp();
        // Comme le moteur : un redémarrage garde la fin de l'exécution arrêtée (le code est remis à zéro).
        if verb != "restart" {
            container.finished_at = "0001-01-01T00:00:00Z".into();
        }
    }
    drop(guard);
    announce(events, id, "start");
    empty_reply(204)
}

/// `/v1.43/containers/json` -> (Some((1, 43)), `/containers/json`).
fn split_version(path: &str) -> (Option<(u32, u32)>, &str) {
    if let Some(rest) = path.strip_prefix("/v")
        && let Some((version, route)) = rest.split_once('/')
        && let Some((major, minor)) = version.split_once('.')
        && let (Ok(major), Ok(minor)) = (major.parse(), minor.parse())
    {
        let route_start = path.len() - route.len() - 1;
        return (Some((major, minor)), &path[route_start..]);
    }
    (None, path)
}

fn route_reply(state: &State, config: &DaemonConfig, route: &str) -> Reply {
    match route {
        "/_ping" => Reply {
            status: config.ping_status,
            content_type: "text/plain; charset=utf-8",
            body: if config.ping_status == 200 {
                "OK".into()
            } else {
                "forbidden".into()
            },
            extra: vec![
                (
                    "Api-Version",
                    format!("{}.{}", config.api_version.0, config.api_version.1),
                ),
                ("Docker-Experimental", "false".into()),
                ("Ostype", "linux".into()),
                (
                    "Server",
                    format!("Docker/{} (linux)", config.engine_version),
                ),
            ],
        },
        "/version" => json_reply(
            200,
            json!({
                "Platform": { "Name": config.platform_name },
                "Components": [
                    { "Name": config.component, "Version": config.engine_version,
                      "Details": { "ApiVersion": format!("{}.{}", config.api_version.0, config.api_version.1),
                                   "Arch": "amd64", "Os": "linux" } },
                    { "Name": "containerd", "Version": "1.6.24", "Details": {} }
                ],
                "Version": config.engine_version,
                "ApiVersion": format!("{}.{}", config.api_version.0, config.api_version.1),
                "MinAPIVersion": "1.12",
                "GitCommit": "311b9ff", "GoVersion": "go1.20.10", "Os": "linux", "Arch": "amd64",
                "KernelVersion": "6.1.0-13-amd64", "BuildTime": "2023-10-26T09:08:01.000000000+00:00"
            }),
        ),
        "/info" => json_reply(
            200,
            json!({
                "ID": "FAKE", "Containers": state.containers.len(), "NCPU": config.ncpu,
                "MemTotal": config.mem_total, "DockerRootDir": config.root_dir,
                "ServerVersion": config.engine_version, "OSType": "linux", "Architecture": "x86_64"
            }),
        ),
        image if image.starts_with("/images/") && image.ends_with("/json") => {
            let name = &image["/images/".len()..image.len() - "/json".len()];
            if state.images.contains(name) {
                json_reply(
                    200,
                    json!({ "Id": format!("sha256:{}", "0123456789abcdef".repeat(4)), "RepoTags": [name] }),
                )
            } else {
                json_reply(404, json!({ "message": format!("No such image: {name}") }))
            }
        }
        "/containers/json" if config.list_status != 200 => json_reply(
            config.list_status,
            json!({ "message": "access denied by authorization plugin" }),
        ),
        "/containers/json" => json_reply(
            200,
            Value::Array(state.containers.values().map(summary).collect()),
        ),
        other => match other
            .strip_prefix("/containers/")
            .and_then(|rest| rest.strip_suffix("/json"))
        {
            Some(id) if state.vanished.contains(id) => json_reply(
                404,
                json!({ "message": format!("No such container: {id}") }),
            ),
            Some(id) if state.broken_inspection.contains(id) => Reply {
                status: 200,
                content_type: "application/json",
                body: "{ ceci n'est pas du JSON".to_owned(),
                extra: vec![],
            },
            Some(id) => match state.containers.get(id) {
                Some(container) => json_reply(200, inspect(container)),
                None => json_reply(
                    404,
                    json!({ "message": format!("No such container: {id}") }),
                ),
            },
            None => json_reply(404, json!({ "message": "page not found" })),
        },
    }
}

fn summary(c: &DaemonContainer) -> Value {
    json!({
        "Id": c.id, "Names": [format!("/{}", c.name)], "Image": c.image,
        "ImageID": format!("sha256:{}", "0123456789abcdef".repeat(4)),
        "Command": "sleep 300", "Created": 1_791_882_000,
        "Ports": c.publish.map(|(host, container)| vec![json!({
            "IP": "0.0.0.0", "PrivatePort": container, "PublicPort": host, "Type": "tcp" })]).unwrap_or_default(),
        "Labels": c.labels, "State": c.status, "Status": c.status,
        "HostConfig": { "NetworkMode": "default" },
        "Mounts": []
    })
}

fn inspect(c: &DaemonContainer) -> Value {
    let running = c.status == "running";
    let bindings = c
        .publish
        .map(|(host, container)| {
            json!({ format!("{container}/tcp"): [{ "HostIp": "", "HostPort": host.to_string() }] })
        })
        .unwrap_or_else(|| json!({}));
    let live = c
        .publish
        .filter(|_| running)
        .map(|(host, container)| {
            json!({ format!("{container}/tcp"): [
                { "HostIp": "0.0.0.0", "HostPort": host.to_string() },
                { "HostIp": "::", "HostPort": host.to_string() }
            ] })
        })
        .unwrap_or_else(|| json!({}));
    json!({
        "Id": c.id,
        "Created": "2026-10-13T09:00:00.987654321Z",
        "Path": c.cmd.first(), "Args": c.cmd.iter().skip(1).collect::<Vec<_>>(),
        "State": {
            "Status": c.status, "Running": running, "Paused": c.status == "paused",
            "Restarting": c.status == "restarting", "OOMKilled": false,
            "Dead": c.status == "dead", "Pid": if running { 4242 } else { 0 },
            "ExitCode": c.exit_code, "Error": "",
            "StartedAt": c.started_at, "FinishedAt": c.finished_at
        },
        "Image": format!("sha256:{}", "0123456789abcdef".repeat(4)),
        "Name": format!("/{}", c.name),
        "RestartCount": 0, "Driver": "overlay2", "Platform": "linux",
        "HostConfig": {
            "NetworkMode": "default",
            "RestartPolicy": { "Name": c.restart, "MaximumRetryCount": 0 },
            "AutoRemove": false,
            "Memory": c.memory, "NanoCpus": c.nano_cpus,
            "PortBindings": bindings,
            "LogConfig": { "Type": c.log_driver, "Config": {} }
        },
        "Mounts": c.mounts.iter().map(|(source, target, read_only)| json!({
            "Type": "bind", "Source": source, "Destination": target,
            "Mode": if *read_only { "ro" } else { "rw" }, "RW": !read_only, "Propagation": "rprivate"
        })).collect::<Vec<_>>(),
        "Config": {
            "Hostname": &c.id[..12], "Image": c.image, "Env": c.env,
            "Cmd": c.cmd, "Labels": c.labels, "Tty": c.tty
        },
        "NetworkSettings": { "Ports": live }
    })
}

async fn write_reply(stream: &mut TcpStream, reply: Reply) {
    let mut head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n",
        reply.status,
        reason(reply.status),
        reply.content_type,
        reply.body.len()
    );
    for (name, value) in &reply.extra {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes()).await;
    let _ = stream.write_all(reply.body.as_bytes()).await;
    let _ = stream.shutdown().await;
}

/// `GET /events` : des lignes JSON en transfert par morceaux, tant que le client reste là.
async fn stream_events(mut stream: TcpStream, events: broadcast::Sender<String>) {
    let mut receiver = events.subscribe();
    let head = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n";
    if stream.write_all(head.as_bytes()).await.is_err() {
        return;
    }
    loop {
        match receiver.recv().await {
            Ok(line) => {
                let chunk = format!("{:x}\r\n{}\r\n", line.len(), line);
                if stream.write_all(chunk.as_bytes()).await.is_err() {
                    return;
                }
            }
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => return,
        }
    }
}

/// `GET /containers/{id}/logs` : les messages du conteneur en transfert par morceaux, en trames de huit
/// octets (conteneur sans terminal) ou bruts (avec terminal), chacun précédé de son horodatage. Avec
/// `follow=1`, la réponse reste ouverte, envoie les nouveaux messages, et se ferme quand le conteneur
/// s'arrête ou disparaît.
async fn stream_logs(mut stream: TcpStream, state: Arc<Mutex<State>>, id: String, query: String) {
    let param = |name: &str| {
        query
            .split('&')
            .find_map(|pair| pair.strip_prefix(&format!("{name}=")))
            .map(str::to_owned)
    };
    let follow = param("follow").is_some_and(|value| value == "1" || value == "true");
    let since_s: Option<i64> = param("since").and_then(|value| value.parse().ok());
    let tail: Option<usize> = param("tail").and_then(|value| value.parse().ok());
    // 0 : introuvable, 1 : journaux non relisibles, 2 : lisible. Le verrou ne traverse aucun `await`.
    let (probe, tty) = {
        let guard = state.lock().unwrap_or_else(PoisonError::into_inner);
        match guard.containers.get(&id) {
            None => (0, false),
            Some(container) if container.log_driver == "none" => (1, false),
            Some(container) => (2, container.tty),
        }
    };
    if probe == 0 {
        write_reply(
            &mut stream,
            json_reply(
                404,
                json!({ "message": format!("No such container: {id}") }),
            ),
        )
        .await;
        return;
    }
    if probe == 1 {
        write_reply(
            &mut stream,
            json_reply(
                501,
                json!({ "message": "configured logging driver does not support reading" }),
            ),
        )
        .await;
        return;
    }
    let mut sent = 0usize;
    let kind = if tty {
        "application/vnd.docker.raw-stream"
    } else {
        "application/vnd.docker.multiplexed-stream"
    };
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n"
    );
    if stream.write_all(head.as_bytes()).await.is_err() {
        return;
    }
    let mut first = true;
    loop {
        let (batch, alive, total) = {
            let guard = state.lock().unwrap_or_else(PoisonError::into_inner);
            let Some(container) = guard.containers.get(&id) else {
                break;
            };
            let all = &container.logs;
            let alive = container.status == "running";
            let mut batch: Vec<DaemonLog> = if first {
                all.iter()
                    .filter(|log| since_s.is_none_or(|since| log.at_ms >= since * 1000))
                    .cloned()
                    .collect()
            } else {
                all.iter().skip(sent).cloned().collect()
            };
            if first && let Some(tail) = tail {
                let skip = batch.len().saturating_sub(tail);
                batch.drain(..skip);
            }
            (batch, alive, all.len())
        };
        first = false;
        sent = total;
        let mut written = 0usize;
        for log in batch {
            let mut payload = stamp(log.at_ms).into_bytes();
            payload.push(b' ');
            payload.extend_from_slice(&log.text);
            let mut message = Vec::new();
            if !tty {
                message.extend_from_slice(&[log.stream, 0, 0, 0]);
                message.extend_from_slice(&u32::try_from(payload.len()).unwrap().to_be_bytes());
            }
            message.extend_from_slice(&payload);
            let chunk = [
                format!("{:x}\r\n", message.len()).into_bytes(),
                message,
                b"\r\n".to_vec(),
            ]
            .concat();
            if stream.write_all(&chunk).await.is_err() {
                return;
            }
            written += 1;
            if written.is_multiple_of(256) {
                *state
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .served_logs
                    .entry(id.clone())
                    .or_default() += 256;
                written = 0;
            }
        }
        if written > 0 {
            *state
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .served_logs
                .entry(id.clone())
                .or_default() += written;
        }
        if !follow || !alive {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(15)).await;
    }
    let _ = stream.write_all(b"0\r\n\r\n").await;
    let _ = stream.shutdown().await;
}

/// `POST /containers/create?name=…` : comme le moteur. Une image absente répond `404 No such image`, un nom pris
/// `409 Conflict`. Le corps lu (étiquettes, variables, commande, limites, ports, montages, règle de
/// redémarrage) est ce que l'inspection rendra ensuite.
fn create_container(
    state: &Arc<Mutex<State>>,
    events: &broadcast::Sender<String>,
    query: &str,
    body: &Value,
) -> Reply {
    let name = query
        .split('&')
        .find_map(|pair| pair.strip_prefix("name="))
        .unwrap_or_default()
        .to_owned();
    let image = body["Image"].as_str().unwrap_or_default().to_owned();
    let mut guard = state.lock().unwrap_or_else(PoisonError::into_inner);
    if !guard.images.contains(&image) {
        return json_reply(404, json!({ "message": format!("No such image: {image}") }));
    }
    if let Some(existing) = guard.containers.values().find(|c| c.name == name) {
        return json_reply(
            409,
            json!({ "message": format!(
                "Conflict. The container name \"/{name}\" is already in use by container \"{}\". You have to remove (or rename) that container to be able to reuse that name.",
                existing.id
            ) }),
        );
    }
    let mut container = DaemonContainer::new(&name, "created");
    container.image = image;
    container.cmd = body["Cmd"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    container.env = body["Env"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    container.labels = body["Labels"]
        .as_object()
        .map(|labels| {
            labels
                .iter()
                .filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_owned())))
                .collect()
        })
        .unwrap_or_default();
    let host = &body["HostConfig"];
    container.restart = host["RestartPolicy"]["Name"]
        .as_str()
        .unwrap_or("no")
        .to_owned();
    container.memory = host["Memory"].as_i64().unwrap_or(0);
    container.nano_cpus = host["NanoCpus"].as_i64().unwrap_or(0);
    container.publish = host["PortBindings"].as_object().and_then(|ports| {
        ports.iter().find_map(|(key, bindings)| {
            let container_port: u16 = key.split('/').next()?.parse().ok()?;
            let host_port: u16 = bindings.get(0)?["HostPort"].as_str()?.parse().ok()?;
            Some((host_port, container_port))
        })
    });
    container.mounts = host["Mounts"]
        .as_array()
        .map(|mounts| {
            mounts
                .iter()
                .filter(|m| m["Type"] == "bind")
                .filter_map(|m| {
                    Some((
                        m["Source"].as_str()?.to_owned(),
                        m["Target"].as_str()?.to_owned(),
                        m["ReadOnly"].as_bool().unwrap_or(false),
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    let id = container.id.clone();
    guard.containers.insert(id.clone(), container);
    guard.created_bodies.push(body.clone());
    drop(guard);
    announce(events, &id, "create");
    json_reply(201, json!({ "Id": id, "Warnings": [] }))
}

/// `POST /images/create?fromImage=…&tag=…` : des lignes de progression au format du moteur, puis l'image est là.
/// Un registre qui ne connaît pas l'image répond `404 pull access denied` (privée ou introuvable), une version
/// absente d'un dépôt connu `404 manifest unknown`. Un client qui coupe la connexion avant la fin est compté.
async fn pull_image(mut stream: TcpStream, state: Arc<Mutex<State>>, query: String) {
    let param = |name: &str| {
        query
            .split('&')
            .find_map(|pair| pair.strip_prefix(&format!("{name}=")))
            .unwrap_or_default()
            .to_owned()
    };
    let repository = param("fromImage");
    let tag = param("tag");
    let reference = format!("{repository}:{tag}");
    let found = {
        let guard = state.lock().unwrap_or_else(PoisonError::into_inner);
        match guard.registry.get(&reference) {
            Some(pull) => Ok(pull.clone()),
            None if guard.known_repositories.contains(&repository) => Err(format!(
                "manifest for {reference} not found: manifest unknown: manifest unknown"
            )),
            None => Err(format!(
                "pull access denied for {repository}, repository does not exist or may require 'docker login': denied: requested access to the resource is denied"
            )),
        }
    };
    let pull = match found {
        Ok(pull) => pull,
        Err(message) => {
            write_reply(&mut stream, json_reply(404, json!({ "message": message }))).await;
            return;
        }
    };
    let head = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n";
    if stream.write_all(head.as_bytes()).await.is_err() {
        return;
    }
    for line in &pull.lines {
        if pull.delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(pull.delay_ms)).await;
        }
        if stream
            .write_all(format!("{line}\n").as_bytes())
            .await
            .is_err()
        {
            state
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .pull_aborts += 1;
            return;
        }
    }
    state
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .images
        .insert(reference);
    let _ = stream.shutdown().await;
}

//! Faux démon de conteneurs : un serveur HTTP/1.1 local qui rejoue des réponses de la forme
//! documentée de l'API du moteur Docker (`/_ping`, `/version`, `/containers/json`,
//! `/containers/{id}/json`, `/events`), pour éprouver l'adaptateur `bollard` sans moteur.
//!
//! IMPORTANT : ces réponses sont FABRIQUÉES d'après la documentation de l'API du moteur et la forme
//! connue des réponses de Docker 24 ; elles ne viennent pas d'un vrai moteur. Leur fidélité n'est
//! prouvée que par la suite de contrat exécutée contre un vrai moteur en CI (`engine_real.rs`).
//!
//! Comme un vrai démon, il refuse une version d'API plus récente que la sienne (`400 client version
//! … is too new`) : c'est ce qui éprouve la négociation de version de l'adaptateur.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
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

    pub fn emit(&self, id: &str, action: &str) {
        let line = json!({
            "Type": "container", "Action": action, "status": action, "id": id,
            "Actor": { "ID": id, "Attributes": { "image": "busybox:1.36", "name": "x" } },
            "scope": "local", "time": 1_791_885_600, "timeNano": 1_791_885_600_000_000_000u64
        });
        // Sans abonné, `send` rend une erreur : personne n'écoute, ce n'est pas un échec.
        let _ = self.events.send(format!("{line}\n"));
    }

    /// Tout ce que les clients ont demandé, `MÉTHODE chemin`.
    pub fn requests(&self) -> Vec<String> {
        self.lock().requests.clone()
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
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
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
    let text = String::from_utf8_lossy(&head).into_owned();
    let request_line = text.lines().next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let (method, target) = (
        parts.next().unwrap_or_default().to_owned(),
        parts.next().unwrap_or_default().to_owned(),
    );
    let (path, _query) = target.split_once('?').unwrap_or((target.as_str(), ""));
    let path = path.to_owned();
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
        "Labels": {}, "State": c.status, "Status": c.status,
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
            "PortBindings": bindings
        },
        "Mounts": [],
        "Config": {
            "Hostname": &c.id[..12], "Image": c.image, "Env": c.env,
            "Cmd": c.cmd, "Labels": {}
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

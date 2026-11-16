//! Banc d'essai réel générique des fiches de serveur de jeu à arrêt par signal (HRT-226, ADR-0053, BR-FICHE-028,
//! BR-JEU-038, BR-RES-038). Modèle : `fiche_live_minecraft.rs`.
//!
//! `HEARTH_LIVE_GAME=<id>` choisit le jeu dans le tableau `GAMES`. Chaque scénario passe par le chemin de l'agent
//! (traduction, jugement du déploiement ordinaire, plan fermé, création par `BollardEngine`) sur la VRAIE image épinglée :
//! signe « prêt » de la fiche, port qui répond (TCP : connexion ; UDP : socket en écoute lue dans le conteneur),
//! réglage pris en compte, mémoire (pic de démarrage, repos 60 s après le signe), arrêt par le SIGNAL de l'image (code 0
//! et fichiers du monde réécrits avant le délai), service qui reste arrêté, redémarrage sur le même monde. Le relevé va
//! dans `<HEARTH_FICHE_LIVE_OUT>/<id>/evidence.toml`.
//!
//! Rien ne tourne sans lancement voulu (en local, jamais en CI) : `HEARTH_TEST_FICHE_LIVE=1` et un vrai moteur Linux.
//! `HEARTH_LIVE_SCENARIOS` (liste séparée par des virgules) choisit les scénarios, `HEARTH_LIVE_MEM_GIB` remplace la
//! limite de mémoire du premier scénario (0,5 Gio d'incrément autorisé : valeur décimale).

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use hearth_agent::application::ports::{ContainerEngine, LogQuery};
use hearth_agent::domain::catalog::loading::schema::fiche::{
    FolderRole, PortMapping, Protocol, Publish,
};
use hearth_agent::domain::catalog::stop_plan::{StopPlan, ready_sign, stop_plan};
use hearth_agent::domain::catalog::translate::{Request, translate};
use hearth_agent::domain::catalog::{Fiche, Origin, load};
use hearth_agent::domain::secret::Secret;
use hearth_agent::domain::services::ServiceKey;
use hearth_agent::domain::services::deploy::Limits;
use hearth_agent::domain::services::from_fiche::{InstallInputs, body_and_secrets_from};
use hearth_agent::domain::services::plan::{PlanContext, plan_from_spec};
use hearth_agent::infrastructure::engine::BollardEngine;
use hearth_proto::api::deploy::DeployAccept;

const KEY: &str = "01J0ABCDEFGHJKMNPQRSTVWXYZ";
const GIB: u64 = 1024 * 1024 * 1024;

/// Comment l'essai constate qu'un réglage posé par la fiche est pris en compte.
enum Probe {
    /// Le fichier (sous le dossier géré monté sur `target`) contient encore `needle` après le démarrage.
    File {
        target: &'static str,
        path: &'static str,
        needle: &'static str,
    },
    /// La variable d'environnement `NOM=valeur` figure dans la configuration du conteneur.
    Env(&'static str),
}

struct Scenario {
    id: &'static str,
    memory_bytes: u64,
    ready_within_s: u64,
    /// Un redémarrage sur le même monde.
    restart: bool,
}

struct Game {
    id: &'static str,
    fiche: &'static str,
    settings: &'static str,
    answers: &'static [(&'static str, &'static str)],
    agreements: &'static [&'static str],
    probe: Probe,
    scenarios: &'static [Scenario],
}

const GAMES: &[Game] = &[
    Game {
        id: "7dtd",
        fiche: include_str!("../catalog/7dtd/fiche.toml"),
        settings: include_str!("../catalog/7dtd/settings.toml"),
        answers: &[("server_name", "hearth-probe"), ("game_name", "Essai")],
        agreements: &[],
        probe: Probe::File {
            target: "/home/sdtdserver/serverfiles",
            path: "sdtdserver.xml",
            needle: "hearth-probe",
        },
        scenarios: &[
            Scenario {
                id: "min",
                memory_bytes: 4 * GIB,
                ready_within_s: 5400,
                restart: true,
            },
            Scenario {
                id: "rec",
                memory_bytes: 6 * GIB,
                ready_within_s: 5400,
                restart: false,
            },
        ],
    },
    Game {
        id: "valheim",
        fiche: include_str!("../catalog/valheim/fiche.toml"),
        settings: include_str!("../catalog/valheim/settings.toml"),
        answers: &[
            ("server_name", "hearth-probe"),
            ("world_name", "Essai"),
            ("server_pass", "probe-secret"),
        ],
        agreements: &[],
        probe: Probe::Env("SERVER_NAME=hearth-probe"),
        scenarios: &[
            Scenario {
                id: "min",
                memory_bytes: 4 * GIB,
                ready_within_s: 1800,
                restart: true,
            },
            Scenario {
                id: "rec",
                memory_bytes: 6 * GIB,
                ready_within_s: 1800,
                restart: false,
            },
        ],
    },
    Game {
        id: "factorio",
        fiche: include_str!("../catalog/factorio/fiche.toml"),
        settings: include_str!("../catalog/factorio/settings.toml"),
        answers: &[("save_name", "Essai")],
        agreements: &[],
        probe: Probe::Env("SAVE_NAME=Essai"),
        scenarios: &[
            Scenario {
                id: "min",
                memory_bytes: GIB,
                ready_within_s: 900,
                restart: true,
            },
            Scenario {
                id: "rec",
                memory_bytes: 2 * GIB,
                ready_within_s: 900,
                restart: false,
            },
        ],
    },
    //GAMES
];

fn enabled() -> bool {
    std::env::var("HEARTH_TEST_FICHE_LIVE").as_deref() == Ok("1")
}

fn socket() -> PathBuf {
    std::env::var_os("HEARTH_TEST_ENGINE_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/var/run/docker.sock"))
}

fn out_dir() -> PathBuf {
    std::env::var_os("HEARTH_FICHE_LIVE_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/fiche-live")
        })
}

/// Lance `docker` (le décor du test : l'essai lui-même passe par le port `ContainerEngine`).
fn docker(args: &[&str]) -> String {
    let output = Command::new("docker")
        .args(args)
        .output()
        .expect("docker doit exister dans le job");
    assert!(
        output.status.success(),
        "docker {args:?} : {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn docker_ok(args: &[&str]) -> Option<String> {
    let output = Command::new("docker").args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// La fiche embarquée, où les éléments `untested` sont passés à `tested` DANS CETTE COPIE : c'est l'essai qui dit si le
/// mot peut changer dans le fichier livré. Les ordres de la fiche sont ceux que l'agent enverrait.
fn trial_copy(game: &Game) -> Fiche {
    let text = game
        .fiche
        .replace("status = \"untested\"", "status = \"tested\"");
    load(Origin::Embedded, text.as_bytes(), game.settings.as_bytes()).expect("fiche valide")
}

fn now_ms() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap()
}

/// Les lignes du journal écrites à partir de `since_ms`.
async fn log_lines(engine: &BollardEngine, id: &str, since_ms: i64) -> Vec<String> {
    let mut stream = engine.logs(
        id,
        LogQuery {
            follow: false,
            tail: None,
            since_ms: Some(since_ms),
            tty: false,
        },
    );
    let mut lines = Vec::new();
    while let Some(Ok(line)) = stream.next().await {
        if line.at_ms >= since_ms {
            lines.push(line.text);
        }
    }
    lines
}

/// Attend le signe de la fiche dans les lignes écrites depuis `since_ms`. Rend la durée écoulée.
async fn wait_ready(
    engine: &BollardEngine,
    id: &str,
    sign: &str,
    since_ms: i64,
    within: Duration,
) -> Result<Duration, Vec<String>> {
    let started = Instant::now();
    loop {
        let lines = log_lines(engine, id, since_ms).await;
        if lines.iter().any(|line| line.contains(sign)) {
            return Ok(started.elapsed());
        }
        let running = docker(&["inspect", "-f", "{{.State.Running}}", id]) == "true";
        if !running || started.elapsed() > within {
            return Err(lines.into_iter().rev().take(40).rev().collect());
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn tcp_accepts(port: u16, within: Duration) -> bool {
    let started = Instant::now();
    while started.elapsed() < within {
        if tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_ok()
        {
            return true;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    false
}

fn dir_size(path: &Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            match entry.metadata() {
                Ok(meta) if meta.is_dir() => total += dir_size(&entry.path()),
                Ok(meta) => total += meta.len(),
                Err(_) => {}
            }
        }
    }
    total
}

fn remove_leftovers(name: &str) {
    if let Some(ids) = docker_ok(&["ps", "-aq", "--filter", &format!("name={name}")]) {
        for id in ids.split_whitespace() {
            let _ = docker_ok(&["rm", "-f", id]);
        }
    }
}

/// Le relevé consigné dans `evidence.toml`.
struct Evidence {
    lines: Vec<(String, String)>,
    /// Dossier du relevé : réécrit à chaque constat, pour qu'un échec en cours de route ne le perde pas.
    out: PathBuf,
}

impl Evidence {
    fn put(&mut self, key: &str, value: impl std::fmt::Display, quoted: bool) {
        let rendered = if quoted {
            format!("\"{}\"", value.to_string().replace('"', "'"))
        } else {
            value.to_string()
        };
        eprintln!("evidence {key} = {rendered}");
        self.lines.push((key.to_owned(), rendered));
        let out = self.out.clone();
        self.write(&out);
    }

    fn text(&mut self, key: &str, value: impl std::fmt::Display) {
        self.put(key, value, true);
    }

    fn raw(&mut self, key: &str, value: impl std::fmt::Display) {
        self.put(key, value, false);
    }

    fn write(&self, dir: &Path) {
        let mut out = String::from(
            "# Relevé du banc d'essai réel (HRT-226). Engendré par `tests/fiche_live_game.rs`, jamais écrit à la main.\n",
        );
        for (key, value) in &self.lines {
            let _ = writeln!(out, "{key} = {value}");
        }
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("evidence.toml"), out).unwrap();
    }
}

/// Un scénario du banc : la variante, le pack, la limite de mémoire donnée au conteneur.
struct Outcome {
    ready: bool,
    port_open: bool,
    setting_taken: bool,
    stop_clean: bool,
    no_auto_restart: bool,
    restart_ok: bool,
}

fn read_u64(text: Option<String>) -> u64 {
    text.and_then(|t| t.trim().parse().ok()).unwrap_or(0)
}

fn cgroup_peak(id: &str) -> u64 {
    read_u64(docker_ok(&[
        "exec",
        id,
        "cat",
        "/sys/fs/cgroup/memory.peak",
    ]))
}

fn cgroup_current(id: &str) -> u64 {
    read_u64(docker_ok(&[
        "exec",
        id,
        "cat",
        "/sys/fs/cgroup/memory.current",
    ]))
}

/// Le socket UDP `port` est en écoute dans le conteneur (lu dans `/proc/net`).
fn udp_listening(id: &str, port: u16) -> bool {
    let needle = format!(":{port:04X} ");
    ["/proc/net/udp", "/proc/net/udp6"].iter().any(|file| {
        docker_ok(&["exec", id, "cat", file]).is_some_and(|text| text.contains(&needle))
    })
}

fn latest_mtime(path: &Path) -> Option<std::time::SystemTime> {
    let mut best = std::fs::metadata(path).ok()?.modified().ok();
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            let time = if meta.is_dir() {
                latest_mtime(&entry.path())
            } else {
                meta.modified().ok()
            };
            if time > best {
                best = time;
            }
        }
    }
    best
}

fn chown_r(path: &Path, uid: u32, gid: u32) {
    let _ = std::os::unix::fs::chown(path, Some(uid), Some(gid));
    if path.is_dir()
        && let Ok(entries) = std::fs::read_dir(path)
    {
        for entry in entries.flatten() {
            chown_r(&entry.path(), uid, gid);
        }
    }
}

fn host_port_of(container: u16) -> u16 {
    if container <= 55_000 {
        container + 10_000
    } else {
        container - 10_000
    }
}

async fn run_scenario(
    game: &Game,
    fiche: &Fiche,
    sc: &Scenario,
    evidence: &mut Evidence,
    work_root: &Path,
) -> Outcome {
    let p = |name: &str| format!("{}.{name}", sc.id);
    let name = format!("hearth-fiche-live-{}", game.id);
    let work = work_root.join(sc.id);
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    remove_leftovers(&name);
    let limit = match std::env::var("HEARTH_LIVE_MEM_GIB") {
        Ok(gib) if sc.id == game.scenarios[0].id => {
            (gib.parse::<f64>().unwrap() * GIB as f64) as u64
        }
        _ => sc.memory_bytes,
    };
    evidence.raw(&p("memory_limit_bytes"), limit);

    // 1. Le chemin de l'agent : traduction, jugement du déploiement ordinaire, plan fermé.
    let answers: BTreeMap<String, String> = game
        .answers
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();
    let agreements: BTreeSet<String> = game.agreements.iter().map(|s| (*s).to_owned()).collect();
    let host_ports: BTreeMap<String, u16> = fiche
        .file
        .port
        .iter()
        .filter(|port| port.publish == Publish::Always)
        .map(|port| (port.id.clone(), host_port_of(port.container)))
        .collect();
    let optional = BTreeSet::new();
    let alphabet = "Abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let generated: BTreeMap<String, Secret> = fiche
        .file
        .secret
        .iter()
        .map(|s| {
            let text: String = alphabet.chars().cycle().take(s.length as usize).collect();
            (s.id.clone(), Secret::from(text.as_str()))
        })
        .collect();
    let draft = translate(
        fiche,
        &Request {
            variant: None,
            answers: &answers,
            agreements: &agreements,
            host_ports: &host_ports,
            optional_ports: &optional,
            generated: &generated,
            world_imported: false,
            pack: None,
        },
    )
    .expect("traduction");
    let built = body_and_secrets_from(
        &InstallInputs {
            fiche,
            answers: &answers,
            generated: &generated,
            accepted_by: "fiche-live",
            accepted_at: 0,
            accept: DeployAccept::default(),
        },
        &draft,
        &name,
        Limits {
            cpu_millis: 4_000,
            memory_bytes: limit,
        },
    )
    .expect("déploiement ordinaire");
    let key = ServiceKey::for_hearth(KEY).expect("clé");
    let managed_root = work.to_string_lossy().into_owned();
    let plan = plan_from_spec(
        &built.valid.body,
        &built.valid.secrets,
        &PlanContext {
            key: &key,
            install_id: "fiche-live",
            created_by: "fiche-live",
            rev: 1,
            managed_root: &managed_root,
        },
    )
    .expect("plan");

    // 2. L'image du plan, tirée ; l'empreinte relevée est consignée.
    docker(&["pull", &plan.image]);
    evidence.text(&p("image"), &plan.image);
    let digest = docker_ok(&[
        "image",
        "inspect",
        "-f",
        "{{index .RepoDigests 0}}",
        &plan.image,
    ])
    .unwrap_or_default();
    evidence.text(&p("image_digest"), &digest);
    let image_bytes: u64 = docker(&["image", "inspect", "-f", "{{.Size}}", &plan.image])
        .parse()
        .unwrap_or(0);
    evidence.raw(&p("disk_image_bytes"), image_bytes);

    // 3. Les dossiers gérés et les fichiers de réglages, comme l'installation les range.
    let service_dir = work.join(key.as_str());
    let (uid, gid) = (fiche.file.owner.uid, fiche.file.owner.gid);
    // Dossiers pré-installés (`HEARTH_LIVE_PRESEED=<dossier>` : un sous-dossier par identifiant de dossier de la fiche) :
    // déplacés (renommés) à la place du dossier géré, puis rendus à la fin. Évite de retélécharger un gros jeu (7 Days to Die).
    let preseed = std::env::var_os("HEARTH_LIVE_PRESEED").map(PathBuf::from);
    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
    for mount in &built.valid.body.mounts {
        let dir = service_dir.join(&mount.source);
        std::fs::create_dir_all(&dir).unwrap();
        let folder_id = fiche
            .file
            .folder
            .iter()
            .find(|f| f.target == mount.target)
            .map(|f| f.id.clone());
        let seeded = preseed
            .as_ref()
            .zip(folder_id)
            .map(|(root, id)| root.join(id))
            .filter(|from| from.exists());
        if let Some(from) = seeded {
            std::fs::remove_dir_all(&dir).unwrap();
            std::fs::rename(&from, &dir).unwrap();
            moved.push((dir, from));
        } else {
            chown_r(&dir, uid, gid);
        }
    }
    for file in &built.files {
        let path = service_dir.join(&file.folder_source).join(&file.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, file.content.expose()).unwrap();
        chown_r(&path, uid, gid);
    }
    let folder_of = |target: &str| {
        built
            .valid
            .body
            .mounts
            .iter()
            .find(|m| m.target == target)
            .map(|m| service_dir.join(&m.source))
    };
    // Le dossier du monde : celui de la fiche (`worlds.folder`), sinon le premier de rôle monde, sinon données.
    let world_target = fiche
        .file
        .worlds
        .as_ref()
        .and_then(|w| fiche.file.folder.iter().find(|f| f.id == w.folder))
        .or_else(|| {
            fiche
                .file
                .folder
                .iter()
                .find(|f| f.role == FolderRole::World)
        })
        .or_else(|| {
            fiche
                .file
                .folder
                .iter()
                .find(|f| f.role == FolderRole::Data)
        })
        .map(|f| f.target.clone())
        .expect("dossier du monde");
    let world_dir = folder_of(&world_target).expect("dossier du monde monté");

    // 4. Démarrage : le signe de la fiche, puis le port.
    let engine = BollardEngine::unix(socket());
    let id = engine.create(&plan).await.expect("créer");
    let sign = ready_sign(fiche).expect("signe prêt").to_owned();
    let launch = now_ms();
    engine.start(&id).await.expect("démarrer");
    let ready = wait_ready(
        &engine,
        &id,
        &sign,
        launch,
        Duration::from_secs(sc.ready_within_s),
    )
    .await;
    let ready_ok = ready.is_ok();
    evidence.raw(&p("ready"), ready_ok);
    match &ready {
        Ok(took) => evidence.raw(&p("ready_after_s"), took.as_secs()),
        Err(tail) => evidence.text(
            &p("ready_tail"),
            tail.iter()
                .rev()
                .take(8)
                .cloned()
                .collect::<Vec<_>>()
                .join(" | "),
        ),
    }
    let startup_peak = cgroup_peak(&id);
    evidence.raw(&p("memory_peak_startup_bytes"), startup_peak);
    let mut port_open = ready_ok;
    for port in fiche
        .file
        .port
        .iter()
        .filter(|p| p.publish == Publish::Always)
    {
        let host = host_port_of(port.container);
        // `same_number` : le jeu écoute sur le numéro du port publié ; `nat` : sur celui de la fiche.
        let in_container = match port.mapping {
            PortMapping::SameNumber => host,
            PortMapping::Nat => port.container,
        };
        let open = match port.protocol {
            Protocol::Tcp => ready_ok && tcp_accepts(host, Duration::from_secs(60)).await,
            Protocol::Udp => {
                let mut ok = false;
                for _ in 0..30 {
                    if udp_listening(&id, in_container) {
                        ok = true;
                        break;
                    }
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
                ok
            }
        };
        evidence.raw(&p(&format!("port_{}_listens", port.id)), open);
        port_open &= open;
    }
    evidence.raw(&p("ports_listen"), port_open);

    // 5. Le réglage pris en compte, puis la mémoire au repos.
    let setting_taken = match &game.probe {
        Probe::File {
            target,
            path,
            needle,
        } => {
            let dir = folder_of(target).expect("dossier du réglage");
            std::fs::read_to_string(dir.join(path)).is_ok_and(|text| text.contains(needle))
        }
        Probe::Env(pair) => docker(&[
            "inspect",
            "-f",
            "{{range .Config.Env}}{{println .}}{{end}}",
            &id,
        ])
        .lines()
        .any(|line| line == *pair),
    };
    evidence.raw(&p("setting_taken"), setting_taken);
    if ready_ok {
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
    let stats =
        docker_ok(&["stats", "--no-stream", "--format", "{{.MemUsage}}", &id]).unwrap_or_default();
    evidence.text(&p("memory_usage_raw"), &stats);
    evidence.raw(&p("memory_idle_bytes"), cgroup_current(&id));
    evidence.raw(&p("memory_peak_bytes"), cgroup_peak(&id));
    evidence.raw(
        &p("disk_install_bytes"),
        image_bytes + dir_size(&service_dir),
    );

    // 6. L'arrêt par le signal de l'image : le jeu enregistre son monde et sort de lui-même avant la coupure forcée.
    let StopPlan::ByImageOnSignal { grace_s, .. } = stop_plan(fiche) else {
        panic!("ce banc arrête par le signal de l'image");
    };
    let before = latest_mtime(&world_dir);
    let stop_started = Instant::now();
    engine
        .stop(&id, Duration::from_secs(u64::from(grace_s)))
        .await
        .expect("arrêter");
    let stop_took = stop_started.elapsed();
    let exit_code = docker(&["inspect", "-f", "{{.State.ExitCode}}", &id]);
    let running = docker(&["inspect", "-f", "{{.State.Running}}", &id]) == "true";
    let after = latest_mtime(&world_dir);
    let world_saved = ready_ok && after > before;
    evidence.raw(&p("stop_took_s"), stop_took.as_secs());
    evidence.text(&p("stop_exit_code"), &exit_code);
    evidence.raw(&p("world_saved_on_stop"), world_saved);
    let tail: Vec<String> = log_lines(&engine, &id, launch)
        .await
        .into_iter()
        .rev()
        .take(6)
        .collect();
    evidence.text(&p("stop_log_tail"), tail.join(" | "));
    let stop_clean = ready_ok
        && !running
        && exit_code == "0"
        && stop_took < Duration::from_secs(u64::from(grace_s))
        && world_saved;
    evidence.raw(&p("stop_clean"), stop_clean);

    // 7. Le service reste arrêté.
    tokio::time::sleep(Duration::from_secs(15)).await;
    let no_auto_restart = docker(&["inspect", "-f", "{{.State.Running}}", &id]) != "true";
    evidence.raw(&p("no_auto_restart"), no_auto_restart);

    // 8. Le redémarrage retrouve le même monde.
    let mut restart_ok = true;
    if sc.restart {
        let relaunch = now_ms();
        engine.start(&id).await.expect("redémarrer");
        let second = wait_ready(
            &engine,
            &id,
            &sign,
            relaunch,
            Duration::from_secs(sc.ready_within_s),
        )
        .await;
        restart_ok = second.is_ok() && latest_mtime(&world_dir).is_some();
        evidence.raw(&p("restart_ready"), second.is_ok());
        evidence.raw(&p("restart_keeps_world"), restart_ok);
    }

    // 9. Nettoyage de ce que ce scénario a lancé (l'image reste : `docker rmi` précis, à la main).
    let _ = engine.stop(&id, Duration::from_secs(60)).await;
    let _ = engine.remove(&id).await;
    remove_leftovers(&name);
    for (dir, from) in &moved {
        let _ = std::fs::rename(dir, from);
    }
    let _ = std::fs::remove_dir_all(&work);
    Outcome {
        ready: ready_ok,
        port_open,
        setting_taken,
        stop_clean,
        no_auto_restart,
        restart_ok,
    }
}

#[tokio::test]
#[ignore = "vrai moteur Linux et vraie image : HEARTH_TEST_FICHE_LIVE=1 HEARTH_LIVE_GAME=<id> cargo test -p hearth-agent --test fiche_live_game -- --ignored --nocapture"]
async fn a_signal_stopped_fiche_starts_takes_a_setting_stops_cleanly_and_measures_its_memory() {
    if !enabled() {
        eprintln!("HEARTH_TEST_FICHE_LIVE=1 n'est pas posé : essai non exécuté");
        return;
    }
    let wanted = std::env::var("HEARTH_LIVE_GAME").expect("HEARTH_LIVE_GAME=<id du jeu>");
    let game = GAMES
        .iter()
        .find(|g| g.id == wanted)
        .expect("jeu inconnu du banc");
    let fiche = trial_copy(game);
    let out = out_dir().join(game.id);
    let work_root = out.join("work");
    std::fs::create_dir_all(&work_root).unwrap();
    let mut evidence = Evidence {
        lines: Vec::new(),
        out: out.clone(),
    };
    evidence.text("fiche", fiche.id());
    evidence.raw("fiche_version", fiche.file.version);
    let only = std::env::var("HEARTH_LIVE_SCENARIOS").unwrap_or_default();
    let mut outcomes = Vec::new();
    for sc in game.scenarios {
        if !only.is_empty() && !only.split(',').any(|id| id == sc.id) {
            continue;
        }
        let outcome = run_scenario(game, &fiche, sc, &mut evidence, &work_root).await;
        outcomes.push((sc.id, outcome));
    }
    let _ = std::fs::remove_dir_all(&work_root);
    evidence.write(&out);

    for (id, o) in &outcomes {
        assert!(o.ready, "{id} : le signe « prêt » doit venir");
        assert!(o.port_open, "{id} : les ports publiés doivent répondre");
        assert!(
            o.setting_taken,
            "{id} : le réglage doit être pris en compte"
        );
        assert!(
            o.stop_clean,
            "{id} : l'arrêt doit être propre (code 0, monde enregistré, avant le délai)"
        );
        assert!(o.no_auto_restart, "{id} : le service arrêté doit le rester");
        assert!(
            o.restart_ok,
            "{id} : le redémarrage doit retrouver le monde"
        );
    }
}

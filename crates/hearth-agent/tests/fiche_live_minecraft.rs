//! Banc d'essai réel de la fiche Minecraft Java (HRT-116, ADR-0053, BR-FICHE-028, BR-JEU-038, BR-RES-038).
//!
//! Trois scénarios, chacun par le chemin de l'agent (traduction, jugement du déploiement ordinaire, plan fermé, création
//! par `BollardEngine`) sur la VRAIE image épinglée : `vanilla_min` (jeu officiel, petite limite), `vanilla_rec` (jeu
//! officiel, limite recommandée) et `pack` (Fabric 1.20.1 + pack serveur Prominence II, grosse limite). Chacun : signe
//! « prêt » de la fiche, port qui répond, réglage pris en compte, arrêt par le SIGNAL de l'image (le monde est enregistré,
//! code 0, avant le délai), service qui reste arrêté, mémoire réelle (pic de démarrage lu dans le cgroupe, mémoire au
//! repos 60 s après le signe). Le relevé est consigné dans `evidence.toml` : c'est lui qui décide du passage de `ready` à
//! `tested` et des chiffres de mémoire `measured`.
//!
//! Rien ne tourne sans lancement voulu (sur la forge, à la main, jamais en CI) : `HEARTH_TEST_FICHE_LIVE=1`, un vrai moteur Linux, et pour le scénario `pack` le fichier du pack
//! serveur (`HEARTH_LIVE_MC_PACK`). `HEARTH_LIVE_MC_SCENARIOS` (liste séparée par des virgules) choisit les scénarios.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use hearth_agent::application::ports::{ContainerEngine, LogQuery};
use hearth_agent::domain::catalog::loading::schema::fiche::{PackGenre, Proof};
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

const FICHE: &str = include_str!("../catalog/minecraft/fiche.toml");
const SETTINGS: &str = include_str!("../catalog/minecraft/settings.toml");
const KEY: &str = "01J0ABCDEFGHJKMNPQRSTVWXYZ";
const NAME: &str = "hearth-fiche-live-minecraft";
/// Port du serveur choisi pour l'essai (le 25565 d'un poste peut être pris).
const HOST_PORT: u16 = 35_565;
const MOTD: &str = "hearth-probe";

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
fn trial_copy() -> Fiche {
    let text = FICHE.replace("status = \"untested\"", "status = \"tested\"");
    load(Origin::Embedded, text.as_bytes(), SETTINGS.as_bytes()).expect("fiche valide")
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

fn remove_leftovers() {
    if let Some(ids) = docker_ok(&["ps", "-aq", "--filter", &format!("name={NAME}")]) {
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
            "# Relevé du banc d'essai réel (HRT-116). Engendré par `tests/fiche_live_minecraft.rs`, jamais écrit à la main.\n",
        );
        for (key, value) in &self.lines {
            let _ = writeln!(out, "{key} = {value}");
        }
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("evidence.toml"), out).unwrap();
    }
}

const GIB: u64 = 1024 * 1024 * 1024;

/// Un scénario du banc : la variante, le pack, la limite de mémoire donnée au conteneur.
struct Scenario {
    id: &'static str,
    variant: Option<&'static str>,
    pack: bool,
    memory_bytes: u64,
    version: Option<&'static str>,
    ready_within_s: u64,
    /// Un redémarrage sur le même monde (coûteux avec un gros pack : seulement pour le jeu officiel).
    restart: bool,
}

const SCENARIOS: [Scenario; 3] = [
    Scenario {
        id: "vanilla_min",
        variant: None,
        pack: false,
        memory_bytes: 2 * GIB,
        version: None,
        ready_within_s: 900,
        restart: true,
    },
    Scenario {
        id: "vanilla_rec",
        variant: None,
        pack: false,
        memory_bytes: 4 * GIB,
        version: None,
        ready_within_s: 900,
        restart: false,
    },
    Scenario {
        id: "pack",
        variant: Some("fabric"),
        pack: true,
        memory_bytes: 10 * GIB,
        version: Some("1.20.1"),
        ready_within_s: 1500,
        restart: false,
    },
];

/// Ce qu'un scénario a constaté.
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

/// Le plus grand usage de mémoire du conteneur depuis sa création (cgroupe v2 : `memory.peak`).
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

fn modified(path: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

async fn run_scenario(
    fiche: &Fiche,
    sc: &Scenario,
    evidence: &mut Evidence,
    work_root: &Path,
) -> Outcome {
    let p = |name: &str| format!("{}.{name}", sc.id);
    let work = work_root.join(sc.id);
    let _ = Command::new("sudo").args(["rm", "-rf"]).arg(&work).status();
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    remove_leftovers();
    evidence.raw(&p("memory_limit_bytes"), sc.memory_bytes);

    // 1. Le chemin de l'agent : traduction, jugement du déploiement ordinaire, plan fermé.
    let mut answers = BTreeMap::new();
    answers.insert("motd".to_owned(), MOTD.to_owned());
    answers.insert("max_players".to_owned(), "3".to_owned());
    if let Some(version) = sc.version {
        answers.insert("version".to_owned(), version.to_owned());
    }
    let agreements = BTreeSet::from(["eula".to_owned()]);
    let host_ports = BTreeMap::from([("game".to_owned(), HOST_PORT)]);
    let optional = BTreeSet::new();
    let generated = BTreeMap::from([(
        "rcon".to_owned(),
        Secret::from("Abcdefghijklmnopqrstuvwxyz012345"),
    )]);
    let draft = translate(
        fiche,
        &Request {
            variant: sc.variant,
            answers: &answers,
            agreements: &agreements,
            host_ports: &host_ports,
            optional_ports: &optional,
            generated: &generated,
            world_imported: false,
            pack: sc.pack.then_some(PackGenre::ServerPack),
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
        NAME,
        Limits {
            cpu_millis: 4_000,
            memory_bytes: sc.memory_bytes,
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
            jvm_options_env: None,
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

    // 3. Les dossiers gérés, les fichiers de réglages et le pack, comme l'installation les range.
    let service_dir = work.join(key.as_str());
    for mount in &built.valid.body.mounts {
        std::fs::create_dir_all(service_dir.join(&mount.source)).unwrap();
    }
    for file in &built.files {
        let path = service_dir.join(&file.folder_source).join(&file.path);
        std::fs::write(&path, file.content.expose()).unwrap();
    }
    if let Some(placement) = &built.pack {
        let from = std::env::var_os("HEARTH_LIVE_MC_PACK")
            .map(PathBuf::from)
            .expect("HEARTH_LIVE_MC_PACK : le pack serveur à essayer");
        let to = service_dir
            .join(&placement.folder_source)
            .join(placement.file_name);
        std::fs::copy(&from, &to).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&to, std::fs::Permissions::from_mode(0o644)).unwrap();
        evidence.raw(&p("pack_bytes"), std::fs::metadata(&to).unwrap().len());
    }
    let data_dir = built
        .valid
        .body
        .mounts
        .iter()
        .find(|mount| mount.target == "/data")
        .map(|mount| service_dir.join(&mount.source))
        .expect("dossier /data");
    let level = data_dir.join("world").join("level.dat");

    // 4. Démarrage : le signe de la fiche (copie essayée), puis le port.
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
                .take(5)
                .cloned()
                .collect::<Vec<_>>()
                .join(" | "),
        ),
    }
    let startup_peak = cgroup_peak(&id);
    evidence.raw(&p("memory_peak_startup_bytes"), startup_peak);
    let port_open = ready_ok && tcp_accepts(HOST_PORT, Duration::from_secs(60)).await;
    evidence.raw(&p("tcp_accepts"), port_open);

    // 5. Le réglage pris en compte (le message d'accueil est dans server.properties) puis la mémoire au repos.
    let properties =
        std::fs::read_to_string(data_dir.join("server.properties")).unwrap_or_default();
    let setting_taken = properties.contains(&format!("motd={MOTD}"));
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

    // 6. L'arrêt par le signal de l'image : le moteur demande l'arrêt avec le délai de la fiche ; le jeu enregistre le
    //    monde et sort de lui-même (code 0) avant la coupure forcée.
    let StopPlan::ByImageOnSignal { grace_s, .. } = stop_plan(fiche) else {
        panic!("la fiche Minecraft arrête par le signal de l'image");
    };
    let before = modified(&level);
    let stop_started = Instant::now();
    engine
        .stop(&id, Duration::from_secs(u64::from(grace_s)))
        .await
        .expect("arrêter");
    let stop_took = stop_started.elapsed();
    let exit_code = docker(&["inspect", "-f", "{{.State.ExitCode}}", &id]);
    let running = docker(&["inspect", "-f", "{{.State.Running}}", &id]) == "true";
    let after = modified(&level);
    let world_saved = ready_ok && after.is_some() && after > before;
    let logs_saved = log_lines(&engine, &id, launch)
        .await
        .iter()
        .any(|line| line.contains("All chunks are saved") || line.contains("Saving chunks"));
    evidence.raw(&p("stop_took_s"), stop_took.as_secs());
    evidence.text(&p("stop_exit_code"), &exit_code);
    evidence.raw(&p("world_saved_on_stop"), world_saved);
    evidence.raw(&p("world_saved_in_logs"), logs_saved);
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

    // 8. Le redémarrage retrouve le même monde (jeu officiel seulement).
    let mut restart_ok = true;
    if sc.restart {
        let relaunch = now_ms();
        engine.start(&id).await.expect("redémarrer");
        let second = wait_ready(&engine, &id, &sign, relaunch, Duration::from_secs(600)).await;
        restart_ok = second.is_ok() && level.exists();
        evidence.raw(&p("restart_ready"), second.is_ok());
        evidence.raw(&p("restart_keeps_world"), restart_ok);
    }

    // 9. Nettoyage de ce que ce scénario a lancé.
    let _ = engine.stop(&id, Duration::from_secs(60)).await;
    let _ = engine.remove(&id).await;
    remove_leftovers();
    let _ = Command::new("sudo").args(["rm", "-rf"]).arg(&work).status();
    let _ = docker_ok(&["image", "rm", &plan.image]);
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
#[ignore = "vrai moteur Linux et vraie image : HEARTH_TEST_FICHE_LIVE=1 HEARTH_LIVE_MC_PACK=<pack serveur> cargo test -p hearth-agent --test fiche_live_minecraft -- --ignored --nocapture"]
async fn the_minecraft_fiche_starts_takes_a_setting_stops_by_the_image_signal_and_measures_its_memory()
 {
    if !enabled() {
        eprintln!("HEARTH_TEST_FICHE_LIVE=1 n'est pas posé : essai non exécuté");
        return;
    }
    let fiche = trial_copy();
    let out = out_dir();
    let work_root = out.join("work");
    std::fs::create_dir_all(&work_root).unwrap();
    let mut evidence = Evidence {
        lines: Vec::new(),
        out: out.clone(),
    };
    evidence.text("fiche", fiche.id());
    evidence.raw("fiche_version", fiche.file.version);
    let only = std::env::var("HEARTH_LIVE_MC_SCENARIOS").unwrap_or_default();
    let mut outcomes = Vec::new();
    for sc in &SCENARIOS {
        if !only.is_empty() && !only.split(',').any(|id| id == sc.id) {
            continue;
        }
        let outcome = run_scenario(&fiche, sc, &mut evidence, &work_root).await;
        outcomes.push((sc.id, outcome));
    }
    let _ = std::fs::remove_dir_all(&work_root);
    evidence.write(&out);

    // Une fois `ready` promu dans la fiche livrée, le signe est EXIGÉ : si l'image ne le dit plus, le job échoue.
    let shipped =
        load(Origin::Embedded, FICHE.as_bytes(), SETTINGS.as_bytes()).expect("fiche livrée");
    for (id, o) in &outcomes {
        if shipped
            .file
            .ready
            .as_ref()
            .is_some_and(|r| r.status == Proof::Tested)
        {
            assert!(o.ready, "{id} : le signe « prêt » doit venir");
        }
        // Ce que l'essai exige toujours de toute fiche.
        assert!(o.port_open, "{id} : le port publié doit répondre");
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

//! Banc d'essai réel de la fiche Terraria (HRT-114, conception technique 12.3, ADR-0053, BR-FICHE-028, BR-JEU-038).
//!
//! La fiche embarquée est installée par le chemin de l'agent (traduction `translate`, jugement du déploiement
//! ordinaire `body_and_secrets_from`, plan fermé `plan_from_spec`, création par `BollardEngine`) sur la VRAIE image
//! `ryshe/terraria`, avec le vrai moteur du runner. Le test déroule : démarrage (signe « prêt » de la fiche), port
//! qui répond, réglage pris en compte (le monde porte le nom posé, le fichier n'est pas réécrit), suite d'arrêt de la
//! fiche (`save`, attente, `exit` par l'ENTRÉE du conteneur, sans terminal, puis aussitôt l'arrêt du moteur), arrêt
//! sans coupure forcée, service qui reste arrêté, monde conservé, redémarrage sur le même monde, mesures au repos.
//!
//! Le résultat est consigné dans `evidence.toml` (dossier `HEARTH_FICHE_LIVE_OUT`, défaut `target/fiche-live`) : c'est
//! lui qui décide du passage des éléments `untested` de la fiche à `tested`. Tant qu'un élément est `untested` dans la
//! fiche livrée, son essai est CONSIGNÉ, pas jugé ; une fois promu à `tested`, le banc l'EXIGE et le job échoue s'il ne
//! tient plus.
//!
//! Hors CI, rien ne tourne : `HEARTH_TEST_FICHE_LIVE=1` et un vrai moteur sont exigés (aucun serveur de jeu sur un
//! poste de développement). Job : `.github/workflows/fiche-live.yml`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use hearth_agent::application::ports::{ContainerEngine, LogQuery};
use hearth_agent::domain::catalog::loading::schema::fiche::Proof;
use hearth_agent::domain::catalog::stop_plan::{PlannedStep, StopPlan, ready_sign, stop_plan};
use hearth_agent::domain::catalog::translate::{Request, translate};
use hearth_agent::domain::catalog::{Fiche, Origin, load};
use hearth_agent::domain::services::ServiceKey;
use hearth_agent::domain::services::deploy::Limits;
use hearth_agent::domain::services::from_fiche::{InstallInputs, body_and_secrets_from};
use hearth_agent::domain::services::plan::{PlanContext, plan_from_spec};
use hearth_agent::infrastructure::engine::BollardEngine;
use hearth_proto::api::deploy::DeployAccept;

const FICHE: &str = include_str!("../catalog/terraria/fiche.toml");
const SETTINGS: &str = include_str!("../catalog/terraria/settings.toml");
const KEY: &str = "01J0ABCDEFGHJKMNPQRSTVWXYZ";
const NAME: &str = "hearth-fiche-live-terraria";
/// Port du serveur choisi pour l'essai (le 7777 d'un poste peut être pris).
const HOST_PORT: u16 = 17_777;
const WORLD: &str = "Essai";
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

fn worlds(dir: &Path) -> Vec<(String, u64, std::time::SystemTime)> {
    let mut found = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.ends_with(".wld")
                && let Ok(meta) = entry.metadata()
            {
                found.push((name, meta.len(), meta.modified().unwrap()));
            }
        }
    }
    found.sort();
    found
}

/// « 123.4MiB / 2GiB » vers des octets (la première valeur).
fn memory_bytes(stats: &str) -> Option<u64> {
    let first = stats.split('/').next()?.trim();
    let split = first.find(|c: char| c.is_alphabetic())?;
    let (number, unit) = first.split_at(split);
    let number: f64 = number.trim().parse().ok()?;
    let factor = match unit.trim() {
        "B" => 1.0,
        "KiB" => 1024.0,
        "kB" => 1000.0,
        "MiB" => 1024.0 * 1024.0,
        "MB" => 1_000_000.0,
        "GiB" => 1024.0 * 1024.0 * 1024.0,
        "GB" => 1_000_000_000.0,
        _ => return None,
    };
    Some((number * factor) as u64)
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
            "# Relevé du banc d'essai réel (HRT-114). Engendré par `tests/fiche_live.rs`, jamais écrit à la main.\n",
        );
        for (key, value) in &self.lines {
            let _ = writeln!(out, "{key} = {value}");
        }
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("evidence.toml"), out).unwrap();
    }
}

#[tokio::test]
#[ignore = "vrai moteur et vraie image : HEARTH_TEST_FICHE_LIVE=1 cargo test -p hearth-agent --test fiche_live -- --ignored --nocapture"]
async fn the_terraria_fiche_starts_takes_a_setting_stops_by_its_orders_and_keeps_its_world() {
    if !enabled() {
        eprintln!("HEARTH_TEST_FICHE_LIVE=1 n'est pas posé : essai non exécuté");
        return;
    }
    let fiche = trial_copy();
    let out = out_dir();
    let work = out.join("work");
    let _ = Command::new("sudo").args(["rm", "-rf"]).arg(&work).status();
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    remove_leftovers();
    let mut evidence = Evidence {
        lines: Vec::new(),
        out: out.clone(),
    };
    evidence.text("fiche", fiche.id());
    evidence.raw("fiche_version", fiche.file.version);

    // 1. L'image de la fiche, par son étiquette ; l'empreinte relevée est consignée (golden, publication).
    let reference = format!("{}:{}", fiche.file.image.repository, fiche.file.image.tag);
    docker(&["pull", &reference]);
    let digest = docker_ok(&[
        "image",
        "inspect",
        "-f",
        "{{index .RepoDigests 0}}",
        &reference,
    ])
    .unwrap_or_default();
    evidence.text("image", &reference);
    evidence.text("image_digest", &digest);
    // L'étiquette doit encore désigner l'empreinte épinglée par la fiche : sinon le constat est clair, pas un
    // « No such image » à la création.
    if let Some(pinned) = fiche.file.image.digest.as_deref() {
        assert!(
            digest.ends_with(pinned),
            "l'étiquette ne pointe plus sur l'empreinte épinglée : {digest} contre {pinned}"
        );
    }
    let image_bytes: u64 = docker(&["image", "inspect", "-f", "{{.Size}}", &reference])
        .parse()
        .unwrap_or(0);
    evidence.raw("disk_image_bytes", image_bytes);

    // 2. Le chemin de l'agent : traduction, jugement du déploiement ordinaire, plan fermé.
    let mut answers = BTreeMap::new();
    answers.insert("world_name".to_owned(), WORLD.to_owned());
    // Petit monde : la génération d'un grand monde n'apporte rien à l'essai.
    answers.insert("world_size".to_owned(), "1".to_owned());
    answers.insert("max_players".to_owned(), "3".to_owned());
    answers.insert("motd".to_owned(), MOTD.to_owned());
    let agreements = BTreeSet::new();
    let host_ports = BTreeMap::from([("game".to_owned(), HOST_PORT)]);
    let optional = BTreeSet::new();
    let generated = BTreeMap::new();
    let draft = translate(
        &fiche,
        &Request {
            variant: None,
            answers: &answers,
            agreements: &agreements,
            host_ports: &host_ports,
            optional_ports: &optional,
            generated: &generated,
            world_imported: false,
        },
    )
    .expect("traduction");
    let built = body_and_secrets_from(
        &InstallInputs {
            fiche: &fiche,
            answers: &answers,
            generated: &generated,
            accepted_by: "fiche-live",
            accepted_at: 0,
            accept: DeployAccept::default(),
        },
        &draft,
        NAME,
        Limits {
            cpu_millis: 2_000,
            memory_bytes: 2 * 1024 * 1024 * 1024,
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
    assert!(
        plan.stdin_open,
        "la fiche déclare des ordres : entrée ouverte"
    );

    // Les dossiers gérés et le fichier de réglages, comme l'installation les écrit (SafeFs en production).
    let service_dir = work.join(key.as_str());
    for mount in &built.valid.body.mounts {
        std::fs::create_dir_all(service_dir.join(&mount.source)).unwrap();
    }
    for file in &built.files {
        let path = service_dir.join(&file.folder_source).join(&file.path);
        std::fs::write(&path, file.content.expose()).unwrap();
    }
    let config_dir = built
        .files
        .first()
        .map(|file| service_dir.join(&file.folder_source))
        .expect("un fichier de réglages");
    let config_path = config_dir.join("serverconfig.txt");
    let config_written = std::fs::read_to_string(&config_path).unwrap();
    assert!(config_written.contains(&format!("worldname={WORLD}")));
    assert!(config_written.contains(&format!("motd={MOTD}")));
    let world_dir = built
        .valid
        .body
        .mounts
        .iter()
        .find(|mount| mount.target.ends_with("/Worlds"))
        .map(|mount| service_dir.join(&mount.source))
        .expect("dossier des mondes");

    // 3. Démarrage : le signe de la fiche, puis le port.
    let engine = BollardEngine::unix(socket());
    let id = engine.create(&plan).await.expect("créer");
    let sign = ready_sign(&fiche).expect("signe prêt").to_owned();
    let launch = now_ms();
    engine.start(&id).await.expect("démarrer");
    let ready = match wait_ready(&engine, &id, &sign, launch, Duration::from_secs(900)).await {
        Ok(took) => took,
        Err(tail) => {
            evidence.raw("ready", false);
            evidence.write(&out);
            let _ = docker_ok(&["rm", "-f", &id]);
            panic!("le signe « {sign} » n'est pas venu ; fin du journal : {tail:#?}");
        }
    };
    evidence.raw("ready", true);
    evidence.raw("ready_after_s", ready.as_secs());
    let port_open = tcp_accepts(HOST_PORT, Duration::from_secs(60)).await;
    evidence.raw("tcp_accepts", port_open);

    // 4. Le réglage pris en compte : le monde porte le nom posé, le fichier n'a pas été réécrit par le jeu.
    let created = worlds(&world_dir);
    let named_as_set = created.iter().any(|w| w.0 == format!("{WORLD}.wld"));
    evidence.raw("world_named_as_set", named_as_set);
    let config_after_start = std::fs::read_to_string(&config_path).unwrap();
    evidence.raw("config_kept", config_after_start == config_written);

    // 5. Mesures au repos (60 s après le signe : le plancher que le banc sait dire).
    tokio::time::sleep(Duration::from_secs(60)).await;
    let stats = docker(&["stats", "--no-stream", "--format", "{{.MemUsage}}", &id]);
    evidence.text("memory_usage_raw", &stats);
    evidence.raw("memory_idle_bytes", memory_bytes(&stats).unwrap_or(0));
    evidence.raw("disk_install_bytes", image_bytes + dir_size(&service_dir));

    // 6. La suite d'arrêt de la fiche (copie essayée) : les ordres par l'ENTRÉE du conteneur, sans terminal.
    let StopPlan::Sequence { steps, grace_s, .. } = stop_plan(&fiche) else {
        panic!("la copie essayée a une suite");
    };
    let before_save = worlds(&world_dir);
    let mut stdin_accepted = true;
    let mut quit_at = None;
    let mut orders = 0;
    let mut after_save = None;
    for step in &steps {
        match step {
            PlannedStep::StdinLine { line, quits } => {
                orders += 1;
                if *quits {
                    quit_at = Some(Instant::now());
                }
                if engine.send_stdin_line(&id, line).await.is_err() {
                    stdin_accepted = false;
                    break;
                }
            }
            PlannedStep::Wait { seconds } => {
                tokio::time::sleep(Duration::from_secs(u64::from(*seconds))).await;
                // Le relevé qui juge `save` est pris AVANT l'ordre de sortie (qui enregistre aussi).
                if quit_at.is_none() {
                    after_save = Some(worlds(&world_dir));
                }
            }
            PlannedStep::WaitLog { .. } => {
                panic!("la fiche n'attend aucun signe dans ses journaux")
            }
        }
    }
    let after_save = after_save.unwrap_or_default();
    // Aussitôt après l'ordre de sortie : l'arrêt demandé au moteur (le service reste arrêté).
    let stop_started = quit_at.unwrap_or_else(Instant::now);
    engine
        .stop(&id, Duration::from_secs(u64::from(grace_s)))
        .await
        .expect("arrêter");
    let stop_took = stop_started.elapsed();
    let exit_code = docker(&["inspect", "-f", "{{.State.ExitCode}}", &id]);
    let running = docker(&["inspect", "-f", "{{.State.Running}}", &id]) == "true";
    evidence.raw("stdin_orders_sent", orders);
    evidence.raw("stdin_accepted", stdin_accepted);
    evidence.raw("stop_took_s", stop_took.as_secs());
    evidence.text("stop_exit_code", &exit_code);
    // Sortie propre : le jeu a quitté de lui-même sur « exit » (code 0), pas tué à l'échéance (137).
    let quit_by_order = stdin_accepted && exit_code == "0";
    evidence.raw("exit_by_order_without_terminal", quit_by_order);
    let mtime_moved = after_save
        .iter()
        .any(|a| before_save.iter().any(|b| b.0 == a.0 && a.2 > b.2));
    evidence.raw("save_order_wrote_world", mtime_moved);
    evidence.raw(
        "stop_clean",
        quit_by_order && stop_took < Duration::from_secs(u64::from(grace_s)),
    );
    assert!(!running, "le conteneur doit être arrêté");

    // 7. Le service reste arrêté (politique `unless-stopped` + arrêt demandé au moteur).
    tokio::time::sleep(Duration::from_secs(15)).await;
    let still_stopped = docker(&["inspect", "-f", "{{.State.Running}}", &id]) != "true";
    evidence.raw("no_auto_restart", still_stopped);

    // 8. Le monde est là, puis le redémarrage retrouve le même monde (aucun nouveau).
    let kept = worlds(&world_dir);
    evidence.raw(
        "world_exists_after_stop",
        kept.iter()
            .any(|w| w.0 == format!("{WORLD}.wld") && w.1 > 0),
    );
    let relaunch = now_ms();
    engine.start(&id).await.expect("redémarrer");
    let second = wait_ready(&engine, &id, &sign, relaunch, Duration::from_secs(600)).await;
    evidence.raw("restart_ready", second.is_ok());
    let after_restart = worlds(&world_dir);
    let same_world = after_restart.len() == kept.len()
        && after_restart
            .iter()
            .map(|w| &w.0)
            .eq(kept.iter().map(|w| &w.0));
    evidence.raw("restart_keeps_world", second.is_ok() && same_world);

    // 9. Nettoyage de ce que ce test a lancé, puis le relevé.
    let _ = engine.stop(&id, Duration::from_secs(10)).await;
    let _ = engine.remove(&id).await;
    remove_leftovers();
    let _ = Command::new("sudo").args(["rm", "-rf"]).arg(&work).status();
    evidence.write(&out);

    // Une fois promus dans la fiche livrée, ces constats sont EXIGÉS : si l'image ne lit plus son entrée, le job
    // échoue au lieu de laisser la fiche affirmer ce qui n'est plus vrai.
    let shipped =
        load(Origin::Embedded, FICHE.as_bytes(), SETTINGS.as_bytes()).expect("fiche livrée");
    if shipped
        .file
        .stop
        .sequence
        .as_ref()
        .is_some_and(|s| s.status == Proof::Tested)
    {
        assert!(
            quit_by_order,
            "la fiche livrée dit la suite d'arrêt essayée : le jeu doit sortir sur `exit` sans terminal"
        );
    }
    if shipped
        .file
        .save
        .as_ref()
        .is_some_and(|s| s.status == Proof::Tested)
    {
        assert!(
            mtime_moved,
            "la fiche livrée dit `save` essayé : le monde doit être réécrit avant `exit`"
        );
    }
    // Ce que l'essai exige toujours (l'entrée sans terminal est CONSIGNÉE tant que la fiche ne l'affirme pas).
    assert!(port_open, "le port publié doit répondre");
    assert!(still_stopped, "le service arrêté doit rester arrêté");
    assert!(
        second.is_ok(),
        "le redémarrage doit atteindre le signe prêt"
    );
    assert!(
        same_world,
        "le redémarrage ne doit pas créer un autre monde"
    );
    assert!(
        named_as_set,
        "le réglage « nom du monde » doit être pris en compte"
    );
}

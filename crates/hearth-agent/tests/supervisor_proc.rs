//! Le superviseur systemd en détection seule (HRT-63, ADR-0037) : le propriétaire d'un conteneur lu dans
//! un faux arbre `/proc`, l'état d'une unité lu dans un faux `systemctl`. Aucune unité n'est jamais
//! touchée ; `systemctl` absent, muet ou lent est un cas normal et silencieux.
//!
//! Règles : BR-SVC-001, BR-SVC-003, BR-SVC-016.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "support/tmp.rs"]
mod tmp;

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::Path;

use hearth_agent::application::ports::{ContainerRef, ServiceSupervisor};
use hearth_agent::domain::services::{ServiceKey, UnitLookup, UnitName, attribute};
#[cfg(unix)]
use hearth_agent::domain::services::{UnitActivity, UnitStatus};
use hearth_agent::infrastructure::supervisor::proc_systemd::scan;
use hearth_agent::infrastructure::supervisor::{NoSupervisor, ProcSystemd};

fn container(name: &str, id_char: char) -> ContainerRef {
    ContainerRef {
        id: id_char.to_string().repeat(64),
        name: name.to_owned(),
    }
}

/// Un processus du faux `/proc` : sa ligne de commande (octets nuls) et son groupe de contrôle.
fn process(root: &Path, pid: u32, args: &[&str], cgroup: &str) {
    let dir = root.join(pid.to_string());
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("cmdline"), format!("{}\0", args.join("\0"))).unwrap();
    std::fs::write(dir.join("cgroup"), cgroup).unwrap();
}

fn supervisor(root: &Path) -> ProcSystemd {
    ProcSystemd::new(
        root.to_path_buf(),
        OsString::from("systemctl-absent-from-this-test"),
    )
}

/// Les propriétaires que la détection établit quand `systemctl show` donne ces processus principaux
/// (lecture du faux `/proc`, puis la règle d'attribution : la même chaîne que `owners`, sans lancer
/// `systemctl`, donc sur tous les systèmes).
fn owners_with_mains(
    root: &Path,
    wanted: &[ContainerRef],
    mains: &[(&str, u32)],
) -> HashMap<String, UnitName> {
    let mains = mains
        .iter()
        .map(|(name, pid)| (unit(name), *pid))
        .collect::<HashMap<_, _>>();
    attribute(&scan(root, wanted), &mains)
}

#[tokio::test]
async fn a_container_run_by_a_service_unit_is_found_and_its_key_is_u_unit() {
    let dir = tmp::tempdir().unwrap();
    process(
        dir.path(),
        4242,
        &["docker", "run", "--name=docker-x", "busybox"],
        "0::/system.slice/docker-x.service\n",
    );
    let web = container("docker-x", 'a');
    let owners = owners_with_mains(
        dir.path(),
        std::slice::from_ref(&web),
        &[("docker-x.service", 4242)],
    );
    let unit = owners.get(&web.id).expect("propriétaire");
    assert_eq!(unit.as_str(), "docker-x.service");
    assert_eq!(ServiceKey::for_unit(unit).as_str(), "u_docker-x.service");
}

#[tokio::test]
async fn only_the_containers_a_client_names_get_an_owner() {
    let dir = tmp::tempdir().unwrap();
    process(
        dir.path(),
        10,
        &[
            "/usr/bin/docker",
            "run",
            "--rm",
            "--name",
            "terraria",
            "img",
        ],
        "0::/system.slice/docker-terraria.service\n",
    );
    process(
        dir.path(),
        11,
        &["podman", "start", "-a", "valheim"],
        "0::/system.slice/podman-valheim.service\n",
    );
    // Le client d'un terminal de session : pas une unité de service.
    process(
        dir.path(),
        12,
        &["docker", "run", "--name=adhoc", "img"],
        "0::/user.slice/user-1000.slice/session-3.scope\n",
    );
    // Le gestionnaire d'un utilisateur : pas une unité du système.
    process(
        dir.path(),
        13,
        &["docker", "run", "--name=mine", "img"],
        "0::/user.slice/user-1000.slice/user@1000.service/app.slice/x.service\n",
    );
    // Un autre programme dont les arguments ressemblent à un `docker run` : ignoré.
    process(
        dir.path(),
        14,
        &["sh", "-c", "docker run --name=spoof img"],
        "0::/system.slice/evil.service\n",
    );
    process(
        dir.path(),
        15,
        &["vim", "run", "--name=spoof", "x"],
        "0::/system.slice/evil.service\n",
    );
    // Une sous-commande qui ne lance rien.
    process(
        dir.path(),
        16,
        &["docker", "ps", "--name=web"],
        "0::/system.slice/cron.service\n",
    );
    // Des entrées de /proc qui ne sont pas des processus.
    std::fs::create_dir_all(dir.path().join("self")).unwrap();
    std::fs::write(dir.path().join("uptime"), "1 1\n").unwrap();

    let wanted = [
        container("terraria", 'a'),
        container("valheim", 'b'),
        container("adhoc", 'c'),
        container("mine", 'd'),
        container("spoof", 'e'),
        container("web", 'f'),
    ];
    // Les clients 10 et 11 sont les processus principaux de leurs unités ; ceux des autres unités aussi
    // (c'est la lecture du groupe de contrôle et du programme qui les écarte, pas le `MainPID`).
    let owners = owners_with_mains(
        dir.path(),
        &wanted,
        &[
            ("docker-terraria.service", 10),
            ("podman-valheim.service", 11),
            ("evil.service", 14),
            ("cron.service", 16),
            ("x.service", 13),
        ],
    );
    let mut found: Vec<(&str, &str)> = wanted
        .iter()
        .filter_map(|c| {
            owners
                .get(&c.id)
                .map(|unit| (c.name.as_str(), unit.as_str()))
        })
        .collect();
    found.sort_unstable();
    assert_eq!(
        found,
        vec![
            ("terraria", "docker-terraria.service"),
            ("valheim", "podman-valheim.service"),
        ]
    );
}

#[tokio::test]
async fn a_container_is_designated_by_name_full_id_or_a_long_id_prefix() {
    let dir = tmp::tempdir().unwrap();
    let by_id = container("by-id", 'a');
    let by_prefix = container("by-prefix", 'b');
    let short = container("short", 'c');
    process(
        dir.path(),
        1,
        &["docker", "start", "-a", &by_id.id],
        "0::/system.slice/one.service\n",
    );
    process(
        dir.path(),
        2,
        &["docker", "start", "-a", &by_prefix.id[..12]],
        "0::/system.slice/two.service\n",
    );
    // Un préfixe trop court ne désigne personne.
    process(
        dir.path(),
        3,
        &["docker", "start", "-a", &short.id[..6]],
        "0::/system.slice/three.service\n",
    );
    let owners = owners_with_mains(
        dir.path(),
        &[by_id.clone(), by_prefix.clone(), short.clone()],
        &[("one.service", 1), ("two.service", 2), ("three.service", 3)],
    );
    assert_eq!(
        owners.get(&by_id.id).map(UnitName::as_str),
        Some("one.service")
    );
    assert_eq!(
        owners.get(&by_prefix.id).map(UnitName::as_str),
        Some("two.service")
    );
    assert!(!owners.contains_key(&short.id));
}

#[tokio::test]
async fn two_units_claiming_the_same_container_leave_it_without_an_owner() {
    let dir = tmp::tempdir().unwrap();
    process(
        dir.path(),
        1,
        &["docker", "run", "--name=web", "img"],
        "0::/system.slice/a.service\n",
    );
    process(
        dir.path(),
        2,
        &["docker", "run", "--name=web", "img"],
        "0::/system.slice/b.service\n",
    );
    // Deux clients de la MÊME unité : pas d'ambiguïté.
    process(
        dir.path(),
        3,
        &["docker", "run", "--name=db", "img"],
        "0::/system.slice/same.service\n",
    );
    process(
        dir.path(),
        4,
        &["docker", "start", "-a", "db"],
        "0::/system.slice/same.service\n",
    );
    let (web, db) = (container("web", 'a'), container("db", 'b'));
    let owners = owners_with_mains(
        dir.path(),
        &[web.clone(), db.clone()],
        &[("a.service", 1), ("b.service", 2), ("same.service", 3)],
    );
    assert!(!owners.contains_key(&web.id));
    assert_eq!(
        owners.get(&db.id).map(UnitName::as_str),
        Some("same.service")
    );
}

#[tokio::test]
async fn a_client_that_is_not_the_main_process_of_its_unit_owns_nothing() {
    let dir = tmp::tempdir().unwrap();
    // Une tâche planifiée lance un conteneur au premier plan : son client tourne dans le groupe de
    // contrôle du service de planification, dont le processus principal est le planificateur (pid 5).
    process(
        dir.path(),
        77,
        &["docker", "run", "--rm", "--name=sauvegarde", "img"],
        "0::/system.slice/cron.service\n",
    );
    // Le vrai service d'un conteneur : son client EST le processus principal.
    process(
        dir.path(),
        90,
        &["docker", "run", "--name=terraria", "img"],
        "0::/system.slice/docker-terraria.service\n",
    );
    let wanted = [container("sauvegarde", 'a'), container("terraria", 'b')];
    let owners = owners_with_mains(
        dir.path(),
        &wanted,
        &[("cron.service", 5), ("docker-terraria.service", 90)],
    );
    assert!(!owners.contains_key(&wanted[0].id), "{owners:?}");
    assert_eq!(
        owners.get(&wanted[1].id).map(UnitName::as_str),
        Some("docker-terraria.service")
    );
    // Une unité sans processus principal (arrêtée, ou `systemctl` muet) ne donne rien.
    assert!(owners_with_mains(dir.path(), &wanted, &[]).is_empty());
    assert!(owners_with_mains(dir.path(), &wanted, &[("docker-terraria.service", 0)]).is_empty());
}

#[tokio::test]
async fn a_unit_that_names_two_containers_supervises_neither() {
    let dir = tmp::tempdir().unwrap();
    // `docker start -a a b` : un seul client, deux conteneurs vivants sous la même clé `u_` possible.
    process(
        dir.path(),
        30,
        &["docker", "start", "-a", "alpha", "beta"],
        "0::/system.slice/pair.service\n",
    );
    process(
        dir.path(),
        31,
        &["docker", "run", "--name=gamma", "img"],
        "0::/system.slice/solo.service\n",
    );
    let wanted = [
        container("alpha", 'a'),
        container("beta", 'b'),
        container("gamma", 'c'),
    ];
    let owners = owners_with_mains(
        dir.path(),
        &wanted,
        &[("pair.service", 30), ("solo.service", 31)],
    );
    assert!(!owners.contains_key(&wanted[0].id) && !owners.contains_key(&wanted[1].id));
    assert_eq!(
        owners.get(&wanted[2].id).map(UnitName::as_str),
        Some("solo.service")
    );
}

#[tokio::test]
async fn a_homonymous_program_started_by_another_service_does_not_take_the_unit() {
    let dir = tmp::tempdir().unwrap();
    // Le vrai client de l'unité, puis un programme homonyme lancé plus tard par un autre processus du
    // même service : seul le principal (pid 40) fait foi, le second revendique le même conteneur en vain.
    process(
        dir.path(),
        40,
        &["docker", "run", "--name=web", "img"],
        "0::/system.slice/docker-web.service\n",
    );
    process(
        dir.path(),
        41,
        &["docker", "run", "--name=web", "img"],
        "0::/system.slice/docker-web.service\n",
    );
    let web = container("web", 'a');
    let owners = owners_with_mains(
        dir.path(),
        std::slice::from_ref(&web),
        &[("docker-web.service", 40)],
    );
    assert_eq!(
        owners.get(&web.id).map(UnitName::as_str),
        Some("docker-web.service")
    );
    // Aucun des deux n'est le processus principal : rien.
    let owners = owners_with_mains(
        dir.path(),
        std::slice::from_ref(&web),
        &[("docker-web.service", 99)],
    );
    assert!(owners.is_empty());
}

#[tokio::test]
async fn a_unit_name_that_is_not_a_unit_name_never_leaves_the_scan() {
    let dir = tmp::tempdir().unwrap();
    for (pid, cgroup) in [
        (1, "0::/system.slice/a;rm -rf.service\n"),
        (2, "0::/system.slice/$(reboot).service\n"),
        (3, "0::/system.slice/-oops.service\n"),
        (4, "0::/system.slice/not-a-service.socket\n"),
        (5, "0::/system.slice/a\nb.service\n"),
    ] {
        process(
            dir.path(),
            pid,
            &["docker", "run", "--name=web", "img"],
            cgroup,
        );
    }
    let web = container("web", 'a');
    let owners = supervisor(dir.path())
        .owners(std::slice::from_ref(&web))
        .await;
    assert!(owners.is_empty(), "{owners:?}");
}

#[tokio::test]
async fn a_nix_wrapped_client_and_a_missing_proc_are_both_normal() {
    let dir = tmp::tempdir().unwrap();
    process(
        dir.path(),
        1,
        &[
            "/nix/store/abc-docker/bin/.docker-wrapped",
            "run",
            "--name=web",
            "img",
        ],
        "0::/system.slice/docker-web.service\n",
    );
    let web = container("web", 'a');
    let owners = owners_with_mains(
        dir.path(),
        std::slice::from_ref(&web),
        &[("docker-web.service", 1)],
    );
    assert_eq!(
        owners.get(&web.id).map(UnitName::as_str),
        Some("docker-web.service")
    );
    // Pas de /proc (poste de développement), rien à chercher : vide, sans bruit.
    let nowhere = supervisor(&dir.path().join("absent"));
    assert!(nowhere.owners(std::slice::from_ref(&web)).await.is_empty());
    assert!(supervisor(dir.path()).owners(&[]).await.is_empty());
    assert!(NoSupervisor.owners(&[web]).await.is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn the_program_is_read_from_the_exe_link_when_there_is_one() {
    use std::os::unix::fs::symlink;
    let dir = tmp::tempdir().unwrap();
    // Le vrai programme est `sh` quoi que dise argv[0].
    process(
        dir.path(),
        1,
        &["docker", "run", "--name=web", "img"],
        "0::/system.slice/spoof.service\n",
    );
    symlink("/bin/sh", dir.path().join("1").join("exe")).unwrap();
    // Un vrai client : `exe` désigne docker.
    process(
        dir.path(),
        2,
        &["whatever", "run", "--name=db", "img"],
        "0::/system.slice/real.service\n",
    );
    symlink("/usr/bin/docker", dir.path().join("2").join("exe")).unwrap();
    let (web, db) = (container("web", 'a'), container("db", 'b'));
    let owners = owners_with_mains(
        dir.path(),
        &[web.clone(), db.clone()],
        &[("spoof.service", 1), ("real.service", 2)],
    );
    assert!(!owners.contains_key(&web.id));
    assert_eq!(
        owners.get(&db.id).map(UnitName::as_str),
        Some("real.service")
    );
}

// ---------------------------------------------------------------------------------------------
// L'état d'une unité : systemctl show
// ---------------------------------------------------------------------------------------------

fn unit(name: &str) -> UnitName {
    UnitName::parse(name).unwrap()
}

#[tokio::test]
async fn a_missing_systemctl_is_a_quiet_unavailable() {
    let dir = tmp::tempdir().unwrap();
    let supervisor = ProcSystemd::new(
        dir.path().to_path_buf(),
        dir.path()
            .join("nowhere")
            .join("systemctl")
            .into_os_string(),
    );
    assert_eq!(
        supervisor.unit(&unit("docker-x.service")).await,
        UnitLookup::Unavailable
    );
    assert_eq!(
        NoSupervisor.unit(&unit("docker-x.service")).await,
        UnitLookup::Unavailable
    );
}

/// Un faux `systemctl` : écrit ses arguments, un par ligne, dans `<script>.args`, puis rend `output`.
#[cfg(unix)]
fn fake_systemctl(dir: &Path, body: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let script = dir.join("systemctl");
    std::fs::write(&script, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    script
}

/// La chaîne complète : faux `/proc`, vrai appel (faux `systemctl`) pour le processus principal de
/// chaque unité revendiquée.
#[cfg(unix)]
#[tokio::test]
async fn the_main_process_is_read_from_systemctl_before_a_unit_owns_a_container() {
    let dir = tmp::tempdir().unwrap();
    process(
        dir.path(),
        4242,
        &["docker", "run", "--name=terraria", "img"],
        "0::/system.slice/docker-terraria.service\n",
    );
    process(
        dir.path(),
        77,
        &["docker", "run", "--rm", "--name=sauvegarde", "img"],
        "0::/system.slice/cron.service\n",
    );
    let script = fake_systemctl(
        dir.path(),
        "case \"$4\" in\n\
         docker-terraria.service) printf 'MainPID=4242\\n' ;;\n\
         cron.service) printf 'MainPID=5\\n' ;;\n\
         esac",
    );
    let supervisor = ProcSystemd::new(dir.path().to_path_buf(), script.into_os_string());
    let (terraria, backup) = (container("terraria", 'a'), container("sauvegarde", 'b'));
    let owners = supervisor.owners(&[terraria.clone(), backup.clone()]).await;
    assert_eq!(
        owners.get(&terraria.id).map(UnitName::as_str),
        Some("docker-terraria.service")
    );
    assert!(!owners.contains_key(&backup.id), "{owners:?}");
    // Sans `systemctl`, aucune unité n'est crue.
    let mute = ProcSystemd::new(
        dir.path().to_path_buf(),
        dir.path().join("nowhere").into_os_string(),
    );
    assert!(mute.owners(&[terraria]).await.is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn systemctl_show_is_called_with_one_argument_each_after_a_double_dash() {
    let dir = tmp::tempdir().unwrap();
    let script = fake_systemctl(
        dir.path(),
        "printf '%s\\n' \"$@\" > \"$0.args\"\n\
         printf 'LoadState=loaded\\nActiveState=active\\nSubState=running\\nRestart=always\\nTimeoutStopUSec=2min\\n'",
    );
    let supervisor = ProcSystemd::new(dir.path().to_path_buf(), script.clone().into_os_string());
    let lookup = supervisor.unit(&unit("docker-x.service")).await;
    assert_eq!(
        lookup,
        UnitLookup::Loaded(UnitStatus {
            activity: UnitActivity::Active,
            auto_restart: false,
            restarts: true,
            timeout_stop_s: Some(120),
        })
    );
    let args = std::fs::read_to_string(format!("{}.args", script.display())).unwrap();
    let args: Vec<&str> = args.lines().collect();
    assert_eq!(args.len(), 4, "{args:?}");
    assert_eq!(args[0], "show");
    assert!(
        args[1].starts_with("--property=LoadState,ActiveState"),
        "{args:?}"
    );
    assert_eq!(args[2], "--");
    assert_eq!(args[3], "docker-x.service");
}

#[cfg(unix)]
#[tokio::test]
async fn a_silent_failing_or_slow_systemctl_is_unavailable_and_an_unknown_unit_is_not_loaded() {
    let dir = tmp::tempdir().unwrap();
    for (body, expected) in [
        ("exit 1", UnitLookup::Unavailable),
        ("printf 'not systemd\\n'", UnitLookup::Unavailable),
        ("", UnitLookup::Unavailable),
        (
            "printf 'LoadState=not-found\\nActiveState=inactive\\nSubState=dead\\n'",
            UnitLookup::NotLoaded,
        ),
    ] {
        let script = fake_systemctl(dir.path(), body);
        let supervisor = ProcSystemd::new(dir.path().to_path_buf(), script.into_os_string());
        assert_eq!(
            supervisor.unit(&unit("docker-x.service")).await,
            expected,
            "{body}"
        );
    }
    // Trop lent : abandonné au délai, sans attendre la fin de la commande.
    let script = fake_systemctl(dir.path(), "sleep 20");
    let supervisor = ProcSystemd::new(dir.path().to_path_buf(), script.into_os_string())
        .with_timeout(std::time::Duration::from_millis(200));
    let started = std::time::Instant::now();
    assert_eq!(
        supervisor.unit(&unit("docker-x.service")).await,
        UnitLookup::Unavailable
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
}

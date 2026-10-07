//! Les codes de sortie de `hearth-agent update-supervise` (HRT-27, BR-UPDATE-033). Le superviseur
//! tourne dans une unité transitoire `Restart=on-failure` : un code de sortie non nul le fait
//! relancer, 0 ne le relance jamais. Quand rien n'est à reprendre (travail absent, illisible, un
//! autre superviseur au travail), il sort en 0 : une relance n'y changerait rien.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

#[cfg(unix)]
use hearth_agent::application::ports::UpdateHost;
use hearth_agent::domain::update::Job;
#[cfg(unix)]
use hearth_agent::infrastructure::update::{FsUpdateHost, Launcher};

fn supervise(job: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_hearth-agent"))
        .arg("update-supervise")
        .arg("--job")
        .arg(job)
        .env("HEARTH_LOG_FORMAT", "text")
        .output()
        .unwrap()
}

fn job(dir: &Path) -> Job {
    Job {
        version: "0.2.0".into(),
        previous: "0.1.0".into(),
        binary: dir.join("bin").join("hearth-agent"),
        staged: dir.join("update").join("hearth-agent.new"),
        backup: dir.join("bin").join(".hearth-agent.previous"),
        probe_addr: "127.0.0.1:7341".into(),
        fingerprint: "ab".repeat(32),
        grace_ms: 0,
        check_window_ms: 50,
        poll_ms: 5,
        requested_by: None,
        client_name: None,
        client_addr: None,
        recover: false,
    }
}

#[test]
fn a_job_that_is_not_there_is_nothing_to_resume_and_exits_zero() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("update")).unwrap();
    let output = supervise(&dir.path().join("update").join("job.json"));
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn a_job_that_cannot_be_read_exits_zero_and_says_so_once_on_stderr() {
    let dir = tempfile::tempdir().unwrap();
    let update = dir.path().join("update");
    fs::create_dir(&update).unwrap();
    fs::write(update.join("job.json"), b"{pas du json").unwrap();
    let output = supervise(&update.join("job.json"));
    assert!(output.status.success(), "{output:?}");
    let text = String::from_utf8_lossy(&output.stderr).to_string()
        + &String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("illisible"), "{text}");
    // Jamais en silence : le résultat « échec, interrompue » est écrit (le client le verra).
    let last = fs::read_to_string(update.join("last.json")).unwrap();
    assert!(last.contains("\"outcome\": \"failed\""), "{last}");
    assert!(last.contains("\"reason\": \"interrupted\""), "{last}");
    assert!(last.contains("\"reported\": false"), "{last}");
    assert!(
        !update.join("job.json").exists(),
        "le travail illisible est retiré après le résultat"
    );
}

#[test]
fn a_job_with_an_unreadable_field_exits_zero_and_writes_a_failed_result_with_its_version() {
    let dir = tempfile::tempdir().unwrap();
    let update = dir.path().join("update");
    fs::create_dir(&update).unwrap();
    let mut work = job(dir.path());
    work.probe_addr = "pas-une-adresse".into();
    fs::write(update.join("job.json"), serde_json::to_vec(&work).unwrap()).unwrap();
    let output = supervise(&update.join("job.json"));
    assert!(output.status.success(), "{output:?}");
    let last = fs::read_to_string(update.join("last.json")).unwrap();
    assert!(last.contains("\"outcome\": \"failed\""), "{last}");
    assert!(last.contains("\"version\": \"0.2.0\""), "{last}");
    assert!(
        !update.join("job.json").exists(),
        "le travail est retiré après le résultat"
    );
}

#[cfg(unix)]
#[test]
fn another_supervisor_at_work_is_refused_without_a_restart_loop() {
    let dir = tempfile::tempdir().unwrap();
    let host = FsUpdateHost::new(dir.path(), dir.path().join("agent"), Launcher::Detached);
    let path = host.write_job(&job(dir.path())).unwrap();
    let _held = host.take_supervisor_lock().unwrap();
    let output = supervise(&path);
    assert!(
        output.status.success(),
        "un second superviseur sort en 0 : systemd ne le relance pas : {output:?}"
    );
    assert!(
        host.read_marker().unwrap().is_none(),
        "il n'a rien écrit pendant qu'un autre travaillait"
    );
}

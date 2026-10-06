//! La mise à jour de l'agent à distance à travers le cas d'usage (BR-UPDATE-011 à 019, 024) : un
//! banc avec un téléchargeur simulé (porte pour garder une mise à jour « en cours »), une machine
//! en mémoire, la vraie vérification minisign (clé jetable) et la vraie base du journal.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::time::Duration;

use hearth_agent::application::update::UpdateError;
use hearth_agent::domain::accounts::Role;
use hearth_agent::domain::audit::{AuditFilter, AuditRecord, RawFilter};
use hearth_agent::domain::update::{UpdateInput, UpdateRecord, UpdateRefusal};
use hearth_proto::api::update::{UpdateOutcome, UpdateProgress, UpdateReason, UpdateStep};
use support::update::{CURRENT, Download, Rig, sha256_hex};
use support::{Env, by, env};
use tokio::sync::broadcast::Receiver;

const BINARY: &[u8] = b"nouvel agent";

fn input<'a>(
    rig: &Rig,
    version: &'a str,
    bytes: &[u8],
    signature: &'a str,
    sum: &'a str,
) -> UpdateInput<'a> {
    let _ = (rig, bytes);
    UpdateInput {
        version,
        url: "https://exemple.org/hearth-agent",
        signature,
        sha256: sum,
    }
}

/// Lance une mise à jour valable de `bytes` en version `0.2.0`.
async fn start_valid(rig: &Rig, bytes: &[u8]) -> Result<UpdateProgress, UpdateError> {
    let signature = rig.keys.sign(bytes);
    let sum = sha256_hex(bytes);
    rig.service
        .start(by().clone(), input(rig, "0.2.0", bytes, &signature, &sum))
        .await
}

async fn next_matching(
    receiver: &mut Receiver<UpdateProgress>,
    wanted: impl Fn(&UpdateProgress) -> bool,
) -> UpdateProgress {
    loop {
        let progress = tokio::time::timeout(Duration::from_secs(5), receiver.recv())
            .await
            .expect("événement attendu")
            .expect("canal ouvert");
        if wanted(&progress) {
            return progress;
        }
    }
}

async fn journal(env: &Env) -> Vec<AuditRecord> {
    env.audit_recorder.flush_all().await;
    let filter = AuditFilter::new(RawFilter {
        actions: vec!["agent.update".to_owned()],
        ..RawFilter::default()
    })
    .unwrap();
    let mut records = env
        .audit
        .search(Role::Admin, &filter)
        .await
        .unwrap()
        .records;
    records.reverse();
    records
}

#[tokio::test]
async fn a_valid_update_shows_every_step_in_order_then_hands_over_to_the_supervisor() {
    let env = env().await;
    let rig = Rig::new(&env, true, false);
    let mut events = rig.feed_receiver();

    let first = start_valid(&rig, BINARY).await.unwrap();
    assert_eq!(first.version, "0.2.0");
    assert_eq!(first.step, UpdateStep::Download);

    let mut steps = Vec::new();
    let mut percents = Vec::new();
    loop {
        let progress = next_matching(&mut events, |_| true).await;
        if progress.step == UpdateStep::Download {
            percents.extend(progress.percent);
        }
        if steps.last() != Some(&progress.step) {
            steps.push(progress.step);
        }
        if progress.step == UpdateStep::Restart {
            break;
        }
    }
    assert_eq!(
        steps,
        [
            UpdateStep::Download,
            UpdateStep::Verify,
            UpdateStep::Install,
            UpdateStep::Restart
        ]
    );
    assert_eq!(
        percents,
        [0, 25, 50, 75, 100],
        "un message par pourcentage qui change"
    );

    // Le superviseur est lancé avec le travail complet ; rien ne s'est exécuté avant.
    // L'étape « redémarrage » est annoncée avant le lancement lui-même (travail bloquant) : on
    // attend que le superviseur soit réellement parti.
    for _ in 0..200 {
        if rig.host.with(|s| !s.launched.is_empty()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let (staged, job, launched) = rig
        .host
        .with(|s| (s.staged.clone(), s.job.clone(), s.launched.clone()));
    assert_eq!(staged.as_deref(), Some(BINARY));
    let job = job.expect("travail écrit");
    assert_eq!(job.version, "0.2.0");
    assert_eq!(job.previous, CURRENT.to_string());
    assert_eq!(job.binary.to_str(), Some("/usr/local/bin/hearth-agent"));
    assert_eq!(job.probe_addr, "127.0.0.1:7341");
    assert_eq!(job.fingerprint, "ab".repeat(32));
    assert_eq!(job.requested_by.as_deref(), Some("root"));
    assert_eq!(launched.len(), 1);
    assert!(rig.service.status().in_progress);
}

#[tokio::test]
async fn nothing_is_written_or_run_before_the_checksum_and_the_signature_are_verified() {
    let env = env().await;

    // Somme fausse.
    let rig = Rig::new(&env, true, false);
    let signature = rig.keys.sign(BINARY);
    let wrong_sum = sha256_hex(b"autre chose");
    let mut events = rig.feed_receiver();
    rig.service
        .start(
            by().clone(),
            input(&rig, "0.2.0", BINARY, &signature, &wrong_sum),
        )
        .await
        .unwrap();
    let done = next_matching(&mut events, |p| p.step == UpdateStep::Done).await;
    assert_eq!(done.outcome, Some(UpdateOutcome::Failed));
    assert_eq!(done.reason, Some(UpdateReason::BadChecksum));
    rig.host.with(|s| {
        assert!(s.staged.is_none(), "rien d'écrit");
        assert!(!s.supervisor_prepared && s.job.is_none() && s.launched.is_empty());
    });

    // Signature d'un autre contenu : bien formée, de la bonne clé, fausse.
    let rig = Rig::new(&env, true, false);
    let other = rig.keys.sign(b"un autre binaire");
    let sum = sha256_hex(BINARY);
    let mut events = rig.feed_receiver();
    rig.service
        .start(by().clone(), input(&rig, "0.2.0", BINARY, &other, &sum))
        .await
        .unwrap();
    let done = next_matching(&mut events, |p| p.step == UpdateStep::Done).await;
    assert_eq!(done.reason, Some(UpdateReason::BadSignature));
    rig.host.with(|s| {
        assert!(s.staged.is_none() && s.launched.is_empty());
    });
    assert!(!rig.service.status().in_progress, "l'état en cours tombe");
    assert_eq!(rig.service.last().unwrap().outcome, UpdateOutcome::Failed);
}

#[tokio::test]
async fn a_signature_of_another_key_is_refused_before_any_download() {
    let env = env().await;
    let rig = Rig::new(&env, true, false);
    let stranger = support::update::Keys::generate();
    let signature = stranger.sign(BINARY);
    let sum = sha256_hex(BINARY);
    let error = rig
        .service
        .start(by().clone(), input(&rig, "0.2.0", BINARY, &signature, &sum))
        .await
        .unwrap_err();
    assert!(matches!(error, UpdateError::BadSignature));
    assert!(
        rig.downloader.fetched.lock().unwrap().is_empty(),
        "rien téléchargé"
    );
    assert!(!rig.service.status().in_progress);
}

#[tokio::test]
async fn only_one_update_at_a_time_and_the_refusal_is_journaled() {
    let env = env().await;
    let rig = Rig::new(&env, true, true);
    start_valid(&rig, BINARY).await.unwrap();
    assert!(rig.service.status().in_progress);

    let error = start_valid(&rig, BINARY).await.unwrap_err();
    assert!(matches!(
        error,
        UpdateError::Refused(UpdateRefusal::InProgress)
    ));
    assert_eq!(
        error.to_string(),
        "Une mise à jour de l'agent est déjà en cours. Réessaye plus tard."
    );
    let entries = journal(&env).await;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].outcome.code(), "failed");
    assert_eq!(entries[0].target.as_deref(), Some("version 0.2.0"));
    assert!(
        entries[0]
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("déjà en cours")),
        "{:?}",
        entries[0].reason
    );

    // Une seule demande est partie.
    assert_eq!(rig.downloader.fetched.lock().unwrap().len(), 1);
    rig.release_gate();
}

#[tokio::test]
async fn a_supervisor_that_still_works_counts_as_an_update_in_progress_after_a_restart() {
    let env = env().await;
    let rig = Rig::new(&env, true, false);
    // Le nouvel agent vient de démarrer pendant le contrôle du superviseur : il ne l'a pas lancé.
    rig.host
        .running
        .store(true, std::sync::atomic::Ordering::SeqCst);
    rig.host.with(|s| {
        s.state = Some(hearth_agent::domain::update::SupervisorState {
            version: "0.2.0".into(),
            step: UpdateStep::Check,
            previous: "0.1.0".into(),
            requester: Default::default(),
        });
    });
    let status = rig.service.status();
    assert!(status.in_progress);
    let progress = status.progress.unwrap();
    assert_eq!(
        (progress.version.as_str(), progress.step),
        ("0.2.0", UpdateStep::Check)
    );
    let error = start_valid(&rig, BINARY).await.unwrap_err();
    assert!(matches!(
        error,
        UpdateError::Refused(UpdateRefusal::InProgress)
    ));
}

#[tokio::test]
async fn a_managed_installation_refuses_and_nothing_is_downloaded() {
    let env = env().await;
    let rig = Rig::new(&env, false, false);
    let error = start_valid(&rig, BINARY).await.unwrap_err();
    assert!(matches!(
        error,
        UpdateError::Refused(UpdateRefusal::Managed)
    ));
    assert!(rig.downloader.fetched.lock().unwrap().is_empty());
    let status = rig.service.status();
    assert!(status.managed && !status.in_progress);
}

#[tokio::test]
async fn an_unreachable_server_or_a_failed_download_ends_with_a_reason_and_the_agent_is_unchanged()
{
    let env = env().await;
    for (download, reason) in [
        (Download::Unreachable, UpdateReason::Unreachable),
        (Download::Failed, UpdateReason::DownloadFailed),
    ] {
        let rig = Rig::new(&env, true, false);
        *rig.downloader.result.lock().unwrap() = download;
        let mut events = rig.feed_receiver();
        start_valid(&rig, BINARY).await.unwrap();
        let done = next_matching(&mut events, |p| p.step == UpdateStep::Done).await;
        assert_eq!(done.outcome, Some(UpdateOutcome::Failed));
        assert_eq!(done.reason, Some(reason));
        rig.host
            .with(|s| assert!(s.launched.is_empty() && s.staged.is_none()));
        assert!(!rig.service.status().in_progress);
    }
    let entries = journal(&env).await;
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().all(|entry| entry.outcome.code() == "failed"));
}

#[tokio::test]
async fn a_binary_that_does_not_announce_the_target_version_is_refused_before_the_supervisor() {
    let env = env().await;
    let rig = Rig::new(&env, true, false);
    rig.host
        .with(|s| s.announced = Some(hearth_agent::domain::install::Version::new(9, 9, 9)));
    let mut events = rig.feed_receiver();
    start_valid(&rig, BINARY).await.unwrap();
    let done = next_matching(&mut events, |p| p.step == UpdateStep::Done).await;
    assert_eq!(done.reason, Some(UpdateReason::BadBinary));
    rig.host.with(|s| {
        assert!(s.launched.is_empty() && s.staged.is_none(), "dépôt retiré");
        assert!(s.cleared >= 1);
    });
}

#[tokio::test]
async fn a_disk_or_launch_failure_cleans_up_and_says_why() {
    let env = env().await;
    let rig = Rig::new(&env, true, false);
    rig.host.with(|s| s.fail_stage = true);
    let mut events = rig.feed_receiver();
    start_valid(&rig, BINARY).await.unwrap();
    let done = next_matching(&mut events, |p| p.step == UpdateStep::Done).await;
    assert_eq!(done.reason, Some(UpdateReason::Staging));

    let rig = Rig::new(&env, true, false);
    rig.host.with(|s| s.fail_launch = true);
    let mut events = rig.feed_receiver();
    start_valid(&rig, BINARY).await.unwrap();
    let done = next_matching(&mut events, |p| p.step == UpdateStep::Done).await;
    assert_eq!(done.reason, Some(UpdateReason::SupervisorLaunch));
    rig.host
        .with(|s| assert!(s.staged.is_none(), "le binaire déposé est retiré"));
}

#[tokio::test]
async fn the_result_written_by_the_supervisor_is_announced_once_and_journaled_once_after_a_restart()
{
    let env = env().await;
    let rig = Rig::new(&env, true, false);
    // Le superviseur a écrit « réussi » ; le nouvel agent démarre.
    rig.host.with(|s| {
        s.last = Some(UpdateRecord {
            version: "0.2.0".into(),
            previous: "0.1.0".into(),
            outcome: UpdateOutcome::Succeeded,
            reason: None,
            at: "2026-10-05T10:00:00Z".into(),
            requested_by: Some("marie".into()),
            client_name: Some("PC-de-marie".into()),
            client_addr: Some("10.0.0.7".into()),
            reported: false,
        });
    });
    let mut events = rig.feed_receiver();
    rig.service.resume().await;
    let done = next_matching(&mut events, |p| p.step == UpdateStep::Done).await;
    assert_eq!(done.outcome, Some(UpdateOutcome::Succeeded));
    // Un second démarrage n'écrit pas une seconde entrée.
    rig.service.resume().await;
    let entries = journal(&env).await;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].outcome.code(), "ok");
    assert_eq!(entries[0].account.as_deref(), Some("marie"));
    assert_eq!(entries[0].target.as_deref(), Some("version 0.2.0"));
    assert!(rig.host.with(|s| s.last.clone()).unwrap().reported);
    assert!(!rig.service.status().in_progress);
}

#[tokio::test]
async fn a_rolled_back_update_is_announced_with_its_reason_and_journaled_as_failed() {
    let env = env().await;
    let rig = Rig::new(&env, true, false);
    rig.host.with(|s| {
        s.last = Some(UpdateRecord {
            version: "0.2.0".into(),
            previous: "0.1.0".into(),
            outcome: UpdateOutcome::RolledBack,
            reason: Some(UpdateReason::NoAnswer),
            at: "2026-10-05T10:00:00Z".into(),
            requested_by: Some("marie".into()),
            client_name: None,
            client_addr: None,
            reported: false,
        });
    });
    let mut events = rig.feed_receiver();
    rig.service.resume().await;
    let done = next_matching(&mut events, |p| p.step == UpdateStep::Done).await;
    assert_eq!(done.outcome, Some(UpdateOutcome::RolledBack));
    assert_eq!(done.reason, Some(UpdateReason::NoAnswer));
    let entries = journal(&env).await;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].outcome.code(), "failed");
    assert!(
        entries[0]
            .reason
            .as_deref()
            .unwrap()
            .contains("retour à la version précédente"),
        "{:?}",
        entries[0].reason
    );
    // BR-UPDATE-016 : pas de relance automatique ; le résultat reste lisible.
    let last = rig.service.last().unwrap();
    assert_eq!(last.outcome, UpdateOutcome::RolledBack);
    assert!(rig.host.with(|s| s.launched.is_empty()));
}

#[tokio::test]
async fn the_result_survives_the_loss_of_the_service_itself() {
    // Le résultat est lu dans le fichier (la machine), pas dans la mémoire du service : un autre
    // service sur la même machine le lit tel quel (BR-UPDATE-017).
    let env = env().await;
    let rig = Rig::new(&env, true, false);
    rig.host.with(|s| {
        s.last = Some(UpdateRecord {
            version: "0.2.0".into(),
            previous: "0.1.0".into(),
            outcome: UpdateOutcome::Succeeded,
            reason: None,
            at: "2026-10-05T10:00:00Z".into(),
            requested_by: None,
            client_name: None,
            client_addr: None,
            reported: true,
        });
    });
    let again = Rig::with_host(&env, true, rig.host.clone());
    assert_eq!(again.service.last().unwrap().version, "0.2.0");
    assert_eq!(
        again.service.status().last.unwrap().outcome,
        UpdateOutcome::Succeeded
    );
}

#[tokio::test]
async fn a_client_subscribing_in_the_middle_gets_the_current_step_first() {
    let env = env().await;
    let rig = Rig::new(&env, true, true);
    start_valid(&rig, BINARY).await.unwrap();
    // Un client arrive pendant le téléchargement : il reçoit l'état courant, puis la suite.
    let (mut receiver, current) = rig.service.subscribe();
    assert_eq!(current.unwrap().step, UpdateStep::Download);
    rig.release_gate();
    let restart = next_matching(&mut receiver, |p| p.step == UpdateStep::Restart).await;
    assert_eq!(restart.version, "0.2.0");
}

#[tokio::test]
async fn a_version_that_is_not_newer_or_a_bad_field_is_refused_without_a_trace() {
    let env = env().await;
    let rig = Rig::new(&env, true, false);
    let signature = rig.keys.sign(BINARY);
    let sum = sha256_hex(BINARY);
    for version in ["0.1.0", "0.0.1"] {
        let error = rig
            .service
            .start(by().clone(), input(&rig, version, BINARY, &signature, &sum))
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            UpdateError::Refused(UpdateRefusal::NotNewer { .. })
        ));
    }
    let error = rig
        .service
        .start(
            by().clone(),
            UpdateInput {
                version: "0.2.0",
                url: "http://exemple.org/x",
                signature: &signature,
                sha256: &sum,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        UpdateError::Refused(UpdateRefusal::Invalid { field: "url", .. })
    ));
    assert!(rig.downloader.fetched.lock().unwrap().is_empty());
    assert!(!rig.service.status().in_progress);
}

// ---------------------------------------------------------------------------------------------
// BR-UPDATE-028 : un travail laissé en cours est conclu au démarrage
// ---------------------------------------------------------------------------------------------

fn intent(step: UpdateStep) -> hearth_agent::domain::update::SupervisorState {
    hearth_agent::domain::update::SupervisorState {
        version: "0.2.0".into(),
        step,
        previous: "0.1.0".into(),
        requester: hearth_agent::domain::update::Requester {
            by: Some("marie".into()),
            name: Some("PC-de-marie".into()),
            addr: Some("10.0.0.7".into()),
        },
    }
}

fn orphan_job() -> hearth_agent::domain::update::Job {
    hearth_agent::domain::update::Job {
        version: "0.2.0".into(),
        previous: "0.0.9".into(),
        binary: "/usr/local/bin/hearth-agent".into(),
        staged: "/var/lib/hearth/update/hearth-agent.new".into(),
        backup: "/usr/local/bin/.hearth-agent.previous".into(),
        probe_addr: "127.0.0.1:7341".into(),
        fingerprint: "ab".repeat(32),
        grace_ms: 0,
        check_window_ms: 60_000,
        poll_ms: 1000,
        requested_by: Some("marie".into()),
        client_name: Some("PC-de-marie".into()),
        client_addr: Some("10.0.0.7".into()),
        recover: false,
    }
}

/// Le travail d'une mise à jour dont l'échange a eu lieu : la version visée est celle de l'agent
/// qui démarre (le nouvel agent), la sauvegarde de l'ancien binaire est restée.
fn swapped_job() -> hearth_agent::domain::update::Job {
    hearth_agent::domain::update::Job {
        version: CURRENT.to_string(),
        ..orphan_job()
    }
}

fn record(outcome: UpdateOutcome, reason: Option<UpdateReason>) -> UpdateRecord {
    UpdateRecord {
        version: "0.2.0".into(),
        previous: "0.1.0".into(),
        outcome,
        reason,
        at: "2026-10-05T10:00:00Z".into(),
        requested_by: Some("marie".into()),
        client_name: None,
        client_addr: None,
        reported: false,
    }
}

#[tokio::test]
async fn the_intent_is_written_as_soon_as_the_update_is_asked_and_follows_every_step() {
    let env = env().await;
    let rig = Rig::new(&env, true, true);
    let mut events = rig.feed_receiver();
    start_valid(&rig, BINARY).await.unwrap();
    let state = rig
        .host
        .with(|s| s.state.clone())
        .expect("trace d'intention");
    assert_eq!(state.step, UpdateStep::Download);
    assert_eq!(state.version, "0.2.0");
    assert_eq!(state.previous, CURRENT.to_string());
    assert_eq!(state.requester.by.as_deref(), Some("root"));
    rig.release_gate();
    next_matching(&mut events, |p| p.step == UpdateStep::Restart).await;
    assert_eq!(
        rig.host.with(|s| s.state.clone()).unwrap().step,
        UpdateStep::Restart
    );
}

#[tokio::test]
async fn an_agent_killed_while_downloading_is_concluded_as_interrupted_at_the_next_start() {
    for step in [
        UpdateStep::Download,
        UpdateStep::Verify,
        UpdateStep::Install,
    ] {
        let env = env().await;
        let rig = Rig::new(&env, true, false);
        rig.host.with(|s| s.state = Some(intent(step)));
        let mut events = rig.feed_receiver();
        rig.service.resume().await;
        let done = next_matching(&mut events, |p| p.step == UpdateStep::Done).await;
        assert_eq!(done.outcome, Some(UpdateOutcome::Failed), "{step:?}");
        assert_eq!(done.reason, Some(UpdateReason::Interrupted));
        rig.host.with(|s| {
            assert!(s.state.is_none() && s.staged.is_none(), "dépôt nettoyé");
            assert!(s.launched.is_empty(), "aucun superviseur");
            assert!(s.last.as_ref().unwrap().reported);
        });
        let last = rig.service.last().unwrap();
        assert_eq!(
            (last.outcome, last.reason),
            (UpdateOutcome::Failed, Some(UpdateReason::Interrupted))
        );
        let entries = journal(&env).await;
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].account.as_deref(),
            Some("marie"),
            "le demandeur, pas la ligne de commande"
        );
        assert_eq!(entries[0].outcome.code(), "failed");
        assert!(!rig.service.status().in_progress);
        // Un second démarrage n'écrit rien de plus.
        rig.service.resume().await;
        assert_eq!(journal(&env).await.len(), 1);
    }
}

#[tokio::test]
async fn a_supervisor_that_never_swapped_is_concluded_as_interrupted_too() {
    let env = env().await;
    let rig = Rig::new(&env, true, false);
    rig.host.with(|s| {
        s.state = Some(intent(UpdateStep::Restart));
        s.job = Some(orphan_job());
    });
    let mut events = rig.feed_receiver();
    rig.service.resume().await;
    let done = next_matching(&mut events, |p| p.step == UpdateStep::Done).await;
    assert_eq!(done.reason, Some(UpdateReason::Interrupted));
    rig.host
        .with(|s| assert!(s.job.is_none() && s.launched.is_empty()));
    assert_eq!(journal(&env).await.len(), 1);
}

#[tokio::test]
async fn a_swap_nobody_concluded_is_taken_over_by_a_recovery_supervisor() {
    let env = env().await;
    let rig = Rig::new(&env, true, false);
    rig.host.with(|s| {
        s.state = Some(intent(UpdateStep::Check));
        s.job = Some(swapped_job());
        s.existing
            .push("/usr/local/bin/.hearth-agent.previous".into());
    });
    rig.service.resume().await;
    let (launched, job) = rig
        .host
        .with(|s| (s.launched.len(), s.job.clone().unwrap()));
    assert_eq!(launched, 1, "un superviseur de reprise est lancé");
    assert!(job.recover, "travail marqué « reprise »");
    assert_eq!(job.requested_by.as_deref(), Some("marie"));
    // La mise à jour reste « en cours » jusqu'à la conclusion du superviseur.
    let status = rig.service.status();
    assert!(status.in_progress);
    assert_eq!(status.progress.unwrap().step, UpdateStep::Check);
    assert!(
        journal(&env).await.is_empty(),
        "rien n'est annoncé avant la conclusion"
    );
}

#[tokio::test]
async fn the_result_of_a_recovery_is_announced_and_journaled_once_it_is_written() {
    let env = env().await;
    let rig = Rig::new(&env, true, false);
    rig.host.with(|s| {
        s.state = Some(intent(UpdateStep::Check));
        s.job = Some(swapped_job());
        s.existing
            .push("/usr/local/bin/.hearth-agent.previous".into());
    });
    let mut events = rig.feed_receiver();
    rig.service.resume().await;
    // Le superviseur de reprise conclut : retour à l'ancien binaire.
    rig.host.with(|s| {
        s.last = Some(record(
            UpdateOutcome::RolledBack,
            Some(UpdateReason::NoAnswer),
        ));
    });
    let done = next_matching(&mut events, |p| p.step == UpdateStep::Done).await;
    assert_eq!(done.outcome, Some(UpdateOutcome::RolledBack));
    let entries = journal(&env).await;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].account.as_deref(), Some("marie"));
}

#[tokio::test]
async fn a_supervisor_that_concludes_while_the_agent_starts_is_still_announced() {
    // Le superviseur est vivant à la lecture du verrou, son résultat est écrit juste après : la
    // surveillance le rattrape (ni journal ni `done` perdus).
    let env = env().await;
    let rig = Rig::new(&env, true, false);
    rig.host
        .running
        .store(true, std::sync::atomic::Ordering::SeqCst);
    rig.host.with(|s| s.state = Some(intent(UpdateStep::Check)));
    let mut events = rig.feed_receiver();
    rig.service.resume().await;
    rig.host
        .with(|s| s.last = Some(record(UpdateOutcome::Succeeded, None)));
    rig.host
        .running
        .store(false, std::sync::atomic::Ordering::SeqCst);
    let done = next_matching(&mut events, |p| p.step == UpdateStep::Done).await;
    assert_eq!(done.outcome, Some(UpdateOutcome::Succeeded));
    assert_eq!(journal(&env).await.len(), 1);
}

#[tokio::test]
async fn a_local_or_private_address_is_refused_before_anything_is_downloaded() {
    let env = env().await;
    let rig = Rig::new(&env, true, false);
    let signature = rig.keys.sign(BINARY);
    let sum = sha256_hex(BINARY);
    let error = rig
        .service
        .start(
            by().clone(),
            UpdateInput {
                version: "0.2.0",
                url: "https://192.168.1.10/hearth-agent",
                signature: &signature,
                sha256: &sum,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        UpdateError::Refused(UpdateRefusal::Invalid { field: "url", .. })
    ));
    assert!(rig.downloader.fetched.lock().unwrap().is_empty());
}

// ---------------------------------------------------------------------------------------------
// Revue round 2 : la version qui tourne décide, illisible n'est pas absent, une seule source d'écriture
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn the_old_version_already_running_with_the_traces_left_is_concluded_without_touching_the_database()
 {
    // Reprise à la main après un `rollback_failed` : l'ancien agent tourne (0.1.0), la sauvegarde
    // de l'ancien binaire et la copie de la base (périmée) sont restées. Au redémarrage, rien ne
    // recopie la copie sur la base qui a vécu depuis.
    let env = env().await;
    let rig = Rig::new(&env, true, false);
    rig.host.with(|s| {
        s.state = Some(intent(UpdateStep::Check));
        let mut job = orphan_job();
        job.previous = CURRENT.to_string();
        s.job = Some(job);
        s.existing
            .push("/usr/local/bin/.hearth-agent.previous".into());
        s.db_copy = true;
    });
    let mut events = rig.feed_receiver();
    rig.service.resume().await;
    let done = next_matching(&mut events, |p| p.step == UpdateStep::Done).await;
    assert_eq!(done.outcome, Some(UpdateOutcome::RolledBack));
    rig.host.with(|s| {
        assert_eq!(s.db_restores, 0, "la base n'est jamais recopiée");
        assert!(!s.db_copy, "la copie périmée est retirée");
        assert!(s.launched.is_empty(), "aucun superviseur de reprise");
        assert!(
            s.removed
                .iter()
                .any(|p| p.ends_with(".hearth-agent.previous"))
        );
        assert!(s.state.is_none() && s.job.is_none());
    });
    assert_eq!(journal(&env).await.len(), 1);
    // Un démarrage de plus : plus rien à conclure.
    rig.service.resume().await;
    assert_eq!(journal(&env).await.len(), 1);
}

#[tokio::test]
async fn a_success_whose_result_was_never_written_is_concluded_as_a_success() {
    // Superviseur tué entre le nettoyage de la sauvegarde et son résultat : la nouvelle version
    // tourne, `job.json` reste, la sauvegarde n'est plus là.
    let env = env().await;
    let rig = Rig::new(&env, true, false);
    rig.host.with(|s| {
        let mut job = orphan_job();
        job.version = CURRENT.to_string();
        job.previous = "0.0.9".into();
        s.job = Some(job);
    });
    let mut events = rig.feed_receiver();
    rig.service.resume().await;
    let done = next_matching(&mut events, |p| p.step == UpdateStep::Done).await;
    assert_eq!(done.outcome, Some(UpdateOutcome::Succeeded));
    rig.host
        .with(|s| assert!(s.launched.is_empty() && s.job.is_none()));
    let entries = journal(&env).await;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].outcome.code(), "ok");
}

#[tokio::test]
async fn an_unreadable_trace_is_a_work_to_conclude_never_nothing() {
    for backup in [false, true] {
        let env = env().await;
        let rig = Rig::new(&env, true, false);
        rig.host.with(|s| {
            s.unreadable = true;
            if backup {
                s.existing
                    .push("/usr/local/bin/.hearth-agent.previous".into());
                s.db_copy = true;
            }
        });
        let mut events = rig.feed_receiver();
        rig.service.resume().await;
        let done = next_matching(&mut events, |p| p.step == UpdateStep::Done).await;
        assert_eq!(done.outcome, Some(UpdateOutcome::Failed));
        let expected = if backup {
            UpdateReason::RollbackFailed
        } else {
            UpdateReason::Interrupted
        };
        assert_eq!(done.reason, Some(expected), "backup = {backup}");
        rig.host.with(|s| {
            if backup {
                assert!(
                    s.db_copy && !s.existing.is_empty(),
                    "copies gardées pour la reprise à la main"
                );
            } else {
                assert!(!s.db_copy);
            }
        });
        assert_eq!(journal(&env).await.len(), 1);
    }
}

#[tokio::test]
async fn a_request_that_arrives_before_the_startup_resume_is_not_taken_for_an_orphan() {
    let env = env().await;
    let rig = Rig::new(&env, true, true);
    start_valid(&rig, BINARY).await.unwrap();
    // Le démarrage tarde : l'intention de la demande en cours ne doit pas être abandonnée.
    rig.service.resume().await;
    assert!(rig.service.status().in_progress);
    assert!(rig.host.with(|s| s.state.is_some() && s.cleared == 0));
    rig.release_gate();
}

#[tokio::test]
async fn the_agent_that_comes_back_relays_the_supervisor_state_without_rewriting_it() {
    let env = env().await;
    let rig = Rig::new(&env, true, false);
    rig.host
        .running
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let before = intent(UpdateStep::Check);
    rig.host.with(|s| s.state = Some(before.clone()));
    rig.service.resume().await;
    assert_eq!(
        rig.host.with(|s| s.state.clone()),
        Some(before),
        "demandeur et version d'avant intacts"
    );
    rig.host
        .running
        .store(false, std::sync::atomic::Ordering::SeqCst);
}

/// Attend (au plus 5 s) qu'une condition sur la machine simulée devienne vraie.
async fn until(rig: &Rig, condition: impl Fn(&support::update::MemState) -> bool) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !rig.host.with(|state| condition(state)) {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .expect("état attendu");
}

#[tokio::test]
async fn a_supervisor_that_dies_after_the_swap_is_taken_over_never_declared_failed() {
    // FIX:01M47PHYR8MD87HAXY9PARQXAN
    // Le superviseur travaillait (la surveillance le voit vivant), puis il meurt sans résultat
    // alors que les binaires sont déjà échangés : la patience ne dit pas « échec du superviseur »
    // (le nouvel agent est en place, l'ancien est gardé), elle suit la même règle qu'au démarrage.
    let env = env().await;
    let rig = Rig::impatient(&env);
    rig.host
        .running
        .store(true, std::sync::atomic::Ordering::SeqCst);
    rig.host.with(|s| {
        s.state = Some(intent(UpdateStep::Check));
        s.job = Some(swapped_job());
        s.existing
            .push("/usr/local/bin/.hearth-agent.previous".into());
    });
    let mut events = rig.feed_receiver();
    rig.service.resume().await;
    rig.host
        .running
        .store(false, std::sync::atomic::Ordering::SeqCst);
    until(&rig, |s| !s.launched.is_empty()).await;
    rig.host.with(|s| {
        assert!(s.job.as_ref().unwrap().recover, "superviseur de reprise");
        assert!(s.last.is_none(), "aucun échec écrit");
    });
    while let Ok(progress) = events.try_recv() {
        assert_ne!(progress.step, UpdateStep::Done, "{progress:?}");
    }
    assert!(journal(&env).await.is_empty());
}

#[tokio::test]
async fn a_supervisor_that_never_shows_up_ends_as_a_launch_failure_after_the_patience() {
    let env = env().await;
    let rig = Rig::impatient(&env);
    rig.host
        .running
        .store(true, std::sync::atomic::Ordering::SeqCst);
    rig.host
        .with(|s| s.state = Some(intent(UpdateStep::Restart)));
    let mut events = rig.feed_receiver();
    rig.service.resume().await;
    rig.host
        .running
        .store(false, std::sync::atomic::Ordering::SeqCst);
    let done = next_matching(&mut events, |p| p.step == UpdateStep::Done).await;
    assert_eq!(done.outcome, Some(UpdateOutcome::Failed));
    assert_eq!(done.reason, Some(UpdateReason::SupervisorLaunch));
    assert_eq!(journal(&env).await.len(), 1);
}

#[tokio::test]
async fn a_third_version_put_in_by_hand_is_never_rolled_back_over_nor_its_database_restored() {
    // FIX:01M47N6Z485TWN2H770KQ5H80R : la mise à jour 0.0.9 vers 0.2.0 a été laissée en cours
    // (sauvegarde de l'ancien binaire, copie de la base) ; l'administrateur a installé à la main
    // une TROISIÈME version, 0.1.0 (ni l'ancienne, ni la visée). Le démarrage ne doit ni lancer
    // une reprise (qui remettrait l'ancien binaire et la copie PÉRIMÉE de la base sur son travail)
    // ni toucher à la base : il conclut, retire les traces et rend la main.
    let env = env().await;
    let rig = Rig::new(&env, true, false);
    rig.host.with(|s| {
        s.state = Some(intent(UpdateStep::Check));
        s.job = Some(orphan_job());
        s.existing
            .push("/usr/local/bin/.hearth-agent.previous".into());
        s.db_copy = true;
    });
    let mut events = rig.feed_receiver();
    rig.service.resume().await;
    let done = next_matching(&mut events, |p| p.step == UpdateStep::Done).await;
    assert_eq!(done.outcome, Some(UpdateOutcome::Failed));
    assert_eq!(done.reason, Some(UpdateReason::Interrupted));
    rig.host.with(|s| {
        assert!(s.launched.is_empty(), "aucun superviseur de reprise");
        assert_eq!(s.db_restores, 0, "la base n'est jamais recopiée");
        assert!(!s.db_copy, "la copie périmée est retirée");
        assert!(
            s.removed
                .iter()
                .any(|p| p.ends_with(".hearth-agent.previous"))
        );
        assert!(s.state.is_none() && s.job.is_none());
    });
    assert_eq!(journal(&env).await.len(), 1);
    // Un démarrage de plus : plus rien à conclure.
    rig.service.resume().await;
    assert_eq!(journal(&env).await.len(), 1);
}

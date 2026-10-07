//! Un superviseur de mise à jour rejouable (HRT-27, BR-UPDATE-030 à 034) : tué à CHAQUE étape,
//! avant et après chaque écriture durable (échange du binaire, marqueur, copie et remise de la
//! base, retour arrière, nettoyage), puis relancé comme le fait l'unité (`Restart=on-failure`), il
//! conclut sans binaire cassé, sans base cassée et sans retour arrière rejoué. Sur de vrais
//! fichiers temporaires ; le service et l'agent qui répond sont simulés (`support::crash`).
//!
//! Ce que cette matrice prouve : l'ORDRE des écritures et qu'une étape se rejoue, avec une mort simulée dans
//! le même processus. Elle ne prouve pas la durabilité : une coupure de courant ou un `fsync` oublié ne s'y
//! voient pas, le cache du système survit à une panique.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::fs;
use std::time::Duration;

use hearth_agent::application::ports::{UpdateHost, UpdateHostError};
use hearth_agent::application::update_supervisor::{End, SuperviseError, Supervised};
use hearth_agent::domain::install::Version;
use hearth_agent::domain::update::{
    Leftovers, MAX_RESUMES, Orphan, Phase, UpdateRecord, classify_orphan,
};
use hearth_proto::api::update::{UpdateOutcome, UpdateReason};
use support::crash::{OLD, World};

const GOOD: &[u8] = b"new-good";
const MUTE: &[u8] = b"new-mute";

/// Lance le superviseur jusqu'à ce qu'il n'y ait plus de travail : chaque lancement est un
/// lancement de l'unité. Rend le dernier résultat obtenu.
fn relaunch_until_done(world: &World) -> Option<Supervised> {
    let mut last = None;
    for _ in 0..(MAX_RESUMES + 3) {
        match world.launch() {
            Ok(Some(done)) => last = Some(done.unwrap()),
            Ok(None) => return last,
            Err(()) => panic!("le superviseur est mort sans que ce soit demandé"),
        }
    }
    panic!(
        "le superviseur ne finit jamais : {:?}",
        world.update_files()
    );
}

fn last(world: &World) -> UpdateRecord {
    world.host.inner.read_last().unwrap().expect("un résultat")
}

/// Combien de points compte une exécution sans accident.
fn count_points(staged: &[u8]) -> usize {
    let world = World::new(staged);
    world.crash.arm(None);
    relaunch_until_done(&world);
    world.crash.ticks()
}

/// Un monde où le superviseur est mort à ce point, puis rien d'autre.
fn died_at(staged: &[u8], point: usize) -> World {
    let world = World::new(staged);
    world.crash.arm(Some(point));
    assert!(
        world.launch().is_err(),
        "le point {point} devait être atteint"
    );
    world.crash.arm(None);
    world
}

/// Le premier monde où le marqueur dit `phase` au moment de la mort du superviseur.
fn died_in(staged: &[u8], phase: Phase) -> World {
    for point in 0..count_points(staged) {
        let world = died_at(staged, point);
        if let Ok(Some(marker)) = world.host.inner.read_marker()
            && marker.phase == phase
        {
            return world;
        }
    }
    panic!("aucun point de mort ne laisse le marqueur à {phase:?}");
}

fn assert_sound(world: &World, label: &str) {
    let machine = world.machine.lock();
    assert!(
        !machine.broken_pair,
        "{label} : l'ancien agent a démarré sur une base migrée"
    );
    assert!(
        !machine.replayed,
        "{label} : un retour arrière a été rejoué"
    );
    assert!(!machine.swap_replayed, "{label} : l'échange a été refait");
    assert!(machine.active, "{label} : le service est à terre");
}

// ---------------------------------------------------------------------------------------------
// 1. Tué à chaque étape, relancé : le même résultat qu'une exécution sans accident
// ---------------------------------------------------------------------------------------------

#[test]
fn a_supervisor_killed_at_every_point_of_a_successful_update_concludes_the_same_way() {
    let points = count_points(GOOD);
    assert!(
        points > 20,
        "{points} points : le test ne couvre presque rien"
    );
    for point in 0..points {
        let label = format!("réussite, mort au point {point}/{points}");
        let world = died_at(GOOD, point);
        let done = relaunch_until_done(&world);
        assert_sound(&world, &label);
        assert_eq!(world.binary(), GOOD, "{label}");
        assert_eq!(world.database(), b"migree", "{label}");
        let record = last(&world);
        assert_eq!(record.outcome, UpdateOutcome::Succeeded, "{label}");
        assert_eq!(record.reason, None, "{label}");
        assert_eq!(
            world.update_files(),
            ["last.json", "lock"],
            "{label} : ni copie ni marqueur ne reste"
        );
        assert_eq!(
            fs::read_dir(world.job.binary.parent().unwrap())
                .unwrap()
                .count(),
            1,
            "{label} : la sauvegarde de l'ancien binaire est retirée"
        );
        if let Some(done) = done {
            assert_eq!(done.outcome, UpdateOutcome::Succeeded, "{label}");
        }
    }
}

#[test]
fn a_supervisor_killed_at_every_point_of_a_rollback_rolls_back_exactly_once() {
    let points = count_points(MUTE);
    assert!(points > 30, "{points} points");
    for point in 0..points {
        let label = format!("retour arrière, mort au point {point}/{points}");
        let world = died_at(MUTE, point);
        relaunch_until_done(&world);
        assert_sound(&world, &label);
        assert_eq!(world.binary(), OLD, "{label} : exactement l'ancien binaire");
        // La base d'avant la migration, puis ce que l'ancien agent y a écrit en revenant : une
        // base remise une seconde fois par-dessus aurait perdu « +vie ».
        assert_eq!(world.database(), b"avant+vie", "{label}");
        let record = last(&world);
        assert_eq!(record.outcome, UpdateOutcome::RolledBack, "{label}");
        assert_eq!(record.reason, Some(UpdateReason::NoAnswer), "{label}");
        assert_eq!(world.update_files(), ["last.json", "lock"], "{label}");
        assert_eq!(
            fs::read_dir(world.job.binary.parent().unwrap())
                .unwrap()
                .count(),
            1,
            "{label} : la sauvegarde de l'ancien binaire est consommée"
        );
    }
}

#[test]
fn a_result_already_announced_by_the_agent_is_not_written_again_by_a_resumed_supervisor() {
    // Le superviseur meurt entre l'écriture du résultat et le nettoyage ; l'agent annonce le résultat
    // (`reported`) ; la reprise ne le réécrit pas (sinon l'annonce serait faite deux fois).
    let points = count_points(GOOD);
    let mut checked = false;
    for point in 0..points {
        let world = died_at(GOOD, point);
        let Ok(Some(marker)) = world.host.inner.read_marker() else {
            continue;
        };
        let Ok(Some(mut record)) = world.host.inner.read_last() else {
            continue;
        };
        if marker.phase != Phase::Concluded {
            continue;
        }
        record.reported = true;
        world.host.inner.write_last(&record).unwrap();
        relaunch_until_done(&world);
        assert!(
            last(&world).reported,
            "point {point} : l'annonce serait refaite"
        );
        checked = true;
    }
    assert!(
        checked,
        "aucun point de mort entre le résultat et le nettoyage"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. Pas de boucle : reprises bornées, abandon journalisé une fois, plus rien ne relance
// ---------------------------------------------------------------------------------------------

#[test]
fn resumes_are_bounded_then_the_work_is_abandoned_once_and_nothing_relaunches_it() {
    let world = World::new(MUTE);
    // Mort à chaque lancement, au troisième point : le compteur de reprises monte à chaque fois.
    for launch in 0..=MAX_RESUMES {
        world.crash.arm(Some(2));
        assert!(world.launch().is_err(), "lancement {launch}");
    }
    let marker = world.host.inner.read_marker().unwrap().unwrap();
    assert_eq!(marker.resumes, MAX_RESUMES, "{marker:?}");
    assert_ne!(marker.phase, Phase::Abandoned);

    // Le lancement suivant dépasse la borne : abandon.
    world.crash.arm(None);
    let Ok(Some(Ok(done))) = world.launch() else {
        panic!("un résultat était attendu");
    };
    assert_eq!(done.end, End::Abandoned);
    assert_eq!(done.reason, Some(UpdateReason::RollbackFailed));
    let record = last(&world);
    assert_eq!(
        (record.outcome, record.reason),
        (UpdateOutcome::Failed, Some(UpdateReason::RollbackFailed))
    );
    assert!(
        !record.reported,
        "l'agent l'annoncera au journal d'activité"
    );
    let marker = world.host.inner.read_marker().unwrap().unwrap();
    assert_eq!(marker.phase, Phase::Abandoned);
    // Copies gardées pour la reprise à la main ; traces de travail retirées.
    let files = world.update_files();
    for kept in ["hearth-agent.new", "phase.json", "last.json"] {
        assert!(files.iter().any(|f| f == kept), "{kept} : {files:?}");
    }
    assert!(!files.iter().any(|f| f == "job.json" || f == "state.json"));
    assert_sound_pair(&world);

    // Plus rien ne relance : l'unité n'a plus de travail, et un travail rejoué de force ne fait rien.
    assert!(matches!(world.launch(), Ok(None)));
    let calls = world.machine.lock().calls.len();
    let forced = hearth_agent::application::update_supervisor::Supervisor {
        host: &world.host,
        install: &world.install,
        space: &support::crash::FakeSpace,
        service: &world.service,
        probe: &world.agent,
        clock: &hearth_agent::infrastructure::clock::SystemClock,
    }
    .run(&world.job)
    .unwrap();
    assert_eq!(forced.end, End::Nothing);
    assert_eq!(
        world.machine.lock().calls.len(),
        calls,
        "aucun arrêt, aucun démarrage"
    );
    assert_eq!(last(&world).at, record.at, "résultat non réécrit");
}

fn assert_sound_pair(world: &World) {
    let machine = world.machine.lock();
    assert!(!machine.broken_pair);
    assert!(machine.active, "l'abandon relance le service");
}

#[test]
fn a_new_update_starts_from_zero_even_after_an_abandoned_one() {
    let world = died_in(MUTE, Phase::Checking);
    // Marqueur abandonné de la même version.
    let mut marker = world.host.inner.read_marker().unwrap().unwrap();
    marker.phase = Phase::Abandoned;
    world.host.inner.write_marker(&marker).unwrap();
    // L'agent, à la demande suivante, retire le marqueur (`discard_marker`) avant d'écrire le travail.
    world.host.inner.discard_marker();
    assert!(world.host.inner.read_marker().unwrap().is_none());
}

// ---------------------------------------------------------------------------------------------
// 2. Un arrêt voulu n'est jamais défait
// ---------------------------------------------------------------------------------------------

#[test]
fn a_service_stopped_by_hand_during_the_check_is_never_started_again_nor_rolled_back() {
    let mut world = World::new(b"new-slow");
    world.job.check_window_ms = 2_000;
    world.host.inner.write_job(&world.job).unwrap();
    let machine = world.machine.clone();
    // `systemctl stop hearth-agent` pendant que le nouvel agent est contrôlé.
    let stopper = std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(10));
            let mut state = machine.lock();
            if state.calls.iter().filter(|c| **c == "restart").count() >= 1 {
                state.active = false;
                state.stopped_on_purpose = true;
                return;
            }
        }
    });
    let Ok(Some(Ok(done))) = world.launch() else {
        panic!("un résultat était attendu");
    };
    stopper.join().unwrap();
    assert_eq!(done.end, End::Paused);
    let state = world.machine.lock();
    assert!(!state.active, "le service reste arrêté");
    assert_eq!(
        state.calls,
        ["stop", "restart"],
        "ni retour arrière ni redémarrage"
    );
    drop(state);
    assert_eq!(world.binary(), b"new-slow", "rien n'est défait");
    assert_eq!(world.database(), b"migree");
    assert!(
        world.host.inner.read_last().unwrap().is_none(),
        "rien n'est conclu"
    );
    let files = world.update_files();
    for kept in ["hearth.db.before", "job.json", "phase.json"] {
        assert!(files.iter().any(|f| f == kept), "{kept} : {files:?}");
    }
    assert!(
        world.job.backup.exists(),
        "l'ancien binaire est gardé pour la reprise au démarrage"
    );
    // Le démarrage suivant de l'agent reprend (BR-UPDATE-028) : le travail est intact.
    let leftovers = Leftovers {
        state: world.host.inner.read_state().unwrap(),
        job: world.host.inner.read_job().unwrap(),
        backup_present: world.job.backup.exists(),
        unreadable: false,
        recovery_attempts: 0,
    };
    let orphan = classify_orphan(&leftovers, false, Version::new(0, 2, 0));
    assert!(matches!(orphan, Orphan::AfterSwap(_)), "{orphan:?}");
}

// ---------------------------------------------------------------------------------------------
// 6. Disque plein, copies absentes ou illisibles
// ---------------------------------------------------------------------------------------------

/// Le superviseur est mort pendant le contrôle ; on abîme le disque, puis on le relance.
fn damaged_then_relaunched(damage: impl FnOnce(&mut World)) -> (World, Supervised) {
    let mut world = died_in(MUTE, Phase::Checking);
    damage(&mut world);
    let Ok(Some(Ok(done))) = world.launch() else {
        panic!("un résultat était attendu");
    };
    (world, done)
}

fn assert_abandoned(world: &World, done: &Supervised) {
    assert_eq!(done.end, End::Abandoned);
    let record = last(world);
    assert_eq!(
        (record.outcome, record.reason),
        (UpdateOutcome::Failed, Some(UpdateReason::RollbackFailed))
    );
    assert_eq!(
        world.host.inner.read_marker().unwrap().unwrap().phase,
        Phase::Abandoned
    );
    assert!(!world.machine.lock().broken_pair);
    assert!(
        world.machine.lock().active,
        "le service est relancé, base et binaire cohérents"
    );
}

#[test]
fn a_full_disk_at_the_database_restore_changes_nothing_and_abandons_with_the_copies_kept() {
    let (world, done) = damaged_then_relaunched(|world| world.host.fail_restore_database = true);
    assert_abandoned(&world, &done);
    assert_eq!(
        world.database(),
        b"migree",
        "la base vivante n'a pas été touchée"
    );
    assert_eq!(
        world.binary(),
        MUTE,
        "le binaire non plus : jamais l'ancien devant la base migrée"
    );
    assert!(
        world.update_files().iter().any(|f| f == "hearth.db.before"),
        "copie gardée"
    );
    assert!(world.job.backup.exists(), "ancien binaire gardé");
}

#[test]
fn a_database_copy_that_is_gone_puts_back_neither_the_database_nor_the_binary() {
    let (world, done) = damaged_then_relaunched(|world| {
        fs::remove_file(world.dir.path().join("update").join("hearth.db.before")).unwrap();
    });
    assert_abandoned(&world, &done);
    assert_eq!(world.database(), b"migree");
    assert_eq!(world.binary(), MUTE);
    assert!(world.job.backup.exists());
}

#[test]
fn a_database_copy_that_is_not_a_file_is_a_lost_copy() {
    let (world, done) = damaged_then_relaunched(|world| {
        let copy = world.dir.path().join("update").join("hearth.db.before");
        fs::remove_file(&copy).unwrap();
        fs::create_dir(&copy).unwrap();
    });
    assert_abandoned(&world, &done);
    assert_eq!(world.binary(), MUTE);
}

#[test]
fn a_lost_binary_backup_abandons_after_the_database_is_back_and_the_pair_stays_coherent() {
    let (world, done) = damaged_then_relaunched(|world| {
        fs::remove_file(&world.job.backup).unwrap();
    });
    assert_abandoned(&world, &done);
    // La base d'avant est remise, le nouvel binaire la migre de nouveau en repartant : un couple
    // cohérent, jamais l'ancien binaire devant une base migrée.
    assert_eq!(world.binary(), MUTE);
    assert_eq!(world.database(), b"migree");
}

// ---------------------------------------------------------------------------------------------
// 1 (bis). Marqueur corrompu, à moitié écrit, d'une autre version
// ---------------------------------------------------------------------------------------------

#[test]
fn a_corrupted_marker_guesses_nothing_and_touches_nothing() {
    for garbage in [
        &b"{\"version\":\"0.2.0\",\"pha"[..],
        b"",
        b"\x00\x01\x02",
        b"[]",
    ] {
        let world = died_in(MUTE, Phase::Checking);
        fs::write(world.marker_path(), garbage).unwrap();
        let binary = world.binary();
        let database = world.database();
        let calls = world.machine.lock().calls.clone();
        let Ok(Some(Ok(done))) = world.launch() else {
            panic!("un résultat était attendu");
        };
        assert_eq!(done.end, End::Abandoned, "{garbage:?}");
        assert_eq!(world.binary(), binary, "binaire intact");
        assert_eq!(world.database(), database, "base intacte");
        assert_eq!(
            world.machine.lock().calls,
            calls,
            "ni arrêt ni démarrage du service"
        );
        let record = last(&world);
        assert_eq!(
            record.reason,
            Some(UpdateReason::RollbackFailed),
            "l'ancien binaire est gardé"
        );
        assert!(world.job.backup.exists(), "copies gardées");
        // Une seconde exécution ne dit plus rien : le marqueur est réécrit en `Abandoned`.
        assert!(matches!(world.launch(), Ok(None)));
        assert_eq!(
            world.host.inner.read_marker().unwrap().unwrap().phase,
            Phase::Abandoned
        );
    }
}

#[test]
fn a_leftover_half_written_temporary_marker_is_ignored() {
    // Un fichier voisin à moitié écrit (mort pendant l'écriture) : le marqueur valide reste le seul lu.
    let world = died_in(MUTE, Phase::Checking);
    fs::write(
        world.dir.path().join("update").join("phase.json.4242.tmp"),
        b"{\"version\":\"0.2",
    )
    .unwrap();
    relaunch_until_done(&world);
    assert_sound(&world, "fichier voisin");
    assert_eq!(world.binary(), OLD);
    assert_eq!(last(&world).outcome, UpdateOutcome::RolledBack);
}

#[test]
fn a_marker_of_another_version_is_not_this_work_and_touches_nothing() {
    let world = died_in(MUTE, Phase::Checking);
    let mut marker = world.host.inner.read_marker().unwrap().unwrap();
    marker.version = "0.1.7".into();
    world.host.inner.write_marker(&marker).unwrap();
    let binary = world.binary();
    let database = world.database();
    let Ok(Some(Ok(done))) = world.launch() else {
        panic!("un résultat était attendu");
    };
    assert_eq!(done.end, End::Abandoned);
    assert_eq!(world.binary(), binary);
    assert_eq!(world.database(), database);
}

// ---------------------------------------------------------------------------------------------
// 5. Deux superviseurs en même temps
// ---------------------------------------------------------------------------------------------

#[test]
fn a_second_supervisor_while_one_works_touches_nothing() {
    let world = World::new(GOOD);
    let held = world.host.inner.take_supervisor_lock().unwrap();
    let calls = world.machine.lock().calls.len();
    let Ok(Some(refused)) = world.launch() else {
        panic!("un résultat était attendu");
    };
    assert!(matches!(
        refused,
        Err(SuperviseError::Host(UpdateHostError::AlreadyRunning))
    ));
    assert_eq!(world.binary(), OLD);
    assert_eq!(world.machine.lock().calls.len(), calls);
    assert!(
        world.host.inner.read_marker().unwrap().is_none(),
        "aucun marqueur écrit"
    );
    drop(held);
    // Le verrou relâché, le travail se fait.
    let done = relaunch_until_done(&world).unwrap();
    assert_eq!(done.outcome, UpdateOutcome::Succeeded);
}

// ---------------------------------------------------------------------------------------------
// 4. La machine redémarre au milieu : l'unité transitoire n'existe plus, l'agent démarre
// ---------------------------------------------------------------------------------------------

/// Ce que fait l'agent qui démarre (`UpdateService::resume`), sur les vrais fichiers : il classe ce
/// qui reste d'après la version qui tourne ; une reprise relance un superviseur (`job.recover`).
fn the_machine_reboots_and_the_agent_starts(world: &World) -> String {
    // Le système redémarre : plus d'unité transitoire, plus de superviseur (le verrou est libre), le
    // service démarre avec le binaire qui est en place.
    {
        let mut machine = world.machine.lock();
        machine.active = false;
    }
    world.crash.arm(None);
    use hearth_agent::application::ports::ServiceManager;
    world.service.restart().unwrap();
    let current = if world.binary() == OLD {
        Version::new(0, 1, 0)
    } else {
        Version::new(0, 2, 0)
    };
    let leftovers = Leftovers {
        state: world.host.inner.read_state().unwrap(),
        job: world.host.inner.read_job().unwrap(),
        backup_present: world.job.backup.exists(),
        unreadable: false,
        recovery_attempts: 0,
    };
    let orphan = classify_orphan(&leftovers, false, current);
    match orphan {
        Orphan::None => "rien".into(),
        Orphan::BeforeLaunch { .. } | Orphan::LaunchedNoSwap(_) => {
            // Abandon propre : l'ancien agent tourne, rien n'a changé sur le serveur.
            world.host.inner.clear_staging();
            "interrompue".into()
        }
        Orphan::Completed { .. } | Orphan::AlreadyRolledBack(_) => {
            world.host.inner.clear_staging();
            "conclue".into()
        }
        Orphan::AfterSwap(mut job) => {
            job.recover = true;
            world.host.inner.write_job(&job).unwrap();
            relaunch_until_done(world);
            "reprise".into()
        }
        other => panic!("{other:?} ne doit pas arriver après un redémarrage"),
    }
}

#[test]
fn a_machine_rebooted_at_any_point_of_an_update_ends_with_a_coherent_agent() {
    for staged in [GOOD, MUTE] {
        let points = count_points(staged);
        for point in 0..points {
            let label = format!(
                "{} mort au point {point}/{points}",
                String::from_utf8_lossy(staged)
            );
            let world = died_at(staged, point);
            let how = the_machine_reboots_and_the_agent_starts(&world);
            assert_sound(&world, &format!("{label} ({how})"));
            // Un couple binaire et base cohérent : jamais l'ancien devant la base migrée.
            match world.binary().as_slice() {
                b"old" => assert!(
                    [&b"avant"[..], b"avant+vie"].contains(&world.database().as_slice()),
                    "{label} ({how}) : ancien binaire, base {:?}",
                    String::from_utf8_lossy(&world.database())
                ),
                _ => assert_eq!(world.database(), b"migree", "{label} ({how})"),
            }
            if how == "reprise" {
                // La reprise conclut : réussite pour un bon agent, retour arrière pour un muet.
                let record = last(&world);
                if staged == GOOD {
                    assert_eq!(record.outcome, UpdateOutcome::Succeeded, "{label}");
                } else {
                    assert_eq!(record.outcome, UpdateOutcome::RolledBack, "{label}");
                    assert_eq!(world.binary(), OLD, "{label}");
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// 3 (bis). Un abandon tué au milieu est réglé à la reprise, sans rien répéter (pré-review HRT-27)
// ---------------------------------------------------------------------------------------------

#[test]
fn an_abandon_killed_at_any_point_is_settled_by_the_next_start_with_one_result() {
    // Les points de l'abandon seul : on laisse le superviseur arriver à la borne, puis on le tue à
    // chaque point de son règlement.
    let setup = || {
        let world = died_in(MUTE, Phase::Checking);
        let mut marker = world.host.inner.read_marker().unwrap().unwrap();
        marker.resumes = MAX_RESUMES;
        world.host.inner.write_marker(&marker).unwrap();
        world
    };
    let probe = setup();
    probe.crash.arm(None);
    relaunch_until_done(&probe);
    let points = probe.crash.ticks();
    assert!(points > 5, "{points} points d'abandon");
    let reference = last(&probe);
    for point in 0..points {
        let world = setup();
        world.crash.arm(Some(point));
        assert!(world.launch().is_err(), "point {point}");
        world.crash.arm(None);
        relaunch_until_done(&world);
        let label = format!("abandon tué au point {point}/{points}");
        assert_sound(&world, &label);
        let record = last(&world);
        assert_eq!(
            (record.outcome, record.reason),
            (UpdateOutcome::Failed, Some(UpdateReason::RollbackFailed)),
            "{label}"
        );
        let marker = world.host.inner.read_marker().unwrap().unwrap();
        assert_eq!(
            (marker.phase, marker.settled),
            (Phase::Abandoned, true),
            "{label}"
        );
        assert!(world.job.backup.exists(), "{label} : copies gardées");
        assert!(world.host.inner.read_job().unwrap().is_none(), "{label}");
        let _ = &reference;
    }
}

#[test]
fn two_stops_by_hand_in_a_row_do_not_exhaust_the_resumes() {
    let mut world = World::new(b"new-slow");
    world.job.check_window_ms = 2_000;
    world.host.inner.write_job(&world.job).unwrap();
    for round in 0..(MAX_RESUMES + 2) {
        let machine = world.machine.clone();
        // Le service est de nouveau « démarré » par l'administrateur avant chaque reprise.
        if round > 0 {
            world.machine.lock().active = true;
        }
        let stopper = std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_millis(10));
                let mut state = machine.lock();
                let started =
                    round > 0 || state.calls.iter().filter(|c| **c == "restart").count() >= 1;
                if started && state.active {
                    state.active = false;
                    return;
                }
            }
        });
        let Ok(Some(Ok(done))) = world.launch() else {
            panic!("un résultat était attendu au tour {round}");
        };
        stopper.join().unwrap();
        assert_eq!(done.end, End::Paused, "tour {round}");
        let marker = world.host.inner.read_marker().unwrap().unwrap();
        assert_eq!(
            marker.resumes, 0,
            "tour {round} : un arrêt voulu ne compte pas"
        );
    }
}

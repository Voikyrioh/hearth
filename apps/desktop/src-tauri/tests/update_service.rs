//! Cas d'usage de la mise à jour du client avec des ports simulés : fréquence (une fois par jour),
//! report de 24 h, silence sans Internet, installation SUR CLIC seulement, échecs d'installation.
//! Horloge injectée : aucun test n'attend.
#![allow(clippy::unwrap_used, clippy::expect_used)] // tests : les helpers peuvent paniquer

use std::collections::VecDeque;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use hearth_desktop_lib::update::domain::{
    Candidate, DownloadPolicy, POSTPONE_MS, Release, UpdateRecord,
};
use hearth_desktop_lib::update::dto::{UpdateFailure, UpdatePhase, UpdateStateDto};
use hearth_desktop_lib::update::ports::{
    Clock, DownloadError, Feed, FeedError, StateSink, UpdateStore,
};
use hearth_desktop_lib::update::service::{InstallRefusal, UpdateService};

const HOUR: i64 = 60 * 60 * 1000;
const START: i64 = 1_800_000_000_000;
const URL: &str =
    "https://github.com/Voikyrioh/hearth/releases/download/v1.1.0/Hearth_1.1.0_x64-setup.exe";

struct FakeClock(AtomicI64);

impl FakeClock {
    fn advance(&self, ms: i64) {
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Clock for FakeClock {
    fn now_ms(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Default)]
struct MemStore {
    record: Mutex<UpdateRecord>,
    saves: AtomicUsize,
}

impl UpdateStore for MemStore {
    fn load(&self) -> UpdateRecord {
        self.record.lock().unwrap().clone()
    }

    fn save(&self, record: &UpdateRecord) -> Result<(), String> {
        *self.record.lock().unwrap() = record.clone();
        self.saves.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[derive(Default)]
struct Sink(Mutex<Vec<UpdateStateDto>>);

impl StateSink for Sink {
    fn publish(&self, state: &UpdateStateDto) {
        self.0.lock().unwrap().push(state.clone());
    }
}

/// Flux scripté : réponses à `check`, à `download` et à `install` dans l'ordre ; compte les appels.
#[derive(Default)]
struct FakeFeed {
    checks: Mutex<VecDeque<Result<Option<Candidate>, FeedError>>>,
    downloads: Mutex<VecDeque<Result<Vec<u8>, DownloadError>>>,
    install_result: Mutex<Option<DownloadError>>,
    progress_script: Mutex<Vec<(u64, Option<u64>)>>,
    check_calls: AtomicUsize,
    download_calls: AtomicUsize,
    install_calls: AtomicUsize,
}

#[async_trait]
impl Feed for FakeFeed {
    async fn check(&self) -> Result<Option<Candidate>, FeedError> {
        self.check_calls.fetch_add(1, Ordering::SeqCst);
        self.checks
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Err(FeedError("rien de prévu".into())))
    }

    async fn download(
        &self,
        _version: &str,
        progress: &mut (dyn FnMut(u64, Option<u64>) + Send),
    ) -> Result<Vec<u8>, DownloadError> {
        self.download_calls.fetch_add(1, Ordering::SeqCst);
        for (received, total) in self.progress_script.lock().unwrap().clone() {
            progress(received, total);
        }
        self.downloads
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Err(DownloadError::Failed("rien de prévu".into())))
    }

    fn install(&self, _version: &str, _bytes: Vec<u8>) -> Result<(), DownloadError> {
        self.install_calls.fetch_add(1, Ordering::SeqCst);
        match self.install_result.lock().unwrap().clone() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

fn candidate(version: &str) -> Candidate {
    Candidate {
        version: version.to_owned(),
        notes: Some("Notes de la version.".to_owned()),
        download_url: URL.to_owned(),
        signature: "c2lnbmF0dXJl".to_owned(),
    }
}

struct Rig {
    clock: Arc<FakeClock>,
    store: Arc<MemStore>,
    feed: Arc<FakeFeed>,
    sink: Arc<Sink>,
    service: UpdateService,
}

fn rig_with(record: UpdateRecord) -> Rig {
    let clock = Arc::new(FakeClock(AtomicI64::new(START)));
    let store = Arc::new(MemStore {
        record: Mutex::new(record),
        saves: AtomicUsize::new(0),
    });
    let feed = Arc::new(FakeFeed::default());
    let sink = Arc::new(Sink::default());
    let service = UpdateService::new(
        clock.clone(),
        store.clone(),
        feed.clone(),
        sink.clone(),
        DownloadPolicy::github_releases(),
        "1.0.0",
    );
    Rig {
        clock,
        store,
        feed,
        sink,
        service,
    }
}

fn rig() -> Rig {
    rig_with(UpdateRecord::default())
}

fn script_check(rig: &Rig, result: Result<Option<Candidate>, FeedError>) {
    rig.feed.checks.lock().unwrap().push_back(result);
}

fn offline() -> Result<Option<Candidate>, FeedError> {
    Err(FeedError("pas de réseau".into()))
}

// ---- BR-UPDATE-001 : au lancement, puis une fois par jour au plus -------------------------------

#[tokio::test]
async fn the_first_launch_checks_then_the_next_automatic_check_waits_a_day() {
    let rig = rig();
    script_check(&rig, Ok(None));
    script_check(&rig, Ok(None));

    rig.service.check_if_due().await;
    assert_eq!(rig.feed.check_calls.load(Ordering::SeqCst), 1);

    // Mêmes instants, 23 h plus tard : toujours rien.
    rig.service.check_if_due().await;
    rig.clock.advance(23 * HOUR);
    rig.service.check_if_due().await;
    rig.service.tick().await;
    assert_eq!(rig.feed.check_calls.load(Ordering::SeqCst), 1);

    rig.clock.advance(HOUR);
    rig.service.check_if_due().await;
    assert_eq!(rig.feed.check_calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn a_restart_within_the_day_does_not_check_again() {
    let rig = rig();
    script_check(&rig, Ok(None));
    rig.service.check_if_due().await;
    let saved = rig.store.load();

    // « Le client est relancé » : un nouveau service sur le même fichier.
    let again = rig_with(saved);
    again.clock.advance(2 * HOUR);
    again.service.check_if_due().await;
    assert_eq!(again.feed.check_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_failed_attempt_counts_it_is_not_retried_before_the_next_day() {
    let rig = rig();
    script_check(&rig, offline());
    rig.service.check_if_due().await;
    rig.service.check_if_due().await;
    rig.clock.advance(6 * HOUR);
    rig.service.tick().await;
    assert_eq!(rig.feed.check_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn the_attempt_is_written_before_the_network_call() {
    // Une application tuée pendant la vérification ne la refait pas dans la journée.
    struct Probe {
        store: Arc<MemStore>,
        seen: Mutex<Option<UpdateRecord>>,
    }
    #[async_trait]
    impl Feed for Probe {
        async fn check(&self) -> Result<Option<Candidate>, FeedError> {
            *self.seen.lock().unwrap() = Some(self.store.load());
            Ok(None)
        }
        async fn download(
            &self,
            _: &str,
            _: &mut (dyn FnMut(u64, Option<u64>) + Send),
        ) -> Result<Vec<u8>, DownloadError> {
            Err(DownloadError::NotStaged)
        }
        fn install(&self, _: &str, _: Vec<u8>) -> Result<(), DownloadError> {
            Ok(())
        }
    }
    let store = Arc::new(MemStore::default());
    let probe = Arc::new(Probe {
        store: store.clone(),
        seen: Mutex::new(None),
    });
    let service = UpdateService::new(
        Arc::new(FakeClock(AtomicI64::new(START))),
        store.clone(),
        probe.clone(),
        Arc::new(Sink::default()),
        DownloadPolicy::github_releases(),
        "1.0.0",
    );
    service.check_if_due().await;
    let seen = probe.seen.lock().unwrap().clone().unwrap();
    assert_eq!(seen.last_attempt_at, Some(START));
    assert_eq!(seen.last_success_at, None);
}

// ---- BR-UPDATE-026 : « Vérifier maintenant » ----------------------------------------------------

#[tokio::test]
async fn check_now_ignores_the_daily_limit_and_restarts_the_window() {
    let rig = rig();
    script_check(&rig, Ok(None));
    script_check(&rig, Ok(None));
    rig.service.check_if_due().await;
    rig.clock.advance(HOUR);
    let state = rig.service.check_now().await;
    assert_eq!(rig.feed.check_calls.load(Ordering::SeqCst), 2);
    assert_eq!(state.last_checked_at, Some((START + HOUR) as f64));
    assert!(state.up_to_date);

    // Le prochain automatique compte depuis la vérification manuelle.
    rig.clock.advance(23 * HOUR);
    rig.service.check_if_due().await;
    assert_eq!(rig.feed.check_calls.load(Ordering::SeqCst), 2);
    rig.clock.advance(HOUR);
    script_check(&rig, Ok(None));
    rig.service.check_if_due().await;
    assert_eq!(rig.feed.check_calls.load(Ordering::SeqCst), 3);
}

// ---- BR-UPDATE-003 : le bandeau ------------------------------------------------------------------

#[tokio::test]
async fn a_newer_release_shows_the_banner_with_its_notes() {
    let rig = rig();
    script_check(&rig, Ok(Some(candidate("1.1.0"))));
    let state = rig.service.check_if_due().await;
    assert!(state.banner_visible);
    let available = state.available.unwrap();
    assert_eq!(available.version, "1.1.0");
    assert_eq!(available.notes, "Notes de la version.");
    assert!(!state.up_to_date);
    assert_eq!(state.phase, UpdatePhase::Idle);
    // Détecter n'installe rien (BR-UPDATE-002).
    assert_eq!(rig.feed.download_calls.load(Ordering::SeqCst), 0);
    assert_eq!(rig.feed.install_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn an_up_to_date_client_shows_no_banner_and_says_so() {
    let rig = rig();
    script_check(&rig, Ok(None));
    let state = rig.service.check_if_due().await;
    assert!(!state.banner_visible);
    assert!(state.up_to_date);
    assert_eq!(state.last_checked_at, Some(START as f64));
}

#[tokio::test]
async fn an_older_or_equal_announcement_is_not_a_banner() {
    let rig = rig();
    script_check(&rig, Ok(Some(candidate("1.0.0"))));
    let state = rig.service.check_now().await;
    assert!(state.available.is_none());
    assert!(state.up_to_date);
    script_check(&rig, Ok(Some(candidate("0.9.0"))));
    assert!(rig.service.check_now().await.available.is_none());
}

#[tokio::test]
async fn an_announcement_from_elsewhere_is_ignored_silently() {
    let rig = rig();
    let mut elsewhere = candidate("1.1.0");
    elsewhere.download_url = "https://example.com/Hearth.exe".to_owned();
    script_check(&rig, Ok(Some(elsewhere)));
    let state = rig.service.check_now().await;
    assert!(state.available.is_none());
    assert!(!state.banner_visible);
    assert!(state.failure.is_none());
    assert_eq!(state.last_checked_at, None);
}

// ---- BR-UPDATE-006 : « Plus tard » ---------------------------------------------------------------

#[tokio::test]
async fn later_hides_the_banner_until_the_next_day_then_it_comes_back() {
    let rig = rig();
    script_check(&rig, Ok(Some(candidate("1.1.0"))));
    rig.service.check_if_due().await;

    let state = rig.service.postpone();
    assert!(!state.banner_visible);
    assert_eq!(state.postponed_until, Some((START + POSTPONE_MS) as f64));
    assert!(state.available.is_some());

    rig.clock.advance(23 * HOUR);
    assert!(!rig.service.state().banner_visible);

    // Le battement de l'horloge republie l'état quand le report est échu (et fait la vérification
    // du jour, qui rend ici la même version).
    script_check(&rig, Ok(Some(candidate("1.1.0"))));
    rig.clock.advance(HOUR);
    let before = rig.sink.0.lock().unwrap().len();
    rig.service.tick().await;
    assert!(rig.service.state().banner_visible);
    assert!(rig.sink.0.lock().unwrap().len() > before);
    assert!(rig.sink.0.lock().unwrap().last().unwrap().banner_visible);
}

#[tokio::test]
async fn a_postponement_survives_a_restart() {
    let rig = rig();
    script_check(&rig, Ok(Some(candidate("1.1.0"))));
    rig.service.check_if_due().await;
    rig.service.postpone();
    let again = rig_with(rig.store.load());
    again.clock.advance(HOUR);
    assert!(!again.service.state().banner_visible);
    assert!(again.service.state().available.is_some());
}

#[tokio::test]
async fn later_without_a_known_release_changes_nothing() {
    let rig = rig();
    let state = rig.service.postpone();
    assert_eq!(state.postponed_until, None);
    assert_eq!(rig.store.saves.load(Ordering::SeqCst), 0);
}

// ---- BR-UPDATE-007 et 008 : sans Internet, service muet -----------------------------------------

#[tokio::test]
async fn without_internet_the_check_fails_silently_and_keeps_the_last_success_date() {
    let rig = rig();
    script_check(&rig, Ok(None));
    rig.service.check_if_due().await;

    rig.clock.advance(3 * 24 * HOUR);
    script_check(&rig, offline());
    let published_before = rig.sink.0.lock().unwrap().len();
    let state = rig.service.check_if_due().await;

    assert_eq!(state.last_checked_at, Some(START as f64)); // « Dernière vérification : il y a 3 jours »
    assert!(state.failure.is_none());
    assert_eq!(state.phase, UpdatePhase::Idle);
    assert!(!state.banner_visible);
    assert!(!state.up_to_date); // on ne prétend pas être à jour sans réponse
    // Jamais de message d'erreur : aucun état publié ne porte d'échec.
    assert!(
        rig.sink.0.lock().unwrap()[published_before..]
            .iter()
            .all(|state| state.failure.is_none())
    );
}

#[tokio::test]
async fn a_silent_service_keeps_a_known_release_on_screen() {
    let rig = rig();
    script_check(&rig, Ok(Some(candidate("1.1.0"))));
    rig.service.check_if_due().await;
    rig.clock.advance(25 * HOUR);
    script_check(&rig, Err(FeedError("503".into())));
    let state = rig.service.check_if_due().await;
    assert!(state.available.is_some());
    assert!(state.banner_visible);
}

#[tokio::test]
async fn a_manual_check_without_internet_shows_no_error_either() {
    let rig = rig();
    script_check(&rig, offline());
    let state = rig.service.check_now().await;
    assert!(state.failure.is_none());
    assert_eq!(state.last_checked_at, None);
    assert_eq!(state.phase, UpdatePhase::Idle);
}

// ---- BR-UPDATE-002 et 004 : installation sur clic -----------------------------------------------

async fn with_known_release() -> Rig {
    let rig = rig();
    script_check(&rig, Ok(Some(candidate("1.1.0"))));
    rig.service.check_if_due().await;
    rig
}

#[tokio::test]
async fn nothing_is_installed_without_a_click() {
    let rig = with_known_release().await;
    rig.clock.advance(10 * 24 * HOUR);
    script_check(&rig, Ok(Some(candidate("1.1.0"))));
    rig.service.tick().await;
    rig.service.check_now().await;
    assert_eq!(rig.feed.download_calls.load(Ordering::SeqCst), 0);
    assert_eq!(rig.feed.install_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_click_downloads_reports_progress_then_installs() {
    let rig = with_known_release().await;
    *rig.feed.progress_script.lock().unwrap() = vec![
        (25, Some(100)),
        (25, Some(100)),
        (50, Some(100)),
        (100, Some(100)),
    ];
    rig.feed
        .downloads
        .lock()
        .unwrap()
        .push_back(Ok(vec![1, 2, 3]));

    rig.service.install().await.unwrap();

    assert_eq!(rig.feed.download_calls.load(Ordering::SeqCst), 1);
    assert_eq!(rig.feed.install_calls.load(Ordering::SeqCst), 1);
    let states = rig.sink.0.lock().unwrap().clone();
    let phases: Vec<_> = states.iter().map(|s| s.phase).collect();
    assert!(phases.contains(&UpdatePhase::Downloading));
    assert_eq!(*phases.last().unwrap(), UpdatePhase::Installing);
    let percents: Vec<_> = states.iter().filter_map(|s| s.progress).collect();
    assert_eq!(percents, [0, 25, 50, 100]); // un événement par changement, pas par morceau
    assert!(states.windows(2).all(|pair| pair[0].seq < pair[1].seq));
}

#[tokio::test]
async fn a_second_click_while_busy_is_refused_and_so_is_a_click_without_a_release() {
    let rig = with_known_release().await;
    rig.service.begin_install().unwrap();
    assert_eq!(rig.service.begin_install(), Err(InstallRefusal::Busy));
    assert_eq!(rig.service.install().await, Err(InstallRefusal::Busy));
    // Une vérification manuelle ne marche pas non plus sur un téléchargement.
    let state = rig.service.check_now().await;
    assert_eq!(state.phase, UpdatePhase::Downloading);

    let empty = rig_with(UpdateRecord::default());
    assert_eq!(
        empty.service.begin_install(),
        Err(InstallRefusal::NothingAvailable)
    );
}

#[tokio::test]
async fn a_release_from_a_previous_session_is_re_read_on_click_before_downloading() {
    // Le fichier garde l'annonce ; le port n'a plus rien en main (relance de l'application).
    let record = UpdateRecord {
        last_attempt_at: Some(START),
        last_success_at: Some(START),
        available: Some(Release {
            version: "1.1.0".to_owned(),
            notes: "n".to_owned(),
        }),
        ..UpdateRecord::default()
    };
    let rig = rig_with(record);
    rig.feed
        .downloads
        .lock()
        .unwrap()
        .push_back(Err(DownloadError::NotStaged));
    rig.feed.downloads.lock().unwrap().push_back(Ok(vec![9]));
    script_check(&rig, Ok(Some(candidate("1.1.0"))));

    rig.service.install().await.unwrap();

    assert_eq!(rig.feed.check_calls.load(Ordering::SeqCst), 1);
    assert_eq!(rig.feed.download_calls.load(Ordering::SeqCst), 2);
    assert_eq!(rig.feed.install_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn if_the_release_vanished_the_click_ends_quietly() {
    let record = UpdateRecord {
        available: Some(Release {
            version: "1.1.0".to_owned(),
            notes: String::new(),
        }),
        ..UpdateRecord::default()
    };
    let rig = rig_with(record);
    rig.feed
        .downloads
        .lock()
        .unwrap()
        .push_back(Err(DownloadError::NotStaged));
    script_check(&rig, Ok(None));
    rig.service.install().await.unwrap();
    let state = rig.service.state();
    assert!(state.available.is_none());
    assert!(state.failure.is_none());
    assert_eq!(state.phase, UpdatePhase::Idle);
    assert_eq!(rig.feed.install_calls.load(Ordering::SeqCst), 0);
}

// ---- BR-UPDATE-009 et 010 : interrompu, corrompu ------------------------------------------------

#[tokio::test]
async fn an_interrupted_download_can_be_started_again() {
    let rig = with_known_release().await;
    rig.feed
        .downloads
        .lock()
        .unwrap()
        .push_back(Err(DownloadError::Interrupted("coupure".into())));
    rig.service.install().await.unwrap();

    let state = rig.service.state();
    assert_eq!(state.failure, Some(UpdateFailure::Interrupted));
    assert_eq!(state.phase, UpdatePhase::Idle);
    assert_eq!(state.progress, None);
    assert!(state.available.is_some(), "la version reste proposée");
    assert_eq!(rig.feed.install_calls.load(Ordering::SeqCst), 0);

    // Relance : l'échec disparaît à la demande suivante et l'installation va au bout.
    rig.feed.downloads.lock().unwrap().push_back(Ok(vec![1]));
    rig.service.install().await.unwrap();
    assert_eq!(rig.feed.install_calls.load(Ordering::SeqCst), 1);
    assert_eq!(rig.service.state().failure, None);
}

#[tokio::test]
async fn a_corrupted_update_is_refused_and_never_installed() {
    let rig = with_known_release().await;
    rig.feed
        .downloads
        .lock()
        .unwrap()
        .push_back(Err(DownloadError::Corrupted("signature".into())));
    rig.service.install().await.unwrap();
    let state = rig.service.state();
    assert_eq!(state.failure, Some(UpdateFailure::Corrupted));
    assert_eq!(rig.feed.install_calls.load(Ordering::SeqCst), 0);
    assert_eq!(state.phase, UpdatePhase::Idle);
    assert_eq!(state.current_version, "1.0.0");
}

#[tokio::test]
async fn an_installer_that_cannot_start_leaves_the_current_version_usable() {
    let rig = with_known_release().await;
    rig.feed.downloads.lock().unwrap().push_back(Ok(vec![1]));
    *rig.feed.install_result.lock().unwrap() = Some(DownloadError::Failed("disque".into()));
    rig.service.install().await.unwrap();
    let state = rig.service.state();
    assert_eq!(state.failure, Some(UpdateFailure::Failed));
    assert_eq!(state.phase, UpdatePhase::Idle);
}

#[tokio::test]
async fn later_after_a_failure_clears_it_and_hides_the_banner() {
    let rig = with_known_release().await;
    rig.feed
        .downloads
        .lock()
        .unwrap()
        .push_back(Err(DownloadError::Corrupted("x".into())));
    rig.service.install().await.unwrap();
    let state = rig.service.postpone();
    assert_eq!(state.failure, None);
    assert!(!state.banner_visible);
}

// ---- BR-UPDATE-005 : après la mise à jour -------------------------------------------------------

#[tokio::test]
async fn after_an_update_the_installed_release_is_forgotten_at_startup() {
    let record = UpdateRecord {
        last_attempt_at: Some(START),
        last_success_at: Some(START),
        available: Some(Release {
            version: "1.0.0".to_owned(), // la version qui tourne
            notes: String::new(),
        }),
        ..UpdateRecord::default()
    };
    let rig = rig_with(record);
    let state = rig.service.state();
    assert!(state.available.is_none());
    assert!(!state.banner_visible);
    assert!(state.up_to_date);
}

//! Les phases d'un serveur de jeu (HRT-101, BR-FICHE-028, BR-FICHE-056) : le signe « prêt » est cherché dans les
//! seules lignes écrites depuis le lancement, comme une sous-chaîne littérale, contre le faux moteur.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use hearth_agent::application::game_phase::GamePhases;
use hearth_agent::application::ports::{Clock, EngineError};
use hearth_agent::domain::catalog::readiness::{GamePhase, ReadyRule};
use hearth_agent::domain::services::RunState;
use hearth_agent::domain::services::ServiceKey;
use hearth_proto::api::service_logs::LogSource;
use support::fake_engine::{FakeEngine, container};
use time::OffsetDateTime;

/// Une horloge que le test avance, en millisecondes depuis l'époque.
struct StepClock(AtomicI64);

impl StepClock {
    fn at(ms: i64) -> Arc<Self> {
        Arc::new(Self(AtomicI64::new(ms)))
    }

    fn advance_s(&self, seconds: i64) {
        self.0.fetch_add(seconds * 1000, Ordering::SeqCst);
    }
}

impl Clock for StepClock {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp_nanos(
            i128::from(self.0.load(Ordering::SeqCst)) * 1_000_000,
        )
        .unwrap()
    }
}

/// Un conteneur en marche chez le faux moteur, et son identifiant.
fn running(engine: &FakeEngine, name: &str) -> String {
    let observation = container(name, RunState::Running);
    let id = observation.id.clone();
    engine.put_silently(observation);
    id
}

fn key() -> ServiceKey {
    ServiceKey::parse("h_01J0ABCDEFGHJKMNPQRSTVWXYZ").unwrap()
}

fn rule(sign: Option<&str>, long: bool) -> ReadyRule {
    ReadyRule {
        sign: sign.map(str::to_owned),
        long_first_start: long,
        timeout_s: 600,
    }
}

#[tokio::test]
async fn a_game_is_ready_only_when_the_sign_appears_in_a_line_written_after_the_launch() {
    let engine = Arc::new(FakeEngine::new());
    let clock = StepClock::at(10_000);
    let phases = GamePhases::new(engine.clone(), clock.clone());
    let c1 = running(&engine, "jeu");
    phases.begin(&key(), &c1, rule(Some("Server started"), false), true);
    assert_eq!(phases.phase_of(&key()), Some(GamePhase::Starting));
    // Une ligne d'AVANT le lancement (celle d'un lancement précédent) ne rend rien prêt.
    engine.push_log(&c1, LogSource::Out, 5_000, "Server started");
    engine.push_log(&c1, LogSource::Out, 11_000, "Loading the world");
    assert_eq!(phases.refresh(&key()).await, Some(GamePhase::Starting));
    // Le signe est une sous-chaîne littérale : un motif qui lui ressemble ne compte pas.
    engine.push_log(&c1, LogSource::Out, 12_000, "Server .* started");
    assert_eq!(phases.refresh(&key()).await, Some(GamePhase::Starting));
    // Le signe passe après le lancement : prêt, à l'heure de cette ligne.
    engine.push_log(&c1, LogSource::Out, 13_000, "[12:00] Server started in 3s");
    clock.advance_s(5);
    let ready = phases.refresh(&key()).await.unwrap();
    let GamePhase::Ready { at } = ready else {
        panic!("{ready:?}");
    };
    assert_eq!(at.unix_timestamp(), 13);
    // Les lectures demandent les lignes depuis le lancement, sans suivre le journal.
    let queries = engine.log_queries();
    assert!(
        queries
            .iter()
            .all(|q| q.since_ms == Some(10_000) && !q.follow)
    );
    // Une fois prêt, plus aucune lecture : la phase est connue.
    let reads = engine.log_read_count();
    assert_eq!(phases.refresh(&key()).await, Some(ready));
    assert_eq!(engine.log_read_count(), reads);
}

#[tokio::test]
async fn an_untested_sign_reads_nothing_and_a_delay_without_the_sign_says_hearth_cannot_know() {
    let engine = Arc::new(FakeEngine::new());
    let clock = StepClock::at(0);
    let phases = GamePhases::new(engine.clone(), clock.clone());
    // Aucun signe essayé : le jeu n'est jamais « prêt », on ne lit rien.
    let c1 = running(&engine, "aveugle");
    phases.begin(&key(), &c1, rule(None, true), true);
    engine.push_log(&c1, LogSource::Out, 100, "Server started");
    assert_eq!(phases.refresh(&key()).await, Some(GamePhase::FirstStart));
    assert_eq!(engine.log_read_count(), 0);
    clock.advance_s(601);
    assert_eq!(phases.refresh(&key()).await, Some(GamePhase::Unknown));
    // Un signe essayé mais jamais vu avant le délai de la fiche : « Hearth ne peut pas savoir ».
    let other = ServiceKey::parse("h_01J0ABCDEFGHJKMNPQRSTVWXYA").unwrap();
    let c2 = running(&engine, "attend");
    phases.begin(&other, &c2, rule(Some("Done"), true), true);
    assert_eq!(phases.refresh(&other).await, Some(GamePhase::FirstStart));
    clock.advance_s(601);
    assert_eq!(phases.refresh(&other).await, Some(GamePhase::Unknown));
}

#[tokio::test]
async fn a_journal_that_cannot_be_read_changes_nothing_and_an_unfollowed_service_has_no_phase() {
    let engine = Arc::new(FakeEngine::new());
    let clock = StepClock::at(0);
    let phases = GamePhases::new(engine.clone(), clock);
    assert_eq!(phases.phase_of(&key()), None);
    assert_eq!(phases.refresh(&key()).await, None);
    let c1 = running(&engine, "muet");
    phases.begin(&key(), &c1, rule(Some("Done"), false), false);
    engine.fail_logs(&c1, Some(EngineError::Unreachable));
    assert_eq!(phases.refresh(&key()).await, Some(GamePhase::Starting));
    phases.forget(&key());
    assert_eq!(phases.phase_of(&key()), None);
}

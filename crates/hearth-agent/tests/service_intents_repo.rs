//! L'arrêt voulu depuis Hearth contre la VRAIE base SQLite (HRT-64, BR-LIFE-010, BR-SVC-005) : écrit avant
//! l'ordre d'arrêt, relu après un redémarrage de l'agent, remplacé par un second arrêt, effacé au démarrage.
//!
//! Règles : BR-LIFE-010, BR-SVC-005.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "support/tmp.rs"]
mod tmp;

use hearth_agent::application::ports::{ServiceIntents, StopIntent};
use hearth_agent::domain::services::ServiceKey;
use hearth_agent::infrastructure::sqlite::{Database, SqliteServiceRepo};
use time::{Duration, OffsetDateTime};

fn at(minutes: i64) -> OffsetDateTime {
    OffsetDateTime::UNIX_EPOCH + Duration::days(20_000) + Duration::minutes(minutes)
}

fn intent(key: &str, container: &str, account: &str, minutes: i64) -> StopIntent {
    StopIntent {
        key: ServiceKey::parse(key).unwrap(),
        container_id: container.to_owned(),
        account: account.to_owned(),
        at: at(minutes),
    }
}

#[tokio::test]
async fn a_stop_intent_survives_an_agent_restart_and_is_replaced_by_the_next_one() {
    let dir = tmp::tempdir().unwrap();
    let first = Database::open(dir.path()).await.unwrap();
    let repo = SqliteServiceRepo::new(first.pool().clone());
    assert!(repo.load().await.unwrap().is_empty());

    repo.save(&intent("c_aaa1", "aaa1", "marie", 1))
        .await
        .unwrap();
    // Un service supervisé : pas d'identifiant de conteneur.
    repo.save(&intent("u_docker-7dtd.service", "", "marie", 2))
        .await
        .unwrap();
    // Le second arrêt du même service remplace le premier (compte et date).
    repo.save(&intent("c_aaa1", "aaa1", "paul", 5))
        .await
        .unwrap();
    drop(repo);
    drop(first);

    // Le redémarrage de l'agent : une autre ouverture de la même base.
    let second = Database::open(dir.path()).await.unwrap();
    let repo = SqliteServiceRepo::new(second.pool().clone());
    let loaded = repo.load().await.unwrap();
    assert_eq!(
        loaded,
        vec![
            intent("c_aaa1", "aaa1", "paul", 5),
            intent("u_docker-7dtd.service", "", "marie", 2),
        ]
    );
}

#[tokio::test]
async fn clearing_forgets_one_service_and_only_that_one() {
    let dir = tmp::tempdir().unwrap();
    let database = Database::open(dir.path()).await.unwrap();
    let repo = SqliteServiceRepo::new(database.pool().clone());
    repo.save(&intent("c_aaa1", "aaa1", "marie", 1))
        .await
        .unwrap();
    repo.save(&intent("c_bbb2", "bbb2", "marie", 2))
        .await
        .unwrap();
    repo.clear(&ServiceKey::parse("c_aaa1").unwrap())
        .await
        .unwrap();
    // Effacer ce qui n'existe pas n'est pas une erreur.
    repo.clear(&ServiceKey::parse("c_zzz9").unwrap())
        .await
        .unwrap();
    assert_eq!(
        repo.load().await.unwrap(),
        vec![intent("c_bbb2", "bbb2", "marie", 2)]
    );
}

#[tokio::test]
async fn clearing_a_given_request_never_forgets_a_stop_asked_again_since() {
    let dir = tmp::tempdir().unwrap();
    let database = Database::open(dir.path()).await.unwrap();
    let repo = SqliteServiceRepo::new(database.pool().clone());
    let key = ServiceKey::parse("c_aaa1").unwrap();
    // Le registre a défait l'arrêt de la minute 1 ; un nouvel arrêt est écrit (minute 3) avant qu'il efface.
    repo.save(&intent("c_aaa1", "aaa1", "marie", 3))
        .await
        .unwrap();
    repo.clear_if(&key, at(1)).await.unwrap();
    assert_eq!(
        repo.load().await.unwrap(),
        vec![intent("c_aaa1", "aaa1", "marie", 3)]
    );
    // La demande elle-même s'efface, relue après un redémarrage comprise.
    let reloaded = repo.load().await.unwrap().remove(0);
    repo.clear_if(&key, reloaded.at).await.unwrap();
    assert!(repo.load().await.unwrap().is_empty());
}

#[tokio::test]
async fn an_unreadable_line_is_skipped_and_never_fatal() {
    let dir = tmp::tempdir().unwrap();
    let database = Database::open(dir.path()).await.unwrap();
    let repo = SqliteServiceRepo::new(database.pool().clone());
    repo.save(&intent("c_aaa1", "aaa1", "marie", 1))
        .await
        .unwrap();
    // Une base éditée à la main : une clé qui n'en est pas une, une date illisible.
    sqlx::query(
        "INSERT INTO service_intents (service_key, container_id, intent, account, at)
         VALUES ('pas une clé', '', 'stopped', 'x', '2026-10-13T00:00:00Z'),
                ('c_bbb2', '', 'stopped', 'x', 'hier')",
    )
    .execute(database.pool())
    .await
    .unwrap();
    assert_eq!(
        repo.load().await.unwrap(),
        vec![intent("c_aaa1", "aaa1", "marie", 1)]
    );
}

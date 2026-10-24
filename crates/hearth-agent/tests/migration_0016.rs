//! Migration `0016` (HRT-108, ADR-0051) : les réglages voulus d'un serveur de jeu et leurs secrets scellés. Migration
//! additive : deux tables neuves, rien d'existant ne bouge. La base elle-même refuse une clé qui n'est pas celle d'un
//! service créé par Hearth, un nonce rejoué, un scellé trop court ; les scellés partent avec leur ligne.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "support/tmp.rs"]
mod tmp;

use std::path::Path;

use hearth_agent::application::ports::{GameSettingsRepo, NewSettings, SettingsStoreError};
use hearth_agent::domain::catalog::settings_state::{SettingsState, Slot};
use hearth_agent::domain::sealed::SealedBlob;
use hearth_agent::domain::services::ServiceKey;
use hearth_agent::infrastructure::sqlite::SqliteGameSettingsRepo;
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use time::OffsetDateTime;

fn migrator_up_to(version: i64) -> sqlx::migrate::Migrator {
    let mut migrator = sqlx::migrate!("./migrations");
    migrator.migrations = migrator
        .migrations
        .iter()
        .filter(|migration| migration.version <= version)
        .cloned()
        .collect::<Vec<_>>()
        .into();
    migrator
}

async fn open(path: &Path) -> SqlitePool {
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(true)
                .foreign_keys(true),
        )
        .await
        .unwrap()
}

const KEY: &str = "h_01JZ0000000000000000000001";

fn state(pairs: &[(&str, Slot)]) -> SettingsState {
    SettingsState::from_slots(
        pairs
            .iter()
            .map(|(id, slot)| ((*id).to_owned(), slot.clone()))
            .collect(),
    )
}

fn blob(byte: u8) -> SealedBlob {
    SealedBlob {
        nonce: [byte; 12],
        bytes: vec![byte; 24],
    }
}

fn new(
    expected: Option<u32>,
    state: SettingsState,
    sealed: Vec<(String, SealedBlob)>,
) -> NewSettings {
    NewSettings {
        key: ServiceKey::parse(KEY).unwrap(),
        expected_rev: expected,
        state,
        file_fingerprint: Some("ab".repeat(32)),
        sealed,
        by: "ada".to_owned(),
        at: OffsetDateTime::UNIX_EPOCH + time::Duration::days(20_000),
    }
}

#[tokio::test]
async fn the_tables_are_added_empty_and_replaying_changes_nothing() {
    let dir = tmp::tempdir().unwrap();
    let pool = open(&dir.path().join("hearth.db")).await;
    migrator_up_to(15).run(&pool).await.unwrap();
    migrator_up_to(16).run(&pool).await.unwrap();
    for query in [
        "SELECT COUNT(*) FROM game_settings",
        "SELECT COUNT(*) FROM game_setting_secrets",
    ] {
        let rows: i64 = sqlx::query_scalar(query).fetch_one(&pool).await.unwrap();
        assert_eq!(rows, 0, "{query}");
    }
    migrator_up_to(16).run(&pool).await.unwrap();
}

#[tokio::test]
async fn the_base_refuses_a_foreign_key_a_replayed_nonce_and_a_scrap_of_a_seal() {
    let dir = tmp::tempdir().unwrap();
    let pool = open(&dir.path().join("hearth.db")).await;
    migrator_up_to(16).run(&pool).await.unwrap();
    let insert = |key: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query(
                "INSERT INTO game_settings (service_key, settings_rev, answers_json, updated_by, updated_at)
                 VALUES (?, 1, '[]', 'ada', '2026-10-13T10:00:00.000Z')",
            )
            .bind(key)
            .execute(&pool)
            .await
        }
    };
    assert!(
        insert("c_0123abcd").await.is_err(),
        "un conteneur hors Hearth"
    );
    assert!(insert("u_docker-x.service").await.is_err(), "une unité");
    assert!(insert(KEY).await.is_ok());
    let seal = |id: &'static str, nonce: &'static [u8], bytes: &'static [u8]| {
        let pool = pool.clone();
        async move {
            sqlx::query(
                "INSERT INTO game_setting_secrets (service_key, setting_id, nonce, sealed) VALUES (?, ?, ?, ?)",
            )
            .bind(KEY)
            .bind(id)
            .bind(nonce)
            .bind(bytes)
            .execute(&pool)
            .await
        }
    };
    assert!(
        seal("password", &[1; 12], &[9; 5]).await.is_err(),
        "scellé trop court"
    );
    assert!(
        seal("password", &[1; 11], &[9; 20]).await.is_err(),
        "nonce trop court"
    );
    assert!(seal("password", &[1; 12], &[9; 20]).await.is_ok());
    assert!(
        seal("motd", &[1; 12], &[8; 20]).await.is_err(),
        "nonce rejoué"
    );
}

#[tokio::test]
async fn the_repository_writes_numbered_revisions_keeps_the_seals_and_refuses_a_stale_write() {
    let dir = tmp::tempdir().unwrap();
    let pool = open(&dir.path().join("hearth.db")).await;
    migrator_up_to(16).run(&pool).await.unwrap();
    let repo = SqliteGameSettingsRepo::new(pool.clone());
    let key = ServiceKey::parse(KEY).unwrap();
    assert!(repo.load(&key).await.unwrap().is_none());

    let first = state(&[
        ("motd", Slot::Value("Salut".into())),
        ("password", Slot::Secret),
        ("seed", Slot::Unwritten),
    ]);
    let rev = repo
        .save(new(None, first, vec![("password".into(), blob(1))]))
        .await
        .unwrap();
    assert_eq!(rev, 1);
    let loaded = repo.load(&key).await.unwrap().unwrap();
    assert_eq!(loaded.rev, 1);
    assert_eq!(loaded.state.value("motd"), Some("Salut"));
    assert_eq!(loaded.state.slot("password"), Some(&Slot::Secret));
    assert_eq!(
        loaded.state.slot("seed"),
        None,
        "« non écrit » n'a pas de ligne"
    );
    assert_eq!(loaded.sealed.len(), 1);
    assert_eq!(loaded.sealed[0].1, blob(1));
    // Le texte de la ligne ne porte jamais une valeur de secret.
    let answers: String = sqlx::query_scalar("SELECT answers_json FROM game_settings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(answers.contains("\"s\":true") && !answers.contains("password\",\"v\""));

    // Une écriture sans le bon numéro est refusée et ne change rien.
    let stale = repo
        .save(new(
            None,
            state(&[("motd", Slot::Value("X".into()))]),
            Vec::new(),
        ))
        .await;
    assert!(matches!(
        stale,
        Err(SettingsStoreError::Conflict {
            current: Some(1),
            ..
        })
    ));
    assert_eq!(
        repo.load(&key).await.unwrap().unwrap().state.value("motd"),
        Some("Salut")
    );

    // Le secret est conservé tant qu'il reste défini, remplacé quand il change, retiré quand il ne l'est plus.
    let kept = state(&[("password", Slot::Secret)]);
    assert_eq!(
        repo.save(new(Some(1), kept.clone(), Vec::new()))
            .await
            .unwrap(),
        2
    );
    assert_eq!(repo.load(&key).await.unwrap().unwrap().sealed[0].1, blob(1));
    assert_eq!(
        repo.save(new(Some(2), kept, vec![("password".into(), blob(2))]))
            .await
            .unwrap(),
        3
    );
    assert_eq!(repo.load(&key).await.unwrap().unwrap().sealed[0].1, blob(2));
    let cleared = state(&[("password", Slot::Unwritten)]);
    assert_eq!(
        repo.save(new(Some(3), cleared, Vec::new())).await.unwrap(),
        4
    );
    assert!(repo.load(&key).await.unwrap().unwrap().sealed.is_empty());

    // Oublier un service emporte ses scellés.
    repo.save(new(
        Some(4),
        state(&[("password", Slot::Secret)]),
        vec![("password".into(), blob(3))],
    ))
    .await
    .unwrap();
    repo.forget(&key).await.unwrap();
    assert!(repo.load(&key).await.unwrap().is_none());
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM game_setting_secrets")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
}

#[tokio::test]
async fn the_settings_of_a_service_leave_with_its_description() {
    let dir = tmp::tempdir().unwrap();
    let pool = open(&dir.path().join("hearth.db")).await;
    migrator_up_to(16).run(&pool).await.unwrap();
    for rev in [1, 2] {
        sqlx::query(
            "INSERT INTO service_specs (service_key, rev, name, name_folded, spec_json, updated_by, created_at, current)
             VALUES (?, ?, 'n', 'n', '{}', 'ada', '2026-10-13T10:00:00.000Z', ?)",
        )
        .bind(KEY)
        .bind(rev)
        .bind(i64::from(rev == 2))
        .execute(&pool)
        .await
        .unwrap();
    }
    let repo = SqliteGameSettingsRepo::new(pool.clone());
    repo.save(new(
        None,
        state(&[("password", Slot::Secret)]),
        vec![("password".into(), blob(1))],
    ))
    .await
    .unwrap();
    // Effacer une ancienne révision (le conteneur précédent est supprimé) ne touche pas aux réglages.
    sqlx::query("DELETE FROM service_specs WHERE service_key = ? AND rev = 1")
        .bind(KEY)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        repo.load(&ServiceKey::parse(KEY).unwrap())
            .await
            .unwrap()
            .is_some()
    );
    // Effacer la dernière révision (suppression du service) emporte la ligne et ses scellés.
    sqlx::query("DELETE FROM service_specs WHERE service_key = ?")
        .bind(KEY)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        repo.load(&ServiceKey::parse(KEY).unwrap())
            .await
            .unwrap()
            .is_none()
    );
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM game_setting_secrets")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
}

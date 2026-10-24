//! Les réglages d'un serveur de jeu au niveau du cas d'usage (HRT-108, ADR-0051) : ce que la route ne montre pas.
//! Recréer un fichier disparu (quatrième moment d'écriture de BR-FICHE-048), deux enregistrements en même temps,
//! le secret relu depuis son scellé. Vraie base, faux dossiers de données.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;

use hearth_agent::application::game_settings::SettingsError;
use hearth_agent::application::ports::SafeFs;
use hearth_agent::domain::audit::Actor;
use hearth_agent::domain::files::RelPath;
use hearth_proto::api::game_settings::{
    ChangeAction, SaveOrigin, SaveSettingsRequest, SettingChange, SettingEffect,
};
use support::api::{DeployRig, state_with_deploy};
use support::fake_engine::FakeEngine;
use support::{Env, env};

const SENTINEL: &str = "Sentinelle-use-case-55Rt";

fn request(rev: u32, stamp: &str, changes: Vec<(&str, &str)>) -> SaveSettingsRequest {
    SaveSettingsRequest {
        base_rev: rev,
        file_stamp: stamp.to_owned(),
        origin: SaveOrigin::Form,
        effect: SettingEffect::Restart,
        changes: changes
            .into_iter()
            .map(|(id, value)| SettingChange {
                id: id.to_owned(),
                action: ChangeAction::Set,
                value: Some(value.to_owned()),
            })
            .collect(),
        others: Vec::new(),
        drift_choices: Vec::new(),
        import_text: None,
    }
}

async fn bench(env: &Env) -> (hearth_agent::entrypoint::http::AppState, DeployRig) {
    let rig = DeployRig::new();
    let update = support::update::Rig::new(env, false, false).service;
    let state = state_with_deploy(env, update, Arc::new(FakeEngine::new()), &rig).await;
    (state, rig)
}

#[tokio::test]
async fn a_vanished_file_is_recreated_from_the_wanted_values_secret_included_and_a_present_one_is_never_touched()
 {
    let env = env().await;
    let (state, rig) = bench(&env).await;
    let key = support::game::terraria_service(&env, &rig.safe_fs, 1).await;
    let actor = Actor::command_line();
    let stamp = state
        .game_settings
        .view(&key)
        .await
        .unwrap()
        .file_stamp
        .unwrap();
    state
        .game_settings
        .save(
            &key,
            &actor,
            &request(0, &stamp, vec![("motd", "Bonjour"), ("password", SENTINEL)]),
        )
        .await
        .expect("enregistré");
    // Un fichier présent n'est jamais retouché par la recréation.
    let present = support::game::file_of(&rig.safe_fs, &key);
    assert_eq!(
        state
            .game_settings
            .recreate_missing_files(&key, &actor)
            .await
            .unwrap(),
        Vec::<String>::new()
    );
    assert_eq!(support::game::file_of(&rig.safe_fs, &key), present);
    // Le jeu (ou une main) l'a effacé.
    let root = rig.safe_fs.open_service_root(&key).unwrap();
    root.remove_tree(&RelPath::parse("config/serverconfig.txt").unwrap())
        .unwrap();
    assert!(
        rig.safe_fs
            .content(&key, "config/serverconfig.txt")
            .is_none()
    );
    let recreated = state
        .game_settings
        .recreate_missing_files(&key, &actor)
        .await
        .unwrap();
    assert_eq!(recreated, vec!["config".to_owned()]);
    let text = String::from_utf8(support::game::file_of(&rig.safe_fs, &key)).unwrap();
    // Les valeurs voulues, le secret descellé pour l'occasion, les réglages imposés recalculés.
    assert!(text.contains("motd=Bonjour"), "{text}");
    assert!(text.contains(&format!("password={SENTINEL}")), "{text}");
    assert!(
        text.contains("port=7777") && text.contains("upnp=0"),
        "{text}"
    );
    // Les droits du fichier produit : root, 0600.
    assert_eq!(
        rig.safe_fs.perms_of(&key, "config/serverconfig.txt"),
        Some((0, 0, 0o600))
    );
    // La vue relit un fichier cohérent : aucun écart.
    let view = state.game_settings.view(&key).await.unwrap();
    assert!(view.drifts.is_empty(), "{:?}", view.drifts);
    assert!(!view.file_changed);
}

#[tokio::test]
async fn two_saves_with_the_same_base_revision_cannot_both_win() {
    let env = env().await;
    let (state, rig) = bench(&env).await;
    let key = support::game::terraria_service(&env, &rig.safe_fs, 1).await;
    let actor = Actor::command_line();
    let stamp = state
        .game_settings
        .view(&key)
        .await
        .unwrap()
        .file_stamp
        .unwrap();
    let (first, second) = (
        request(0, &stamp, vec![("motd", "A")]),
        request(0, &stamp, vec![("motd", "B")]),
    );
    let (a, b) = tokio::join!(
        state.game_settings.save(&key, &actor, &first),
        state.game_settings.save(&key, &actor, &second),
    );
    let outcomes = [a, b];
    let wins = outcomes.iter().filter(|o| o.is_ok()).count();
    let conflicts = outcomes
        .iter()
        .filter(|o| matches!(o, Err(SettingsError::Conflict { .. })))
        .count();
    assert_eq!((wins, conflicts), (1, 1), "{outcomes:?}");
    let file = String::from_utf8(support::game::file_of(&rig.safe_fs, &key)).unwrap();
    assert_eq!(file.matches("motd=").count(), 1, "{file}");
}

#[tokio::test]
async fn a_service_that_is_not_a_game_server_has_no_settings() {
    let env = env().await;
    let (state, _rig) = bench(&env).await;
    let key =
        hearth_agent::domain::services::ServiceKey::parse("h_01JZ0000000000000000000555").unwrap();
    assert!(matches!(
        state.game_settings.view(&key).await,
        Err(SettingsError::NotFound)
    ));
    assert!(matches!(
        state
            .game_settings
            .save(&key, &Actor::command_line(), &request(0, "x", Vec::new()))
            .await,
        Err(SettingsError::NotFound)
    ));
}

#[tokio::test]
async fn a_second_agent_that_lost_the_race_refuses_and_never_restores_the_file_of_the_winner() {
    use hearth_agent::application::game_settings::GameSettings;
    let env = env().await;
    let (state, rig) = bench(&env).await;
    let key = support::game::terraria_service(&env, &rig.safe_fs, 1).await;
    let actor = Actor::command_line();
    // Deux cas d'usage qui ne partagent pas leur exclusion (deux processus) sur la même base et les mêmes fichiers.
    let other = GameSettings::new(
        Arc::new(hearth_agent::application::catalog::CatalogService::embedded(true)),
        support::game::specs_of(&env),
        Arc::new(
            hearth_agent::infrastructure::sqlite::SqliteGameSettingsRepo::new(
                env.db.pool().clone(),
            ),
        ),
        rig.safe_fs.clone(),
        Arc::new(hearth_agent::application::service_ops::OpLocks::new()),
        env.audit_sink.clone(),
        env.clock.clone(),
    );
    let stamp = state
        .game_settings
        .view(&key)
        .await
        .unwrap()
        .file_stamp
        .unwrap();
    let (first, second) = (
        request(0, &stamp, vec![("motd", "A")]),
        request(0, &stamp, vec![("motd", "B")]),
    );
    let (a, b) = tokio::join!(
        state.game_settings.save(&key, &actor, &first),
        other.save(&key, &actor, &second),
    );
    assert!([&a, &b].iter().any(|o| o.is_ok()), "{a:?} {b:?}");
    for lost in [&a, &b].into_iter().filter(|o| o.is_err()) {
        assert!(
            matches!(
                lost,
                Err(SettingsError::FileChanged { .. } | SettingsError::Conflict { .. })
            ),
            "{lost:?}"
        );
    }
    // Le fichier porte un enregistrement abouti ou celui du perdant, jamais l'ancien contenu remis par-dessus.
    // Fichier et base concordent : aucun écart, et la valeur voulue est celle que le fichier porte.
    let file = String::from_utf8(support::game::file_of(&rig.safe_fs, &key)).unwrap();
    let view = state.game_settings.view(&key).await.unwrap();
    assert!(view.drifts.is_empty(), "{:?} {file}", view.drifts);
    let wanted = view
        .settings
        .iter()
        .find(|s| s.id == "motd")
        .and_then(|s| s.value.clone())
        .expect("motd voulu");
    assert!(
        file.contains(&format!(
            "motd={wanted}
"
        )),
        "{wanted} {file}"
    );
}

/// Un journal d'activité qui n'écrit plus rien : la révélation ne doit alors rien rendre.
struct BrokenJournal;

#[async_trait::async_trait]
impl hearth_agent::application::ports::AuditSink for BrokenJournal {
    async fn record(
        &self,
        _actor: Actor,
        _action: hearth_agent::domain::audit::AuditAction,
        _target: hearth_agent::domain::audit::Target,
        _outcome: hearth_agent::domain::audit::Outcome,
    ) {
    }

    async fn record_required(
        &self,
        _actor: Actor,
        _action: hearth_agent::domain::audit::AuditAction,
        _target: hearth_agent::domain::audit::Target,
        _outcome: hearth_agent::domain::audit::Outcome,
    ) -> Result<(), hearth_agent::application::ports::AuditWriteError> {
        Err(hearth_agent::application::ports::AuditWriteError)
    }

    async fn record_grouped_required(
        &self,
        _actor: Actor,
        _action: hearth_agent::domain::audit::AuditAction,
        _target: hearth_agent::domain::audit::Target,
        _outcome: hearth_agent::domain::audit::Outcome,
    ) -> Result<(), hearth_agent::application::ports::AuditWriteError> {
        Err(hearth_agent::application::ports::AuditWriteError)
    }
}

#[tokio::test]
async fn the_revealed_password_follows_the_saved_one_and_a_journal_that_fails_rules_the_reveal_out()
{
    use hearth_agent::application::game_settings::GameSettings;
    let env = env().await;
    let (state, rig) = bench(&env).await;
    let key = support::game::terraria_service(&env, &rig.safe_fs, 1).await;
    let actor = Actor::command_line();
    // Aucun mot de passe : toute personne qui joint le port entre (BR-JEU-027).
    assert!(
        state
            .game_settings
            .reveal_game_password(&key, actor.clone())
            .await
            .unwrap()
            .is_none()
    );
    // Enregistré, il se relit ; changé, c'est le plus récent.
    for (value, expected) in [
        (Some(SENTINEL), Some(SENTINEL)),
        (Some("Autre-mot-9"), Some("Autre-mot-9")),
    ] {
        let view = state.game_settings.view(&key).await.unwrap();
        let rev = view.rev;
        let stamp = view.file_stamp.unwrap();
        state
            .game_settings
            .save(
                &key,
                &actor,
                &request(rev, &stamp, vec![("password", value.unwrap())]),
            )
            .await
            .expect("enregistré");
        let revealed = state
            .game_settings
            .reveal_game_password(&key, actor.clone())
            .await
            .unwrap();
        assert_eq!(revealed.as_ref().map(|s| s.expose()), expected);
    }
    // Un journal d'activité qui échoue : la valeur est jetée, rien n'est rendu.
    let broken = GameSettings::new(
        Arc::new(hearth_agent::application::catalog::CatalogService::embedded(true)),
        support::game::specs_of(&env),
        Arc::new(
            hearth_agent::infrastructure::sqlite::SqliteGameSettingsRepo::new(
                env.db.pool().clone(),
            ),
        ),
        rig.safe_fs.clone(),
        Arc::new(hearth_agent::application::service_ops::OpLocks::new()),
        Arc::new(BrokenJournal),
        env.clock.clone(),
    );
    let refused = broken.reveal_game_password(&key, actor).await;
    assert!(matches!(refused, Err(SettingsError::JournalUnavailable)));
}

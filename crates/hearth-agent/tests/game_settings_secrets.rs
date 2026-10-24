//! Les secrets des réglages d'un serveur de jeu, de bout en bout contre la VRAIE base SQLite, la VRAIE clé en fichier
//! et le VRAI algorithme (HRT-108, BR-FICHE-047, BR-FICHE-053, ADR-0051, ADR-0044).
//!
//! Le test sentinelle passe par le chemin d'initialisation du journal de PRODUCTION (`logging::try_init_with`) au
//! niveau le plus bavard et balaie le contenu BRUT du fichier de base, de son journal d'écriture (`-wal`) et de son
//! index partagé (`-shm`), comme `service_spec_secrets.rs`. La valeur d'un secret n'existe qu'à un endroit : le
//! fichier du jeu (en mémoire dans ce banc, droits `0600` sur un vrai serveur).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use hearth_agent::domain::audit::Actor;
use hearth_agent::infrastructure::logging::{self, LogFormat};
use hearth_proto::api::game_settings::{
    AnalyzeRequest, ChangeAction, OtherChange, SaveOrigin, SaveSettingsRequest, SettingChange,
    SettingEffect,
};
use support::api::{DeployRig, state_with_deploy};
use support::env;
use support::fake_engine::FakeEngine;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::writer::BoxMakeWriter;

const SENTINEL_PASSWORD: &str = "SENTINEL-GAME-PASSWORD-7f3a91";
const SENTINEL_IMPORTED: &str = "SENTINEL-IMPORTED-PASSWORD-4be2";
const SENTINEL_OTHER: &str = "SENTINEL-OTHER-TOKEN-90cd";
const WITNESS: &str = "WITNESS-MOTD-visible-c41";

fn all_bytes(dir: &Path) -> Vec<u8> {
    let mut bytes = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        // La base et ses compagnons : tout ce que SQLite écrit.
        if path.is_file() && name.starts_with("hearth.db") {
            bytes.extend(std::fs::read(&path).unwrap_or_default());
        }
    }
    bytes
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

fn captured_traces() -> Arc<Mutex<Vec<u8>>> {
    #[derive(Clone)]
    struct Sink(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for Sink {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Sink {
        type Writer = Sink;
        fn make_writer(&'a self) -> Sink {
            self.clone()
        }
    }
    static TRACES: OnceLock<Arc<Mutex<Vec<u8>>>> = OnceLock::new();
    TRACES
        .get_or_init(|| {
            let buffer = Arc::new(Mutex::new(Vec::new()));
            logging::try_init_with(
                BoxMakeWriter::new(Sink(buffer.clone())),
                LogFormat::Json,
                false,
                EnvFilter::new("trace,sqlx=trace,sqlx::query=trace,hearth_agent=trace"),
            )
            .expect("première installation du journal de ce binaire de test");
            buffer
        })
        .clone()
}

#[tokio::test]
async fn no_game_password_reaches_the_database_its_wal_the_traces_or_a_debug_text() {
    let traces = captured_traces();
    let env = env().await;
    let rig = DeployRig::new();
    let update = support::update::Rig::new(&env, false, false).service;
    let state = state_with_deploy(&env, update, Arc::new(FakeEngine::new()), &rig).await;
    let key = support::game::terraria_service(&env, &rig.safe_fs, 1).await;
    let actor = Actor::command_line();
    let stamp = state
        .game_settings
        .view(&key)
        .await
        .unwrap()
        .file_stamp
        .unwrap();
    tracing::info!("witness-log-line");
    // Un enregistrement : un secret, un message d'accueil témoin, et un « autre réglage » à nom de secret.
    let request = SaveSettingsRequest {
        base_rev: 0,
        file_stamp: stamp,
        origin: SaveOrigin::Form,
        effect: SettingEffect::Restart,
        changes: vec![
            SettingChange {
                id: "password".into(),
                action: ChangeAction::Set,
                value: Some(SENTINEL_PASSWORD.into()),
            },
            SettingChange {
                id: "motd".into(),
                action: ChangeAction::Set,
                value: Some(WITNESS.into()),
            },
        ],
        others: vec![OtherChange {
            key: "mod_token".into(),
            remove: false,
            value: Some(SENTINEL_OTHER.into()),
        }],
        drift_choices: Vec::new(),
        import_text: None,
    };
    let done = state
        .game_settings
        .save(&key, &actor, &request)
        .await
        .expect("enregistré");
    // Un texte collé analysé : un mot de passe qui n'est ni gardé ni tracé.
    let analysed = state
        .game_settings
        .analyze(
            &key,
            &AnalyzeRequest {
                text: format!("password={SENTINEL_IMPORTED}\n"),
            },
        )
        .await
        .expect("analysé");
    let view = state.game_settings.view(&key).await.expect("vue");
    let shown = format!("{request:?} {done:?} {analysed:?} {view:?}");
    let sentinels = [SENTINEL_PASSWORD, SENTINEL_IMPORTED, SENTINEL_OTHER];

    // 1. Les octets BRUTS de la base, de son journal d'écriture et de son index partagé, base OUVERTE.
    let live = all_bytes(env.dir.path());
    assert!(
        contains(&live, WITNESS),
        "témoin : le balayage doit voir ce qui n'est pas secret (base et journal lus)"
    );
    for sentinel in sentinels {
        assert!(
            !contains(&live, sentinel),
            "{sentinel} en clair dans la base"
        );
        assert!(
            !contains(shown.as_bytes(), sentinel),
            "{sentinel} dans un Debug"
        );
    }
    // 2. Après un point de contrôle complet (le journal est rejoué dans le fichier de base).
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(env.db.pool())
        .await
        .unwrap();
    let checkpointed = all_bytes(env.dir.path());
    assert!(contains(&checkpointed, WITNESS));
    for sentinel in sentinels {
        assert!(
            !contains(&checkpointed, sentinel),
            "{sentinel} après point de contrôle"
        );
    }
    // 3. Aucune trace, même au niveau le plus bavard, SQL compris.
    let logged = traces.lock().unwrap().clone();
    assert!(
        contains(&logged, "witness-log-line"),
        "témoin : le suivi doit voir passer les traces"
    );
    for sentinel in sentinels {
        assert!(!contains(&logged, sentinel), "{sentinel} dans une trace");
    }
    // 4. Le mot de passe est dans le fichier du jeu, et seulement là.
    let file = support::game::file_of(&rig.safe_fs, &key);
    assert!(contains(&file, SENTINEL_PASSWORD));
    env.db.pool().close().await;
}

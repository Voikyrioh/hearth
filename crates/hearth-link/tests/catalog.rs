//! Lire le catalogue PAR LA LIAISON contre un VRAI agent (TLS 1.3, SQLite, faux moteur) : les deux lectures sont
//! ouvertes à tout compte authentifié (HRT-102, BR-FICHE-001). Un agent de publication ne propose aucune fiche
//! d'essai : la liste est vide et une fiche d'essai se lit comme une fiche inconnue.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;

use hearth_agent::application::ports::EngineInfo;
use hearth_link::{InputField, LinkError};
use hearth_proto::error::ErrorCode;
use support::agent::fake_engine::FakeEngine;
use support::{Options, World};

const GIB: u64 = 1024 * 1024 * 1024;

fn options() -> Options {
    let engine = Arc::new(FakeEngine::new());
    engine.set_info(Some(EngineInfo {
        cpus: 8,
        memory_bytes: 16 * GIB,
        storage_dir: Some("/var/lib/docker".into()),
    }));
    Options {
        device_key: true,
        engine: Some(engine),
        ..Options::default()
    }
}

#[tokio::test]
async fn a_published_agent_lists_no_trial_fiche_and_reads_one_as_unknown_for_any_account() {
    // « marie » n'a pas le droit de gérer les services : la lecture du catalogue est ouverte à tout compte.
    let world = World::connected(options()).await;
    let view = world.manager.read_catalog(&world.id).await.unwrap();
    assert!(view.fiches.is_empty());
    assert!(view.refused.is_empty());

    let unknown = world.manager.read_fiche(&world.id, "terraria").await;
    assert!(
        matches!(
            unknown,
            Err(LinkError::Rejected(Some(ErrorCode::FicheUnknown)))
        ),
        "{unknown:?}"
    );
}

#[tokio::test]
async fn an_identifier_that_is_not_one_never_leaves() {
    let world = World::connected(options()).await;
    for bad in ["", "../hello", "a/b", "A", "a b"] {
        let refused = world.manager.read_fiche(&world.id, bad).await;
        assert!(
            matches!(refused, Err(LinkError::InvalidInput(InputField::Name))),
            "{bad}: {refused:?}"
        );
    }
}

//! Lire le catalogue PAR LA LIAISON contre un VRAI agent (TLS 1.3, SQLite, faux moteur) : les deux lectures sont
//! ouvertes à tout compte authentifié (HRT-102, BR-FICHE-001). Un agent de publication LISTE ses fiches d'essai avec leur
//! raison (HRT-236) mais ne les propose pas : elles ne sont pas installables et se lisent comme une fiche inconnue.
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
async fn a_published_agent_lists_trial_fiches_not_installable_and_reads_one_as_unknown_for_any_account()
 {
    // « marie » n'a pas le droit de gérer les services : la lecture du catalogue est ouverte à tout compte.
    let world = World::connected(options()).await;
    let view = world.manager.read_catalog(&world.id).await.unwrap();
    // Minecraft (HRT-116), Terraria (HRT-226), Valheim (HRT-228) et Enshrouded (HRT-235) sont publiées ; les huit fiches
    // `preview` (7 Days to Die...) sont listées avec leur raison, non installables.
    let published: Vec<&str> = view
        .fiches
        .iter()
        .filter(|f| f.installable)
        .map(|f| f.id.as_str())
        .collect();
    assert_eq!(
        published,
        ["enshrouded", "minecraft", "terraria", "valheim"]
    );
    let waiting: Vec<_> = view.fiches.iter().filter(|f| !f.installable).collect();
    assert_eq!(waiting.len(), 8);
    assert!(
        waiting
            .iter()
            .all(|f| f.preview_reason.as_deref().is_some_and(|r| !r.is_empty()))
    );
    assert!(view.refused.is_empty());

    let unknown = world.manager.read_fiche(&world.id, "7dtd").await;
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
            matches!(refused, Err(LinkError::InvalidInput(InputField::Fiche))),
            "{bad}: {refused:?}"
        );
    }
}

#[tokio::test]
async fn a_world_read_never_leaves_with_an_identifier_that_is_not_one() {
    // HRT-106 : la liste et l'inspection d'un monde construisent leur chemin avec le même garde.
    let world = World::connected(options()).await;
    let pick = hearth_proto::api::worlds::WorldRef {
        folder: "/srv/jeux".into(),
        path: "Worlds/Alpha.wld".into(),
    };
    for bad in ["", "../hello", "a/b", "A"] {
        let listed = world.manager.read_worlds(&world.id, bad).await;
        assert!(
            matches!(listed, Err(LinkError::InvalidInput(InputField::Fiche))),
            "{bad}: {listed:?}"
        );
        let inspected = world.manager.inspect_world(&world.id, bad, &pick).await;
        assert!(
            matches!(inspected, Err(LinkError::InvalidInput(InputField::Fiche))),
            "{bad}: {inspected:?}"
        );
    }
}

// ---- HRT-240 : lire l'illustration embarquée d'une fiche par la liaison (ADR-0061, BR-FICHE-062) -------------

#[tokio::test]
async fn the_minecraft_illustration_is_read_bounded_typed_and_checked_against_its_digest() {
    let world = World::connected(options()).await;
    let view = world.manager.read_catalog(&world.id).await.unwrap();
    let card = view.fiches.iter().find(|f| f.id == "minecraft").unwrap();
    let illustration = card
        .illustration
        .as_ref()
        .expect("Minecraft porte son illustration");
    let hex = illustration.digest.strip_prefix("sha256:").unwrap();
    let bytes = world
        .manager
        .read_fiche_illustration(&world.id, "minecraft", hex)
        .await
        .unwrap();
    assert_eq!(bytes.len() as u32, illustration.bytes);
    assert_eq!(
        hearth_proto::jpeg::jpeg_dimensions(&bytes),
        Some((920, 430))
    );

    // Une empreinte autre que celle des octets rendus : refusée, jamais rendue.
    let other = "ab".repeat(32);
    let wrong = world
        .manager
        .read_fiche_illustration(&world.id, "minecraft", &other)
        .await;
    assert!(matches!(wrong, Err(LinkError::DigestMismatch)), "{wrong:?}");
}

#[tokio::test]
async fn an_unknown_fiche_or_one_without_illustration_is_a_typed_refusal() {
    let world = World::connected(options()).await;
    let hex = "00".repeat(32);
    let unknown = world
        .manager
        .read_fiche_illustration(&world.id, "nope", &hex)
        .await;
    assert!(
        matches!(
            unknown,
            Err(LinkError::Rejected(Some(ErrorCode::FicheUnknown)))
        ),
        "{unknown:?}"
    );
    // Terraria est listée mais ne porte pas d'illustration (jeu Steam).
    let absent = world
        .manager
        .read_fiche_illustration(&world.id, "terraria", &hex)
        .await;
    assert!(
        matches!(
            absent,
            Err(LinkError::Rejected(Some(ErrorCode::IllustrationAbsent)))
        ),
        "{absent:?}"
    );
}

#[tokio::test]
async fn an_identifier_or_a_digest_that_is_not_one_never_leaves() {
    let world = World::connected(options()).await;
    let hex = "00".repeat(32);
    for bad in ["", "../hello", "a/b", "A", "a b"] {
        let refused = world
            .manager
            .read_fiche_illustration(&world.id, bad, &hex)
            .await;
        assert!(
            matches!(refused, Err(LinkError::InvalidInput(InputField::Fiche))),
            "{bad}: {refused:?}"
        );
    }
    for bad in ["", "zz", &"AB".repeat(32), &"a".repeat(63)] {
        let refused = world
            .manager
            .read_fiche_illustration(&world.id, "minecraft", bad)
            .await;
        assert!(
            matches!(refused, Err(LinkError::InvalidInput(InputField::Fiche))),
            "{bad}: {refused:?}"
        );
    }
}

#[tokio::test]
async fn the_list_reads_with_and_without_the_new_members() {
    // Agent ancien : ni raison, ni mémoire, ni identifiant Steam, ni illustration (membres facultatifs).
    let old = r#"{"fiches":[{"id":"x","name":"X","summary":"s","version":1,"status":"published"}],"refused":[]}"#;
    let view: hearth_proto::api::catalog::CatalogView = serde_json::from_str(old).unwrap();
    let card = &view.fiches[0];
    assert!(card.installable && card.preview_reason.is_none() && card.memory.is_none());
    assert!(card.steam_app_id.is_none() && card.illustration.is_none());
}

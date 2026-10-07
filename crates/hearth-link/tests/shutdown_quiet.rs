//! Après `LinkManager::shutdown()`, le gestionnaire n'écrit plus rien (HRT-18, T43).
//!
//! Les dossiers « recréés » vus dans les tests venaient de bancs qui ne l'arrêtaient jamais ; ce test
//! affirme que l'arrêt, lui, est silencieux : le dossier du carnet est supprimé après l'arrêt, et
//! une seconde plus tard personne ne l'a recréé (`write_atomic` recrée le dossier parent).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::time::Duration;
use support::{Options, World};

#[tokio::test]
async fn nothing_is_written_after_shutdown() {
    let world = World::connected(Options::default()).await;
    // De l'activité à persister : une liste lue, le flux vivant.
    world.manager.accounts_list(&world.id).await.unwrap();
    assert!(world.dir.path().join("servers.json").is_file());

    world.manager.shutdown().await;
    std::fs::remove_dir_all(world.dir.path()).unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;

    assert!(
        !world.dir.path().exists(),
        "le gestionnaire a écrit après son arrêt : {:?}",
        std::fs::read_dir(world.dir.path())
            .map(|d| d.flatten().map(|e| e.file_name()).collect::<Vec<_>>())
    );
}

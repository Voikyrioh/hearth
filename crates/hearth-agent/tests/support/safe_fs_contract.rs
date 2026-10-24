#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]
//! Suite de contrat du port `SafeFs` (HRT-145) : mêmes assertions contre le faux (partout) et contre
//! l'adaptateur réel (CI Linux). Ne couvre que ce que les deux savent faire ; les pièges (liens,
//! tubes, courses, montages) ne se jouent que contre le vrai.

use std::time::Duration;

use hearth_agent::application::ports::{FsError, Ownership, Perms, SafeFs};
use hearth_agent::domain::files::RelPath;
use hearth_agent::domain::services::ServiceKey;

const BUDGET: Duration = Duration::from_secs(10);

pub fn perms(uid: u32, gid: u32, mode: u32) -> Perms {
    Perms::new(Ownership::new(uid, gid).unwrap(), mode).unwrap()
}

fn rel(text: &str) -> RelPath {
    RelPath::parse(text).unwrap()
}

pub fn run(fs: &dyn SafeFs, uid: u32, gid: u32) {
    let key = ServiceKey::parse("u_terraria").unwrap();
    let dir = perms(uid, gid, 0o750);
    let file = perms(uid, gid, 0o640);

    // Une racine absente ne s'ouvre pas sans être demandée.
    assert!(matches!(fs.open_service_root(&key), Err(FsError::NotFound)));
    let root = fs.ensure_service_root(&key).unwrap();
    assert!(fs.open_service_root(&key).is_ok());

    // Dossiers : parents créés, idempotent, jamais la racine.
    assert_eq!(root.mkdir(&rel("world/saves"), dir), Ok(true));
    assert_eq!(root.mkdir(&rel("world/saves"), dir), Ok(false));
    assert_eq!(root.mkdir(&RelPath::root(), dir), Err(FsError::Refused));

    // Écriture puis lecture bornée.
    root.write_atomic(&rel("world/server.cfg"), b"port=7777", file)
        .unwrap();
    root.write_atomic(&rel("world/server.cfg"), b"port=7778", file)
        .unwrap();
    assert_eq!(
        root.read_bounded(&rel("world/server.cfg"), 64).unwrap(),
        b"port=7778"
    );
    assert_eq!(
        root.read_bounded(&rel("world/server.cfg"), 3),
        Err(FsError::TooLarge)
    );
    assert_eq!(
        root.read_bounded(&rel("world/absent"), 64),
        Err(FsError::NotFound)
    );
    assert_eq!(
        root.write_atomic(&rel("missing/x"), b"x", file),
        Err(FsError::NotFound)
    );
    assert_eq!(
        root.write_atomic(&rel("world"), b"x", file),
        Err(FsError::NotRegular)
    );

    // Mesure : deux éléments sous `world` (saves, server.cfg), dans le budget la mesure est complète.
    let measure = root.measure(&rel("world"), BUDGET).unwrap();
    assert_eq!(measure.items, 2);
    assert!(!measure.partial);
    assert!(measure.bytes_on_disk > 0);

    // Retrait : un arbre, jamais la racine, une cible absente est dite absente.
    assert_eq!(root.remove_tree(&RelPath::root()), Err(FsError::Refused));
    root.remove_tree(&rel("world")).unwrap();
    assert_eq!(root.remove_tree(&rel("world")), Err(FsError::NotFound));
    assert_eq!(root.measure(&RelPath::root(), BUDGET).unwrap().items, 0);

    // Effacement borné (HRT-85) : un arbre part avec ses dossiers, le suivi dit combien, la racine reste ;
    // une cible absente est dite absente, jamais la racine.
    root.mkdir(&rel("data/deep"), dir).unwrap();
    root.write_atomic(&rel("data/a"), b"a", file).unwrap();
    root.write_atomic(&rel("data/deep/b"), b"b", file).unwrap();
    let mut seen = Vec::new();
    let erased = root
        .erase_tree(&rel("data"), &mut |removed| seen.push(removed))
        .unwrap();
    assert_eq!((erased.removed, erased.left), (4, 0), "{erased:?}");
    assert_eq!(erased.first_error, None);
    assert_eq!(seen.last().copied(), Some(4), "le suivi dit le total");
    assert_eq!(
        root.erase_tree(&rel("data"), &mut |_| {}).unwrap_err(),
        FsError::NotFound
    );
    assert_eq!(
        root.erase_tree(&RelPath::root(), &mut |_| {}).unwrap_err(),
        FsError::Refused
    );
    // Une racine vide se retire sans rien toucher ; pleine, elle reste.
    root.write_atomic(&rel("stay"), b"s", file).unwrap();
    assert_eq!(fs.remove_empty_service_root(&key), Ok(false));
    assert_eq!(root.read_bounded(&rel("stay"), 16).unwrap(), b"s");
    root.remove_tree(&rel("stay")).unwrap();
    let empty = ServiceKey::parse("u_empty").unwrap();
    fs.ensure_service_root(&empty).unwrap();
    assert_eq!(fs.remove_empty_service_root(&empty), Ok(true));
    assert!(matches!(
        fs.open_service_root(&empty),
        Err(FsError::NotFound)
    ));
    assert_eq!(fs.remove_empty_service_root(&empty), Ok(false));

    // Le voisin n'est pas touché par la suppression de la racine d'un service.
    let other = ServiceKey::parse("u_other").unwrap();
    let other_root = fs.ensure_service_root(&other).unwrap();
    other_root
        .write_atomic(&rel("keep"), b"mine", file)
        .unwrap();
    root.write_atomic(&rel("x"), b"x", file).unwrap();
    fs.remove_service_root(&key).unwrap();
    assert!(matches!(fs.open_service_root(&key), Err(FsError::NotFound)));
    assert_eq!(other_root.read_bounded(&rel("keep"), 16).unwrap(), b"mine");
}

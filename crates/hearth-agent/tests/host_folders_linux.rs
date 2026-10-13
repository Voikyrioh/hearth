//! Les dossiers refusés au montage sur un vrai système Linux (HRT-75, BR-DEPLOY-012) : de vrais liens
//! symboliques, un vrai fichier de comptes relu à chaque vérification. Vide hors Linux : le poste de
//! développement Windows ne l'exécute pas, la CI Linux oui.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::os::unix::fs::symlink;
use std::path::Path;

use hearth_agent::application::folders::{FolderFacts, check_mount_folder};
use hearth_agent::domain::services::FolderRefusal;
use hearth_agent::infrastructure::host::folders::SystemFolders;
use support::tmp;

fn facts(root: &Path) -> FolderFacts {
    FolderFacts::new(
        "/var/run/docker.sock",
        "/var/lib/docker",
        root.join("hearth-data").to_string_lossy().into_owned(),
        vec![root.join("agent").to_string_lossy().into_owned()],
    )
    .expect("faits complets")
}

fn judge(host: &SystemFolders, root: &Path, raw: &Path) -> Result<String, FolderRefusal> {
    check_mount_folder(host, &facts(root), &raw.to_string_lossy())
        .map(|path| path.as_str().to_owned())
}

#[test]
fn real_links_detours_and_accounts_are_judged_on_the_real_file_system() {
    let dir = tmp::tempdir().expect("dossier temporaire");
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let alice = root.join("home/alice");
    std::fs::create_dir_all(alice.join(".ssh")).unwrap();
    std::fs::create_dir_all(alice.join("games")).unwrap();
    std::fs::create_dir_all(root.join("srv/data")).unwrap();
    let passwd = root.join("passwd");
    std::fs::write(
        &passwd,
        format!(
            "alice:x:1000:1000::{}:/bin/sh\nghost:x:1:1:::/bin/false\n",
            alice.display()
        ),
    )
    .unwrap();
    let host = SystemFolders::with_passwd(passwd.to_string_lossy().into_owned());

    // Le dossier de compilation ne doit pas être lui-même sous un dossier refusé par la liste fermée du
    // système (`/tmp`, `/root`, `/usr`...) : sinon rien de ce qu'on crée ici ne peut passer. Le test
    // s'arrête alors proprement, il ne dépend pas de l'emplacement des sources.
    if judge(&host, &root, &root.join("srv/data")).is_err() {
        eprintln!("test ignoré : le dossier temporaire est sous un dossier refusé par le système");
        return;
    }

    // un dossier sans rapport passe
    assert!(judge(&host, &root, &root.join("srv/data")).is_ok());
    assert!(judge(&host, &root, &alice.join("games")).is_ok());

    // le .ssh d'un compte, son parent, le détour par « .. »
    for refused in [
        alice.join(".ssh"),
        alice.clone(),
        root.join("home"),
        alice.join("games/../.ssh"),
    ] {
        assert_eq!(
            judge(&host, &root, &refused),
            Err(FolderRefusal::Protected),
            "{refused:?}"
        );
    }

    // un vrai lien symbolique qui mène à /usr, et un qui mène au .ssh
    symlink("/usr", root.join("srv/data/escape")).unwrap();
    symlink(alice.join(".ssh"), root.join("srv/data/key")).unwrap();
    symlink("../../home", root.join("srv/data/relative")).unwrap();
    for refused in [
        root.join("srv/data/escape"),
        root.join("srv/data/escape/lib"),
        root.join("srv/data/key"),
        root.join("srv/data/relative"),
        root.join("srv/data/escape/.."),
    ] {
        assert_eq!(
            judge(&host, &root, &refused),
            Err(FolderRefusal::Protected),
            "{refused:?}"
        );
    }

    // un compte ajouté après coup est couvert à la vérification suivante
    let carol = root.join("srv/data");
    assert!(judge(&host, &root, &carol).is_ok());
    std::fs::write(
        &passwd,
        format!(
            "alice:x:1000:1000::{}:/bin/sh\ncarol:x:1001:1001::{}:/bin/sh\n",
            alice.display(),
            carol.display()
        ),
    )
    .unwrap();
    assert_eq!(judge(&host, &root, &carol), Err(FolderRefusal::Protected));

    // un fichier de comptes illisible : tout est refusé
    let blind = SystemFolders::with_passwd(root.join("absent").to_string_lossy().into_owned());
    assert_eq!(
        judge(&blind, &root, &root.join("srv/data/other")),
        Err(FolderRefusal::Protected)
    );
}

#[test]
fn the_real_system_folders_are_refused_through_the_real_links_of_the_machine() {
    // /bin, /lib, /sbin sont des liens vers /usr sur la plupart des distributions récentes ; /var/run vers /run
    let host = SystemFolders::new();
    let root = Path::new("/nonexistent-hearth-test");
    for raw in [
        "/usr",
        "/bin",
        "/sbin",
        "/lib",
        "/etc",
        "/var/run",
        "/run",
        "/proc/self",
        "/",
    ] {
        assert_eq!(
            judge(&host, root, Path::new(raw)),
            Err(FolderRefusal::Protected),
            "{raw}"
        );
    }
}

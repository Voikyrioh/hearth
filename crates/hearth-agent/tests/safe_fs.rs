//! `SafeFs` (HRT-145, ADR-0046) : la suite de contrat contre le faux (partout) et contre l'adaptateur réel
//! (CI Linux seulement ; sous Windows, le réel refuse tout geste, `Unsupported`). Les pièges (lien planté
//! vers l'extérieur, course, tube, montage) sont dans `src/infrastructure/fs/tests_unix.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "support/fake_safe_fs.rs"]
mod fake_safe_fs;
#[path = "support/safe_fs_contract.rs"]
mod safe_fs_contract;
#[path = "support/tmp.rs"]
mod tmp;

#[test]
fn the_fake_honours_the_contract() {
    safe_fs_contract::run(&fake_safe_fs::FakeSafeFs::new(), 0, 0);
}

#[cfg(unix)]
#[test]
fn the_real_adapter_honours_the_contract() {
    use std::os::unix::fs::PermissionsExt;

    use hearth_agent::infrastructure::fs::SystemSafeFs;

    let dir = tmp::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir(&data).unwrap();
    std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o700)).unwrap();
    let fs = SystemSafeFs::new(data);
    safe_fs_contract::run(
        &fs,
        rustix::process::geteuid().as_raw(),
        rustix::process::getegid().as_raw(),
    );
}

#[cfg(not(unix))]
#[test]
fn the_real_adapter_refuses_everything_off_unix() {
    use hearth_agent::application::ports::{FsError, SafeFs};
    use hearth_agent::domain::services::ServiceKey;
    use hearth_agent::infrastructure::fs::SystemSafeFs;

    let dir = tmp::tempdir().unwrap();
    let fs = SystemSafeFs::new(dir.path().to_path_buf());
    let key = ServiceKey::parse("u_terraria").unwrap();
    assert!(matches!(
        fs.open_service_root(&key),
        Err(FsError::Unsupported)
    ));
    assert!(matches!(
        fs.ensure_service_root(&key),
        Err(FsError::Unsupported)
    ));
    assert!(matches!(
        fs.remove_service_root(&key),
        Err(FsError::Unsupported)
    ));
}

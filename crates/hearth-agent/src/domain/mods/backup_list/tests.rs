use super::*;

fn sha(n: u8) -> String {
    format!("{n:02x}").repeat(32)
}

fn identity(id: &str, version: &str) -> IdentitySummary {
    IdentitySummary {
        mod_id: id.to_owned(),
        name: Some(id.to_owned()),
        version: Some(version.to_owned()),
        loader: JarLoader::Fabric,
        environment: Environment::Both,
    }
}

fn seen(name: &str, retired: bool, n: u8, id: Option<&str>) -> SnapshotFile {
    SnapshotFile {
        name: name.to_owned(),
        retired,
        size: 10,
        sha256: Some(sha(n)),
        identity: id.map(|id| identity(id, "1.0.0")),
        origin: Origin::Hearth,
        unread: false,
    }
}

fn listed(path: &str, n: u8) -> ManifestFile {
    ManifestFile {
        path: path.to_owned(),
        size: 10,
        sha256: sha(n),
    }
}

fn snapshot(files: Vec<SnapshotFile>) -> Snapshot {
    Snapshot {
        loader: Some(JarLoader::Fabric),
        minecraft: Some("1.20.1".into()),
        files,
    }
}

#[test]
fn an_entry_keeps_identity_and_origin_only_when_path_and_hash_match_the_manifest() {
    let snap = snapshot(vec![
        seen("a.jar", false, 1, Some("a")),
        seen("changed.jar", false, 2, Some("changed")),
    ]);
    let manifest = [
        listed("mods/a.jar", 1),
        // Le fichier a change entre la lecture et la copie : l'empreinte du manifeste est autre.
        listed("mods/changed.jar", 9),
        listed("mods/off.jar.disabled", 3),
        listed("mods-retires/old.jar", 4),
        listed("config/x.toml", 5),
        listed("mods/sub/deep.jar", 6),
        listed("mods/readme.txt", 7),
    ];
    let set = reconcile(&snap, &manifest, "mods", "mods-retires");
    assert_eq!(set.state, SetState::Recorded);
    assert_eq!(set.minecraft.as_deref(), Some("1.20.1"));
    assert_eq!(
        (set.active, set.disabled, set.retired, set.not_listed),
        (2, 1, 1, 0)
    );
    assert_eq!(set.entries.len(), 4, "seuls les jars des deux dossiers");
    assert_eq!(set.entries[0].identity.as_ref().unwrap().mod_id, "a");
    assert_eq!(set.entries[0].origin, Origin::Hearth);
    assert!(set.entries[1].identity.is_none(), "empreinte differente");
    assert_eq!(set.entries[1].origin, Origin::Manual);
    assert_eq!(set.entries[2].place, Place::Disabled);
    assert_eq!(set.entries[3].place, Place::Retired);
    assert_eq!(
        set.entries.iter().map(|e| e.rank).collect::<Vec<_>>(),
        [0, 1, 2, 3]
    );
}

#[test]
fn a_file_that_vanished_during_the_copy_is_absent_and_an_unread_one_makes_the_list_partial() {
    let mut unread = seen("slow.jar", false, 1, None);
    unread.sha256 = None;
    unread.unread = true;
    let snap = snapshot(vec![seen("gone.jar", false, 2, Some("gone")), unread]);
    let set = reconcile(&snap, &[listed("mods/slow.jar", 1)], "mods", "mods-retires");
    assert_eq!(set.state, SetState::Partial);
    assert_eq!(set.entries.len(), 1);
    assert!(set.entries[0].identity.is_none());
}

#[test]
fn an_unsafe_name_is_kept_without_its_name_and_the_folder_is_capped() {
    let snap = snapshot(vec![]);
    let mut manifest = vec![listed("mods/bad\u{202e}.jar", 1)];
    for n in 0..(MAX_PER_FOLDER + 3) {
        manifest.push(ManifestFile {
            path: format!("mods/m{n}.jar"),
            size: 1,
            sha256: sha(2),
        });
    }
    let set = reconcile(&snap, &manifest, "mods", "mods-retires");
    assert!(set.entries[0].file_name.is_none());
    assert_eq!(set.entries.len(), MAX_PER_FOLDER);
    assert_eq!(set.not_listed, 4);
}

#[test]
fn the_sentinel_states_have_no_entry() {
    assert_eq!(RecordedSet::unreadable().state, SetState::Unreadable);
    assert!(RecordedSet::not_in_backup().entries.is_empty());
}

fn entry(rank: u32, place: Place, id: Option<(&str, &str)>, file: &str) -> Entry {
    Entry {
        rank,
        file_name: Some(file.to_owned()),
        place,
        identity: id.map(|(id, version)| identity(id, version)),
        origin: Origin::Manual,
        size: 1,
        sha256: sha(1),
    }
}

fn current(place: Place, id: Option<(&str, &str)>, file: &str) -> CurrentMod {
    CurrentMod {
        file_name: Some(file.to_owned()),
        place,
        identity: id.map(|(id, version)| identity(id, version)),
    }
}

fn recorded(entries: Vec<Entry>) -> RecordedSet {
    RecordedSet {
        entries,
        ..RecordedSet::bare(SetState::Recorded)
    }
}

#[test]
fn the_comparison_names_added_removed_changed_enabled_and_disabled_mods() {
    let backup = recorded(vec![
        entry(0, Place::Active, Some(("same", "1.0.0")), "same-1.jar"),
        entry(1, Place::Active, Some(("gone", "1.0.0")), "gone-1.jar"),
        entry(2, Place::Active, Some(("moved", "1.0.0")), "moved-1.jar"),
        entry(
            3,
            Place::Active,
            Some(("lithium", "0.11.1")),
            "lithium-0.11.1.jar",
        ),
        entry(4, Place::Active, Some(("wasoff", "1.0.0")), "wasoff.jar"),
        entry(
            5,
            Place::Disabled,
            Some(("wason", "1.0.0")),
            "wason.jar.disabled",
        ),
        entry(6, Place::Retired, Some(("retired", "1.0.0")), "retired.jar"),
        entry(7, Place::Active, None, "mystery.jar"),
    ]);
    let now = [
        current(Place::Active, Some(("same", "1.0.0")), "same-1.jar"),
        // Mis dans « Mods retires » depuis : compte comme retire.
        current(Place::Retired, Some(("moved", "1.0.0")), "moved-1.jar"),
        current(
            Place::Active,
            Some(("lithium", "0.11.2")),
            "lithium-0.11.2.jar",
        ),
        current(
            Place::Disabled,
            Some(("wasoff", "1.0.0")),
            "wasoff.jar.disabled",
        ),
        current(Place::Active, Some(("wason", "1.0.0")), "wason.jar"),
        current(Place::Active, Some(("fresh", "3.0.0")), "fresh-3.jar"),
        current(Place::Active, Some(("retired", "1.0.0")), "retired.jar"),
        current(Place::Disabled, None, "mystery.jar.disabled"),
    ];
    let changes = compare(&backup, &now);
    let kinds = |kind: ChangeKind| -> Vec<Option<String>> {
        changes
            .iter()
            .filter(|c| c.kind == kind)
            .map(|c| c.mod_id.clone().or_else(|| c.file_name.clone()))
            .collect()
    };
    assert_eq!(
        kinds(ChangeKind::Removed),
        [Some("gone".into()), Some("moved".into())]
    );
    assert_eq!(kinds(ChangeKind::VersionChanged), [Some("lithium".into())]);
    // Desactive depuis : le fichier sans identite se reconnait par son nom sans `.disabled`.
    assert_eq!(
        kinds(ChangeKind::Disabled),
        [Some("wasoff".into()), Some("mystery.jar.disabled".into())]
    );
    assert_eq!(kinds(ChangeKind::Enabled), [Some("wason".into())]);
    assert_eq!(
        kinds(ChangeKind::Added),
        [Some("fresh".into()), Some("retired".into())]
    );
    let lithium = changes
        .iter()
        .find(|c| c.mod_id.as_deref() == Some("lithium"))
        .unwrap();
    assert_eq!(lithium.backup_version.as_deref(), Some("0.11.1"));
    assert_eq!(lithium.current_version.as_deref(), Some("0.11.2"));
}

#[test]
fn identical_lists_have_no_change() {
    let backup = recorded(vec![entry(0, Place::Active, Some(("a", "1.0.0")), "a.jar")]);
    let now = [current(Place::Active, Some(("a", "1.0.0")), "a.jar")];
    assert!(compare(&backup, &now).is_empty());
}

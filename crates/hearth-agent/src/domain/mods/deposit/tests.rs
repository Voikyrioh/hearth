use super::*;
use crate::domain::mods::jar::{Dependency, Environment, ModId, ModVersion};
use crate::domain::mods::version_req::{GameVersion, VersionReq};

fn identity(
    loader: JarLoader,
    id: &str,
    version: &str,
    mc: Option<&str>,
    deps: &[&str],
) -> JarIdentity {
    JarIdentity {
        loader,
        id: ModId::parse(id).unwrap(),
        name: None,
        version: ModVersion::parse(version),
        minecraft: mc.and_then(|range| VersionReq::parse_fabric(&[range])),
        minecraft_declared: mc.is_some(),
        depends: deps
            .iter()
            .map(|dep| Dependency {
                id: ModId::parse(dep).unwrap(),
                range_understood: true,
            })
            .collect(),
        provides: Vec::new(),
        environment: Environment::Both,
    }
}

fn fabric(id: &str, version: &str) -> JarIdentity {
    identity(JarLoader::Fabric, id, version, Some("~1.20.1"), &[])
}

fn deposit(identities: Vec<JarIdentity>) -> Deposit {
    Deposit {
        archive_refused: false,
        too_large: false,
        identities,
    }
}

fn installed(file: &str, enabled: bool, identity: JarIdentity) -> Installed {
    Installed {
        file: file.to_owned(),
        enabled,
        identities: vec![identity],
        from_modrinth_pack: false,
    }
}

fn profile() -> ServerProfile {
    ServerProfile {
        loader: Some(JarLoader::Fabric),
        minecraft: GameVersion::parse("1.20.1"),
    }
}

fn refusal(judgement: &Judgement) -> &'static str {
    judgement
        .as_ref()
        .err()
        .map(|(r, _)| r.code())
        .expect("refusé")
}

#[test]
fn a_new_mod_is_added_under_the_name_hearth_chooses() {
    let out = judge_deposits(
        &profile(),
        &[],
        &[deposit(vec![fabric("lithium", "0.11.2")])],
    );
    let accepted = out[0].as_ref().expect("accepté");
    assert_eq!(accepted.target_file, "lithium-0.11.2.jar");
    assert!(accepted.replaces.is_none());
    assert_eq!(accepted.verdict, Verdict::Compatible);
}

#[test]
fn a_newer_version_replaces_the_installed_one_even_when_that_one_is_disabled() {
    let old = [installed(
        "lithium-0.11.1.jar",
        true,
        fabric("lithium", "0.11.1"),
    )];
    let out = judge_deposits(
        &profile(),
        &old,
        &[deposit(vec![fabric("lithium", "0.11.2")])],
    );
    let accepted = out[0].as_ref().expect("accepté");
    assert_eq!(
        accepted.replaces,
        Some(("lithium-0.11.1.jar".to_owned(), Some("0.11.1".to_owned())))
    );
    let off = [installed(
        "lithium-0.11.1.jar.disabled",
        false,
        fabric("lithium", "0.11.1"),
    )];
    let out = judge_deposits(
        &profile(),
        &off,
        &[deposit(vec![fabric("lithium", "0.11.2")])],
    );
    assert_eq!(
        out[0]
            .as_ref()
            .unwrap()
            .replaces
            .as_ref()
            .map(|r| r.0.as_str()),
        Some("lithium-0.11.1.jar.disabled")
    );
}

#[test]
fn the_same_version_is_refused_and_a_taken_name_is_refused() {
    let have = [installed(
        "lithium-0.11.2.jar",
        true,
        fabric("lithium", "0.11.2"),
    )];
    let out = judge_deposits(
        &profile(),
        &have,
        &[deposit(vec![fabric("lithium", "0.11.2")])],
    );
    assert_eq!(refusal(&out[0]), "MOD_SAME_VERSION");
    // Un autre mod occupe déjà le nom de pose.
    let other = [installed(
        "lithium-0.11.2.jar",
        true,
        fabric("other", "1.0.0"),
    )];
    let out = judge_deposits(
        &profile(),
        &other,
        &[deposit(vec![fabric("lithium", "0.11.2")])],
    );
    assert_eq!(refusal(&out[0]), "MOD_NAME_TAKEN");
    // Un fichier désactivé de ce nom compte aussi.
    let off = [installed(
        "lithium-0.11.2.jar.disabled",
        false,
        fabric("other", "1.0.0"),
    )];
    let out = judge_deposits(
        &profile(),
        &off,
        &[deposit(vec![fabric("lithium", "0.11.2")])],
    );
    assert_eq!(refusal(&out[0]), "MOD_NAME_TAKEN");
}

#[test]
fn a_pack_modrinth_mod_cannot_be_replaced() {
    let mut have = installed("pack-1.0.0.jar", true, fabric("pack", "1.0.0"));
    have.from_modrinth_pack = true;
    let out = judge_deposits(
        &profile(),
        &[have],
        &[deposit(vec![fabric("pack", "2.0.0")])],
    );
    assert_eq!(refusal(&out[0]), "MOD_IN_MODRINTH_PACK");
}

#[test]
fn the_engine_the_game_version_and_the_archive_decide_before_anything_else() {
    let forge = identity(JarLoader::Forge, "f", "1.0.0", None, &[]);
    let old = identity(JarLoader::Fabric, "o", "1.0.0", Some("~1.19.2"), &[]);
    let unreadable = Deposit {
        archive_refused: true,
        too_large: false,
        identities: vec![],
    };
    let huge = Deposit {
        archive_refused: false,
        too_large: true,
        identities: vec![],
    };
    let out = judge_deposits(
        &profile(),
        &[],
        &[
            deposit(vec![forge]),
            deposit(vec![old]),
            unreadable,
            huge,
            deposit(vec![]),
        ],
    );
    let codes: Vec<_> = out.iter().map(refusal).collect();
    assert_eq!(
        codes,
        [
            "MOD_INCOMPATIBLE_LOADER",
            "MOD_INCOMPATIBLE_MINECRAFT",
            "MOD_ARCHIVE_REFUSED",
            "MOD_TOO_LARGE",
            "MOD_UNRECOGNIZED"
        ]
    );
    let Err((Refusal::IncompatibleLoader { jar, server }, _)) = &out[0] else {
        panic!("moteur");
    };
    assert_eq!((*jar, *server), (JarLoader::Forge, Some(JarLoader::Fabric)));
}

#[test]
fn two_deposits_of_the_same_mod_are_both_refused() {
    let out = judge_deposits(
        &profile(),
        &[],
        &[
            deposit(vec![fabric("a", "1.0.0")]),
            deposit(vec![fabric("a", "2.0.0")]),
            deposit(vec![fabric("b", "1.0.0")]),
        ],
    );
    assert_eq!(refusal(&out[0]), "MOD_DUPLICATE_IN_BATCH");
    assert_eq!(refusal(&out[1]), "MOD_DUPLICATE_IN_BATCH");
    assert!(out[2].is_ok());
}

#[test]
fn a_missing_dependency_is_to_complete_and_a_batch_neighbour_satisfies_it() {
    let needy = identity(
        JarLoader::Fabric,
        "needy",
        "1.0.0",
        Some("~1.20.1"),
        &["lib"],
    );
    let alone = judge_deposits(&profile(), &[], &[deposit(vec![needy.clone()])]);
    assert!(matches!(
        &alone[0].as_ref().unwrap().verdict,
        Verdict::ToComplete(missing) if missing.len() == 1
    ));
    let together = judge_deposits(
        &profile(),
        &[],
        &[deposit(vec![needy]), deposit(vec![fabric("lib", "1.0.0")])],
    );
    assert_eq!(together[0].as_ref().unwrap().verdict, Verdict::Compatible);
}

#[test]
fn an_unknown_server_version_is_unverified_not_refused() {
    let unknown = ServerProfile {
        loader: Some(JarLoader::Fabric),
        minecraft: None,
    };
    let out = judge_deposits(&unknown, &[], &[deposit(vec![fabric("a", "1.0.0")])]);
    assert_eq!(out[0].as_ref().unwrap().verdict, Verdict::Unverified);
}
